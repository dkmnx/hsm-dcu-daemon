//! Error types for the `dcu-tunnel-daemon` crate.

/// Top-level error type for the daemon.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("config: {0}")]
    Config(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("serial: {0}")]
    Serial(#[from] dcu_serial::SerialError),

    #[error("tun: {0}")]
    Tun(#[from] dcu_tun::TunError),

    #[error("dbus: {0}")]
    Dbus(String),

    #[error("spinel: {0}")]
    Spinel(#[from] spinel::SpinelError),

    #[error("task cancelled")]
    Cancelled,

    #[error("ncp: {0}")]
    Ncp(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),
}

impl From<dcu_dbus::types::DbusError> for DaemonError {
    fn from(e: dcu_dbus::types::DbusError) -> Self {
        DaemonError::Dbus(e.to_string())
    }
}

impl From<String> for DaemonError {
    fn from(s: String) -> Self {
        DaemonError::Ncp(s)
    }
}
