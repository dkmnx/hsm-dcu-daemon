//! Raw-kernel `unsafe` shims for `dcu-tun`.
//!
//! All `ioctl`, `socket`, `setsockopt`, and C-struct manipulation against
//! Linux kernel ABI is isolated here. The rest of the crate sees only safe
//! wrappers.

#![allow(unsafe_code)]

use std::os::fd::FromRawFd;
use std::os::unix::io::RawFd;

use nix::errno::Errno;

pub use libc::{self, AF_INET6, IFF_NO_PI, IFF_TUN, O_NONBLOCK, O_RDWR};

/// Linux `struct in6_ifreq` (from `<linux/if.h>`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct In6Ifreq {
    pub ifr6_addr: libc::in6_addr,
    pub ifr6_prefixlen: u32,
    pub ifr6_ifindex: u32,
}

/// Linux `struct in6_rtmsg` (from `<linux/ipv6_route.h>`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct In6Rtmsg {
    pub rtmsg_dst: libc::in6_addr,
    pub rtmsg_src: libc::in6_addr,
    pub rtmsg_gateway: libc::in6_addr,
    pub rtmsg_type: u32,
    pub rtmsg_dst_len: u16,
    pub rtmsg_src_len: u16,
    pub rtmsg_metric: u32,
    pub rtmsg_info: u32,
    pub rtmsg_flags: u32,
    pub rtmsg_ifindex: i32,
}

/// Linux `struct ipv6_mreq` (from `<netinet/in.h>`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Ipv6Mreq {
    pub mreq6_addr: libc::in6_addr,
    pub mreq6_ifindex: i32,
}

// ---------------------------------------------------------------------------
// C-struct helpers (union access + zeroed init)
// ---------------------------------------------------------------------------

/// Create a zeroed `libc::ifreq`.
pub fn zeroed_ifreq() -> libc::ifreq {
    unsafe { std::mem::zeroed() }
}

/// Create a zeroed `In6Rtmsg`.
pub fn zeroed_in6_rtmsg() -> In6Rtmsg {
    unsafe { std::mem::zeroed() }
}

/// Read `ifr_ifru.ifru_flags` from a `libc::ifreq`.
pub fn ifru_flags(ifr: &libc::ifreq) -> i32 {
    unsafe { ifr.ifr_ifru.ifru_flags as i32 }
}

/// Write `ifr_ifru.ifru_flags` on a `libc::ifreq`.
pub fn set_ifru_flags(ifr: &mut libc::ifreq, flags: i32) {
    ifr.ifr_ifru.ifru_flags = flags as libc::c_short;
}

/// Write `ifr_ifru.ifru_mtu` on a `libc::ifreq`.
pub fn set_ifru_mtu(ifr: &mut libc::ifreq, mtu: libc::c_int) {
    ifr.ifr_ifru.ifru_mtu = mtu;
}

/// Read `ifr_ifru.ifru_ifindex` from a `libc::ifreq`.
pub fn ifru_ifindex(ifr: &libc::ifreq) -> u32 {
    unsafe { ifr.ifr_ifru.ifru_ifindex as u32 }
}

/// Read the interface name back from `ifr.ifr_name` after TUNSETIFF.
pub fn read_ifreq_name(ifr: &libc::ifreq) -> String {
    let end = ifr
        .ifr_name
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(ifr.ifr_name.len());
    let bytes: Vec<u8> = ifr.ifr_name[..end].iter().map(|&b| b as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

// ---------------------------------------------------------------------------
// ioctl wrappers
// ---------------------------------------------------------------------------

/// `ioctl(fd, TUNSETIFF, &mut ifr)` — attach TUN interface, read back name.
pub fn tunsetiff(fd: RawFd, ifr: &mut libc::ifreq) -> Result<(), Errno> {
    let ret = unsafe { libc::ioctl(fd, libc::TUNSETIFF, ifr) };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(())
}

/// `ioctl(fd, SIOCGIFFLAGS, &ifr)` — read interface flags.
pub fn siocgifflags(fd: RawFd, ifr: &libc::ifreq) -> Result<(), Errno> {
    let ret = unsafe { libc::ioctl(fd, libc::SIOCGIFFLAGS, ifr) };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(())
}

/// `ioctl(fd, SIOCSIFFLAGS, &ifr)` — write interface flags.
pub fn siocsifflags(fd: RawFd, ifr: &libc::ifreq) -> Result<(), Errno> {
    let ret = unsafe { libc::ioctl(fd, libc::SIOCSIFFLAGS, ifr) };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(())
}

/// `ioctl(fd, SIOCSIFMTU, &ifr)` — set interface MTU.
pub fn siocsifmtu(fd: RawFd, ifr: &libc::ifreq) -> Result<(), Errno> {
    let ret = unsafe { libc::ioctl(fd, libc::SIOCSIFMTU, ifr) };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(())
}

/// `ioctl(fd, SIOGIFINDEX, &ifr)` — read interface index.
pub fn siogifindex(fd: RawFd, ifr: &libc::ifreq) -> Result<(), Errno> {
    let ret = unsafe { libc::ioctl(fd, libc::SIOGIFINDEX, ifr) };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(())
}

/// `ioctl(fd, SIOCDIFADDR, &req)` — delete IPv6 address (in6_ifreq).
pub fn siocdifaddr_in6(fd: RawFd, req: &In6Ifreq) -> Result<(), Errno> {
    let ret = unsafe { libc::ioctl(fd, libc::SIOCDIFADDR, req) };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(())
}

/// `ioctl(fd, SIOCSIFADDR, &req)` — add IPv6 address (in6_ifreq).
pub fn siocsifaddr_in6(fd: RawFd, req: &In6Ifreq) -> Result<(), Errno> {
    let ret = unsafe { libc::ioctl(fd, libc::SIOCSIFADDR, req) };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(())
}

/// `ioctl(fd, SIOCADDRT, &rt)` — add IPv6 route.
pub fn siocaddrt(fd: RawFd, rt: &In6Rtmsg) -> Result<(), Errno> {
    let ret = unsafe { libc::ioctl(fd, libc::SIOCADDRT, rt) };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(())
}

/// `ioctl(fd, SIOCDELRT, &rt)` — delete IPv6 route.
pub fn siocdelrt(fd: RawFd, rt: &In6Rtmsg) -> Result<(), Errno> {
    let ret = unsafe { libc::ioctl(fd, libc::SIOCDELRT, rt) };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// socket / setsockopt wrappers
// ---------------------------------------------------------------------------

/// `socket(AF_INET6, SOCK_DGRAM, 0)` — returns an owned fd.
pub fn socket_inet6_dgram() -> Result<std::fs::File, Errno> {
    let fd = unsafe { libc::socket(AF_INET6, libc::SOCK_DGRAM, 0) };
    if fd < 0 {
        return Err(Errno::last());
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

/// `setsockopt(fd, IPPROTO_IPV6, IPV6_JOIN_GROUP, &mreq, sizeof(mreq))`
pub fn setsockopt_ipv6_join_group(fd: RawFd, mreq: &Ipv6Mreq) -> Result<(), Errno> {
    setsockopt_inner(fd, libc::IPPROTO_IPV6, 20, mreq)
}

/// `setsockopt(fd, IPPROTO_IPV6, IPV6_LEAVE_GROUP, &mreq, sizeof(mreq))`
pub fn setsockopt_ipv6_leave_group(fd: RawFd, mreq: &Ipv6Mreq) -> Result<(), Errno> {
    setsockopt_inner(fd, libc::IPPROTO_IPV6, 21, mreq)
}

fn setsockopt_inner(fd: RawFd, level: i32, optname: i32, optval: &Ipv6Mreq) -> Result<(), Errno> {
    let ret = unsafe {
        libc::setsockopt(
            fd,
            level,
            optname,
            optval as *const _ as *const libc::c_void,
            std::mem::size_of::<Ipv6Mreq>() as libc::socklen_t,
        )
    };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// read / write wrappers
// ---------------------------------------------------------------------------

/// `read(fd, buf, len)` — read from a raw fd.
pub fn read_fd(fd: RawFd, buf: &mut [u8]) -> Result<usize, Errno> {
    let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };
    if n < 0 {
        return Err(Errno::last());
    }
    Ok(n as usize)
}

/// `write(fd, buf, len)` — write to a raw fd.
pub fn write_fd(fd: RawFd, buf: &[u8]) -> Result<usize, Errno> {
    let n = unsafe { libc::write(fd, buf.as_ptr().cast::<libc::c_void>(), buf.len()) };
    if n < 0 {
        return Err(Errno::last());
    }
    Ok(n as usize)
}
