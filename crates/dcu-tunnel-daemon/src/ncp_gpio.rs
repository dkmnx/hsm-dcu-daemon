//! NCP hard-reset and power GPIO control.
//!
//! Port of the sysfs GPIO toggle in
//! `NCPInstanceBase-AsyncIO.cpp:82-119`. Writes to the configured
//! `Config:NCP:HardResetPath` / `PowerPath` sysfs file to toggle
//! the GPIO value.

use std::fs::File;
use std::io::Write;
use std::time::Duration;

use crate::config::Config;

/// Write a single byte + newline to a sysfs GPIO file.
fn sysfs_write(path: &str, value: u8) -> Result<(), String> {
    let mut f = File::create(path).map_err(|e| format!("open {path}: {e}"))?;
    f.write_all(&[value, b'\n'])
        .map_err(|e| format!("write {path}: {e}"))?;
    Ok(())
}

/// Perform a hard reset of the NCP by toggling the GPIO at the
/// configured `HardResetPath`. Writes `'0'`, sleeps 20ms, then
/// writes `'1'` — matching the C `hard_reset_ncp()` toggle sequence.
pub fn hard_reset(config: &Config) -> Result<(), String> {
    let Some(path) = &config.nc_hard_reset_path else {
        return Ok(());
    };
    sysfs_write(path, b'0')?;
    std::thread::sleep(Duration::from_millis(20));
    sysfs_write(path, b'1')?;
    tracing::info!("NCP hard reset via {path}");
    Ok(())
}

/// Toggle the NCP power GPIO at the configured `PowerPath`.
/// Same toggle pattern as `hard_reset`.
pub fn power_toggle(config: &Config) -> Result<(), String> {
    let Some(path) = &config.nc_power_path else {
        return Ok(());
    };
    sysfs_write(path, b'0')?;
    std::thread::sleep(Duration::from_millis(20));
    sysfs_write(path, b'1')?;
    tracing::info!("NCP power toggle via {path}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_reset_noop_when_none() {
        let config = Config::default();
        assert!(hard_reset(&config).is_ok());
    }

    #[test]
    fn power_toggle_noop_when_none() {
        let config = Config::default();
        assert!(power_toggle(&config).is_ok());
    }

    #[test]
    fn sysfs_write_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gpio");
        let path_str = path.to_str().unwrap();

        sysfs_write(path_str, b'0').unwrap();
        let contents = std::fs::read(&path).unwrap();
        assert_eq!(contents, b"0\n");

        sysfs_write(path_str, b'1').unwrap();
        let contents = std::fs::read(&path).unwrap();
        assert_eq!(contents, b"1\n");
    }
}
