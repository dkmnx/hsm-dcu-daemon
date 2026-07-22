//! Low-level netlink/ioctl wrappers for interface management.
//!
//! Reimplements `src/util/netif-mgmt.c`. All operations use a single
//! netif-management fd. Address and route manipulation use the Linux
//! `struct in6_ifreq` / `struct in6_rtmsg` layouts from `linux/if.h` and
//! `linux/ipv6_route.h`.
//!
//! Every kernel ABI call lives in `ffi.rs`. This module exposes only safe
//! Rust wrappers.

use std::net::Ipv6Addr;
use std::os::unix::io::{AsRawFd, OwnedFd};

use ipnet::Ipv6Net;
use nix::ifaddrs::InterfaceAddress;

use crate::error::TunError;
use crate::ffi::{In6Ifreq, Ipv6Mreq};

/// Open the netif-management socket used for `SIOC*IF*` ioctls.
#[cfg(target_os = "linux")]
pub fn open_netif_socket() -> Result<OwnedFd, TunError> {
    crate::ffi::socket_inet6_dgram()
        .map(OwnedFd::from)
        .map_err(|e| TunError::Open(e.into()))
}

fn to_in6_addr(addr: Ipv6Addr) -> libc::in6_addr {
    libc::in6_addr {
        s6_addr: addr.octets(),
    }
}

fn name_ifreq(name: &str) -> libc::ifreq {
    let mut ifr = crate::ffi::zeroed_ifreq();
    for (d, s) in ifr.ifr_name.iter_mut().zip(name.as_bytes()) {
        *d = *s as libc::c_char;
    }
    ifr
}

/// Get the current interface flags (`SIOCGIFFLAGS`).
#[cfg(target_os = "linux")]
pub fn get_interface_flags(fd: &impl AsRawFd, name: &str) -> Result<i32, TunError> {
    let ifr = name_ifreq(name);
    crate::ffi::siocgifflags(fd.as_raw_fd(), &ifr)
        .map_err(|e| TunError::Ioctl { op: "SIOCGIFFLAGS", source: e.into() })?;
    Ok(crate::ffi::ifru_flags(&ifr))
}

/// Set the interface flags (`SIOCSIFFLAGS`).
#[cfg(target_os = "linux")]
pub fn set_interface_flags(fd: &impl AsRawFd, name: &str, flags: i32) -> Result<(), TunError> {
    let mut ifr = name_ifreq(name);
    crate::ffi::set_ifru_flags(&mut ifr, flags);
    crate::ffi::siocsifflags(fd.as_raw_fd(), &ifr)
        .map_err(|e| TunError::Ioctl { op: "SIOCSIFFLAGS", source: e.into() })
}

/// Returns `true` if the interface is administratively up.
pub fn interface_is_up(fd: &impl AsRawFd, name: &str) -> Result<bool, TunError> {
    Ok((get_interface_flags(fd, name)? & libc::IFF_UP) == libc::IFF_UP)
}

/// Bring the interface up (`true`) or down (`false`).
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
    crate::ffi::set_ifru_mtu(&mut ifr, mtu as libc::c_int);
    crate::ffi::siocsifmtu(fd.as_raw_fd(), &ifr)
        .map_err(|e| TunError::Ioctl { op: "SIOCSIFMTU", source: e.into() })
}

/// Resolve the kernel interface index (`SIOGIFINDEX`).
#[cfg(target_os = "linux")]
fn interface_index(fd: &impl AsRawFd, name: &str) -> Result<u32, TunError> {
    let ifr = name_ifreq(name);
    crate::ffi::siogifindex(fd.as_raw_fd(), &ifr)
        .map_err(|e| TunError::Ioctl { op: "SIOGIFINDEX", source: e.into() })?;
    Ok(crate::ffi::ifru_ifindex(&ifr))
}

/// Add an IPv6 address to the interface (`SIOCSIFADDR` with `in6_ifreq`).
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
    let _ = crate::ffi::siocdifaddr_in6(fd.as_raw_fd(), &req);
    crate::ffi::siocsifaddr_in6(fd.as_raw_fd(), &req)
        .map_err(|e| TunError::Ioctl { op: "SIOCSIFADDR", source: e.into() })
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
    crate::ffi::siocdifaddr_in6(fd.as_raw_fd(), &req)
        .map_err(|e| TunError::Ioctl { op: "SIOCDIFADDR", source: e.into() })
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
    let mut rt = crate::ffi::zeroed_in6_rtmsg();
    rt.rtmsg_dst = to_in6_addr(dest.addr());
    rt.rtmsg_dst_len = dest.prefix_len() as u16;
    let mut flags = libc::RTF_UP as u32;
    if dest.prefix_len() == 128 {
        flags |= libc::RTF_HOST as u32;
    }
    rt.rtmsg_flags = flags;
    rt.rtmsg_metric = metric;
    rt.rtmsg_ifindex = ifindex as i32;

    crate::ffi::siocaddrt(fd.as_raw_fd(), &rt)
        .map_err(|e| TunError::Ioctl { op: "SIOCADDRT", source: e.into() })
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
    let mut rt = crate::ffi::zeroed_in6_rtmsg();
    rt.rtmsg_dst = to_in6_addr(dest.addr());
    rt.rtmsg_dst_len = dest.prefix_len() as u16;
    let mut flags = libc::RTF_UP as u32;
    if dest.prefix_len() == 128 {
        flags |= libc::RTF_HOST as u32;
    }
    rt.rtmsg_flags = flags;
    rt.rtmsg_metric = metric;
    rt.rtmsg_ifindex = ifindex as i32;

    crate::ffi::siocdelrt(fd.as_raw_fd(), &rt)
        .map_err(|e| TunError::Ioctl { op: "SIOCDELRT", source: e.into() })
}

/// Enumerate IPv6 addresses currently assigned to `name`.
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

/// Join an IPv6 multicast group on the interface (MLD `IPV6_JOIN_GROUP`).
#[cfg(target_os = "linux")]
pub fn join_multicast_address(
    fd: &impl AsRawFd,
    name: &str,
    addr: Ipv6Addr,
) -> Result<(), TunError> {
    let ifindex = interface_index(fd, name)?;
    let mreq = Ipv6Mreq {
        mreq6_addr: to_in6_addr(addr),
        mreq6_ifindex: ifindex as i32,
    };
    crate::ffi::setsockopt_ipv6_join_group(fd.as_raw_fd(), &mreq)
        .map_err(|e| TunError::Ioctl { op: "IPV6_JOIN_GROUP", source: e.into() })
}

/// Leave an IPv6 multicast group on the interface (`IPV6_LEAVE_GROUP`).
#[cfg(target_os = "linux")]
pub fn leave_multicast_address(
    fd: &impl AsRawFd,
    name: &str,
    addr: Ipv6Addr,
) -> Result<(), TunError> {
    let ifindex = interface_index(fd, name)?;
    let mreq = Ipv6Mreq {
        mreq6_addr: to_in6_addr(addr),
        mreq6_ifindex: ifindex as i32,
    };
    crate::ffi::setsockopt_ipv6_leave_group(fd.as_raw_fd(), &mreq)
        .map_err(|e| TunError::Ioctl { op: "IPV6_LEAVE_GROUP", source: e.into() })
}

/// Compute the prefix length from a netmask address.
pub(crate) fn prefix_len(mask: Ipv6Addr) -> u8 {
    mask.octets().iter().map(|b| b.count_ones() as u8).sum()
}
