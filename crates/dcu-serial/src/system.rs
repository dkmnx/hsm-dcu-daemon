#![allow(unsafe_code)]

//! System transport: spawn a child process behind a PTY.
//!
//! Implements the `system:` and `system-forkpty:` prefixes from
//! `Config:NCP:SocketPath`. The command is executed via `/bin/sh -c <cmd>`
//! with its stdin/stdout/stderr connected to a pseudo-terminal.
//!
//! The master PTY fd is wrapped with `tokio::io::unix::AsyncFd` for
//! non-blocking async I/O. Child processes are tracked in a global table
//! and terminated on drop or process exit.

use std::os::unix::io::{AsRawFd, IntoRawFd, RawFd};
use std::pin::Pin;
use std::sync::{LazyLock, Mutex};
use std::task::{Context, Poll};

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use nix::pty::openpty;
use nix::sys::signal::{Signal, kill};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{ForkResult, Pid, close, dup2, execvp, fork, setsid};
use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::error::SerialError;
use crate::transport::Transport;

/// Global table tracking spawned child PIDs and their master fds.
/// Entries are removed when PtyFd drops; the actual waitpid happens
/// in a spawned thread to avoid blocking the tokio runtime.
static CHILD_TABLE: LazyLock<Mutex<Vec<ChildEntry>>> = LazyLock::new(|| Mutex::new(Vec::new()));

struct ChildEntry {
    pid: Pid,
    master_fd: RawFd,
}

/// Wrapper around a raw PTY master fd for `AsyncFd` integration.
/// Owns the fd — closing it on drop.
struct PtyFd {
    fd: RawFd,
}

impl PtyFd {
    fn new(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl AsRawFd for PtyFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for PtyFd {
    fn drop(&mut self) {
        // Remove from child table (non-blocking).
        let entry = CHILD_TABLE.lock().ok().and_then(|mut table| {
            table
                .iter()
                .position(|e| e.master_fd == self.fd)
                .map(|pos| table.remove(pos))
        });

        // Close master fd first — child sees hangup on its side of the PTY.
        let _ = close(self.fd);

        // Reap the child in a blocking thread so we don't stall tokio.
        if let Some(entry) = entry {
            std::thread::spawn(move || {
                let _ = kill(entry.pid, Signal::SIGHUP);
                // Bounded non-blocking retries
                for _ in 0..10 {
                    match waitpid(entry.pid, Some(WaitPidFlag::WNOHANG)) {
                        Ok(WaitStatus::StillAlive) => {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                        _ => return,
                    }
                }
                let _ = kill(entry.pid, Signal::SIGTERM);
                for _ in 0..5 {
                    match waitpid(entry.pid, Some(WaitPidFlag::WNOHANG)) {
                        Ok(WaitStatus::StillAlive) => {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                        _ => return,
                    }
                }
                // Final: force kill and blocking wait
                let _ = kill(entry.pid, Signal::SIGKILL);
                let _ = waitpid(entry.pid, None);
            });
        }
    }
}

/// A system transport wrapping a PTY master fd with async I/O.
pub struct SystemTransport {
    inner: AsyncFd<PtyFd>,
    pid: Pid,
}

/// Convert a `nix::errno::Errno` to `std::io::Error`.
fn nix_err(e: nix::errno::Errno) -> std::io::Error {
    std::io::Error::from_raw_os_error(e as i32)
}

/// Close all file descriptors greater than `above` by iterating /proc/self/fd.
/// Snapshots the fd list first to avoid closing the directory handle mid-iteration.
fn close_fds_above(above: RawFd) {
    let fds: Vec<RawFd> = std::fs::read_dir("/proc/self/fd")
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|name| name.parse::<RawFd>().ok())
        .filter(|&fd| fd > above)
        .collect();
    for fd in fds {
        let _ = close(fd);
    }
}

impl SystemTransport {
    /// Spawn a child process behind a PTY. The command is executed via
    /// `/bin/sh -c <command>`.
    pub async fn spawn(command: &str) -> Result<Self, SerialError> {
        // Open a PTY pair with default termios (no stdin dependency).
        // openpty(None, None) returns a PTY with sane defaults.
        let pty = openpty(None, None).map_err(|e| SerialError::Io(nix_err(e)))?;

        // Take raw fds and transfer ownership: into_raw_fd() consumes the
        // OwnedFd so its Drop will NOT close the fd. PtyFd and the child
        // process become the sole owners.
        let master_fd = pty.master.into_raw_fd();
        let slave_fd = pty.slave.into_raw_fd();

        // Fork
        match unsafe { fork() }.map_err(|e| SerialError::Io(nix_err(e)))? {
            ForkResult::Parent { child } => {
                // Parent: close slave (owned by child now), track child, wrap master
                let _ = close(slave_fd);

                // Set master to non-blocking
                let flags =
                    fcntl(master_fd, FcntlArg::F_GETFL).map_err(|e| SerialError::Io(nix_err(e)))?;
                let flags = OFlag::from_bits_truncate(flags).union(OFlag::O_NONBLOCK);
                fcntl(master_fd, FcntlArg::F_SETFL(flags))
                    .map_err(|e| SerialError::Io(nix_err(e)))?;

                // Track the child
                if let Ok(mut table) = CHILD_TABLE.lock() {
                    table.push(ChildEntry {
                        pid: child,
                        master_fd,
                    });
                }

                // PtyFd takes ownership of master_fd
                let pty_fd = PtyFd::new(master_fd);
                let inner = AsyncFd::new(pty_fd).map_err(SerialError::Io)?;

                tracing::info!(
                    "system transport: spawned pid={} for command {:?}",
                    child,
                    command
                );

                Ok(Self { inner, pid: child })
            }
            ForkResult::Child => {
                // Child: setsid, redirect stdio, close all fds, exec
                let _ = setsid();

                // Make slave the controlling terminal
                unsafe {
                    libc::ioctl(slave_fd, libc::TIOCSCTTY, 0);
                }

                // Redirect stdin/stdout/stderr to slave
                let _ = dup2(slave_fd, libc::STDIN_FILENO);
                let _ = dup2(slave_fd, libc::STDOUT_FILENO);
                let _ = dup2(slave_fd, libc::STDERR_FILENO);

                // Close slave fd if it's > 2 (already redirected)
                if slave_fd > 2 {
                    let _ = close(slave_fd);
                }

                // Close master fd in child
                let _ = close(master_fd);

                // Close ALL other fds to prevent leaks (TUN, D-Bus socket, etc.)
                close_fds_above(libc::STDERR_FILENO);

                // Exec via /bin/sh -c <command>.
                // Use libc::_exit (not std::process::exit) to avoid running
                // atexit handlers or global Drop in the forked child.
                let shell = std::ffi::CString::new("/bin/sh").unwrap();
                let arg_c = std::ffi::CString::new("-c").unwrap();
                let cmd_c = match std::ffi::CString::new(command) {
                    Ok(c) => c,
                    Err(_) => unsafe {
                        libc::_exit(127);
                    },
                };

                let _ = execvp(&shell, &[&shell, &arg_c, &cmd_c]);
                unsafe {
                    libc::_exit(127);
                }
            }
        }
    }
}

impl Transport for SystemTransport {
    fn raw_fd(&self) -> Option<RawFd> {
        Some(self.inner.get_ref().as_raw_fd())
    }

    fn info(&self) -> String {
        format!("system:pid={}", self.pid)
    }
}

impl AsyncRead for SystemTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            let mut guard = match self.inner.poll_read_ready(cx) {
                Poll::Ready(Ok(guard)) => guard,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            };

            let n = unsafe {
                let unfilled = buf.initialize_unfilled();
                libc::read(
                    guard.get_inner().as_raw_fd(),
                    unfilled.as_mut_ptr() as *mut libc::c_void,
                    unfilled.len(),
                )
            };

            match n {
                -1 => {
                    let err = std::io::Error::last_os_error();
                    if err.kind() == std::io::ErrorKind::WouldBlock {
                        guard.clear_ready();
                        continue;
                    }
                    return Poll::Ready(Err(err));
                }
                0 => return Poll::Ready(Ok(())),
                n => {
                    buf.advance(n as usize);
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

impl AsyncWrite for SystemTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        loop {
            let mut guard = match self.inner.poll_write_ready(cx) {
                Poll::Ready(Ok(guard)) => guard,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            };

            let n = unsafe {
                libc::write(
                    guard.get_inner().as_raw_fd(),
                    buf.as_ptr() as *const libc::c_void,
                    buf.len(),
                )
            };

            match n {
                -1 => {
                    let err = std::io::Error::last_os_error();
                    if err.kind() == std::io::ErrorKind::WouldBlock {
                        guard.clear_ready();
                        continue;
                    }
                    return Poll::Ready(Err(err));
                }
                n => return Poll::Ready(Ok(n as usize)),
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pty_fd_ownership() {
        // Verify PtyFd can be created and reports its fd
        let pty_fd = PtyFd::new(42);
        assert_eq!(pty_fd.as_raw_fd(), 42);
        // PtyFd::drop will call close(42) — but 42 isn't a real fd in the test,
        // so it just returns EBADF which we ignore. The important thing is that
        // the struct compiles and the ownership model is correct.
    }
}
