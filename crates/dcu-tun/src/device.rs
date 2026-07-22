//! TUN device allocation (`/dev/net/tun`).
//!
//! Reimplements `src/util/tunnel.c`: open the tunnel char device, attach an
//! interface via `TUNSETIFF`, and expose MTU / up-down control. The kernel
//! assigns the real interface name, which we read back.

use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, OwnedFd, RawFd};

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
    /// Whether this configuration is usable. The MTU bounds are constrained
    /// to the valid range (valid: 1200, 1280; invalid: 64, 65535).
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
        let name = set_iff(fd.as_raw_fd(), &config)?;

        Ok(Self { fd, name })
    }

    /// The (kernel-assigned) interface name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set the interface MTU.
    pub fn set_mtu(&self, mtu: u16) -> Result<(), TunError> {
        crate::ioctl::set_interface_mtu(&self.fd, &self.name, mtu)
    }

    /// Bring the interface up (`true`) or down (`false`).
    pub fn set_up(&self, up: bool) -> Result<(), TunError> {
        crate::ioctl::set_interface_up(&self.fd, &self.name, up)
    }

    /// Returns `true` if the interface is administratively up.
    pub fn is_up(&self) -> Result<bool, TunError> {
        crate::ioctl::interface_is_up(&self.fd, &self.name)
    }

    /// Duplicate the underlying fd.
    pub fn try_clone_fd(&self) -> Result<OwnedFd, TunError> {
        self.fd.try_clone().map_err(TunError::Open)
    }

    /// Consume the device, closing the fd.
    pub fn close(self) {}

    /// Build a `TunDevice` from an already-open fd and the interface name.
    pub fn from_fd_and_name(fd: OwnedFd, name: &str) -> Self {
        assert!(name.len() < libc::IFNAMSIZ, "interface name too long");
        TunDevice {
            fd,
            name: name.to_string(),
        }
    }
}

impl AsRawFd for TunDevice {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

/// Open `/dev/net/tun` read/write non-blocking.
#[cfg(target_os = "linux")]
fn open_tun_device() -> Result<OwnedFd, TunError> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/net/tun")
        .map_err(TunError::Open)?;
    Ok(OwnedFd::from(file))
}

/// Attach the TUN interface via `TUNSETIFF` and read back the assigned name.
#[cfg(target_os = "linux")]
fn set_iff(fd: RawFd, config: &TunConfig) -> Result<String, TunError> {
    let mut ifr = crate::ffi::zeroed_ifreq();

    let name_bytes = config.name.as_bytes();
    for (d, s) in ifr.ifr_name.iter_mut().zip(name_bytes.iter()) {
        *d = *s as libc::c_char;
    }

    let mut flags: libc::c_short = libc::IFF_TUN as libc::c_short;
    if config.no_packet_info {
        flags |= libc::IFF_NO_PI as libc::c_short;
    }
    crate::ffi::set_ifru_flags(&mut ifr, flags as i32);

    crate::ffi::tunsetiff(fd, &mut ifr)
        .map_err(|e| TunError::Ioctl {
            op: "TUNSETIFF",
            source: std::io::Error::from_raw_os_error(e as i32),
        })?;

    let name = crate::ffi::read_ifreq_name(&ifr);
    if name.is_empty() {
        return Err(TunError::InvalidConfig("interface name empty after TUNSETIFF".into()));
    }

    Ok(name)
}

#[cfg(not(target_os = "linux"))]
compile_error!("dcu-tun only supports Linux (TUNSETIFF / netlink ioctls)");
