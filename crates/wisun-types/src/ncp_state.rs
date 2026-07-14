use std::error::Error;
use std::fmt;
use std::str::FromStr;

/// NCP state, mapping `nl::wpantund::NCPState` from NCPTypes.h:34-46.
///
/// The C enum values are:
/// `UNINITIALIZED=0, FAULT=1, UPGRADING=2, DEEP_SLEEP=3, OFFLINE=4,
///  COMMISSIONED=5, ASSOCIATING=6, CREDENTIALS_NEEDED=7, ASSOCIATED=8,
///  ISOLATED=9, NET_WAKE_WAKING=10, NET_WAKE_ASLEEP=11`
///
/// D-Bus string mappings are defined in `wpan-properties.h:437-448`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum NcpState {
    Uninitialized = 0,
    Fault = 1,
    Upgrading = 2,
    DeepSleep = 3,
    Offline = 4,
    Commissioned = 5,
    Associating = 6,
    CredentialsNeeded = 7,
    Associated = 8,
    Isolated = 9,
    NetWakeWaking = 10,
    NetWakeAsleep = 11,
}

impl NcpState {
    /// Returns `true` if the NCP is in a fully associated state.
    ///
    /// Matches `ncp_state_is_associated()` from NCPTypes.cpp:152-163
    /// under `TI_WISUN_FAN` semantics.
    pub fn is_associated(&self) -> bool {
        matches!(
            self,
            NcpState::Associated
                | NcpState::Isolated
                | NcpState::NetWakeWaking
                | NcpState::NetWakeAsleep
        )
    }

    /// Returns `true` if the NCP is offline.
    pub fn is_offline(&self) -> bool {
        self == &NcpState::Offline
    }

    /// Returns `true` if the NCP is in a fault state.
    pub fn is_fault(&self) -> bool {
        self == &NcpState::Fault
    }

    /// Returns `true` if the NCP is still initializing.
    ///
    /// Matches `ncp_state_is_initializing()` from `NCPTypes.cpp:124-133`,
    /// which is true for **only** `Uninitialized` and `Upgrading` — **not**
    /// `Fault`. Used as the entry guard in nearly every Spinel task.
    pub fn is_initializing(&self) -> bool {
        matches!(self, NcpState::Uninitialized | NcpState::Upgrading)
    }

    /// Returns `true` if the NCP is in a commissioned state.
    ///
    /// Matches `ncp_state_is_commissioned()` from `NCPTypes.cpp:108-121`:
    /// `Commissioned | Associated | NetWakeAsleep | Isolated | NetWakeWaking`.
    /// No `TI_WISUN_FAN` guard — same for all builds.
    pub fn is_commissioned(&self) -> bool {
        matches!(
            self,
            NcpState::Commissioned
                | NcpState::Associated
                | NcpState::NetWakeAsleep
                | NcpState::Isolated
                | NcpState::NetWakeWaking
        )
    }
}

impl FromStr for NcpState {
    type Err = ParseNcpStateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "uninitialized" => Ok(NcpState::Uninitialized),
            "uninitialized:fault" => Ok(NcpState::Fault),
            "uninitialized:upgrading" => Ok(NcpState::Upgrading),
            "offline:deep-sleep" => Ok(NcpState::DeepSleep),
            "offline" => Ok(NcpState::Offline),
            "offline:commissioned" => Ok(NcpState::Commissioned),
            "associating" => Ok(NcpState::Associating),
            "associating:credentials-needed" => Ok(NcpState::CredentialsNeeded),
            "associated" => Ok(NcpState::Associated),
            "associated:no-parent" => Ok(NcpState::Isolated),
            "associated:netwake-waking" => Ok(NcpState::NetWakeWaking),
            "associated:netwake-asleep" => Ok(NcpState::NetWakeAsleep),
            _ => Err(ParseNcpStateError {
                invalid: s.to_string(),
            }),
        }
    }
}

impl fmt::Display for NcpState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            NcpState::Uninitialized => "uninitialized",
            NcpState::Fault => "uninitialized:fault",
            NcpState::Upgrading => "uninitialized:upgrading",
            NcpState::DeepSleep => "offline:deep-sleep",
            NcpState::Offline => "offline",
            NcpState::Commissioned => "offline:commissioned",
            NcpState::Associating => "associating",
            NcpState::CredentialsNeeded => "associating:credentials-needed",
            NcpState::Associated => "associated",
            NcpState::Isolated => "associated:no-parent",
            NcpState::NetWakeWaking => "associated:netwake-waking",
            NcpState::NetWakeAsleep => "associated:netwake-asleep",
        };
        f.write_str(s)
    }
}

impl TryFrom<u32> for NcpState {
    type Error = &'static str;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(NcpState::Uninitialized),
            1 => Ok(NcpState::Fault),
            2 => Ok(NcpState::Upgrading),
            3 => Ok(NcpState::DeepSleep),
            4 => Ok(NcpState::Offline),
            5 => Ok(NcpState::Commissioned),
            6 => Ok(NcpState::Associating),
            7 => Ok(NcpState::CredentialsNeeded),
            8 => Ok(NcpState::Associated),
            9 => Ok(NcpState::Isolated),
            10 => Ok(NcpState::NetWakeWaking),
            11 => Ok(NcpState::NetWakeAsleep),
            _ => Err("unknown NcpState value"),
        }
    }
}

/// Error returned when a string cannot be parsed as an `NcpState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseNcpStateError {
    invalid: String,
}

impl fmt::Display for ParseNcpStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid NCP state string: '{}'", self.invalid)
    }
}

impl Error for ParseNcpStateError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ncp_state_from_c_enum() {
        let c_to_rust = vec![
            (0, NcpState::Uninitialized),
            (1, NcpState::Fault),
            (2, NcpState::Upgrading),
            (3, NcpState::DeepSleep),
            (4, NcpState::Offline),
            (5, NcpState::Commissioned),
            (6, NcpState::Associating),
            (7, NcpState::CredentialsNeeded),
            (8, NcpState::Associated),
            (9, NcpState::Isolated),
            (10, NcpState::NetWakeWaking),
            (11, NcpState::NetWakeAsleep),
        ];
        for (c_val, rust_state) in c_to_rust {
            assert_eq!(rust_state as u32, c_val);
        }
    }

    #[test]
    fn ncp_state_dbus_string_round_trip() {
        let states = vec![
            ("uninitialized", NcpState::Uninitialized),
            ("uninitialized:fault", NcpState::Fault),
            ("uninitialized:upgrading", NcpState::Upgrading),
            ("offline:deep-sleep", NcpState::DeepSleep),
            ("offline", NcpState::Offline),
            ("offline:commissioned", NcpState::Commissioned),
            ("associating", NcpState::Associating),
            (
                "associating:credentials-needed",
                NcpState::CredentialsNeeded,
            ),
            ("associated", NcpState::Associated),
            ("associated:no-parent", NcpState::Isolated),
            ("associated:netwake-waking", NcpState::NetWakeWaking),
            ("associated:netwake-asleep", NcpState::NetWakeAsleep),
        ];
        for (s, expected) in states {
            let parsed: NcpState = s.parse().unwrap();
            assert_eq!(parsed, expected, "Failed to parse {s}");
            assert_eq!(parsed.to_string(), s);
        }
    }

    #[test]
    fn ncp_state_try_from_u32() {
        for i in 0..12 {
            assert!(NcpState::try_from(i).is_ok());
        }
        assert!(NcpState::try_from(12).is_err());
        assert!(NcpState::try_from(255).is_err());
    }

    #[test]
    fn ncp_state_helpers() {
        assert!(NcpState::Associated.is_associated());
        assert!(NcpState::Isolated.is_associated());
        assert!(NcpState::NetWakeWaking.is_associated());
        assert!(NcpState::NetWakeAsleep.is_associated());
        assert!(!NcpState::Offline.is_associated());
        assert!(!NcpState::Uninitialized.is_associated());

        assert!(NcpState::Offline.is_offline());
        assert!(!NcpState::Associated.is_offline());

        assert!(NcpState::Fault.is_fault());
        assert!(!NcpState::Associated.is_fault());
    }

    #[test]
    fn ncp_state_initializing() {
        // Per NCPTypes.cpp:124-133 — only Uninitialized + Upgrading.
        assert!(NcpState::Uninitialized.is_initializing());
        assert!(NcpState::Upgrading.is_initializing());
        assert!(!NcpState::Fault.is_initializing());
        assert!(!NcpState::Offline.is_initializing());
        assert!(!NcpState::Associated.is_initializing());
        assert!(!NcpState::Associating.is_initializing());
        assert!(!NcpState::DeepSleep.is_initializing());
    }

    #[test]
    fn ncp_state_is_commissioned() {
        // Matches NCPTypes.cpp:108-121 — no TI guard.
        assert!(NcpState::Commissioned.is_commissioned());
        assert!(NcpState::Associated.is_commissioned());
        assert!(NcpState::NetWakeAsleep.is_commissioned());
        assert!(NcpState::Isolated.is_commissioned());
        assert!(NcpState::NetWakeWaking.is_commissioned());
        // Not commissioned:
        assert!(!NcpState::Offline.is_commissioned());
        assert!(!NcpState::Uninitialized.is_commissioned());
        assert!(!NcpState::Fault.is_commissioned());
        assert!(!NcpState::Associating.is_commissioned());
        assert!(!NcpState::CredentialsNeeded.is_commissioned());
        assert!(!NcpState::DeepSleep.is_commissioned());
        assert!(!NcpState::Upgrading.is_commissioned());
    }

    #[test]
    fn ncp_state_invalid_string() {
        assert!("invalid".parse::<NcpState>().is_err());
        assert!("".parse::<NcpState>().is_err());
    }
}
