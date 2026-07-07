use std::fmt;

/// Wi-SUN error status codes, mapping `wpantund_status_t` from `wpan-error.h:27-75`.
///
/// Known values 0-32 map to `kWPANTUNDStatus_*` constants.
/// Values 33-39 are reserved for future use, included so that all
/// codes 0-39 round-trip through the `From<i32>` / `Into<i32>` impls.
///
/// NCP error codes (range `0xEA0000..=0xEAFFFF`) are **not** represented
/// as individual enum variants — they are collapsed to `NcpError` by
/// `From<i32>`, which loses the specific sub-code. Use `NCP_ERROR_BASE`
/// and `is_ncp_error()` to inspect raw values before conversion if the
/// sub-code matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum WpanError {
    Ok = 0,
    Failure = 1,
    InvalidArgument = 2,
    InvalidWhenDisabled = 3,
    InvalidForCurrentState = 4,
    InvalidType = 5,
    InvalidRange = 6,
    Timeout = 7,
    SocketReset = 8,
    Busy = 9,
    Already = 10,
    Canceled = 11,
    InProgress = 12,
    TryAgainLater = 13,
    FeatureNotSupported = 14,
    FeatureNotImplemented = 15,
    PropertyNotFound = 16,
    PropertyEmpty = 17,
    JoinFailedUnknown = 18,
    JoinFailedAtScan = 19,
    JoinFailedAtAuthenticate = 20,
    FormFailedAtScan = 21,
    NcpCrashed = 22,
    NcpFatal = 23,
    NcpInvalidArgument = 24,
    NcpInvalidRange = 25,
    MissingXpanid = 26,
    NcpReset = 27,
    InterfaceNotFound = 28,
    JoinerFailedSecurity = 29,
    JoinerFailedNoPeers = 30,
    JoinerFailedResponseTimeout = 31,
    JoinerFailedUnknown = 32,
    /// Reserved for future standard status codes.
    Reserved33 = 33,
    Reserved34 = 34,
    Reserved35 = 35,
    Reserved36 = 36,
    Reserved37 = 37,
    Reserved38 = 38,
    Reserved39 = 39,
    /// NCP error (range `0xEA0000..=0xEAFFFF`).
    ///
    /// Caution: `From<i32>` maps any code in this range to this single
    /// variant, discarding the sub-code. Use `is_ncp_error()` before
    /// conversion if you need to preserve the raw value.
    NcpError = 0xEA0000,
}

/// Base of the NCP error code range (`kWPANTUNDStatus_NCPError_First`).
pub const NCP_ERROR_BASE: i32 = 0xEA0000;

/// End of the NCP error code range (`kWPANTUNDStatus_NCPError_Last`).
pub const NCP_ERROR_END: i32 = 0xEAFFFF;

const _: () = assert!(NCP_ERROR_END >= NCP_ERROR_BASE);

/// NCP error mask (`WPANTUND_NCPERROR_MASK` from `wpan-error.h:77`).
pub const NCP_ERROR_MASK: i32 = 0xFFFF;

impl WpanError {
    /// All non-sentinel variant values 0-39 (excludes `NcpError`).
    pub const VARIANTS: &'static [Self] = &[
        Self::Ok,
        Self::Failure,
        Self::InvalidArgument,
        Self::InvalidWhenDisabled,
        Self::InvalidForCurrentState,
        Self::InvalidType,
        Self::InvalidRange,
        Self::Timeout,
        Self::SocketReset,
        Self::Busy,
        Self::Already,
        Self::Canceled,
        Self::InProgress,
        Self::TryAgainLater,
        Self::FeatureNotSupported,
        Self::FeatureNotImplemented,
        Self::PropertyNotFound,
        Self::PropertyEmpty,
        Self::JoinFailedUnknown,
        Self::JoinFailedAtScan,
        Self::JoinFailedAtAuthenticate,
        Self::FormFailedAtScan,
        Self::NcpCrashed,
        Self::NcpFatal,
        Self::NcpInvalidArgument,
        Self::NcpInvalidRange,
        Self::MissingXpanid,
        Self::NcpReset,
        Self::InterfaceNotFound,
        Self::JoinerFailedSecurity,
        Self::JoinerFailedNoPeers,
        Self::JoinerFailedResponseTimeout,
        Self::JoinerFailedUnknown,
        Self::Reserved33,
        Self::Reserved34,
        Self::Reserved35,
        Self::Reserved36,
        Self::Reserved37,
        Self::Reserved38,
        Self::Reserved39,
    ];

    /// Returns `true` if this status represents a success.
    pub fn is_success(self) -> bool {
        self == WpanError::Ok
    }

    /// Returns `true` if this status represents an error (non-Ok).
    pub fn is_error(self) -> bool {
        self != WpanError::Ok
    }

    /// Returns the raw `i32` status code for this variant.
    pub fn raw_code(self) -> i32 {
        self as i32
    }

    /// Returns `true` if the given code falls in the NCP error range.
    pub fn is_ncp_error(code: i32) -> bool {
        (code & !NCP_ERROR_MASK) == NCP_ERROR_BASE
    }
}

impl From<i32> for WpanError {
    fn from(code: i32) -> Self {
        match code {
            0 => WpanError::Ok,
            1 => WpanError::Failure,
            2 => WpanError::InvalidArgument,
            3 => WpanError::InvalidWhenDisabled,
            4 => WpanError::InvalidForCurrentState,
            5 => WpanError::InvalidType,
            6 => WpanError::InvalidRange,
            7 => WpanError::Timeout,
            8 => WpanError::SocketReset,
            9 => WpanError::Busy,
            10 => WpanError::Already,
            11 => WpanError::Canceled,
            12 => WpanError::InProgress,
            13 => WpanError::TryAgainLater,
            14 => WpanError::FeatureNotSupported,
            15 => WpanError::FeatureNotImplemented,
            16 => WpanError::PropertyNotFound,
            17 => WpanError::PropertyEmpty,
            18 => WpanError::JoinFailedUnknown,
            19 => WpanError::JoinFailedAtScan,
            20 => WpanError::JoinFailedAtAuthenticate,
            21 => WpanError::FormFailedAtScan,
            22 => WpanError::NcpCrashed,
            23 => WpanError::NcpFatal,
            24 => WpanError::NcpInvalidArgument,
            25 => WpanError::NcpInvalidRange,
            26 => WpanError::MissingXpanid,
            27 => WpanError::NcpReset,
            28 => WpanError::InterfaceNotFound,
            29 => WpanError::JoinerFailedSecurity,
            30 => WpanError::JoinerFailedNoPeers,
            31 => WpanError::JoinerFailedResponseTimeout,
            32 => WpanError::JoinerFailedUnknown,
            33 => WpanError::Reserved33,
            34 => WpanError::Reserved34,
            35 => WpanError::Reserved35,
            36 => WpanError::Reserved36,
            37 => WpanError::Reserved37,
            38 => WpanError::Reserved38,
            39 => WpanError::Reserved39,
            _ => {
                if Self::is_ncp_error(code) {
                    WpanError::NcpError
                } else {
                    WpanError::Failure
                }
            }
        }
    }
}

impl From<WpanError> for i32 {
    fn from(err: WpanError) -> Self {
        err as i32
    }
}

impl fmt::Display for WpanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            WpanError::Ok => "OK",
            WpanError::Failure => "Failure",
            WpanError::InvalidArgument => "Invalid argument",
            WpanError::InvalidWhenDisabled => "Invalid when disabled",
            WpanError::InvalidForCurrentState => "Invalid for current state",
            WpanError::InvalidType => "Invalid type",
            WpanError::InvalidRange => "Invalid range",
            WpanError::Timeout => "Timeout",
            WpanError::SocketReset => "Socket reset",
            WpanError::Busy => "Busy",
            WpanError::Already => "Already",
            WpanError::Canceled => "Canceled",
            WpanError::InProgress => "In progress",
            WpanError::TryAgainLater => "Try again later",
            WpanError::FeatureNotSupported => "Feature not supported",
            WpanError::FeatureNotImplemented => "Feature not implemented",
            WpanError::PropertyNotFound => "Property not found",
            WpanError::PropertyEmpty => "Property empty",
            WpanError::JoinFailedUnknown => "Join failed (unknown)",
            WpanError::JoinFailedAtScan => "Join failed at scan",
            WpanError::JoinFailedAtAuthenticate => "Join failed at authenticate",
            WpanError::FormFailedAtScan => "Form failed at scan",
            WpanError::NcpCrashed => "NCP crashed",
            WpanError::NcpFatal => "NCP fatal",
            WpanError::NcpInvalidArgument => "NCP invalid argument",
            WpanError::NcpInvalidRange => "NCP invalid range",
            WpanError::MissingXpanid => "Missing XPANID",
            WpanError::NcpReset => "NCP reset",
            WpanError::InterfaceNotFound => "Interface not found",
            WpanError::JoinerFailedSecurity => "Joiner failed (security)",
            WpanError::JoinerFailedNoPeers => "Joiner failed (no peers)",
            WpanError::JoinerFailedResponseTimeout => "Joiner failed (response timeout)",
            WpanError::JoinerFailedUnknown => "Joiner failed (unknown)",
            WpanError::Reserved33 => "Reserved (33)",
            WpanError::Reserved34 => "Reserved (34)",
            WpanError::Reserved35 => "Reserved (35)",
            WpanError::Reserved36 => "Reserved (36)",
            WpanError::Reserved37 => "Reserved (37)",
            WpanError::Reserved38 => "Reserved (38)",
            WpanError::Reserved39 => "Reserved (39)",
            WpanError::NcpError => "NCP error",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_count_matches_c() {
        assert_eq!(WpanError::VARIANTS.len(), 40);
    }

    #[test]
    fn error_code_c_to_rust_round_trip() {
        for code in 0..40 {
            let err = WpanError::from(code);
            let back: i32 = err.into();
            assert_eq!(back, code);
        }
    }

    #[test]
    fn error_code_helpers() {
        assert!(WpanError::Ok.is_success());
        assert!(!WpanError::Ok.is_error());
        assert!(WpanError::Failure.is_error());
        assert!(!WpanError::Failure.is_success());
    }

    #[test]
    fn ncp_error_range() {
        assert!(WpanError::is_ncp_error(0xEA0001));
        assert!(WpanError::is_ncp_error(0xEA1234));
        assert!(!WpanError::is_ncp_error(5));
        assert!(!WpanError::is_ncp_error(0xEB0000));
    }

    #[test]
    fn ncp_error_from_code() {
        let err = WpanError::from(0xEA0001);
        assert_eq!(err as i32, 0xEA0000);
        assert_eq!(err.raw_code(), 0xEA0000);
    }

    #[test]
    fn raw_code() {
        assert_eq!(WpanError::Ok.raw_code(), 0);
        assert_eq!(WpanError::Failure.raw_code(), 1);
        assert_eq!(WpanError::InterfaceNotFound.raw_code(), 28);
    }

    #[test]
    fn ncp_error_constants() {
        assert_eq!(NCP_ERROR_BASE, 0xEA0000);
        assert_eq!(NCP_ERROR_END, 0xEAFFFF);
        assert_eq!(NCP_ERROR_MASK, 0xFFFF);
    }
}
