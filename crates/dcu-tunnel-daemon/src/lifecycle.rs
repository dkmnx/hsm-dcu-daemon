//! Daemon lifecycle: PID file, chroot, privilege drop.
//!
//! Port of `src/wfantund/wpantund.cpp:920-995`. All privileged setup
//! (serial, D-Bus, TUN) must complete **before** calling these functions.
//! Order: PID file → chroot → priv-drop. Each step is a no-op (with warning)
//! when the corresponding config is `None` or the process is not root.

use std::fs::File;
use std::io::Write;

use nix::unistd::{self, UnlinkatFlags};

use crate::config::Config;
use crate::error::DaemonError;

/// Guard that removes the PID file on drop using `unlinkat` relative to
/// the parent directory FD captured before chroot. Works correctly even
/// after the root directory has changed.
pub struct PidFileGuard {
    dir_file: Option<File>,
    basename: Option<String>,
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        if let (Some(dir_file), Some(name)) = (&self.dir_file, &self.basename) {
            let _ = unistd::unlinkat(dir_file, name.as_str(), UnlinkatFlags::NoRemoveDir);
            // File closes on drop automatically.
        }
    }
}

/// Write the current PID to the configured path. Returns a guard that
/// removes the file on drop (via `unlinkat` relative to parent dir FD,
/// so it works after chroot).
pub fn write_pidfile(config: &Config) -> PidFileGuard {
    let Some(path) = &config.daemon_pid_file else {
        return PidFileGuard {
            dir_file: None,
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
            let (dir_file, basename) = match open_parent_dir(path) {
                Ok((file, name)) => (Some(file), Some(name)),
                Err(e) => {
                    tracing::warn!("Cannot open PID file parent dir: {e}");
                    (None, None)
                }
            };
            PidFileGuard { dir_file, basename }
        }
        Err(e) => {
            tracing::warn!("Cannot create PID file {path}: {e}");
            PidFileGuard {
                dir_file: None,
                basename: None,
            }
        }
    }
}

/// Open the parent directory of `path` and return (File, basename).
fn open_parent_dir(path: &str) -> Result<(File, String), String> {
    let p = std::path::Path::new(path);
    let parent = p.parent().unwrap_or(std::path::Path::new("/"));
    let basename = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| format!("no basename in {path}"))?;
    let file = File::open(parent)
        .map_err(|e| format!("open {}: {}", parent.display(), e))?;
    Ok((file, basename))
}

/// Change the root directory to `path` (chdir → chroot → chdir("/")).
pub fn chroot_to(config: &Config) -> Result<(), DaemonError> {
    let Some(path) = &config.daemon_chroot else {
        return Ok(());
    };
    if !is_root() {
        tracing::warn!("Not running as root, cannot chroot to {path}");
        return Ok(());
    }

    unistd::chdir(path.as_str())
        .map_err(|e| DaemonError::Config(format!("chdir to {path}: {e}")))?;
    unistd::chroot(path.as_str())
        .map_err(|e| DaemonError::Config(format!("chroot to {path}: {e}")))?;
    if let Err(e) = unistd::chdir("/") {
        tracing::warn!("Failed to chdir to / after chroot: {e}");
    }
    tracing::info!("Successfully changed root directory to {path}");
    Ok(())
}

/// Drop privileges by clearing groups, setting GID and UID to the
/// configured user. Uses `User::from_name` for thread safety.
pub fn priv_drop_to(config: &Config) -> Result<(), DaemonError> {
    let Some(username) = &config.daemon_priv_drop_to_user else {
        return Ok(());
    };
    if !is_root() {
        tracing::warn!("Not running as root, cannot drop privileges to {username}");
        return Ok(());
    }

    // Resolve uid/gid via getpwnam (thread-safe in nix).
    let user = unistd::User::from_name(username)
        .map_err(|e| DaemonError::Config(format!("getpwnam: {e}")))?
        .ok_or_else(|| DaemonError::Config(format!("cannot lookup user {username}")))?;

    let target_gid = user.gid;
    let target_uid = user.uid;

    // Clear supplementary groups BEFORE setgid (CRITICAL: prevents
    // inheriting root's groups which would allow privilege escalation).
    unistd::setgroups(&[])
        .map_err(|e| DaemonError::Config(format!("setgroups: {e}")))?;
    tracing::info!("Cleared supplementary groups");

    // setgid first (must be root for this).
    unistd::setgid(target_gid)
        .map_err(|e| DaemonError::Config(format!("setgid to GID {target_gid}: {e}")))?;
    tracing::info!("Group privileges dropped to GID:{target_gid}");

    // setuid (irreversible for non-root after this call).
    unistd::setuid(target_uid)
        .map_err(|e| DaemonError::Config(format!("setuid to UID {target_uid}: {e}")))?;
    tracing::info!("User privileges dropped to UID:{target_uid} ({username})");
    Ok(())
}

fn is_root() -> bool {
    unistd::getuid().is_root()
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
        assert!(guard.dir_file.is_none());
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
