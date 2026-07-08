#![no_main]
//! Fuzz target: decode arbitrary bytes as a Spinel frame, then re-encode the
//! result (if decode succeeded) and assert round-trip stability. Must never
//! panic for any input.
//!
//! Run with `cargo +nightly fuzz run spinel_frame_fuzz` (requires cargo-fuzz).

use libfuzzer_sys::fuzz_target;
use spinel::frame::SpinelFrame;

fuzz_target!(|data: &[u8]| {
    // Decoding must never panic; it either returns a frame or an error.
    if let Ok(frame) = SpinelFrame::decode(data) {
        // Re-encode and ensure the encoded bytes decode back to the same frame.
        let encoded = frame.encode();
        if let Ok(round) = SpinelFrame::decode(&encoded) {
            assert_eq!(round.header, frame.header);
            assert_eq!(round.command_id, frame.command_id);
            assert_eq!(round.payload, frame.payload);
        }
    }
});
