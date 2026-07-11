//! TUN device allocation (`/dev/net/tun`).
//!
//! Reimplements `src/util/tunnel.c`: open the tunnel char device, attach an
//! interface via `TUNSETIFF`, and expose MTU / up-down control. The kernel
//! assigns the real interface name, which we read back.

use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd, RawFd};

use crate::error::TunError;

/// Default TUN interface name, matching `TUNNEL_DEFAULT_INTERFACE_NAME`
/// in `src/util/tunnel.h`.
pub const DEFAULT_INTERFACE_NAME: &str = "wfan0";

/// Default MTU, matching the `TunnelIPv6Interface` constructor default (1280).
pub const DEFAULT_MTU: u16 = 1280;

/// Configuration for opening a TUN device.
#[derive(Debug, Clone)]
pub struct TunConfig {
    /// Interface name (e.g. `"wfan0"`). The kernel may rename it if the
    /// requested name is already taken; the assigned name is read back via
    /// [`TunDevice::name`].
    pub name: String,
    /// Interface MTU. Validated by [`TunConfig::is_valid`].
    pub mtu: u16,
    /// When `true`, set `IFF_NO_PI` so the kernel does not prepend a 4-byte
    /// protocol-info header to each packet.
    pub no_packet_info: bool,
}

impl Default for TunConfig {
    fn default() -> Self {
        Self {
            name: DEFAULT_INTERFACE_NAME.to_string(),
            mtu: DEFAULT_MTU,
            no_packet_info: true,
        }
    }
}

impl TunConfig {
    /// Whether this configuration is usable. The MTU bounds are taken from
    /// the phase-1C spec Test 4 (valid: 1200, 1280; invalid: 64, 65535).
    pub fn is_valid(&self) -> bool {
        (1200..=1280).contains(&self.mtu)
    }
}

/// An open TUN device. Owns its file descriptor, so closing is automatic on
/// drop.
pub struct TunDevice {
    fd: OwnedFd,
    name: String,
}

impl TunDevice {
    /// Open the TUN device described by `config`.
    ///
    /// Steps (matching `tunnel_open` in `tunnel.c`):
    /// 1. Open `/dev/net/tun` read/write, non-blocking.
    /// 2. `ioctl(TUNSETIFF, ifr{IFF_TUN | IFF_NO_PI})`.
    /// 3. Read back the assigned interface name.
    pub fn open(config: TunConfig) -> Result<Self, TunError> {
        if config.name.len() >= libc::IFNAMSIZ {
            return Err(TunError::NameTooLong);
        }
        if !config.is_valid() {
            return Err(TunError::InvalidConfig(format!(
                "mtu {} out of valid range 1200..=1280",
                config.mtu
            )));
        }

        let fd = open_tun_device()?;
        let name = set_iff(&fd, &config)?;

        Ok(Self { fd, name })
    }

    /// The (kernel-assigned) interface name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set the interface MTU, using a netif-management socket as the C does
    /// (`netif_mgmt_set_mtu`).
    pub fn set_mtu(&self, mtu: u16) -> Result<(), TunError> {
        let netif_fd = crate::ioctl::open_netif_socket()?;
        crate::ioctl::set_interface_mtu(&netif_fd, &self.name, mtu)
    }

    /// Bring the interface up (`true`) or down (`false`). Bringing it down
    /// also clears `IFF_RUNNING`, matching `netif_mgmt_set_up(fd, name, false)`.
    pub fn set_up(&self, up: bool) -> Result<(), TunError> {
        let netif_fd = crate::ioctl::open_netif_socket()?;
        crate::ioctl::set_interface_up(&netif_fd, &self.name, up)
    }

    /// Returns `true` if the interface is administratively up.
    pub fn is_up(&self) -> Result<bool, TunError> {
        let netif_fd = crate::ioctl::open_netif_socket()?;
        crate::ioctl::interface_is_up(&netif_fd, &self.name)
    }

    /// Duplicate the underlying fd (used by the async read/write bridge in
    /// `TunnelIPv6Interface`, which needs a second handle to the same device).
    pub fn try_clone_fd(&self) -> Result<OwnedFd, TunError> {
        self.fd.try_clone().map_err(TunError::Open)
    }

    /// Consume the device, closing the fd. Equivalent to dropping it.
    pub fn close(self) {
        // OwnedFd closes on drop; nothing else to do.
    }
}

impl AsRawFd for TunDevice {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

/// Open `/dev/net/tun` (`O_RDWR | O_NONBLOCK`).
#[cfg(target_os = "linux")]
fn open_tun_device() -> Result<OwnedFd, TunError> {
    // SAFETY: open with a static C-string path; the returned fd is owned by
    // OwnedFd.
    let fd = unsafe { libc::open(c"/dev/net/tun".as_ptr(), libc::O_RDWR | libc::O_NONBLOCK) };
    if fd < 0 {
        return Err(TunError::Open(std::io::Error::last_os_error()));
    }
    // SAFETY: fd is a valid, freshly opened descriptor owned by us.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Attach the TUN interface via `TUNSETIFF` and read back the assigned name.
#[cfg(target_os = "linux")]
fn set_iff(fd: &OwnedFd, config: &TunConfig) -> Result<String, TunError> {
    let mut ifr: libc::ifreq = unsafe { std::mem::zeroed() };

    let name_bytes = config.name.as_bytes();
    // ifr_name is IFNAMSIZ (16) bytes; already bounds-checked by the caller.
    for (d, s) in ifr.ifr_name.iter_mut().zip(name_bytes.iter()) {
        *d = *s as libc::c_char;
    }

    let mut flags: libc::c_short = libc::IFF_TUN as libc::c_short;
    if config.no_packet_info {
        flags |= libc::IFF_NO_PI as libc::c_short;
    }
    ifr.ifr_ifru.ifru_flags = flags;

    // SAFETY: TUNSETIFF writes the assigned name back into ifr.ifr_name.
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), libc::TUNSETIFF, &ifr) };
    if ret < 0 {
        return Err(TunError::Ioctl {
            op: "TUNSETIFF",
            source: std::io::Error::last_os_error(),
        });
    }

    let end = ifr
        .ifr_name
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(ifr.ifr_name.len());
    let name_bytes: Vec<u8> = ifr.ifr_name[..end].iter().map(|&b| b as u8).collect();
    let name = String::from_utf8(name_bytes)
        .map_err(|_| TunError::InvalidConfig("interface name not UTF-8".into()))?;

    Ok(name)
}

#[cfg(not(target_os = "linux"))]
compile_error!("dcu-tun only supports Linux (TUNSETIFF / netlink ioctls)");
