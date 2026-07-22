//! Configuration file parser for `wpantund.conf` format.
//!
//! NOT TOML. Key-value with colon-delimited namespaces,
//! shell-style quoting (`'` / `"`), backslash escapes, `#` comments.
//! Reimplements `src/util/config-file.c`.

use std::collections::HashMap;

use crate::DaemonError;

/// Daemon configuration parsed from `wpantund.conf`.
#[derive(Debug, Clone)]
pub struct Config {
    // NCP transport
    pub nc_socket_path: String,
    pub nc_socket_baud: u32,
    pub nc_driver_name: String,

    // TUN
    pub tun_interface_name: String,

    // Daemon behaviour
    pub daemon_pid_file: Option<String>,
    pub daemon_priv_drop_to_user: Option<String>,
    pub daemon_chroot: Option<String>,
    pub daemon_terminate_on_fault: bool,
    pub daemon_auto_associate_after_reset: bool,
    pub daemon_auto_firmware_update: bool,
    pub daemon_auto_deep_sleep: bool,
    pub daemon_syslog_mask: Option<String>,

    // NCP hardware
    pub nc_hard_reset_path: Option<String>,
    pub nc_power_path: Option<String>,
    pub nc_reliability_layer: Option<String>,
    pub nc_tx_power: Option<i8>,
    pub nc_cca_threshold: Option<i8>,

    // IPv6
    pub ipv6_wfantund_global_address: Option<std::net::Ipv6Addr>,

    // Firmware
    pub firmware_check_command: Option<String>,
    pub firmware_upgrade_command: Option<String>,

    // NetworkRetain
    pub daemon_network_retain_command: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            nc_socket_path: "/dev/ttyACM0".into(),
            nc_socket_baud: 115200,
            nc_driver_name: "spinel".into(),
            tun_interface_name: "wfan0".into(),
            daemon_pid_file: None,
            daemon_priv_drop_to_user: None,
            daemon_chroot: None,
            daemon_terminate_on_fault: false,
            daemon_auto_associate_after_reset: true,
            daemon_auto_firmware_update: false,
            daemon_auto_deep_sleep: false,
            daemon_syslog_mask: None,
            nc_hard_reset_path: None,
            nc_power_path: None,
            nc_reliability_layer: None,
            nc_tx_power: None,
            nc_cca_threshold: None,
            ipv6_wfantund_global_address: None,
            firmware_check_command: None,
            firmware_upgrade_command: None,
            daemon_network_retain_command: None,
        }
    }
}

impl Config {
    /// Parse a `wpantund.conf`-style string into a `Config`.
    ///
    /// Lines: `Config:TUN:InterfaceName "wfan0"` (key whitespace value).
    /// Comments start with `#`. Values may be quoted with `'` or `"`;
    /// backslash escapes are supported.
    pub fn parse(content: &str) -> Result<Self, DaemonError> {
        let mut map: HashMap<String, String> = HashMap::new();

        for (lineno, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let (key, value) = split_key_value(line)
                .map_err(|e| DaemonError::Config(format!("line {}: {}", lineno + 1, e)))?;

            map.insert(key, value);
        }

        Config::from_map(&map)
    }

    /// Load and parse a `wpantund.conf` file.
    pub fn load(path: &str) -> Result<Self, DaemonError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| DaemonError::Config(format!("reading {path}: {e}")))?;
        Self::parse(&content)
    }

    fn from_map(map: &HashMap<String, String>) -> Result<Self, DaemonError> {
        let mut cfg = Config::default();

        if let Some(v) = map.get("Config:NCP:SocketPath") {
            cfg.nc_socket_path = v.clone();
        }
        if let Some(v) = map.get("Config:NCP:SocketBaud") {
            cfg.nc_socket_baud = v
                .parse()
                .map_err(|e| DaemonError::Config(format!("Config:NCP:SocketBaud: {e}")))?;
        }
        if let Some(v) = map.get("Config:NCP:DriverName") {
            cfg.nc_driver_name = v.clone();
        }
        if let Some(v) = map.get("Config:TUN:InterfaceName") {
            cfg.tun_interface_name = v.clone();
        }
        if let Some(v) = map.get("Config:Daemon:PidFile") {
            cfg.daemon_pid_file = Some(v.clone());
        }
        if let Some(v) = map.get("Config:Daemon:PrivDropToUser") {
            cfg.daemon_priv_drop_to_user = Some(v.clone());
        }
        if let Some(v) = map.get("Config:Daemon:Chroot") {
            cfg.daemon_chroot = Some(v.clone());
        }
        if let Some(v) = map.get("Config:Daemon:TerminateOnFault") {
            cfg.daemon_terminate_on_fault = parse_bool(v)?;
        }
        if let Some(v) = map.get("Config:Daemon:AutoAssociateAfterReset") {
            cfg.daemon_auto_associate_after_reset = parse_bool(v)?;
        }
        if let Some(v) = map.get("Config:Daemon:AutoFirmwareUpdate") {
            cfg.daemon_auto_firmware_update = parse_bool(v)?;
        }
        if let Some(v) = map.get("Config:Daemon:AutoDeepSleep") {
            cfg.daemon_auto_deep_sleep = parse_bool(v)?;
        }
        if let Some(v) = map.get("Daemon:SyslogMask") {
            cfg.daemon_syslog_mask = Some(v.clone());
        }
        if let Some(v) = map.get("Config:NCP:HardResetPath") {
            cfg.nc_hard_reset_path = Some(v.clone());
        }
        if let Some(v) = map.get("Config:NCP:PowerPath") {
            cfg.nc_power_path = Some(v.clone());
        }
        if let Some(v) = map.get("Config:NCP:ReliabilityLayer") {
            cfg.nc_reliability_layer = Some(v.clone());
        }
        if let Some(v) = map.get("NCP:TXPower") {
            cfg.nc_tx_power = Some(
                v.parse()
                    .map_err(|e| DaemonError::Config(format!("NCP:TXPower: {e}")))?,
            );
        }
        if let Some(v) = map.get("NCP:CCAThreshold") {
            cfg.nc_cca_threshold = Some(
                v.parse()
                    .map_err(|e| DaemonError::Config(format!("NCP:CCAThreshold: {e}")))?,
            );
        }
        if let Some(v) = map.get("IPv6:WfantundGlobalAddress") {
            cfg.ipv6_wfantund_global_address =
                Some(v.parse().map_err(|e| {
                    DaemonError::Config(format!("IPv6:WfantundGlobalAddress: {e}"))
                })?);
        }
        if let Some(v) = map.get("Config:NCP:FirmwareCheckCommand") {
            cfg.firmware_check_command = Some(v.clone());
        }
        if let Some(v) = map.get("Config:NCP:FirmwareUpgradeCommand") {
            cfg.firmware_upgrade_command = Some(v.clone());
        }
        if let Some(v) = map.get("Config:Daemon:NetworkRetainCommand") {
            cfg.daemon_network_retain_command = Some(v.clone());
        }

        Ok(cfg)
    }
}

/// Split a line into (key, value), stripping quotes and handling escapes.
fn split_key_value(line: &str) -> Result<(String, String), String> {
    let (key, rest) = line
        .split_once(char::is_whitespace)
        .ok_or_else(|| format!("missing value after key in {line:?}"))?;

    let value = unquote(rest.trim())?;

    Ok((key.to_string(), value))
}

/// Unquote a shell-style string: strip surrounding `'`/`"` and interpret
/// escape sequences. Bare unquoted values are returned as-is.
fn unquote(s: &str) -> Result<String, String> {
    if s.is_empty() {
        return Ok(String::new());
    }

    let quote_char = s.as_bytes()[0];
    // Not quoted
    if quote_char != b'\'' && quote_char != b'"' {
        return Ok(s.to_string());
    }
    // Incomplete quote
    if s.len() < 2 || s.as_bytes()[s.len() - 1] != quote_char {
        return Err(format!("unterminated quote in {s:?}"));
    }

    let inner = &s[1..s.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some('\'') => out.push('\''),
                Some('"') => out.push('"'),
                Some(c) => out.push(c), // unrecognized: keep char
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }

    Ok(out)
}

fn parse_bool(s: &str) -> Result<bool, DaemonError> {
    match s.to_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(DaemonError::Config(format!("invalid boolean: {s}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_wpantund_conf_basic() {
        let content = r#"
# This is a comment
Config:TUN:InterfaceName "wfan0"
Config:NCP:SocketPath "/dev/ttyUSB0"
Config:NCP:DriverName "spinel"
Config:NCP:SocketBaud 115200
"#;
        let config = Config::parse(content).unwrap();
        assert_eq!(config.tun_interface_name, "wfan0");
        assert_eq!(config.nc_socket_path, "/dev/ttyUSB0");
        assert_eq!(config.nc_driver_name, "spinel");
        assert_eq!(config.nc_socket_baud, 115200);
    }

    #[test]
    fn parse_unquoted_values() {
        let content = r#"Config:TUN:InterfaceName wfan0"#;
        let config = Config::parse(content).unwrap();
        assert_eq!(config.tun_interface_name, "wfan0");
    }

    #[test]
    fn parse_escaped_quotes() {
        // Single-quoted value with embedded single quote via escape
        let content = r#"Config:NCP:SocketPath "/dev/ttyACM0""#;
        let config = Config::parse(content).unwrap();
        assert_eq!(config.nc_socket_path, "/dev/ttyACM0");
    }

    #[test]
    fn parse_missing_value_is_error() {
        let result = Config::parse("Config:TUN:InterfaceName\n");
        assert!(result.is_err());
    }

    #[test]
    fn config_defaults_are_sane() {
        let config = Config::default();
        assert_eq!(config.nc_socket_path, "/dev/ttyACM0");
        assert_eq!(config.nc_socket_baud, 115200);
        assert_eq!(config.tun_interface_name, "wfan0");
        assert!(config.daemon_auto_associate_after_reset);
        assert!(!config.daemon_terminate_on_fault);
        assert!(!config.daemon_auto_firmware_update);
    }

    #[test]
    fn parse_all_bool_formats() {
        let content = r#"
Config:Daemon:TerminateOnFault true
Config:Daemon:AutoFirmwareUpdate 0
Config:Daemon:AutoDeepSleep no
"#;
        let config = Config::parse(content).unwrap();
        assert!(config.daemon_terminate_on_fault);
        assert!(!config.daemon_auto_firmware_update);
        assert!(!config.daemon_auto_deep_sleep);
    }

    #[test]
    fn parse_empty_and_comments() {
        let content = "# only a comment\n\n\n  \nConfig:TUN:InterfaceName test0\n";
        let config = Config::parse(content).unwrap();
        assert_eq!(config.tun_interface_name, "test0");
    }
}
