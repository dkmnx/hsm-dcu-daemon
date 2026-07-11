//! Low-level netlink/ioctl wrappers for interface management.
//!
//! Reimplements `src/util/netif-mgmt.c`. All operations use a single
//! `socket(AF_INET6, SOCK_DGRAM, 0)` "netif-management" fd, matching the C
//! `netif_mgmt_open()`. Address and route manipulation use the Linux
//! `struct in6_ifreq` / `struct in6_rtmsg` layouts from `linux/if.h` and
//! `linux/ipv6_route.h`.
//!
//! Every `unsafe` ioctl/union-field access is confined to this module (the
//! ioctl-exempt crate per AGENTS.md).

use std::net::Ipv6Addr;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd};

use ipnet::Ipv6Net;
use nix::ifaddrs::InterfaceAddress;

use crate::error::TunError;

/// Open the netif-management socket used for `SIOC*IF*` ioctls.
#[cfg(target_os = "linux")]
pub fn open_netif_socket() -> Result<OwnedFd, TunError> {
    // SAFETY: a datagram socket over AF_INET6 is always valid to create.
    let fd = unsafe { libc::socket(libc::AF_INET6, libc::SOCK_DGRAM, 0) };
    if fd < 0 {
        return Err(TunError::Open(std::io::Error::last_os_error()));
    }
    // SAFETY: fd is a valid, freshly created descriptor owned by us.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Linux `struct in6_ifreq` (from `<linux/if.h>`), used by `SIOCSIFADDR` /
/// `SIOCDIFADDR` for IPv6 address add/remove.
#[repr(C)]
#[derive(Clone, Copy)]
struct In6Ifreq {
    ifr6_addr: libc::in6_addr,
    ifr6_prefixlen: u32,
    ifr6_ifindex: u32,
}

/// Linux `struct in6_rtmsg` (from `<linux/ipv6_route.h>`), used by
/// `SIOCADDRT` / `SIOCDELRT` for IPv6 route add/remove.
#[repr(C)]
#[derive(Clone, Copy)]
struct In6Rtmsg {
    rtmsg_dst: libc::in6_addr,
    rtmsg_src: libc::in6_addr,
    rtmsg_gateway: libc::in6_addr,
    rtmsg_type: u32,
    rtmsg_dst_len: u16,
    rtmsg_src_len: u16,
    rtmsg_metric: u32,
    rtmsg_info: u32,
    rtmsg_flags: u32,
    rtmsg_ifindex: i32,
}

/// Copy an [`Ipv6Addr`] into a `libc::in6_addr`.
fn to_in6_addr(addr: Ipv6Addr) -> libc::in6_addr {
    let octets = addr.octets();
    libc::in6_addr {
        // in6_addr is a 16-byte union; s6_addr is portable.
        s6_addr: octets,
    }
}

/// Build a `libc::ifreq` carrying only the interface name.
fn name_ifreq(name: &str) -> libc::ifreq {
    let mut ifr: libc::ifreq = unsafe { std::mem::zeroed() };
    for (d, s) in ifr.ifr_name.iter_mut().zip(name.as_bytes()) {
        *d = *s as libc::c_char;
    }
    ifr
}

/// Get the current interface flags (`SIOCGIFFLAGS`).
#[cfg(target_os = "linux")]
pub fn get_interface_flags(fd: &impl AsRawFd, name: &str) -> Result<i32, TunError> {
    let ifr = name_ifreq(name);
    // SAFETY: SIOCGIFFLAGS reads flags into ifr.ifr_ifru.ifru_flags.
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCGIFFLAGS, &ifr) };
    if ret < 0 {
        return Err(TunError::Ioctl {
            op: "SIOCGIFFLAGS",
            source: std::io::Error::last_os_error(),
        });
    }
    // SAFETY: reading the union field we just populated.
    Ok(unsafe { ifr.ifr_ifru.ifru_flags as i32 })
}

/// Set the interface flags (`SIOCSIFFLAGS`).
#[cfg(target_os = "linux")]
pub fn set_interface_flags(fd: &impl AsRawFd, name: &str, flags: i32) -> Result<(), TunError> {
    let mut ifr = name_ifreq(name);
    ifr.ifr_ifru.ifru_flags = flags as libc::c_short;
    // SAFETY: SIOCSIFFLAGS writes flags from ifr.ifr_ifru.ifru_flags.
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCSIFFLAGS, &ifr) };
    if ret < 0 {
        return Err(TunError::Ioctl {
            op: "SIOCSIFFLAGS",
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(())
}

/// Returns `true` if the interface is administratively up.
pub fn interface_is_up(fd: &impl AsRawFd, name: &str) -> Result<bool, TunError> {
    Ok(
        (get_interface_flags(fd, name)? & libc::IFF_UP) == libc::IFF_UP,
    )
}

/// Bring the interface up (`true`) or down (`false`). Down also clears
/// `IFF_RUNNING`, matching `netif_mgmt_set_up(fd, name, false)`.
#[cfg(target_os = "linux")]
pub fn set_interface_up(fd: &impl AsRawFd, name: &str, up: bool) -> Result<(), TunError> {
    let flags = get_interface_flags(fd, name)?;
    let new_flags = if up {
        flags | libc::IFF_UP
    } else {
        flags & !(libc::IFF_UP | libc::IFF_RUNNING)
    };
    set_interface_flags(fd, name, new_flags)
}

/// Set the interface MTU (`SIOCSIFMTU`).
#[cfg(target_os = "linux")]
pub fn set_interface_mtu(fd: &impl AsRawFd, name: &str, mtu: u16) -> Result<(), TunError> {
    let mut ifr = name_ifreq(name);
    ifr.ifr_ifru.ifru_mtu = mtu as libc::c_int;
    // SAFETY: SIOCSIFMTU reads ifr.ifr_ifru.ifru_mtu.
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCSIFMTU, &ifr) };
    if ret < 0 {
        return Err(TunError::Ioctl {
            op: "SIOCSIFMTU",
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(())
}

/// Resolve the kernel interface index (`SIOGIFINDEX`).
#[cfg(target_os = "linux")]
fn interface_index(fd: &impl AsRawFd, name: &str) -> Result<u32, TunError> {
    let ifr = name_ifreq(name);
    // SAFETY: SIOGIFINDEX writes ifr.ifr_ifru.ifru_ifindex.
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOGIFINDEX, &ifr) };
    if ret < 0 {
        return Err(TunError::Ioctl {
            op: "SIOGIFINDEX",
            source: std::io::Error::last_os_error(),
        });
    }
    // SAFETY: reading the union field populated by the ioctl.
    Ok(unsafe { ifr.ifr_ifru.ifru_ifindex as u32 })
}

/// Add an IPv6 address to the interface (`SIOCSIFADDR` with `in6_ifreq`).
///
/// Mirrors `netif_mgmt_add_ipv6_address`: Linux requires removing the
/// address first, then adding it.
#[cfg(target_os = "linux")]
pub fn add_ipv6_address(
    fd: &impl AsRawFd,
    name: &str,
    addr: Ipv6Addr,
    prefix_len: u8,
) -> Result<(), TunError> {
    if addr.is_unspecified() {
        return Err(TunError::Unspecified(addr));
    }
    let ifindex = interface_index(fd, name)?;
    let req = In6Ifreq {
        ifr6_addr: to_in6_addr(addr),
        ifr6_prefixlen: prefix_len as u32,
        ifr6_ifindex: ifindex,
    };
    // Remove first (idempotent on Linux), then add.
    // SAFETY: SIOCDIFADDR removes the address described by req.
    unsafe {
        libc::ioctl(fd.as_raw_fd(), libc::SIOCDIFADDR, &req);
    }
    // SAFETY: SIOCSIFADDR adds the address described by req.
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCSIFADDR, &req) };
    if ret < 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EALREADY) {
            return Ok(());
        }
        return Err(TunError::Ioctl {
            op: "SIOCSIFADDR",
            source: err,
        });
    }
    Ok(())
}

/// Remove an IPv6 address from the interface (`SIOCDIFADDR`).
#[cfg(target_os = "linux")]
pub fn remove_ipv6_address(
    fd: &impl AsRawFd,
    name: &str,
    addr: Ipv6Addr,
    prefix_len: u8,
) -> Result<(), TunError> {
    if addr.is_unspecified() {
        return Err(TunError::Unspecified(addr));
    }
    let ifindex = interface_index(fd, name)?;
    let req = In6Ifreq {
        ifr6_addr: to_in6_addr(addr),
        ifr6_prefixlen: prefix_len as u32,
        ifr6_ifindex: ifindex,
    };
    // SAFETY: SIOCDIFADDR removes the address described by req.
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCDIFADDR, &req) };
    if ret < 0 {
        return Err(TunError::Ioctl {
            op: "SIOCDIFADDR",
            source: std::io::Error::last_os_error(),
        });
    }
    Ok(())
}

/// Add an IPv6 route (`SIOCADDRT` with `in6_rtmsg`).
#[cfg(target_os = "linux")]
pub fn add_ipv6_route(
    fd: &impl AsRawFd,
    name: &str,
    dest: Ipv6Net,
    metric: u32,
) -> Result<(), TunError> {
    let ifindex = interface_index(fd, name)?;
    let mut rt: In6Rtmsg = unsafe { std::mem::zeroed() };
    rt.rtmsg_dst = to_in6_addr(dest.addr());
    rt.rtmsg_dst_len = dest.prefix_len() as u16;
    let mut flags = libc::RTF_UP as u32;
    if dest.prefix_len() == 128 {
        flags |= libc::RTF_HOST as u32;
    }
    rt.rtmsg_flags = flags;
    rt.rtmsg_metric = metric;
    rt.rtmsg_ifindex = ifindex as i32;

    // SAFETY: SIOCADDRT installs the route described by rt.
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCADDRT, &rt) };
    if ret < 0 {
        let err = std::io::Error::last_os_error();
        if matches!(err.raw_os_error(), Some(libc::EALREADY) | Some(libc::EEXIST)) {
            return Ok(());
        }
        return Err(TunError::Ioctl {
            op: "SIOCADDRT",
            source: err,
        });
    }
    Ok(())
}

/// Remove an IPv6 route (`SIOCDELRT`).
#[cfg(target_os = "linux")]
pub fn remove_ipv6_route(
    fd: &impl AsRawFd,
    name: &str,
    dest: Ipv6Net,
    metric: u32,
) -> Result<(), TunError> {
    let ifindex = interface_index(fd, name)?;
    let mut rt: In6Rtmsg = unsafe { std::mem::zeroed() };
    rt.rtmsg_dst = to_in6_addr(dest.addr());
    rt.rtmsg_dst_len = dest.prefix_len() as u16;
    let mut flags = libc::RTF_UP as u32;
    if dest.prefix_len() == 128 {
        flags |= libc::RTF_HOST as u32;
    }
    rt.rtmsg_flags = flags;
    rt.rtmsg_metric = metric;
    rt.rtmsg_ifindex = ifindex as i32;

    // SAFETY: SIOCDELRT removes the route described by rt.
    let ret = unsafe { libc::ioctl(fd.as_raw_fd(), libc::SIOCDELRT, &rt) };
    if ret < 0 {
        let err = std::io::Error::last_os_error();
        if matches!(err.raw_os_error(), Some(libc::EALREADY) | Some(libc::EEXIST)) {
            return Ok(());
        }
        return Err(TunError::Ioctl {
            op: "SIOCDELRT",
            source: err,
        });
    }
    Ok(())
}

/// Enumerate IPv6 addresses currently assigned to `name`.
///
/// Uses `getifaddrs` (safe) rather than netlink; this is the polling
/// equivalent of the C netlink `RTM_NEWADDR`/`RTM_DELADDR` listener, which
/// lives in the async daemon (phase 3A), not here.
pub fn list_ipv6_addresses(name: &str) -> Result<Vec<Ipv6Net>, TunError> {
    let mut out = Vec::new();
    for ifaddr in nix::ifaddrs::getifaddrs()? {
        if ifaddr.interface_name != name {
            continue;
        }
        if let Some(net) = ipv6_net_of(&ifaddr) {
            out.push(net);
        }
    }
    Ok(out)
}

/// Extract an `Ipv6Net` from an `InterfaceAddress` (address + netmask).
fn ipv6_net_of(ifaddr: &InterfaceAddress) -> Option<Ipv6Net> {
    let addr = match ifaddr.address {
        Some(ref sa) => match sa.as_sockaddr_in6() {
            Some(in6) => in6.ip(),
            None => return None,
        },
        None => return None,
    };
    let prefix = match ifaddr.netmask {
        Some(ref sa) => match sa.as_sockaddr_in6() {
            Some(in6) => prefix_len(in6.ip()),
            None => 128,
        },
        None => 128,
    };
    Ipv6Net::new(addr, prefix).ok()
}

/// Compute the prefix length from a netmask address.
pub(crate) fn prefix_len(mask: Ipv6Addr) -> u8 {
    mask.octets().iter().map(|b| b.count_ones() as u8).sum()
}
