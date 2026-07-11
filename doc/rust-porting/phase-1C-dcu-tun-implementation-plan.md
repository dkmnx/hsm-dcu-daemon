# Implementation Plan: Phase 1C `dcu-tun` — TUN Interface Crate

**Status:** Implemented (2026-07-10). All 8 verification steps pass: `cargo build`, `cargo clippy -D warnings`, and `cargo test` (4 passed, 2 ignored).
**Goal:** Port `src/util/tunnel.c`, `tunnel.h`, `TunnelIPv6Interface.*`, `netif-mgmt.c`, `IPv6Helpers.cpp` to a new `crates/dcu-tun` Rust crate that the `dcu-daemon` (phase 3A) depends on.

---

## Corrections to the spec (`phase-1C-dcu-tun.md`)

The spec has defects found by reading the actual C source. This plan fixes them:

1. **`TunConfig::is_valid` does not exist in the spec struct** but Test 4 calls it. Add the method. Valid MTU per Test 4: `1200` and `1280` valid; `64` and `65535` invalid → `is_valid() == (mtu >= 1200 && mtu <= 1280)`.
2. **`Ipv6Net` is undefined.** Use `ipnet::Ipv6Net` (add `ipnet = "2"` dep).
3. **`ioctl.rs` in the spec assumes `nix` exposes `SIOCAIFADDR_IN6` / `set_interface_address` IPv4-specific helpers.** The real C (`netif-mgmt.c`) is Linux IPv6 via `struct in6_ifreq` + `SIOCSIFADDR`/`SIOCDIFADDR`, plus `struct in6_rtmsg` + `SIOCADDRT`/`SIOCDELRT` for routes, all on a `socket(AF_INET6, SOCK_DGRAM, 0)` "netif-mgmt" fd. We implement these directly.
4. **`unsafe` is required for ioctl.** The workspace sets `unsafe_code = "deny"`, but AGENTS.md explicitly exempts `dcu-tun` (ioctl) and `dcu-serial` (serial). So `dcu-tun/Cargo.toml` overrides the workspace lint: it does **not** use `lints.workspace = true`; instead it re-declares `[lints.rust]` with `unsafe_code = "allow"` (must be `"allow"`, not `"warn"`, because `warnings = "deny"` promotes warn-level lints to errors). All `unsafe` is confined to `ioctl.rs` and the fd→`AsyncFd` bridge in `interface.rs`.
5. **Async read/write:** wrap the TUN fd in `tokio::io::unix::AsyncFd` (requires `tokio` features `net`, `io-util`). `TunnelIPv6Interface` keeps the original `OwnedFd` for sync ops and a `try_clone()`-ed fd inside `AsyncFd` for async.
6. **`wisun-types` dependency dropped** from `dcu-tun` — the C uses raw `in6_addr`; we use std `Ipv6Addr`.
7. **Netlink/MLD monitoring omitted.** `TunnelIPv6Interface.cpp` has `setup_signals`/`processNetlinkFD`/`processMLDMonitorFD` (Linux netlink + raw ICMPv6 MLD parsing). That is event-loop machinery for address/link-state callbacks — belongs in the async daemon (phase 3A), not the transport crate. We expose `list_addresses()` (via `getifaddrs`) for polling instead, and note the gap.
8. **`IPv6PacketMatcher` (`IPv6PacketMatcher.cpp`, 555 LOC) is NOT part of this crate.** The spec lists it under "Source Files to Port" but it is firewall/packet classification used by the daemon's data path, not TUN lifecycle. Deferred to phase 3A. `packet.rs` covers only the minimal IPv6 header parse the spec's Test 6 needs.

---

## Crate Structure

```text
crates/dcu-tun/
├── Cargo.toml
└── src/
    ├── lib.rs          # module decls, re-exports
    ├── error.rs        # TunError (thiserror), Ipv6Net type alias
    ├── device.rs       # TunDevice, TunConfig (TUNSETIFF open/close/set_mtu/set_up)
    ├── ioctl.rs        # netif-mgmt ioctl wrappers + getifaddrs list
    ├── interface.rs    # TunnelIPv6Interface: addresses, routes, sync + async packet read/write
    └── packet.rs       # is_ipv6_packet, get_ipv6_payload, IPv6Header
```

---

## File Specs

### `Cargo.toml`

```toml
[package]
name = "dcu-tun"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"

[dependencies]
tokio = { version = "1", features = ["net", "io-util"] }
nix = { version = "0.29", features = ["net", "socket", "ifaddrs"] }
libc = "0.2"
ipnet = "2"
tracing = "0.1"
thiserror = "2"

[dev-dependencies]
tempfile = "3"

# dcu-tun is the ioctl-exempt crate (AGENTS.md). Override the workspace
# `unsafe_code = "deny"` to warn; keep everything else denied.
[lints.rust]
unsafe_code = "warn"
warnings = "deny"

[lints.clippy]
all = "deny"
```

- `[dependencies]`:
  - `tokio = { version = "1", features = ["net", "io-util"] }` (unix `AsyncFd` available via `io-util`/`net` on linux-x64).
  - `nix = { version = "0.29", features = ["net", "socket", "ifaddrs"] }` (ifaddrs for listing, net for sockaddr/ifname).
  - `libc = "0.2"` (ioctl constants + structs: `ifreq`, `in6_ifreq`, `in6_rtmsg`, `TUNSETIFF`, `IFF_TUN`, `IFF_NO_PI`, `IFF_UP`, `IFF_RUNNING`, raw `ioctl`/`read`/`write`).
  - `ipnet = "2"`.
  - `tracing = "0.1"`, `thiserror = "2"`.
- `[dev-dependencies]`: `tempfile = "3"`.
- **Lint override** (critical): do NOT set `lints.workspace = true`. Re-declare `[lints.rust]` with `unsafe_code = "warn"` and keep `warnings = "deny"` + `[lints.clippy] all = "deny"`.

### `error.rs`

- `pub type Ipv6Net = ipnet::Ipv6Net;`
- `#[derive(Debug, thiserror::Error)] pub enum TunError` with variants:
  - `#[error("failed to open tun device: {0}")] Open(std::io::Error)`
  - `#[error("ioctl {op} failed: {source}")] Ioctl { op: &'static str, source: std::io::Error }`
  - `#[error("address {0} is unspecified")] Unspecified(std::net::Ipv6Addr)`
  - `#[error("interface name too long")] NameTooLong`
  - `#[error("invalid config: {0}")] InvalidConfig(String)`
  - `#[error("getifaddrs failed: {0}")] AddrList(std::io::Error)`

### `device.rs`

- `#[derive(Debug, Clone)] pub struct TunConfig { pub name: String, pub mtu: u16, pub no_packet_info: bool }` with `Default { name: "wfan0", mtu: 1280, no_packet_info: true }` and `pub fn is_valid(&self) -> bool { self.mtu >= 1200 && self.mtu <= 1280 }`.
- `pub struct TunDevice { fd: OwnedFd, name: String }`.
- `impl TunDevice`:
  - `pub fn open(config: TunConfig) -> Result<Self, TunError>`: open `/dev/net/tun` (`libc::open` O_RDWR|O_NONBLOCK), `ioctl(TUNSETIFF, ifr{IFF_TUN | (IFF_NO_PI if no_packet_info)})`, read back `ifr.ifr_name`. If `config.name` empty or taken, kernel assigns (we copy assigned name). Store `OwnedFd` + name.
  - `pub fn name(&self) -> &str`.
  - `pub fn set_mtu(&self, mtu: u16)` → `ioctl::set_interface_mtu`.
  - `pub fn set_up(&self, up: bool)` → `ioctl::set_interface_flags` (IFF_UP; down also clears IFF_RUNNING, matching C `set_up(false)`).
  - `pub fn try_clone_fd(&self) -> Result<OwnedFd, TunError>` for async bridge.
  - `impl AsRawFd`. Drop is automatic via `OwnedFd`; provide `pub fn close(self)` consuming for API parity (replaces spec's explicit `close()` + manual `Drop`).
  - **Linux-only gate:** the ioctl module is `#[cfg(target_os = "linux")]`; non-Linux gets a `compile_error!`.

### `ioctl.rs`

All functions are `pub fn ... -> Result<(), TunError>` wrapping `unsafe { libc::ioctl(...) }`. They take the netif-mgmt fd. Provide a helper `pub fn open_netif_socket() -> Result<OwnedFd, TunError>` = `socket(AF_INET6, SOCK_DGRAM, 0)`.

- `get_interface_flags(fd, name) -> Result<i32, TunError>` (SIOCGIFFLAGS)
- `set_interface_flags(fd, name, flags) -> Result<(), TunError>` (SIOCSIFFLAGS)
- `set_interface_up(fd, name, up)` (sets/clears IFF_UP|IFF_RUNNING)
- `set_interface_mtu(fd, name, mtu)` (SIOCSIFMTU)
- `interface_index(fd, name) -> Result<u32, TunError>` (SIOGIFINDEX)
- `add_ipv6_address(fd, name, addr: Ipv6Addr, prefix_len: u8)` — Linux path: `SIOCSIFADDR` with `struct in6_ifreq { addr, prefixlen, ifindex }`; remove-first (C calls remove then add).
- `remove_ipv6_address(fd, name, addr)` — `SIOCDIFADDR` with `in6_ifreq`.
- `add_ipv6_route(fd, name, dest: Ipv6Net, metric: u32)` — `SIOCADDRT` with `in6_rtmsg { dst, dst_len, flags=RTF_UP|(RTF_HOST if /128), metric, ifindex }`.
- `remove_ipv6_route(fd, name, dest, metric)` — `SIOCDELRT`.
- `pub fn list_ipv6_addresses(name: &str) -> Result<Vec<Ipv6Net>, TunError>` via `nix::ifaddrs::getifaddrs()` (safe), filter AF_INET6 + name match, build `ipnet::Ipv6Net::new(addr, prefixlen)`.

### `interface.rs`

- `pub struct TunnelIPv6Interface { device: TunDevice, netif_fd: OwnedFd, async_fd: AsyncFd<OwnedFd>, mtu: u16 }`.
- `new(config: TunConfig) -> Result<Self, TunError>`: open device, open netif socket, `set_mtu`, build `AsyncFd::new(device.try_clone_fd()?)`.
- `add_address(&self, addr, prefix_len)`, `remove_address`, `list_addresses() -> Result<Vec<Ipv6Net>>` (delegates to ioctl::list), `add_route(dest: Ipv6Net, gateway: Option<Ipv6Addr>, metric: u32)` (gateway unused on Linux SIOCADRT path but kept for API parity; ignored), `remove_route`, `is_up() -> Result<bool>` (flags & IFF_UP), `read_packet(&self, buf) -> Result<usize>` (libc::read; strip 4-byte AF header if present, matching C `read()`), `write_packet(&self, buf)` (libc::write), `async_read_packet`/`async_write_packet` via `AsyncFd::readable()/writable()` + `try_io`.
- Note: C `read()` strips a 4-byte subheader when `data[0]==0 && data[1]==0`; replicate.

### `packet.rs`

- `pub fn is_ipv6_packet(buf: &[u8]) -> bool` (len >= 40 && (buf[0] >> 4) == 6).
- `pub fn get_ipv6_payload(buf: &[u8]) -> &[u8]` (returns buf as-is; TUN raw frames are already IPv6 — the 4-byte header strip is in `interface.rs` `read_packet`).
- `#[derive(Debug, Clone, PartialEq, Eq)] pub struct IPv6Header { pub version: u8, pub traffic_class: u8, pub flow_label: u32, pub payload_length: u16, pub next_header: u8, pub hop_limit: u8, pub source: Ipv6Addr, pub destination: Ipv6Addr }`.
- `pub fn parse_ipv6_header(buf: &[u8]) -> Result<IPv6Header, TunError>`: require len >= 40; decode version (top nibble), traffic class (buf[0] low 4 bits << 4 | buf[1] high 4), flow label (20 bits from buf[1..4]), payload_length (buf[4..6] big-endian), next_header (buf[6]), hop_limit (buf[7]), source/dest (buf[8..24], buf[24..40]). Matches Test 6 vector exactly.

---

## Tests (in `src/lib.rs` `#[cfg(test)]`)

- **Test 1** `tun_device_lifecycle` — `#[ignore]` (needs `/dev/net/tun` + root).
- **Test 2** `tun_device_in_namespace` — `#[ignore]` (needs unshare/CAP_NET_ADMIN).
- **Test 3** `ipv6_address_format` — runs; pure parse of `2020:abcd::212:4b00:14f7:d160`.
- **Test 4** `mtu_bounds_check` — runs; exercises `TunConfig::is_valid()` (added).
- **Test 5** `packet_round_trip_via_pipe` — runs; write a known IPv6 packet to a `pipe()`, read back, assert bytes equal (no TUN needed).
- **Test 6** `parse_ipv6_header_basic` — runs; exact vector from spec; asserts version=6, payload_length=16, next_header=0x3A, hop_limit=64.

Tests 3/4/5/6 run in CI without privileges. 1/2 ignored.

---

## Verification (matches spec checklist, corrected)

1. `cargo build -p dcu-tun` — compiles on linux-x64.
2. `cargo clippy -p dcu-tun --all-targets -- -D warnings` — zero warnings (note `unsafe_code` is `warn` here, allowed by AGENTS.md).
3. `cargo test -p dcu-tun` — Tests 3/4/5/6 pass; 1/2 ignored.
4. Manual (root): `cargo test -p dcu-tun -- --ignored tun_device_lifecycle` after `sudo unshare -rn`.

---

## Out of scope (deferred)

- Netlink address/link-state subscription + MLD listener (event-loop concern → phase 3A).
- `IPv6PacketMatcher` firewall classification (→ phase 3A data path).
- `tunnel_set_hwaddr` (C is a TODO stub; skip).
- macOS/BSD utun path (Linux-only target per AGENTS.md; `#[cfg(target_os = "linux")]` gate the ioctl module, with a clear `compile_error!`/stub for non-Linux).

---

## Risk / Notes

- `libc` `in6_ifreq` / `in6_rtmsg` field layouts must match kernel `linux/if.h` / `linux/ipv6_route.h`. Use `#[repr(C)]` structs with explicit field order from the C source.
- `ipnet` is the chosen `Ipv6Net`; if the daemon later standardizes on a different net type, adjust the re-export in `error.rs`.
- Async read/write through `AsyncFd` requires the fd be in non-blocking mode (the C opens `O_NONBLOCK`); keep that flag.
