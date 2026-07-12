//! TCP socket transport for remote NCP connections.

use std::net::SocketAddr;
use std::os::unix::io::{AsRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

use crate::error::SerialError;
use crate::transport::Transport;

/// A TCP socket transport matching the C `host:port` / `tcp:` path in
/// `Config:NCP:SocketPath`. Resolves DNS and connects over IPv4 or IPv6.
pub struct TcpTransport {
    inner: TcpStream,
    peer: SocketAddr,
}

/// Intermediate parse result: either a resolved `SocketAddr` or a
/// `(host, port)` pair that needs DNS resolution.
enum ParsedAddr {
    Resolved(SocketAddr),
    NeedsDns(String, u16),
}

impl TcpTransport {
    /// Connect timeout for DNS resolution + TCP handshake.
    const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

    /// Parse a `host:port` or `[ipv6]:port` address string and connect.
    /// Uses a 5-second timeout for DNS + connect to avoid blocking startup
    /// on unreachable hosts.
    pub async fn connect(addr: &str) -> Result<Self, SerialError> {
        let parsed = Self::parse_addr(addr)?;
        let resolved = match parsed {
            ParsedAddr::Resolved(a) => a,
            ParsedAddr::NeedsDns(host, port) => {
                use tokio::net::lookup_host;
                let fut = lookup_host(format!("{host}:{port}"));
                let mut addrs = tokio::time::timeout(Self::CONNECT_TIMEOUT, fut)
                    .await
                    .map_err(|_| {
                        SerialError::InvalidConfig(format!(
                            "DNS resolution for {host}:{port} timed out"
                        ))
                    })?
                    .map_err(SerialError::Io)?;
                addrs.next().ok_or_else(|| {
                    SerialError::InvalidConfig(format!("DNS resolution failed for {host}:{port}"))
                })?
            }
        };
        let connect_fut = TcpStream::connect(resolved);
        let inner = tokio::time::timeout(Self::CONNECT_TIMEOUT, connect_fut)
            .await
            .map_err(|_| {
                SerialError::InvalidConfig(format!(
                    "TCP connect to {addr} timed out after {}s",
                    Self::CONNECT_TIMEOUT.as_secs()
                ))
            })?
            .map_err(SerialError::Io)?;
        let peer = inner.peer_addr()?;
        Ok(Self { inner, peer })
    }

    /// Parse `host:port` or `[ipv6]:port` into a `ParsedAddr`.
    fn parse_addr(addr: &str) -> Result<ParsedAddr, SerialError> {
        // Try direct parse first (handles "127.0.0.1:8080", "[::1]:8080")
        if let Ok(parsed) = addr.parse::<SocketAddr>() {
            return Ok(ParsedAddr::Resolved(parsed));
        }

        // Try bracketed IPv6 or hostname: "[::1]:8080" or "[myhost]:3000"
        if addr.starts_with('[') {
            if let Some(close) = addr.find(']') {
                let inner = &addr[1..close];
                let rest = &addr[close + 1..];
                if let Some(port_str) = rest.strip_prefix(':') {
                    if let Ok(port) = port_str.parse::<u16>() {
                        if let Ok(ip) = inner.parse::<std::net::IpAddr>() {
                            return Ok(ParsedAddr::Resolved(SocketAddr::new(ip, port)));
                        }
                        return Ok(ParsedAddr::NeedsDns(inner.to_string(), port));
                    }
                }
            }
        }

        // Try host:port — split on the last colon
        if let Some(colon) = addr.rfind(':') {
            let host = &addr[..colon];
            if let Ok(port) = addr[colon + 1..].parse::<u16>() {
                // If host is a literal IP, resolve directly
                if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                    return Ok(ParsedAddr::Resolved(SocketAddr::new(ip, port)));
                }
                return Ok(ParsedAddr::NeedsDns(host.to_string(), port));
            }
        }

        // Last resort: try as a bare port number (connect to localhost)
        if let Ok(port) = addr.parse::<u16>() {
            return Ok(ParsedAddr::Resolved(SocketAddr::new(
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                port,
            )));
        }

        Err(SerialError::InvalidConfig(format!(
            "invalid TCP address: {addr}"
        )))
    }
}

impl Transport for TcpTransport {
    fn raw_fd(&self) -> Option<RawFd> {
        Some(self.inner.as_raw_fd())
    }

    fn info(&self) -> String {
        format!("TCP:{}", self.peer)
    }
}

impl AsyncRead for TcpTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for TcpTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_addr_ipv4_literal() {
        let addr = TcpTransport::parse_addr("127.0.0.1:8080").unwrap();
        match addr {
            ParsedAddr::Resolved(a) => {
                assert_eq!(a.port(), 8080);
                assert!(a.is_ipv4());
            }
            _ => panic!("expected Resolved"),
        }
    }

    #[test]
    fn parse_addr_ipv6_bracketed_literal() {
        let addr = TcpTransport::parse_addr("[::1]:9090").unwrap();
        match addr {
            ParsedAddr::Resolved(a) => {
                assert_eq!(a.port(), 9090);
                assert!(a.is_ipv6());
            }
            _ => panic!("expected Resolved"),
        }
    }

    #[test]
    fn parse_addr_hostname_needs_dns() {
        let addr = TcpTransport::parse_addr("example.com:80").unwrap();
        match addr {
            ParsedAddr::NeedsDns(host, port) => {
                assert_eq!(host, "example.com");
                assert_eq!(port, 80);
            }
            _ => panic!("expected NeedsDns"),
        }
    }

    #[test]
    fn parse_addr_bare_port() {
        let addr = TcpTransport::parse_addr("9999").unwrap();
        match addr {
            ParsedAddr::Resolved(a) => {
                assert_eq!(a.port(), 9999);
                assert!(a.is_ipv4());
            }
            _ => panic!("expected Resolved"),
        }
    }

    #[test]
    fn parse_addr_invalid() {
        assert!(TcpTransport::parse_addr("not-a-port").is_err());
    }

    #[test]
    fn parse_addr_ipv6_hostname_needs_dns() {
        let addr = TcpTransport::parse_addr("[myhost]:3000").unwrap();
        match addr {
            ParsedAddr::NeedsDns(host, port) => {
                assert_eq!(host, "myhost");
                assert_eq!(port, 3000);
            }
            _ => panic!("expected NeedsDns for bracketed hostname"),
        }
    }
}
