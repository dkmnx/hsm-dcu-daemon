//! IPv6 tunnel interface management.
//!
//! Reimplements `src/util/TunnelIPv6Interface.cpp` for the Linux data path:
//! address add/remove/list, route add/remove, and synchronous + asynchronous
//! packet read/write over the TUN fd.
//!
//! Netlink address/link-state subscription and the MLD listener from the C
//! source are event-loop concerns and live in the async daemon (phase 3A),
//! not in this transport crate.

use std::net::Ipv6Addr;
use std::os::unix::io::{AsRawFd, OwnedFd, RawFd};

use tokio::io::unix::AsyncFd;

use crate::device::{TunConfig, TunDevice};
use crate::error::{Ipv6Net, TunError};
use crate::ioctl;

/// A Linux TUN IPv6 interface: a [`TunDevice`] plus a netif-management socket
/// and an async handle for the event loop.
pub struct TunnelIPv6Interface {
    device: TunDevice,
    netif_fd: OwnedFd,
    async_fd: AsyncFd<OwnedFd>,
    mtu: u16,
}

impl TunnelIPv6Interface {
    /// Create and configure the TUN interface: open the device, open the
    /// netif-management socket, and apply the MTU.
    pub fn new(config: TunConfig) -> Result<Self, TunError> {
        if !config.is_valid() {
            return Err(TunError::InvalidConfig(format!(
                "mtu {} out of valid range 1200..=1280",
                config.mtu
            )));
        }
        let device = TunDevice::open(config.clone())?;
        let netif_fd = ioctl::open_netif_socket()?;
        device.set_mtu(config.mtu)?;

        // A second handle to the same kernel device for async I/O.
        let async_fd = AsyncFd::new(device.try_clone_fd()?)
            .map_err(|e| TunError::Open(std::io::Error::other(e)))?;

        Ok(Self {
            device,
            netif_fd,
            async_fd,
            mtu: config.mtu,
        })
    }

    /// The kernel-assigned interface name.
    pub fn name(&self) -> &str {
        self.device.name()
    }

    /// Clone this interface handle. Duplicates the existing kernel TUN fd
    /// (via `try_clone_fd`) so both the original and the clone refer to the
    /// same kernel device. Safe to use from a separate task than the original
    /// (e.g. one task reads the TUN while another writes).
    pub fn try_clone(&self) -> Result<Self, TunError> {
        let netif_fd = ioctl::open_netif_socket()?;
        let dup_fd = self.device.try_clone_fd()?;
        let async_fd =
            AsyncFd::new(dup_fd).map_err(|e| TunError::Open(std::io::Error::other(e)))?;
        let device = TunDevice::from_fd_and_name(self.device.try_clone_fd()?, self.name());
        Ok(Self {
            device,
            netif_fd,
            async_fd,
            mtu: self.mtu,
        })
    }

    /// The configured MTU.
    pub fn mtu(&self) -> u16 {
        self.mtu
    }

    /// Add an IPv6 address to the interface.
    pub fn add_address(&self, addr: Ipv6Addr, prefix_len: u8) -> Result<(), TunError> {
        ioctl::add_ipv6_address(&self.netif_fd, self.name(), addr, prefix_len)
    }

    /// Remove an IPv6 address from the interface.
    pub fn remove_address(&self, addr: Ipv6Addr, prefix_len: u8) -> Result<(), TunError> {
        ioctl::remove_ipv6_address(&self.netif_fd, self.name(), addr, prefix_len)
    }

    /// List all IPv6 addresses currently assigned to the interface.
    pub fn list_addresses(&self) -> Result<Vec<Ipv6Net>, TunError> {
        ioctl::list_ipv6_addresses(self.name())
    }

    /// Add an IPv6 route. `gateway` is accepted for API parity with the C
    /// `add_route` but is unused on the Linux `SIOCADDRT` path here.
    pub fn add_route(
        &self,
        dest: Ipv6Net,
        _gateway: Option<Ipv6Addr>,
        metric: u32,
    ) -> Result<(), TunError> {
        ioctl::add_ipv6_route(&self.netif_fd, self.name(), dest, metric)
    }

    /// Remove an IPv6 route.
    pub fn remove_route(&self, dest: Ipv6Net, metric: u32) -> Result<(), TunError> {
        ioctl::remove_ipv6_route(&self.netif_fd, self.name(), dest, metric)
    }

    /// Join an IPv6 multicast group on this interface (MLD).
    pub fn join_multicast_address(&self, addr: Ipv6Addr) -> Result<(), TunError> {
        ioctl::join_multicast_address(&self.netif_fd, self.name(), addr)
    }

    /// Leave an IPv6 multicast group on this interface (MLD).
    pub fn leave_multicast_address(&self, addr: Ipv6Addr) -> Result<(), TunError> {
        ioctl::leave_multicast_address(&self.netif_fd, self.name(), addr)
    }

    /// Returns `true` if the interface is administratively up.
    pub fn is_up(&self) -> Result<bool, TunError> {
        ioctl::interface_is_up(&self.netif_fd, self.name())
    }

    /// Bring the interface up or down.
    pub fn set_up(&self, up: bool) -> Result<(), TunError> {
        ioctl::set_interface_up(&self.netif_fd, self.name(), up)
    }

    /// Read a packet from the TUN device. Returns the number of bytes read.
    /// The fd is `O_NONBLOCK` — call this only when the device is known to
    /// be readable (e.g. after an event-loop readiness notification, or via
    /// [`Self::async_read_packet`] which gates on readiness).
    ///
    /// A 4-byte protocol-info header (all-zero first two bytes) is stripped,
    /// matching `TunnelIPv6Interface::read()`. With the default
    /// `IFF_NO_PI` flag this header is never present, so the strip is
    /// effectively a no-op — it is kept for binary compatibility with the C
    /// daemon in case `IFF_NO_PI` is ever cleared.
    pub fn read_packet(&self, buf: &mut [u8]) -> Result<usize, TunError> {
        let n = read_fd(self.device.as_raw_fd(), buf)?;
        if n >= 4 && buf[0] == 0 && buf[1] == 0 {
            buf.copy_within(4..n, 0);
            Ok(n - 4)
        } else {
            Ok(n)
        }
    }

    /// Write a packet to the TUN device (blocking).
    pub fn write_packet(&self, buf: &[u8]) -> Result<usize, TunError> {
        Ok(write_fd(self.device.as_raw_fd(), buf)?)
    }

    /// Read a packet asynchronously (for the tokio event loop).
    pub async fn async_read_packet(&self, buf: &mut [u8]) -> Result<usize, TunError> {
        loop {
            let mut guard = self.async_fd.readable().await.map_err(read_into_err)?;
            match guard.try_io(|inner| read_fd(inner.as_raw_fd(), buf)) {
                Ok(result) => {
                    let n = result.map_err(TunError::Open)?;
                    if n >= 4 && buf[0] == 0 && buf[1] == 0 {
                        buf.copy_within(4..n, 0);
                        return Ok(n - 4);
                    }
                    return Ok(n);
                }
                Err(_would_block) => continue,
            }
        }
    }

    /// Write a packet asynchronously (for the tokio event loop).
    pub async fn async_write_packet(&self, buf: &[u8]) -> Result<usize, TunError> {
        loop {
            let mut guard = self.async_fd.writable().await.map_err(write_into_err)?;
            match guard.try_io(|inner| write_fd(inner.as_raw_fd(), buf)) {
                Ok(result) => return result.map_err(TunError::Open),
                Err(_would_block) => continue,
            }
        }
    }
}

impl AsRawFd for TunnelIPv6Interface {
    fn as_raw_fd(&self) -> RawFd {
        self.device.as_raw_fd()
    }
}

fn read_fd(fd: RawFd, buf: &mut [u8]) -> std::io::Result<usize> {
    // SAFETY: fd is a valid TUN fd owned by self; buf is a valid slice of
    // length >= 0. read writes at most buf.len() bytes.
    let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };
    if n < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(n as usize)
}

fn write_fd(fd: RawFd, buf: &[u8]) -> std::io::Result<usize> {
    // SAFETY: fd is a valid TUN fd owned by self; buf is a valid slice.
    let n = unsafe { libc::write(fd, buf.as_ptr().cast::<libc::c_void>(), buf.len()) };
    if n < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(n as usize)
}

fn read_into_err(e: std::io::Error) -> TunError {
    TunError::Open(e)
}

fn write_into_err(e: std::io::Error) -> TunError {
    TunError::Open(e)
}
