use crate::error::SpinelError;

/// Maximum value for Spinel packed unsigned integer (LEB128).
///
/// C spinel.h defines UINT_PACKED as 21-bit (3 bytes, max 2,097,151).
/// This matches the format-level enforcement in spinel.h:180-199.
/// The raw C decoder supports up to 5 bytes, but Spinel property keys
/// and command IDs never exceed 21 bits in practice.
pub const SPINEL_MAX_UINT_PACKED: u32 = 0x1FFFFF; // 2,097,151

/// Encoder for Spinel pack format.
///
/// Writes values into a byte buffer using Spinel wire format encoding.
pub struct PackWriter {
    buf: Vec<u8>,
}

impl PackWriter {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Packed unsigned integer (LEB128) — the dominant integer type.
    ///
    /// Values < 127 encode as 1 byte. Larger values use 7-bit chunks
    /// with continuation bit, up to 3 bytes (max 2,097,151).
    pub fn write_uint_packed(&mut self, val: u32) {
        assert!(val <= SPINEL_MAX_UINT_PACKED);
        if val < 0x80 {
            self.buf.push(val as u8);
        } else if val < 0x4000 {
            self.buf.push(0x80 | (val & 0x7F) as u8);
            self.buf.push((val >> 7) as u8);
        } else {
            self.buf.push(0x80 | (val & 0x7F) as u8);
            self.buf.push(0x80 | ((val >> 7) & 0x7F) as u8);
            self.buf.push((val >> 14) as u8);
        }
    }

    /// Single byte (0 or 1).
    pub fn write_bool(&mut self, val: bool) {
        self.buf.push(u8::from(val));
    }

    /// Unsigned 8-bit integer.
    pub fn write_uint8(&mut self, val: u8) {
        self.buf.push(val);
    }

    /// Signed 8-bit integer.
    pub fn write_int8(&mut self, val: i8) {
        self.buf.push(val as u8);
    }

    /// Unsigned 16-bit integer, little-endian.
    pub fn write_uint16(&mut self, val: u16) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    /// Signed 16-bit integer, little-endian.
    pub fn write_int16(&mut self, val: i16) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    /// Unsigned 32-bit integer, little-endian.
    pub fn write_uint32(&mut self, val: u32) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    /// Signed 32-bit integer, little-endian.
    pub fn write_int32(&mut self, val: i32) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    /// Unsigned 64-bit integer, little-endian.
    pub fn write_uint64(&mut self, val: u64) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    /// Signed 64-bit integer, little-endian.
    pub fn write_int64(&mut self, val: i64) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    /// IPv6 address, 16 bytes, big-endian.
    pub fn write_ipv6(&mut self, addr: &[u8; 16]) {
        self.buf.extend_from_slice(addr);
    }

    /// EUI-64 address, 8 bytes, big-endian.
    pub fn write_eui64(&mut self, addr: &[u8; 8]) {
        self.buf.extend_from_slice(addr);
    }

    /// EUI-48 address, 6 bytes, big-endian.
    pub fn write_eui48(&mut self, addr: &[u8; 6]) {
        self.buf.extend_from_slice(addr);
    }

    /// Raw bytes, no length prefix ("D" format).
    pub fn write_bytes(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Data with uint16 LE length prefix ("d" format).
    pub fn write_data_with_len(&mut self, data: &[u8]) {
        self.write_uint16(data.len() as u16);
        self.buf.extend_from_slice(data);
    }

    /// UTF-8 string, NUL-terminated ("U" format).
    pub fn write_utf8(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
    }

    /// Begin a struct — reserves 2 bytes for uint16 LE length prefix.
    ///
    /// Returns the position of the length field for later patching
    /// with [`write_struct_end`].
    pub fn write_struct_start(&mut self) -> usize {
        let pos = self.buf.len();
        self.buf.extend_from_slice(&[0, 0]);
        pos
    }

    /// End a struct — patches the uint16 LE length prefix.
    pub fn write_struct_end(&mut self, start_pos: usize) {
        let total_len = (self.buf.len() - start_pos - 2) as u16;
        self.buf[start_pos..start_pos + 2].copy_from_slice(&total_len.to_le_bytes());
    }

    /// Consume the writer and return the encoded bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

impl Default for PackWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Decoder for Spinel pack format.
///
/// Reads values from a byte buffer using Spinel wire format decoding.
pub struct PackReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> PackReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    /// Packed unsigned integer (LEB128).
    pub fn read_uint_packed(&mut self) -> Result<u32, SpinelError> {
        let mut result: u32 = 0;
        let mut shift: u32 = 0;

        loop {
            if self.offset >= self.data.len() {
                return Err(SpinelError::Underflow);
            }
            let byte = self.data[self.offset];
            self.offset += 1;

            result |= u32::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift >= 35 {
                return Err(SpinelError::InvalidPackedUint);
            }
        }

        if result > SPINEL_MAX_UINT_PACKED {
            return Err(SpinelError::InvalidPackedUint);
        }
        Ok(result)
    }

    /// Single byte (bool).
    pub fn read_bool(&mut self) -> Result<bool, SpinelError> {
        Ok(self.read_uint8()? != 0)
    }

    /// Unsigned 8-bit integer.
    pub fn read_uint8(&mut self) -> Result<u8, SpinelError> {
        if self.offset >= self.data.len() {
            return Err(SpinelError::Underflow);
        }
        let val = self.data[self.offset];
        self.offset += 1;
        Ok(val)
    }

    /// Signed 8-bit integer.
    pub fn read_int8(&mut self) -> Result<i8, SpinelError> {
        Ok(self.read_uint8()? as i8)
    }

    /// Unsigned 16-bit integer, little-endian.
    pub fn read_uint16(&mut self) -> Result<u16, SpinelError> {
        if self.offset + 2 > self.data.len() {
            return Err(SpinelError::Underflow);
        }
        let val = u16::from_le_bytes([self.data[self.offset], self.data[self.offset + 1]]);
        self.offset += 2;
        Ok(val)
    }

    /// Signed 16-bit integer, little-endian.
    pub fn read_int16(&mut self) -> Result<i16, SpinelError> {
        Ok(self.read_uint16()? as i16)
    }

    /// Unsigned 32-bit integer, little-endian.
    pub fn read_uint32(&mut self) -> Result<u32, SpinelError> {
        if self.offset + 4 > self.data.len() {
            return Err(SpinelError::Underflow);
        }
        let val = u32::from_le_bytes([
            self.data[self.offset],
            self.data[self.offset + 1],
            self.data[self.offset + 2],
            self.data[self.offset + 3],
        ]);
        self.offset += 4;
        Ok(val)
    }

    /// Signed 32-bit integer, little-endian.
    pub fn read_int32(&mut self) -> Result<i32, SpinelError> {
        Ok(self.read_uint32()? as i32)
    }

    /// Unsigned 64-bit integer, little-endian.
    pub fn read_uint64(&mut self) -> Result<u64, SpinelError> {
        if self.offset + 8 > self.data.len() {
            return Err(SpinelError::Underflow);
        }
        let val = u64::from_le_bytes([
            self.data[self.offset],
            self.data[self.offset + 1],
            self.data[self.offset + 2],
            self.data[self.offset + 3],
            self.data[self.offset + 4],
            self.data[self.offset + 5],
            self.data[self.offset + 6],
            self.data[self.offset + 7],
        ]);
        self.offset += 8;
        Ok(val)
    }

    /// Signed 64-bit integer, little-endian.
    pub fn read_int64(&mut self) -> Result<i64, SpinelError> {
        Ok(self.read_uint64()? as i64)
    }

    /// IPv6 address, 16 bytes, big-endian.
    pub fn read_ipv6(&mut self) -> Result<[u8; 16], SpinelError> {
        if self.offset + 16 > self.data.len() {
            return Err(SpinelError::Underflow);
        }
        let mut addr = [0u8; 16];
        addr.copy_from_slice(&self.data[self.offset..self.offset + 16]);
        self.offset += 16;
        Ok(addr)
    }

    /// EUI-64 address, 8 bytes, big-endian.
    pub fn read_eui64(&mut self) -> Result<[u8; 8], SpinelError> {
        if self.offset + 8 > self.data.len() {
            return Err(SpinelError::Underflow);
        }
        let mut addr = [0u8; 8];
        addr.copy_from_slice(&self.data[self.offset..self.offset + 8]);
        self.offset += 8;
        Ok(addr)
    }

    /// EUI-48 address, 6 bytes, big-endian.
    pub fn read_eui48(&mut self) -> Result<[u8; 6], SpinelError> {
        if self.offset + 6 > self.data.len() {
            return Err(SpinelError::Underflow);
        }
        let mut addr = [0u8; 6];
        addr.copy_from_slice(&self.data[self.offset..self.offset + 6]);
        self.offset += 6;
        Ok(addr)
    }

    /// Read exactly `len` bytes.
    pub fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], SpinelError> {
        if self.offset + len > self.data.len() {
            return Err(SpinelError::Underflow);
        }
        let slice = &self.data[self.offset..self.offset + len];
        self.offset += len;
        Ok(slice)
    }

    /// Read data with uint16 LE length prefix ("d" format).
    pub fn read_data_with_len(&mut self) -> Result<&'a [u8], SpinelError> {
        let len = self.read_uint16()? as usize;
        self.read_bytes(len)
    }

    /// Read a struct: uint16 LE length prefix + fields.
    pub fn read_struct(&mut self) -> Result<&'a [u8], SpinelError> {
        self.read_data_with_len()
    }

    /// Read UTF-8 string (NUL-terminated).
    pub fn read_utf8(&mut self) -> Result<String, SpinelError> {
        let start = self.offset;
        while self.offset < self.data.len() && self.data[self.offset] != 0 {
            self.offset += 1;
        }
        if self.offset >= self.data.len() {
            return Err(SpinelError::Underflow);
        }
        let s = std::str::from_utf8(&self.data[start..self.offset])
            .map_err(|_| SpinelError::InvalidUtf8)?;
        self.offset += 1; // skip NUL
        Ok(s.to_string())
    }

    /// Number of bytes remaining to read.
    pub fn remaining(&self) -> usize {
        self.data.len() - self.offset
    }

    /// Returns `true` if no bytes remain.
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_uint_single_byte() {
        let mut writer = PackWriter::new();
        writer.write_uint_packed(42);
        let data = writer.into_bytes();
        assert_eq!(data, vec![42]);
        let mut reader = PackReader::new(&data);
        assert_eq!(reader.read_uint_packed().unwrap(), 42);
    }

    #[test]
    fn packed_uint_two_bytes() {
        let mut writer = PackWriter::new();
        writer.write_uint_packed(200);
        let data = writer.into_bytes();
        assert_eq!(data, vec![0xC8, 0x01]);
        let mut reader = PackReader::new(&data);
        assert_eq!(reader.read_uint_packed().unwrap(), 200);
    }

    #[test]
    fn packed_uint_max_value() {
        let mut writer = PackWriter::new();
        writer.write_uint_packed(SPINEL_MAX_UINT_PACKED);
        let data = writer.into_bytes();
        assert_eq!(data.len(), 3);
        let mut reader = PackReader::new(&data);
        assert_eq!(reader.read_uint_packed().unwrap(), SPINEL_MAX_UINT_PACKED);
    }

    #[test]
    fn packed_uint_three_bytes() {
        let mut writer = PackWriter::new();
        writer.write_uint_packed(100_000);
        let data = writer.into_bytes();
        assert_eq!(data.len(), 3);
        let mut reader = PackReader::new(&data);
        assert_eq!(reader.read_uint_packed().unwrap(), 100_000);
    }

    #[test]
    fn packed_command_id() {
        let mut writer = PackWriter::new();
        writer.write_uint_packed(6);
        let data = writer.into_bytes();
        assert_eq!(data, vec![0x06]);
    }

    #[test]
    fn utf8_string_nul_terminated() {
        let mut writer = PackWriter::new();
        writer.write_utf8("Hello");
        let data = writer.into_bytes();
        assert_eq!(data, b"Hello\0");

        let mut reader = PackReader::new(&data);
        let s = reader.read_utf8().unwrap();
        assert_eq!(s, "Hello");
    }

    #[test]
    fn utf8_string_empty() {
        let mut writer = PackWriter::new();
        writer.write_utf8("");
        let data = writer.into_bytes();
        assert_eq!(data, b"\0");

        let mut reader = PackReader::new(&data);
        let s = reader.read_utf8().unwrap();
        assert_eq!(s, "");
    }

    #[test]
    fn struct_has_uint16_length_prefix() {
        let mut writer = PackWriter::new();
        let start = writer.write_struct_start();
        writer.write_uint8(0xAA);
        writer.write_uint16(0xBBCC);
        writer.write_struct_end(start);
        let data = writer.into_bytes();

        assert_eq!(data, vec![0x03, 0x00, 0xAA, 0xCC, 0xBB]);

        let mut reader = PackReader::new(&data);
        let content = reader.read_struct().unwrap();
        assert_eq!(content.len(), 3);

        let mut sub_reader = PackReader::new(content);
        assert_eq!(sub_reader.read_uint8().unwrap(), 0xAA);
        assert_eq!(sub_reader.read_uint16().unwrap(), 0xBBCC);
    }

    #[test]
    fn data_with_length_prefix() {
        let mut writer = PackWriter::new();
        writer.write_data_with_len(b"test");
        let data = writer.into_bytes();

        assert_eq!(data[0..2], [0x04, 0x00]);
        assert_eq!(&data[2..], b"test");

        let mut reader = PackReader::new(&data);
        let content = reader.read_data_with_len().unwrap();
        assert_eq!(content, b"test");
    }

    #[test]
    fn fixed_width_integers_round_trip() {
        let mut writer = PackWriter::new();
        writer.write_uint8(0xAB);
        writer.write_int8(-42);
        writer.write_uint16(0xCDEF);
        writer.write_int16(-1234);
        writer.write_uint32(0xDEADBEEF);
        writer.write_int32(-100000);
        writer.write_uint64(0x0102030405060708);
        writer.write_int64(-1);
        let data = writer.into_bytes();

        let mut reader = PackReader::new(&data);
        assert_eq!(reader.read_uint8().unwrap(), 0xAB);
        assert_eq!(reader.read_int8().unwrap(), -42);
        assert_eq!(reader.read_uint16().unwrap(), 0xCDEF);
        assert_eq!(reader.read_int16().unwrap(), -1234);
        assert_eq!(reader.read_uint32().unwrap(), 0xDEADBEEF);
        assert_eq!(reader.read_int32().unwrap(), -100000);
        assert_eq!(reader.read_uint64().unwrap(), 0x0102030405060708);
        assert_eq!(reader.read_int64().unwrap(), -1);
    }

    #[test]
    fn ipv6_round_trip() {
        let addr = [
            0x20, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01,
        ];
        let mut writer = PackWriter::new();
        writer.write_ipv6(&addr);
        let data = writer.into_bytes();

        let mut reader = PackReader::new(&data);
        assert_eq!(reader.read_ipv6().unwrap(), addr);
    }

    #[test]
    fn eui64_round_trip() {
        let addr = [0x00, 0x12, 0x4B, 0x00, 0x14, 0xF7, 0xD2, 0xE6];
        let mut writer = PackWriter::new();
        writer.write_eui64(&addr);
        let data = writer.into_bytes();

        let mut reader = PackReader::new(&data);
        assert_eq!(reader.read_eui64().unwrap(), addr);
    }

    #[test]
    fn eui48_round_trip() {
        let addr = [0x02, 0x00, 0x12, 0x4B, 0x00, 0x14];
        let mut writer = PackWriter::new();
        writer.write_eui48(&addr);
        let data = writer.into_bytes();
        assert_eq!(data.len(), 6);

        let mut reader = PackReader::new(&data);
        assert_eq!(reader.read_eui48().unwrap(), addr);
    }

    #[test]
    fn underflow_errors() {
        let data = [0x01];
        let mut reader = PackReader::new(&data);
        assert_eq!(reader.read_uint16(), Err(SpinelError::Underflow));
        assert_eq!(reader.read_uint32(), Err(SpinelError::Underflow));
        assert_eq!(reader.read_uint64(), Err(SpinelError::Underflow));
    }
}
