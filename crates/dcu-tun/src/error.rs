//! Error types and shared aliases for the `dcu-tun` crate.

use std::net::Ipv6Addr;

/// A network prefix on an interface, expressed as an IPv6 address plus
/// prefix length (e.g. `2020:abcd::/64`). Re-export of [`ipnet::Ipv6Net`],
/// the canonical type used by the daemon for address/route entries.
pub type Ipv6Net = ipnet::Ipv6Net;

/// Errors raised while managing the Linux TUN interface.
#[derive(Debug, thiserror::Error)]
pub enum TunError {
    /// Opening `/dev/net/tun` or the netif-management socket failed.
    #[error("failed to open tun device: {0}")]
    Open(#[from] std::io::Error),

    /// An `ioctl` call returned an error. `op` names the operation for logs.
    #[error("ioctl {op} failed: {source}")]
    Ioctl {
        /// Human-readable name of the failing ioctl (e.g. `"TUNSETIFF"`).
        op: &'static str,
        /// Underlying OS error.
        source: std::io::Error,
    },

    /// An address operation was given the unspecified (`::`) address.
    #[error("address {0} is unspecified")]
    Unspecified(Ipv6Addr),

    /// The requested interface name exceeds `IFNAMSIZ`.
    #[error("interface name too long")]
    NameTooLong,

    /// Configuration validation failed.
    #[error("invalid config: {0}")]
    InvalidConfig(String),

    /// Enumerating interface addresses via `getifaddrs` failed.
    #[error("getifaddrs failed: {0}")]
    AddrList(#[from] nix::Error),
}
