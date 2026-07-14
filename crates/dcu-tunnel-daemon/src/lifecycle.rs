//! Daemon lifecycle: PID file, chroot, privilege drop.
//!
//! Port of `src/wfantund/wpantund.cpp:920-995`. All privileged setup
//! (serial, D-Bus, TUN) must complete **before** calling these functions.
//! Order: PID file → chroot → priv-drop. Each step is a no-op (with warning)
//! when the corresponding config is `None` or the process is not root.

use std::fs::File;
use std::io::Write;
use std::os::unix::io::RawFd;

use crate::config::Config;
use crate::error::DaemonError;

/// Guard that removes the PID file on drop using `unlinkat` relative to
/// the parent directory FD captured before chroot. Works correctly even
/// after the root directory has changed.
pub struct PidFileGuard {
    dir_fd: Option<RawFd>,
    basename: Option<String>,
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        if let (Some(dir_fd), Some(name)) = (self.dir_fd, &self.basename) {
            let c_name =
                std::ffi::CString::new(name.as_str()).expect("PID filename is not a C string");
            // SAFETY: dir_fd was opened by us before chroot; unlinkat is
            // a kernel call that doesn't resolve against the process root.
            #[allow(unsafe_code)]
            unsafe {
                libc::unlinkat(dir_fd, c_name.as_ptr(), 0);
                libc::close(dir_fd);
            }
        }
    }
}

/// Write the current PID to the configured path. Returns a guard that
/// removes the file on drop (via `unlinkat` relative to parent dir FD,
/// so it works after chroot).
pub fn write_pidfile(config: &Config) -> PidFileGuard {
    let Some(path) = &config.daemon_pid_file else {
        return PidFileGuard {
            dir_fd: None,
            basename: None,
        };
    };
    match File::create(path) {
        Ok(mut f) => {
            let pid = std::process::id();
            if writeln!(f, "{pid}").is_err() {
                tracing::warn!("Failed to write PID file {path}");
            }
            // Open parent directory FD before chroot for reliable cleanup.
            let (dir_fd, basename) = match open_parent_dir(path) {
                Ok((fd, name)) => (Some(fd), Some(name)),
                Err(e) => {
                    tracing::warn!("Cannot open PID file parent dir: {e}");
                    (None, None)
                }
            };
            PidFileGuard { dir_fd, basename }
        }
        Err(e) => {
            tracing::warn!("Cannot create PID file {path}: {e}");
            PidFileGuard {
                dir_fd: None,
                basename: None,
            }
        }
    }
}

/// Open the parent directory of `path` and return (dirfd, basename).
fn open_parent_dir(path: &str) -> Result<(RawFd, String), String> {
    let p = std::path::Path::new(path);
    let parent = p.parent().unwrap_or(std::path::Path::new("/"));
    let basename = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| format!("no basename in {path}"))?;
    let c_parent = std::ffi::CString::new(parent.to_string_lossy().as_ref())
        .map_err(|e| format!("parent path: {e}"))?;
    // SAFETY: opendir is a POSIX call; we check for null below.
    #[allow(unsafe_code)]
    let dir = unsafe { libc::opendir(c_parent.as_ptr()) };
    if dir.is_null() {
        return Err(format!(
            "opendir {}: {}",
            parent.display(),
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: dirfd extracts the fd from the DIR*; closedir would close it,
    // but we need the fd to outlive chroot, so we use dirfd and let the
    // guard close it manually.
    #[allow(unsafe_code)]
    let fd = unsafe { libc::dirfd(dir) };
    // We must not closedir(dir) — the fd is used by the guard.
    // Leaking the DIR* is acceptable; the fd is the important part.
    let _ = dir;
    Ok((fd, basename))
}

/// Change the root directory to `path` (chdir → chroot → chdir("/")).
#[allow(unsafe_code)]
pub fn chroot_to(config: &Config) -> Result<(), DaemonError> {
    let Some(path) = &config.daemon_chroot else {
        return Ok(());
    };
    if !is_root() {
        tracing::warn!("Not running as root, cannot chroot to {path}");
        return Ok(());
    }
    let c_path = std::ffi::CString::new(path.as_str())
        .map_err(|e| DaemonError::Config(format!("chroot path: {e}")))?;

    let ret = unsafe { libc::chdir(c_path.as_ptr()) };
    if ret != 0 {
        return Err(DaemonError::Config(format!(
            "chdir to {path}: {}",
            std::io::Error::last_os_error()
        )));
    }
    let ret = unsafe { libc::chroot(c_path.as_ptr()) };
    if ret != 0 {
        return Err(DaemonError::Config(format!(
            "chroot to {path}: {}",
            std::io::Error::last_os_error()
        )));
    }
    let root = std::ffi::CString::new("/").unwrap();
    let ret = unsafe { libc::chdir(root.as_ptr()) };
    if ret != 0 {
        tracing::warn!(
            "Failed to chdir to / after chroot: {}",
            std::io::Error::last_os_error()
        );
    }
    tracing::info!("Successfully changed root directory to {path}");
    Ok(())
}

/// Drop privileges by clearing groups, setting GID and UID to the
/// configured user. Uses `getpwnam_r` for thread safety.
#[allow(unsafe_code)]
pub fn priv_drop_to(config: &Config) -> Result<(), DaemonError> {
    let Some(username) = &config.daemon_priv_drop_to_user else {
        return Ok(());
    };
    if !is_root() {
        tracing::warn!("Not running as root, cannot drop privileges to {username}");
        return Ok(());
    }
    let c_name = std::ffi::CString::new(username.as_str())
        .map_err(|e| DaemonError::Config(format!("username: {e}")))?;

    // Resolve uid/gid via getpwnam_r (thread-safe).
    let mut buf = [0u8; 4096];
    let mut passwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    let ret = unsafe {
        libc::getpwnam_r(
            c_name.as_ptr(),
            &mut passwd,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if ret != 0 || result.is_null() {
        return Err(DaemonError::Config(format!(
            "getpwnam_r: cannot lookup user {username}: {}",
            std::io::Error::from_raw_os_error(ret)
        )));
    }
    let target_gid = unsafe { (*result).pw_gid };
    let target_uid = unsafe { (*result).pw_uid };

    // Clear supplementary groups BEFORE setgid (CRITICAL: prevents
    // inheriting root's groups which would allow privilege escalation).
    let ret = unsafe { libc::setgroups(0, std::ptr::null()) };
    if ret != 0 {
        return Err(DaemonError::Config(format!(
            "setgroups: {}",
            std::io::Error::last_os_error()
        )));
    }
    tracing::info!("Cleared supplementary groups");

    // setgid first (must be root for this).
    let ret = unsafe { libc::setgid(target_gid) };
    if ret != 0 {
        return Err(DaemonError::Config(format!(
            "setgid to GID {target_gid}: {}",
            std::io::Error::last_os_error()
        )));
    }
    tracing::info!("Group privileges dropped to GID:{target_gid}");

    // setuid (irreversible for non-root after this call).
    let ret = unsafe { libc::setuid(target_uid) };
    if ret != 0 {
        return Err(DaemonError::Config(format!(
            "setuid to UID {target_uid}: {}",
            std::io::Error::last_os_error()
        )));
    }
    tracing::info!("User privileges dropped to UID:{target_uid} ({username})");
    Ok(())
}

fn is_root() -> bool {
    #[allow(unsafe_code)]
    unsafe {
        libc::getuid() == 0
    }
}

/// Apply the full lifecycle sequence: PID file → chroot → priv-drop.
/// Call this after all privileged setup is complete.
pub fn apply_lifecycle(config: &Config) -> Result<PidFileGuard, DaemonError> {
    let pid_guard = write_pidfile(config);
    chroot_to(config)?;
    priv_drop_to(config)?;
    Ok(pid_guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pidfile_write_and_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pid");
        let path_str = path.to_str().unwrap().to_string();

        let config = Config {
            daemon_pid_file: Some(path_str),
            ..Default::default()
        };
        let guard = write_pidfile(&config);

        let contents = std::fs::read_to_string(&path).unwrap();
        let pid: u32 = contents.trim().parse().unwrap();
        assert_eq!(pid, std::process::id());

        drop(guard);
        assert!(!path.exists());
    }

    #[test]
    fn pidfile_noop_when_none() {
        let config = Config::default();
        let guard = write_pidfile(&config);
        assert!(guard.dir_fd.is_none());
    }

    #[test]
    fn chroot_noop_when_none() {
        let config = Config::default();
        assert!(chroot_to(&config).is_ok());
    }

    #[test]
    fn priv_drop_noop_when_none() {
        let config = Config::default();
        assert!(priv_drop_to(&config).is_ok());
    }

    #[test]
    fn apply_lifecycle_returns_guard() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pid");
        let path_str = path.to_str().unwrap().to_string();

        let config = Config {
            daemon_pid_file: Some(path_str),
            ..Default::default()
        };
        let guard = apply_lifecycle(&config).unwrap();
        assert!(path.exists());
        drop(guard);
        assert!(!path.exists());
    }
}
