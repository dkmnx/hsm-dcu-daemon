//! Transport dispatch: classify `Config:NCP:SocketPath` and open the
//! correct transport.
//!
//! Replaces C's `get_super_socket_type_from_path()` + `open_super_socket()`
//! in `socket-utils.c`.

use crate::error::SerialError;
use crate::system::{FdTransport, SystemSocketpairTransport, SystemTransport};
use crate::tcp::TcpTransport;
use crate::transport::Transport;
use crate::uart::{SerialConfig, UartTransport};

/// Transport type tags matching the C `SUPER_SOCKET_TYPE_*` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketPathType {
    /// `serial:`, `file:`, or bare `/dev/tty*` path.
    Device,
    /// `tcp:` or bare `host:port` / port number.
    Tcp,
    /// `system:` — forkpty + exec child process.
    System,
    /// `system-forkpty:` — explicit forkpty.
    SystemForkpty,
    /// `system-socketpair:` — fork + socketpair.
    SystemSocketpair,
    /// `fd:` — dup a raw file descriptor.
    Fd,
}

/// Parsed comma-separated socket options from the path string.
/// Only applicable to device/fd transports.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SocketOptions {
    /// Override baud rate (`b115200`).
    pub baud_rate: Option<u32>,
    /// Raw mode (`raw`).
    pub raw_mode: bool,
    /// Default mode (`default`) — raw + default baud.
    pub default_mode: bool,
    /// Hardware flow control (`crtscts=0|1`).
    pub crtscts: Option<bool>,
    /// CLOCAL flag (`clocal=0|1`).
    pub clocal: Option<bool>,
    /// Software input flow control (`ixoff=0|1`).
    pub ixoff: Option<bool>,
    /// Software output flow control (`ixon=0|1`).
    pub ixon: Option<bool>,
    /// Any character restarts output (`ixany=0|1`).
    pub ixany: Option<bool>,
}

/// Classify a socket path string by its prefix.
///
/// Case-insensitive prefix check matching C's
/// `get_super_socket_type_from_path()`.
pub fn classify_path(path: &str) -> SocketPathType {
    let lower = path.to_ascii_lowercase();

    if lower.starts_with("system-forkpty:") {
        SocketPathType::SystemForkpty
    } else if lower.starts_with("system-socketpair:") {
        SocketPathType::SystemSocketpair
    } else if lower.starts_with("system:") {
        SocketPathType::System
    } else if lower.starts_with("fd:") {
        SocketPathType::Fd
    } else if lower.starts_with("tcp:") {
        SocketPathType::Tcp
    } else if lower.starts_with("file:") || lower.starts_with("serial:") {
        SocketPathType::Device
    } else if path.starts_with('[') || is_inet(path) || is_port(path) {
        SocketPathType::Tcp
    } else {
        // Default: treat as a device path (e.g. /dev/ttyUSB0)
        SocketPathType::Device
    }
}

/// Parse comma-separated options from the path string and return
/// `(filename, options)`.
///
/// The filename is the part before the first `,` (or after the prefix).
/// Matches C's option parsing in `open_super_socket()`.
pub fn parse_socket_options(path: &str) -> (String, SocketOptions) {
    let path_type = classify_path(path);

    // Strip prefix to get the raw content after "type:"
    let after_prefix = match path_type {
        SocketPathType::System => strip_prefix_case_insensitive(path, "system:"),
        SocketPathType::SystemForkpty => strip_prefix_case_insensitive(path, "system-forkpty:"),
        SocketPathType::SystemSocketpair => {
            strip_prefix_case_insensitive(path, "system-socketpair:")
        }
        SocketPathType::Fd => strip_prefix_case_insensitive(path, "fd:"),
        SocketPathType::Tcp => strip_prefix_case_insensitive(path, "tcp:"),
        SocketPathType::Device => {
            let lower = path.to_ascii_lowercase();
            if lower.starts_with("serial:") {
                &path["serial:".len()..]
            } else if lower.starts_with("file:") {
                &path["file:".len()..]
            } else {
                // Bare device path — no prefix, options after first comma
                path
            }
        }
    };

    // For TCP with no prefix (bare host:port), the whole string is the address
    if path_type == SocketPathType::Tcp && after_prefix == path {
        return (path.to_string(), SocketOptions::default());
    }

    // Split on first comma for options
    if let Some(comma_pos) = after_prefix.find(',') {
        let filename = after_prefix[..comma_pos].to_string();
        let options_str = &after_prefix[comma_pos..];
        let options = parse_options_string(options_str);
        (filename, options)
    } else {
        (after_prefix.to_string(), SocketOptions::default())
    }
}

/// Parse a comma-separated options string like `,b115200,raw,crtscts=1`.
fn parse_options_string(options: &str) -> SocketOptions {
    let mut opts = SocketOptions::default();

    for option in options.split(',') {
        let option = option.trim();
        if option.is_empty() {
            continue;
        }

        let lower = option.to_ascii_lowercase();

        if lower.starts_with('b') && lower.len() > 1 {
            // Baud rate: b115200
            if let Ok(baud) = lower[1..].parse::<u32>() {
                opts.baud_rate = Some(baud);
            }
        } else if lower == "default" {
            opts.default_mode = true;
        } else if lower == "raw" {
            opts.raw_mode = true;
        } else if let Some(val) = lower.strip_prefix("crtscts=") {
            opts.crtscts = parse_bool_option(val);
        } else if let Some(val) = lower.strip_prefix("clocal=") {
            opts.clocal = parse_bool_option(val);
        } else if let Some(val) = lower.strip_prefix("ixoff=") {
            opts.ixoff = parse_bool_option(val);
        } else if let Some(val) = lower.strip_prefix("ixon=") {
            opts.ixon = parse_bool_option(val);
        } else if let Some(val) = lower.strip_prefix("ixany=") {
            opts.ixany = parse_bool_option(val);
        }
        // Unknown options are silently ignored (matching C behavior)
    }

    opts
}

fn parse_bool_option(val: &str) -> Option<bool> {
    match val {
        "1" | "true" | "on" => Some(true),
        "0" | "false" | "off" => Some(false),
        _ => None,
    }
}

/// Check if a string looks like an inet address (contains no slashes,
/// is not a port, not a system command, and is not empty).
fn is_inet(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.starts_with('[') {
        return true;
    }
    if s.contains('/') {
        return false;
    }
    !is_port(s) && !is_system_command(s)
}

/// Check if a string is purely digits (a port number).
fn is_port(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// Check if a string starts with a `system:` family prefix.
fn is_system_command(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.starts_with("system:")
        || lower.starts_with("system-forkpty:")
        || lower.starts_with("system-socketpair:")
}

/// Strip a prefix case-insensitively, returning the remainder.
fn strip_prefix_case_insensitive<'a>(s: &'a str, prefix: &str) -> &'a str {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        &s[prefix.len()..]
    } else {
        s
    }
}

/// Open the appropriate transport for the given socket path + baud rate.
///
/// This is the Rust equivalent of C's `open_super_socket()`.
/// Currently supports Device (UART) and TCP transports.
/// System/fd transports are not yet implemented.
pub async fn open_transport(
    socket_path: &str,
    baud: u32,
) -> Result<Box<dyn Transport>, SerialError> {
    let (filename, options) = parse_socket_options(socket_path);
    let effective_baud = options.baud_rate.unwrap_or(baud);

    match classify_path(socket_path) {
        SocketPathType::Device => {
            let mut config = SerialConfig {
                path: filename,
                baud_rate: effective_baud,
                ..Default::default()
            };
            if options.default_mode {
                config.baud_rate = options
                    .baud_rate
                    .unwrap_or(SerialConfig::default().baud_rate);
                config.flow_control = false;
            }
            if let Some(crtscts) = options.crtscts {
                config.flow_control = crtscts;
            }
            Ok(Box::new(UartTransport::open(config)?))
        }
        SocketPathType::Tcp => {
            let addr_str = if filename.is_empty() {
                socket_path
            } else {
                &filename
            };
            // For bare host:port without tcp: prefix, pass the original
            // since parse_socket_options already handled it
            let addr = if classify_path(socket_path) == SocketPathType::Tcp
                && !socket_path.to_ascii_lowercase().starts_with("tcp:")
            {
                socket_path
            } else {
                addr_str
            };
            Ok(Box::new(TcpTransport::connect(addr).await?))
        }
        SocketPathType::System | SocketPathType::SystemForkpty => {
            Ok(Box::new(SystemTransport::spawn(&filename).await?))
        }
        SocketPathType::SystemSocketpair => {
            Ok(Box::new(SystemSocketpairTransport::spawn(&filename).await?))
        }
        SocketPathType::Fd => Ok(Box::new(FdTransport::spawn(&filename).await?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- classify_path tests ---

    #[test]
    fn classify_system_prefix() {
        assert_eq!(
            classify_path("system:my_ncp_binary"),
            SocketPathType::System
        );
    }

    #[test]
    fn classify_system_forkpty_prefix() {
        assert_eq!(
            classify_path("system-forkpty:my_ncp"),
            SocketPathType::SystemForkpty
        );
    }

    #[test]
    fn classify_system_socketpair_prefix() {
        assert_eq!(
            classify_path("system-socketpair:my_ncp"),
            SocketPathType::SystemSocketpair
        );
    }

    #[test]
    fn classify_fd_prefix() {
        assert_eq!(classify_path("fd:3"), SocketPathType::Fd);
    }

    #[test]
    fn classify_tcp_prefix() {
        assert_eq!(classify_path("tcp:localhost:9000"), SocketPathType::Tcp);
    }

    #[test]
    fn classify_serial_prefix() {
        assert_eq!(classify_path("serial:/dev/ttyUSB0"), SocketPathType::Device);
    }

    #[test]
    fn classify_file_prefix() {
        assert_eq!(classify_path("file:/dev/ttyACM0"), SocketPathType::Device);
    }

    #[test]
    fn classify_bare_device_path() {
        assert_eq!(classify_path("/dev/ttyUSB0"), SocketPathType::Device);
    }

    #[test]
    fn classify_bare_port() {
        assert_eq!(classify_path("9999"), SocketPathType::Tcp);
    }

    #[test]
    fn classify_bare_host_port() {
        assert_eq!(classify_path("127.0.0.1:8080"), SocketPathType::Tcp);
    }

    #[test]
    fn classify_bracketed_ipv6() {
        assert_eq!(classify_path("[::1]:8080"), SocketPathType::Tcp);
    }

    #[test]
    fn classify_case_insensitive() {
        assert_eq!(classify_path("SYSTEM:cmd"), SocketPathType::System);
        assert_eq!(classify_path("TCP:localhost:9000"), SocketPathType::Tcp);
        assert_eq!(classify_path("Serial:/dev/ttyUSB0"), SocketPathType::Device);
    }

    // --- parse_socket_options tests ---

    #[test]
    fn parse_options_baud_rate() {
        let (file, opts) = parse_socket_options("/dev/ttyUSB0,b115200");
        assert_eq!(file, "/dev/ttyUSB0");
        assert_eq!(opts.baud_rate, Some(115_200));
    }

    #[test]
    fn parse_options_raw_mode() {
        let (file, opts) = parse_socket_options("serial:/dev/ttyUSB0,raw");
        assert_eq!(file, "/dev/ttyUSB0");
        assert!(opts.raw_mode);
    }

    #[test]
    fn parse_options_default_mode() {
        let (file, opts) = parse_socket_options("/dev/ttyUSB0,default");
        assert_eq!(file, "/dev/ttyUSB0");
        assert!(opts.default_mode);
    }

    #[test]
    fn parse_options_multiple() {
        let (file, opts) = parse_socket_options("serial:/dev/ttyUSB0,b57600,raw,crtscts=0");
        assert_eq!(file, "/dev/ttyUSB0");
        assert_eq!(opts.baud_rate, Some(57_600));
        assert!(opts.raw_mode);
        assert_eq!(opts.crtscts, Some(false));
    }

    #[test]
    fn parse_options_tcp_prefix() {
        let (file, opts) = parse_socket_options("tcp:localhost:9000");
        assert_eq!(file, "localhost:9000");
        assert_eq!(opts, SocketOptions::default());
    }

    #[test]
    fn parse_options_system_prefix() {
        let (file, opts) = parse_socket_options("system:ncp_binary");
        assert_eq!(file, "ncp_binary");
        assert_eq!(opts, SocketOptions::default());
    }

    #[test]
    fn parse_options_bare_device_with_opts() {
        let (file, opts) = parse_socket_options("/dev/ttyUSB0,b115200");
        assert_eq!(file, "/dev/ttyUSB0");
        assert_eq!(opts.baud_rate, Some(115_200));
    }

    // --- helper function tests ---

    #[test]
    fn is_port_true() {
        assert!(is_port("8080"));
        assert!(is_port("0"));
    }

    #[test]
    fn is_port_false() {
        assert!(!is_port("abc"));
        assert!(!is_port(""));
        assert!(!is_port("127.0.0.1:8080"));
    }

    #[test]
    fn is_system_command_true() {
        assert!(is_system_command("system:cmd"));
        assert!(is_system_command("system-forkpty:cmd"));
        assert!(is_system_command("system-socketpair:cmd"));
    }

    #[test]
    fn is_system_command_false() {
        assert!(!is_system_command("/dev/ttyUSB0"));
        assert!(!is_system_command("tcp:localhost:9000"));
    }
}
