use crate::error::SpinelError;

/// HDLC flag byte — marks frame boundaries.
pub const FLAG_BYTE: u8 = 0x7E;
/// HDLC escape byte.
pub const ESCAPE_BYTE: u8 = 0x7D;
/// XON (flow control).
pub const XON: u8 = 0x11;
/// XOFF (flow control).
pub const XOFF: u8 = 0x13;
/// Special byte used in some HDLC variants.
pub const SPECIAL_BYTE: u8 = 0xF8;
/// XOR value for escape sequences.
pub const ESCAPE_XFORM: u8 = 0x20;

/// CRC-16/X.25 initial value.
///
/// Note: The C code in DataPump.cpp:92-94 labels this table as "KERMIT"
/// but applies init=0xFFFF (line 242) and final XOR=0xFFFF (line 274),
/// which is CRC-16/X.25 (aka CRC-16/ISO-HDLC, CRC-16/IBM-SDLC).
/// The table itself is the reflected polynomial 0x1021 lookup, shared by
/// both KERMIT and X.25 — only the init/final-XOR differ.
const CRC_INIT: u16 = 0xFFFF;

/// CRC-16/X.25 lookup table (reflected polynomial 0x1021).
///
/// Matches `SpinelNCPInstance-DataPump.cpp:95-129` exactly.
static CRC_TABLE: [u16; 256] = [
    0x0000, 0x1189, 0x2312, 0x329b, 0x4624, 0x57ad, 0x6536, 0x74bf, 0x8c48, 0x9dc1, 0xaf5a, 0xbed3,
    0xca6c, 0xdbe5, 0xe97e, 0xf8f7, 0x1081, 0x0108, 0x3393, 0x221a, 0x56a5, 0x472c, 0x75b7, 0x643e,
    0x9cc9, 0x8d40, 0xbfdb, 0xae52, 0xdaed, 0xcb64, 0xf9ff, 0xe876, 0x2102, 0x308b, 0x0210, 0x1399,
    0x6726, 0x76af, 0x4434, 0x55bd, 0xad4a, 0xbcc3, 0x8e58, 0x9fd1, 0xeb6e, 0xfae7, 0xc87c, 0xd9f5,
    0x3183, 0x200a, 0x1291, 0x0318, 0x77a7, 0x662e, 0x54b5, 0x453c, 0xbdcb, 0xac42, 0x9ed9, 0x8f50,
    0xfbef, 0xea66, 0xd8fd, 0xc974, 0x4204, 0x538d, 0x6116, 0x709f, 0x0420, 0x15a9, 0x2732, 0x36bb,
    0xce4c, 0xdfc5, 0xed5e, 0xfcd7, 0x8868, 0x99e1, 0xab7a, 0xbaf3, 0x5285, 0x430c, 0x7197, 0x601e,
    0x14a1, 0x0528, 0x37b3, 0x263a, 0xdecd, 0xcf44, 0xfddf, 0xec56, 0x98e9, 0x8960, 0xbbfb, 0xaa72,
    0x6306, 0x728f, 0x4014, 0x519d, 0x2522, 0x34ab, 0x0630, 0x17b9, 0xef4e, 0xfec7, 0xcc5c, 0xddd5,
    0xa96a, 0xb8e3, 0x8a78, 0x9bf1, 0x7387, 0x620e, 0x5095, 0x411c, 0x35a3, 0x242a, 0x16b1, 0x0738,
    0xffcf, 0xee46, 0xdcdd, 0xcd54, 0xb9eb, 0xa862, 0x9af9, 0x8b70, 0x8408, 0x9581, 0xa71a, 0xb693,
    0xc22c, 0xd3a5, 0xe13e, 0xf0b7, 0x0840, 0x19c9, 0x2b52, 0x3adb, 0x4e64, 0x5fed, 0x6d76, 0x7cff,
    0x9489, 0x8500, 0xb79b, 0xa612, 0xd2ad, 0xc324, 0xf1bf, 0xe036, 0x18c1, 0x0948, 0x3bd3, 0x2a5a,
    0x5ee5, 0x4f6c, 0x7df7, 0x6c7e, 0xa50a, 0xb483, 0x8618, 0x9791, 0xe32e, 0xf2a7, 0xc03c, 0xd1b5,
    0x2942, 0x38cb, 0x0a50, 0x1bd9, 0x6f66, 0x7eef, 0x4c74, 0x5dfd, 0xb58b, 0xa402, 0x9699, 0x8710,
    0xf3af, 0xe226, 0xd0bd, 0xc134, 0x39c3, 0x284a, 0x1ad1, 0x0b58, 0x7fe7, 0x6e6e, 0x5cf5, 0x4d7c,
    0xc60c, 0xd785, 0xe51e, 0xf497, 0x8028, 0x91a1, 0xa33a, 0xb2b3, 0x4a44, 0x5bcd, 0x6956, 0x78df,
    0x0c60, 0x1de9, 0x2f72, 0x3efb, 0xd68d, 0xc704, 0xf59f, 0xe416, 0x90a9, 0x8120, 0xb3bb, 0xa232,
    0x5ac5, 0x4b4c, 0x79d7, 0x685e, 0x1ce1, 0x0d68, 0x3ff3, 0x2e7a, 0xe70e, 0xf687, 0xc41c, 0xd595,
    0xa12a, 0xb0a3, 0x8238, 0x93b1, 0x6b46, 0x7acf, 0x4854, 0x59dd, 0x2d62, 0x3ceb, 0x0e70, 0x1ff9,
    0xf78f, 0xe606, 0xd49d, 0xc514, 0xb1ab, 0xa022, 0x92b9, 0x8330, 0x7bc7, 0x6a4e, 0x58d5, 0x495c,
    0x3de3, 0x2c6a, 0x1ef1, 0x0f78,
];

/// Update CRC-16 with one byte, using the reflected table.
fn crc16_update(crc: u16, byte: u8) -> u16 {
    (crc >> 8) ^ CRC_TABLE[((crc ^ byte as u16) & 0xFF) as usize]
}

/// Returns `true` if the byte needs HDLC escaping.
fn needs_escape(byte: u8) -> bool {
    matches!(byte, FLAG_BYTE | ESCAPE_BYTE | XON | XOFF | SPECIAL_BYTE)
}

/// HDLC encoder — produces framed, escaped, CRC-protected output.
pub struct HdlcEncoder {
    crc: u16,
}

impl HdlcEncoder {
    pub fn new() -> Self {
        Self { crc: CRC_INIT }
    }

    /// Encode raw bytes with HDLC framing.
    ///
    /// Output: FLAG + escaped_data + CRC(2 bytes LE) + FLAG
    pub fn encode_bytes(&mut self, data: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        output.push(FLAG_BYTE);

        for &byte in data {
            self.crc = crc16_update(self.crc, byte);
            if needs_escape(byte) {
                output.push(ESCAPE_BYTE);
                output.push(byte ^ ESCAPE_XFORM);
            } else {
                output.push(byte);
            }
        }

        // Append CRC (final XOR applied)
        let crc = self.crc ^ 0xFFFF;
        let crc_bytes = crc.to_le_bytes();
        for &byte in &crc_bytes {
            if needs_escape(byte) {
                output.push(ESCAPE_BYTE);
                output.push(byte ^ ESCAPE_XFORM);
            } else {
                output.push(byte);
            }
        }

        output.push(FLAG_BYTE);
        self.crc = CRC_INIT; // reset for next frame
        output
    }

    /// Encode a [`SpinelFrame`] (header + command + payload) with HDLC framing.
    ///
    /// Convenience wrapper that first calls [`SpinelFrame::encode`] then
    /// [`HdlcEncoder::encode_bytes`].
    pub fn encode_frame(&mut self, frame: &crate::frame::SpinelFrame) -> Vec<u8> {
        self.encode_bytes(&frame.encode())
    }
}

impl Default for HdlcEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// HDLC decoder state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HdlcState {
    /// Looking for opening FLAG byte.
    SeekingFlag,
    /// Inside a frame, collecting data bytes.
    Receiving,
    /// In an escape sequence, next byte needs XOR.
    Escaped,
}

/// HDLC decoder — parses framed, escaped, CRC-protected input.
pub struct HdlcDecoder {
    state: HdlcState,
    buffer: Vec<u8>,
}

impl HdlcDecoder {
    pub fn new() -> Self {
        Self {
            state: HdlcState::SeekingFlag,
            buffer: Vec::new(),
        }
    }

    /// Feed one byte. Returns a complete frame when decoded.
    ///
    /// - `Some(Ok(frame_data))` — complete frame (without CRC)
    /// - `Some(Err(SpinelError::CrcMismatch))` — CRC failed
    /// - `None` — still collecting bytes
    pub fn feed_byte(&mut self, byte: u8) -> Option<Result<Vec<u8>, SpinelError>> {
        match self.state {
            HdlcState::SeekingFlag => {
                if byte == FLAG_BYTE {
                    self.state = HdlcState::Receiving;
                }
                None
            }
            HdlcState::Receiving => {
                if byte == FLAG_BYTE {
                    // End of frame
                    if self.buffer.len() <= 2 {
                        // Frame too short (just CRC or empty)
                        self.buffer.clear();
                        return None;
                    }

                    // Remove CRC from buffer
                    let frame_data = self.buffer[..self.buffer.len() - 2].to_vec();
                    let received_crc = u16::from_le_bytes([
                        self.buffer[self.buffer.len() - 2],
                        self.buffer[self.buffer.len() - 1],
                    ]);

                    // Verify CRC (recompute from scratch — O(n) but
                    // simple and correct)
                    let mut check_crc = CRC_INIT;
                    for &b in &frame_data {
                        check_crc = crc16_update(check_crc, b);
                    }
                    check_crc ^= 0xFFFF;

                    self.buffer.clear();
                    self.state = HdlcState::SeekingFlag;

                    if received_crc == check_crc {
                        Some(Ok(frame_data))
                    } else {
                        Some(Err(SpinelError::CrcMismatch))
                    }
                } else if byte == ESCAPE_BYTE {
                    self.state = HdlcState::Escaped;
                    None
                } else {
                    self.buffer.push(byte);
                    None
                }
            }
            HdlcState::Escaped => {
                self.state = HdlcState::Receiving;
                // ESC followed by FLAG is a frame abort per
                // DataPump.cpp:250-254 — discard and resync.
                if byte == FLAG_BYTE {
                    self.buffer.clear();
                    return None;
                }
                let unescaped = byte ^ ESCAPE_XFORM;
                self.buffer.push(unescaped);
                None
            }
        }
    }
}

impl Default for HdlcDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::SpinelFrame;

    #[test]
    fn hdlc_crc_round_trip() {
        let mut encoder = HdlcEncoder::new();
        let data = vec![0x80, 0x06];
        let encoded = encoder.encode_bytes(&data);

        // Verify frame structure: FLAG ... CRC ... FLAG
        assert_eq!(encoded[0], FLAG_BYTE);
        assert_eq!(*encoded.last().unwrap(), FLAG_BYTE);
        assert!(encoded.len() >= 5); // FLAG + 2 data + 2 CRC + FLAG

        // Full round-trip
        let mut decoder = HdlcDecoder::new();
        let mut result = None;
        for byte in &encoded {
            result = decoder.feed_byte(*byte);
        }
        let frame_data = result.unwrap().unwrap();
        assert_eq!(frame_data, data);
    }

    /// Golden vector test (spec Test 4).
    ///
    /// CRC-16/X.25 of `[0x80, 0x06]` is the integer `0xE6BD` (high byte 0xE6,
    /// low byte 0xBD). On the wire the CRC is stored little-endian, so the two
    /// CRC bytes are `[0xBD, 0xE6]` = `0xE6BDu16.to_le_bytes()`. The assertion
    /// below reads the actual two CRC bytes (indices len-3..len-1, immediately
    /// before the trailing FLAG) and compares them against that.
    #[test]
    fn hdlc_crc_golden_vector() {
        let mut encoder = HdlcEncoder::new();
        let data = vec![0x80, 0x06]; // FLAG header + CMD_PROP_VALUE_IS (packed)
        let encoded = encoder.encode_bytes(&data);

        let expected_crc = 0xE6BDu16.to_le_bytes(); // [0xBD, 0xE6] on the wire (LE)
        // HDLC layout: FLAG + data + CRC(2, LE) + FLAG. The two CRC bytes sit
        // immediately before the trailing FLAG, i.e. at encoded[len-3..len-1].
        let frame_end = encoded.len() - 3;
        assert_eq!(
            encoded[frame_end..frame_end + 2],
            expected_crc,
            "CRC does not match expected CRC-16/X.25 of [0x80,0x06]"
        );
        // Sanity: the byte after the CRC window must be the trailing FLAG, and
        // the window must NOT include it.
        assert_eq!(encoded[encoded.len() - 1], FLAG_BYTE);

        // Full round-trip decode
        let mut decoder = HdlcDecoder::new();
        let mut result = None;
        for byte in &encoded {
            result = decoder.feed_byte(*byte);
        }
        let frame_data = result.unwrap().unwrap();
        assert_eq!(frame_data, data);
    }

    #[test]
    fn hdlc_escape_special_bytes() {
        let mut encoder = HdlcEncoder::new();
        let data = vec![FLAG_BYTE, ESCAPE_BYTE, XON, XOFF, SPECIAL_BYTE];
        let encoded = encoder.encode_bytes(&data);

        // No unescaped FLAG in the middle
        let inner = &encoded[1..encoded.len() - 1];
        assert!(!inner.contains(&0x7E));

        // Decode and verify round-trip
        let mut decoder = HdlcDecoder::new();
        let mut decoded = None;
        for byte in encoded {
            decoded = decoder.feed_byte(byte);
        }
        assert_eq!(decoded.unwrap().unwrap(), data);
    }

    #[test]
    fn hdlc_full_round_trip() {
        let mut encoder = HdlcEncoder::new();
        let frame = SpinelFrame::new(0x06, vec![0x00, 0x01]);
        let hdlc_encoded = encoder.encode_frame(&frame);

        assert_eq!(hdlc_encoded[0], FLAG_BYTE);
        assert_eq!(*hdlc_encoded.last().unwrap(), FLAG_BYTE);

        let mut decoder = HdlcDecoder::new();
        let mut result = None;
        for byte in &hdlc_encoded {
            result = decoder.feed_byte(*byte);
        }
        let frame_data = result.unwrap().unwrap();
        let decoded = SpinelFrame::decode(&frame_data).unwrap();
        assert_eq!(decoded.command_id, 0x06);
    }

    #[test]
    fn hdlc_crc_mismatch_detected() {
        let mut encoder = HdlcEncoder::new();
        let data = vec![0x80, 0x06];
        let mut encoded = encoder.encode_bytes(&data);

        // Corrupt the CRC
        let last = encoded.len() - 2;
        encoded[last] ^= 0xFF;

        let mut decoder = HdlcDecoder::new();
        let mut result = None;
        for byte in encoded {
            result = decoder.feed_byte(byte);
        }
        assert_eq!(result.unwrap(), Err(SpinelError::CrcMismatch));
    }

    #[test]
    fn hdlc_esc_flag_aborts_frame() {
        // ESC followed by FLAG should abort the frame (DataPump.cpp:250-254)
        let mut decoder = HdlcDecoder::new();
        // Start a frame
        assert_eq!(decoder.feed_byte(FLAG_BYTE), None);
        // Send some data
        assert_eq!(decoder.feed_byte(0x80), None);
        assert_eq!(decoder.feed_byte(0x06), None);
        // ESC + FLAG = frame abort
        assert_eq!(decoder.feed_byte(ESCAPE_BYTE), None);
        assert_eq!(decoder.feed_byte(FLAG_BYTE), None);
        // Next FLAG starts a new frame
        let mut encoder = HdlcEncoder::new();
        let data = vec![0x42];
        let encoded = encoder.encode_bytes(&data);
        let mut result = None;
        for byte in encoded {
            result = decoder.feed_byte(byte);
        }
        assert_eq!(result.unwrap().unwrap(), data);
    }

    #[test]
    fn hdlc_empty_frame_ignored() {
        let mut decoder = HdlcDecoder::new();
        // Two consecutive FLAGs with no data
        assert_eq!(decoder.feed_byte(FLAG_BYTE), None);
        assert_eq!(decoder.feed_byte(FLAG_BYTE), None);
    }

    #[test]
    fn hdlc_interleaved_flags() {
        let mut decoder = HdlcDecoder::new();
        // Extra FLAGs before actual frame
        assert_eq!(decoder.feed_byte(FLAG_BYTE), None);
        assert_eq!(decoder.feed_byte(FLAG_BYTE), None);

        let mut encoder = HdlcEncoder::new();
        let data = vec![0x42];
        let encoded = encoder.encode_bytes(&data);

        let mut result = None;
        for byte in encoded {
            result = decoder.feed_byte(byte);
        }
        assert_eq!(result.unwrap().unwrap(), data);
    }
}
