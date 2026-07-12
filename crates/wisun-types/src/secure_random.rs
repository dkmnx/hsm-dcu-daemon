//! Cryptographically secure random byte generation.
//!
//! Replaces C's `sec_random_init()` / `sec_random_fill()` from
//! `src/util/sec-random.c`. Uses `getrandom` which reads from
//! `/dev/urandom` (or the platform's equivalent).

/// Fill a buffer with cryptographically secure random bytes.
///
/// Equivalent to C's `sec_random_fill(buffer, length)`.
pub fn secure_random_fill(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    getrandom::getrandom(buf)
}

/// Generate a random u16 (convenience for PANID, XPANID).
pub fn random_u16() -> Result<u16, getrandom::Error> {
    let mut buf = [0u8; 2];
    secure_random_fill(&mut buf)?;
    Ok(u16::from_be_bytes(buf))
}

/// Generate random bytes for a network key (16 bytes).
pub fn random_key() -> Result<[u8; 16], getrandom::Error> {
    let mut buf = [0u8; 16];
    secure_random_fill(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secure_random_fill_works() {
        let mut buf = [0u8; 32];
        secure_random_fill(&mut buf).unwrap();
        // Verify non-zero (extremely unlikely all zeros from urandom)
        assert!(buf.iter().any(|&b| b != 0));
    }

    #[test]
    fn random_u16_works() {
        let val = random_u16().unwrap();
        let _ = val;
    }

    #[test]
    fn random_key_length() {
        let key = random_key().unwrap();
        assert_eq!(key.len(), 16);
    }
}
