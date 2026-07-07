use crate::error::SpinelError;
use crate::pack::{PackReader, PackWriter};

/// Header FLAG bit (bit 7).
pub const SPINEL_HEADER_FLAG: u8 = 0x80;
/// Header IID mask (bits 5-4).
pub const SPINEL_HEADER_IID_MASK: u8 = 0x30;
/// Header IID shift.
pub const SPINEL_HEADER_IID_SHIFT: u8 = 4;
/// Header TID mask (bits 3-0).
pub const SPINEL_HEADER_TID_MASK: u8 = 0x0F;

/// A decoded Spinel frame (without HDLC framing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpinelFrame {
    /// Header byte: FLAG | IID | TID.
    pub header: u8,
    /// Command ID (encoded as UINT_PACKED on wire).
    pub command_id: u32,
    /// Payload bytes (remaining after header + command).
    pub payload: Vec<u8>,
}

impl SpinelFrame {
    /// Create a new frame with default header (FLAG=1, IID=0, TID=0).
    pub fn new(command_id: u32, payload: Vec<u8>) -> Self {
        Self {
            header: SPINEL_HEADER_FLAG,
            command_id,
            payload,
        }
    }

    /// Create a frame with explicit header.
    pub fn with_header(header: u8, command_id: u32, payload: Vec<u8>) -> Self {
        Self {
            header,
            command_id,
            payload,
        }
    }

    /// Encode frame to bytes WITHOUT HDLC framing.
    ///
    /// Wire format: `UINT8(header) + UINT_PACKED(command_id) + payload`
    pub fn encode(&self) -> Vec<u8> {
        let mut writer = PackWriter::new();
        writer.write_uint8(self.header);
        writer.write_uint_packed(self.command_id);
        writer.write_bytes(&self.payload);
        writer.into_bytes()
    }

    /// Decode frame from bytes (without HDLC framing).
    pub fn decode(data: &[u8]) -> Result<Self, SpinelError> {
        if data.is_empty() {
            return Err(SpinelError::InvalidHeader);
        }

        let mut reader = PackReader::new(data);
        let header = reader.read_uint8()?;

        if header & SPINEL_HEADER_FLAG == 0 {
            return Err(SpinelError::InvalidHeader);
        }

        let command_id = reader.read_uint_packed()?;
        let remaining = reader.remaining();
        let payload = if remaining > 0 {
            reader.read_bytes(remaining)?.to_vec()
        } else {
            Vec::new()
        };

        Ok(Self {
            header,
            command_id,
            payload,
        })
    }

    /// Get Interface ID from header (bits 5-4).
    pub fn iid(&self) -> u8 {
        (self.header & SPINEL_HEADER_IID_MASK) >> SPINEL_HEADER_IID_SHIFT
    }

    /// Get Transaction ID from header (bits 3-0).
    /// TID=0 means no response expected (unsolicited).
    pub fn tid(&self) -> u8 {
        self.header & SPINEL_HEADER_TID_MASK
    }

    /// Returns `true` if the FLAG bit is set (bit 7).
    pub fn has_flag(&self) -> bool {
        self.header & SPINEL_HEADER_FLAG != 0
    }
}

/// Create a header byte from components.
pub fn make_header(iid: u8, tid: u8) -> u8 {
    debug_assert!(iid <= 3, "IID must be 0-3");
    debug_assert!(tid <= 15, "TID must be 0-15");
    SPINEL_HEADER_FLAG
        | ((iid << SPINEL_HEADER_IID_SHIFT) & SPINEL_HEADER_IID_MASK)
        | (tid & SPINEL_HEADER_TID_MASK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_header_layout() {
        let frame = SpinelFrame::with_header(0x81, 6, vec![0x00, 0x01, 0x02]);

        assert!(frame.has_flag());
        assert_eq!(frame.iid(), 0);
        assert_eq!(frame.tid(), 1);

        let encoded = frame.encode();
        let decoded = SpinelFrame::decode(&encoded).unwrap();
        assert_eq!(decoded.header, 0x81);
        assert_eq!(decoded.command_id, 6);
        assert_eq!(decoded.payload, vec![0x00, 0x01, 0x02]);
    }

    #[test]
    fn frame_round_trip_packed_command() {
        let frame = SpinelFrame::new(0x06, vec![0x01, 0x02]);
        let encoded = frame.encode();

        assert_eq!(encoded[0], 0x80);
        assert_eq!(encoded[1], 0x06);
        assert_eq!(&encoded[2..], &[0x01, 0x02]);
    }

    #[test]
    fn frame_empty_payload() {
        let frame = SpinelFrame::new(6, Vec::new());
        let encoded = frame.encode();
        let decoded = SpinelFrame::decode(&encoded).unwrap();
        assert_eq!(decoded.command_id, 6);
        assert!(decoded.payload.is_empty());
    }

    #[test]
    fn frame_with_iid_and_tid() {
        let frame = SpinelFrame::with_header(make_header(2, 5), 6, vec![]);
        assert_eq!(frame.iid(), 2);
        assert_eq!(frame.tid(), 5);
        assert!(frame.has_flag());
    }

    #[test]
    fn frame_decode_requires_flag() {
        let data = [0x00, 0x06];
        assert_eq!(SpinelFrame::decode(&data), Err(SpinelError::InvalidHeader));
    }

    #[test]
    fn frame_decode_empty() {
        assert_eq!(SpinelFrame::decode(&[]), Err(SpinelError::InvalidHeader));
    }

    #[test]
    fn frame_large_command_id() {
        // Command ID > 127 requires multi-byte LEB128
        let frame = SpinelFrame::new(200, vec![0x01]);
        let encoded = frame.encode();
        let decoded = SpinelFrame::decode(&encoded).unwrap();
        assert_eq!(decoded.command_id, 200);
    }

    #[test]
    fn make_header_values() {
        assert_eq!(make_header(0, 0), 0x80);
        assert_eq!(make_header(0, 1), 0x81);
        assert_eq!(make_header(1, 0), 0x90);
        assert_eq!(make_header(2, 5), 0xA5);
        assert_eq!(make_header(3, 15), 0xBF);
    }
}
