//! Raw-kernel `unsafe` shims for the system transport.
//!
//! `fork`, `ioctl(TIOCSCTTY)`, raw-fd `read`/`write`, and `_exit` are
//! isolated here. The rest of `system.rs` sees only safe wrappers.

#![allow(unsafe_code)]

use std::os::unix::io::RawFd;

use nix::errno::Errno;
use nix::fcntl::OFlag;
use nix::unistd::ForkResult;

/// `fork()` — split the process. Returns `ForkResult::Parent` or `Child`.
pub fn fork() -> Result<ForkResult, Errno> {
    unsafe { nix::unistd::fork() }
}

/// `ioctl(fd, TIOCSCTTY, 0)` — make `fd` the controlling terminal.
pub fn tiocsctty(fd: RawFd) {
    unsafe {
        libc::ioctl(fd, libc::TIOCSCTTY, 0);
    }
}

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

/// `_exit(code)` — terminate immediately without running atexit handlers.
pub fn exit_raw(code: i32) -> ! {
    unsafe {
        libc::_exit(code);
    }
}

/// `socketpair(AF_UNIX, SOCK_STREAM, 0)` — create a Unix socketpair.
pub fn socketpair_unix() -> Result<(RawFd, RawFd), Errno> {
    let mut fds = [0i32; 2];
    let ret = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) };
    if ret != 0 {
        return Err(Errno::last());
    }
    Ok((fds[0], fds[1]))
}

/// `dup(fd)` — duplicate a file descriptor.
pub fn dup_fd(fd: RawFd) -> Result<RawFd, Errno> {
    let new_fd = unsafe { libc::dup(fd) };
    if new_fd < 0 {
        return Err(Errno::last());
    }
    Ok(new_fd)
}

/// `fcntl(fd, F_GETFL)` — get file status flags.
pub fn fcntl_getfl(fd: RawFd) -> Result<i32, Errno> {
    let ret = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(ret)
}

/// `fcntl(fd, F_SETFL, flags)` — set file status flags.
pub fn fcntl_setfl(fd: RawFd, flags: i32) -> Result<(), Errno> {
    let ret = unsafe { libc::fcntl(fd, libc::F_SETFL, flags) };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(())
}

/// Set `O_NONBLOCK` on a file descriptor.
pub fn set_nonblock(fd: RawFd) -> Result<(), Errno> {
    let flags = fcntl_getfl(fd)?;
    let flags = OFlag::from_bits_truncate(flags).union(OFlag::O_NONBLOCK);
    fcntl_setfl(fd, flags.bits())
}

/// `dup2(oldfd, newfd)` — duplicate `oldfd` onto `newfd`.
pub fn dup2_fd(oldfd: RawFd, newfd: RawFd) -> Result<(), Errno> {
    let ret = unsafe { libc::dup2(oldfd, newfd) };
    if ret < 0 {
        return Err(Errno::last());
    }
    Ok(())
}
