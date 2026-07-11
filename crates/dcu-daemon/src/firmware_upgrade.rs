//! NCP firmware upgrade via `tokio::process::Command`.
//!
//! Reimplements `src/dcud/FirmwareUpgrade.cpp`: the C class spawns forked
//! helper processes (double-fork for privilege separation) via setter methods
//! `set_firmware_upgrade_command()` and `set_firmware_check_command()`. The
//! Rust version uses `tokio::process::Command` instead of `fork()`.

use crate::DaemonError;

/// Check whether the NCP firmware requires an upgrade by running the
/// configured check command with the firmware version as an argument.
///
/// The check command receives the version as its last argument. Exit code 0
/// means "already up to date" (no upgrade needed); non-zero means upgrade
/// is required, matching the C `is_firmware_upgrade_required(version)` where
/// the check child writes 0 when upgrade IS needed (ret = (c == 0)).
pub async fn is_firmware_upgrade_required(
    check_command: &str,
    version: &str,
) -> Result<bool, DaemonError> {
    let status = tokio::process::Command::new(check_command)
        .arg(version)
        .status()
        .await?;

    // Non-zero exit = upgrade required.
    Ok(!status.success())
}

/// Perform a firmware upgrade using the configured upgrade command.
///
/// The command is spawned directly as an external process (no shell wrapper)
/// unless the command contains shell metacharacters that require it.
pub async fn upgrade_firmware(upgrade_command: &str) -> Result<(), DaemonError> {
    // Split on whitespace for direct exec (avoiding sh -c injection).
    let parts = shell_words::split(upgrade_command)
        .map_err(|e| DaemonError::Config(format!("firmware command parse: {e}")))?;
    if parts.is_empty() {
        return Err(DaemonError::Config("empty firmware command".into()));
    }
    let status = tokio::process::Command::new(&parts[0])
        .args(&parts[1..])
        .status()
        .await?;

    if status.success() {
        Ok(())
    } else {
        Err(DaemonError::Ncp(format!(
            "firmware upgrade exited with {:?}",
            status.code()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn upgrade_required_detected() {
        let result = is_firmware_upgrade_required("/usr/bin/false", "v1.0").await;
        assert!(result.is_ok());
        assert!(result.unwrap(), "non-zero exit = upgrade required");
    }

    #[tokio::test]
    async fn upgrade_not_required() {
        let result = is_firmware_upgrade_required("/usr/bin/true", "v1.0").await;
        assert!(result.is_ok());
        assert!(!result.unwrap(), "zero exit = already up to date");
    }
}
