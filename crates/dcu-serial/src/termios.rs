//! Safe termios helper for `dcu-serial`.
//!
//! All `unsafe` FFI is confined to this module. Callers see a safe
//! `apply_termios_flags(fd, config)` function.

#![allow(unsafe_code)]

use std::os::unix::io::RawFd;

use libc::{self, termios as LibcTermios, TCSAFLUSH};
use nix::errno::Errno;

use crate::error::SerialError;
use crate::uart::SerialConfig;

/// Apply CLOCAL, IXON, IXOFF, IXANY flags to an open serial fd.
pub fn apply_termios_flags(fd: RawFd, config: &SerialConfig) -> Result<(), SerialError> {
    let mut termios = tcgetattr(fd)?;
    apply_flags(&mut termios, config);
    tcsetattr(fd, &termios)
}

#[inline]
fn apply_flags(termios: &mut LibcTermios, config: &SerialConfig) {
    if config.clocal {
        termios.c_lflag |= libc::CLOCAL;
    } else {
        termios.c_lflag &= !libc::CLOCAL;
    }
    if config.ixon {
        termios.c_iflag |= libc::IXON;
    } else {
        termios.c_iflag &= !libc::IXON;
    }
    if config.ixoff {
        termios.c_iflag |= libc::IXOFF;
    } else {
        termios.c_iflag &= !libc::IXOFF;
    }
    if config.ixany {
        termios.c_iflag |= libc::IXANY;
    } else {
        termios.c_iflag &= !libc::IXANY;
    }
}

fn tcgetattr(fd: RawFd) -> Result<LibcTermios, SerialError> {
    unsafe {
        let mut t: LibcTermios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut t) != 0 {
            return Err(SerialError::Io(std::io::Error::from_raw_os_error(
                Errno::last() as i32
            )));
        }
        Ok(t)
    }
}

fn tcsetattr(fd: RawFd, termios: &LibcTermios) -> Result<(), SerialError> {
    unsafe {
        if libc::tcsetattr(fd, TCSAFLUSH, termios) != 0 {
            return Err(SerialError::Io(std::io::Error::from_raw_os_error(
                Errno::last() as i32
            )));
        }
        Ok(())
    }
}
