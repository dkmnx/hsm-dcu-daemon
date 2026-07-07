# Phase 1C: `dcu-tun` — TUN Interface

## Overview

Manage the Linux TUN network interface: create device, configure IPv6 addresses, routes, MTU. This is how the daemon exposes an IPv6 interface to the host.

**Replaces**: `src/util/tunnel.c`, `src/util/TunnelIPv6Interface.*`, `src/util/netif-mgmt.c`, `src/util/IPv6Helpers.*`

**Effort**: 3-4 days

## Source Files to Port

| C/C++ File                         | LOC  | What to Extract                                   |
| ---------------------------------- | ---- | ------------------------------------------------- |
| `src/util/tunnel.c`                | 239  | TUN device allocation (`/dev/net/tun`, `IFF_TUN`) |
| `src/util/tunnel.h`                | 52   | Tunnel API                                        |
| `src/util/TunnelIPv6Interface.cpp` | 815  | IPv6 tunnel interface management                  |
| `src/util/TunnelIPv6Interface.h`   | 139  | Class definition                                  |
| `src/util/netif-mgmt.c`            | 538  | Network interface `ioctl` operations              |
| `src/util/IPv6Helpers.cpp`         | 64   | IPv6 address manipulation                         |
| `src/util/IPv6PacketMatcher.cpp`   | 555  | IPv6 packet classification                        |

**Total C/C++ code**: ~2,402 LOC

## Crate Structure

```text
dcu-tun/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── device.rs           # TUN device open/close/configure
    ├── interface.rs        # IPv6 interface management
    ├── ioctl.rs            # Low-level ioctl wrappers
    └── packet.rs           # Packet read/write, MTU handling
```

## Detailed File Specs

### `device.rs`

Reimplements `tunnel.c` TUN device allocation:

```rust
use std::os::unix::io::{AsRawFd, RawFd, FromRawFd};

pub struct TunDevice {
    fd: RawFd,
    name: String,
}

#[derive(Debug, Clone)]
pub struct TunConfig {
    pub name: String,           // Interface name (default: "wfan0")
    pub mtu: u16,               // MTU (default: 1280)
    pub no_packet_info: bool,   // IFF_NO_PI flag (always true)
}

impl TunDevice {
    /// Open TUN device. Equivalent to tunnel.c open_tun() + ifreq setup.
    pub fn open(config: TunConfig) -> Result<Self, TunError>;

    /// Close TUN device.
    pub fn close(&mut self) -> Result<(), TunError>;

    /// Get interface name (assigned by kernel if requested name taken).
    pub fn name(&self) -> &str;

    /// Set MTU on the interface.
    pub fn set_mtu(&self, mtu: u16) -> Result<(), TunError>;

    /// Bring interface up/down.
    pub fn set_up(&self, up: bool) -> Result<(), TunError>;
}

impl AsRawFd for TunDevice { ... }

impl Drop for TunDevice {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
```

The C implementation from `tunnel.c`:
1. Open `/dev/net/tun`
2. `ioctl(fd, TUNSETIFF, &ifr)` with `IFF_TUN | IFF_NO_PI`
3. Copy assigned name back to `ifr.ifr_name`
4. Set MTU via `SIOCSIFMTU`
5. Bring up via `SIOCSIFFLAGS`

### `interface.rs`

Reimplements `TunnelIPv6Interface.cpp`:

```rust
pub struct TunnelIPv6Interface {
    device: TunDevice,
    addresses: Vec<Ipv6Net>,
}

impl TunnelIPv6Interface {
    /// Create and configure the TUN interface.
    pub fn new(config: TunConfig) -> Result<Self, TunError>;

    /// Add IPv6 address. Equivalent to SIOCAIFADDR_IN6.
    pub fn add_address(&self, addr: Ipv6Addr, prefix_len: u8) -> Result<(), TunError>;

    /// Remove IPv6 address.
    pub fn remove_address(&self, addr: Ipv6Addr, prefix_len: u8) -> Result<(), TunError>;

    /// List all IPv6 addresses on the interface.
    pub fn list_addresses(&self) -> Result<Vec<Ipv6Net>, TunError>;

    /// Add IPv6 route.
    pub fn add_route(&self, dest: Ipv6Net, gateway: Option<Ipv6Addr>) -> Result<(), TunError>;

    /// Remove IPv6 route.
    pub fn remove_route(&self, dest: Ipv6Net) -> Result<(), TunError>;

    /// Check if interface is up.
    pub fn is_up(&self) -> Result<bool, TunError>;

    /// Read a packet from the TUN device (blocking).
    pub fn read_packet(&self, buf: &mut [u8]) -> Result<usize, TunError>;

    /// Write a packet to the TUN device.
    pub fn write_packet(&self, buf: &[u8]) -> Result<usize, TunError>;

    /// Async read via tokio (for event loop integration).
    pub async fn async_read_packet(&self, buf: &mut [u8]) -> Result<usize, TunError>;

    /// Async write via tokio.
    pub async fn async_write_packet(&self, buf: &[u8]) -> Result<usize, TunError>;
}
```

### `ioctl.rs`

Low-level ioctl wrappers matching `netif-mgmt.c`:

```rust
use nix::sys::ioctl;

// SIOCSIFADDR - set interface address
// SIOCGIFFLAGS / SIOCSIFFLAGS - get/set interface flags
// SIOCSIFMTU - set MTU
// SIOCAIFADDR_IN6 - add IPv6 address with prefix
// SIOCDIFADDR_IN6 - delete IPv6 address

pub unsafe fn set_interface_address(fd: RawFd, name: &str, addr: &SockaddrIn6) -> Result<(), TunError>;
pub unsafe fn get_interface_flags(fd: RawFd, name: &str) -> Result<i32, TunError>;
pub unsafe fn set_interface_flags(fd: RawFd, name: &str, flags: i32) -> Result<(), TunError>;
pub unsafe fn set_interface_mtu(fd: RawFd, name: &str, mtu: u16) -> Result<(), TunError>;
pub unsafe fn add_ipv6_address(fd: RawFd, name: &str, addr: &In6Addr, prefix_len: u8) -> Result<(), TunError>;
pub unsafe fn delete_ipv6_address(fd: RawFd, name: &str, addr: &In6Addr, prefix_len: u8) -> Result<(), TunError>;
```

### `packet.rs`

```rust
/// Check if a buffer contains an IPv6 packet.
pub fn is_ipv6_packet(buf: &[u8]) -> bool;

/// Get IPv6 payload from raw TUN packet (strip TUN header if present).
pub fn get_ipv6_payload(buf: &[u8]) -> &[u8];

/// Parse IPv6 header fields.
pub fn parse_ipv6_header(buf: &[u8]) -> Result<IPv6Header, TunError>;
```

## Tests

### Test 1: TUN Device Open/Close

```rust
#[test]
#[ignore]  // Requires /dev/net/tun and root
fn tun_device_lifecycle() {
    let config = TunConfig {
        name: "test_tun0".into(),
        mtu: 1280,
        no_packet_info: true,
    };
    let dev = TunDevice::open(config).unwrap();
    assert_eq!(dev.name(), "test_tun0");
    dev.close().unwrap();
}
```

### Test 2: TUN Device in Network Namespace (CI-safe)

```rust
#[test]
fn tun_device_in_namespace() {
    // Create a network namespace, open TUN inside it
    // This doesn't require root on the host
    // Requires CAP_NET_ADMIN or unshare
}
```

### Test 3: IPv6 Address Formatting

```rust
#[test]
fn ipv6_address_format() {
    let addr: Ipv6Addr = "2020:abcd::212:4b00:14f7:d160".parse().unwrap();
    assert_eq!(addr.octets(), [0x20, 0x20, 0xAB, 0xCD, 0, 0, 0, 0,
                               0x02, 0x12, 0x4B, 0x00, 0x14, 0xF7, 0xD1, 0x60]);
}
```

### Test 4: MTU Validation

```rust
#[test]
fn mtu_bounds_check() {
    assert!(TunConfig { name: "t".into(), mtu: 1280, no_packet_info: true }.is_valid());
    assert!(TunConfig { name: "t".into(), mtu: 1200, no_packet_info: true }.is_valid());
    assert!(!TunConfig { name: "t".into(), mtu: 64, no_packet_info: true }.is_valid());
    assert!(!TunConfig { name: "t".into(), mtu: 65535, no_packet_info: true }.is_valid());
}
```

### Test 5: Packet Read/Write Mock

```rust
#[test]
fn packet_round_trip_via_pipe() {
    // Use a pipe pair to simulate TUN read/write
    // (testing the packet parsing layer without hardware)
}
```

### Test 6: IPv6 Header Parsing

```rust
#[test]
fn parse_ipv6_header_basic() {
    let packet = vec![
        0x60, 0x00, 0x00, 0x00,  // version=6, traffic class, flow label
        0x00, 0x10,              // payload length = 16
        0x3A,                    // next header = ICMPv6
        0x40,                    // hop limit = 64
        // source: 2020:abcd::1
        0x20, 0x20, 0xAB, 0xCD, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 1,
        // dest: 2020:abcd::2
        0x20, 0x20, 0xAB, 0xCD, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 2,
    ];
    let header = parse_ipv6_header(&packet).unwrap();
    assert_eq!(header.version, 6);
    assert_eq!(header.payload_length, 16);
    assert_eq!(header.next_header, 0x3A);
    assert_eq!(header.hop_limit, 64);
}
```

## Dependencies

```toml
[dependencies]
nix = { version = "0.29", features = ["ioctl", "net", "fs"] }
tokio = { version = "1", features = ["net", "io-util"] }
wisun-types = { path = "../wisun-types" }
tracing = "0.1"
thiserror = "2"

[dev-dependencies]
tempfile = "3"
```

## Verification Checklist

- [ ] TUN device opens and closes cleanly
- [ ] IPv6 addresses can be added and removed
- [ ] Packet read/write works via PTY loopback
- [ ] Async read/write integrates with tokio
- [ ] `ioctl` calls match C implementation (compare strace output)
- [ ] `cargo test` passes (non-ignored tests)
- [ ] `cargo clippy` produces zero warnings
- [ ] Only `ioctl.rs` contains `unsafe` code
