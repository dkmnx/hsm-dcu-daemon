//! `dcu-mock` — Mock NCP for integration testing.
//!
//! Emulates a TI Wi-SUN NCP over an in-memory `DuplexStream` (or optionally
//! a real PTY) so tests can run without physical hardware.
//!
//! See `doc/rust-porting/phase-4A-mock-ncp.md` for the full design spec.

pub mod builder;
pub mod config;
pub mod failure;
pub mod mock_ncp;
pub mod pty_transport;
pub mod scenarios;
pub mod topology;
