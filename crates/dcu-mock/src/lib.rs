//! `dcu-mock` — Mock NCP for integration testing.
//!
//! Emulates a TI Wi-SUN NCP over an in-memory `DuplexStream` (or optionally
//! a real PTY) so tests can run without physical hardware.
//!
//! Mock NCP that simulates the Spinel firmware over a PTY so tests can run
//! without physical hardware.

pub mod builder;
pub mod config;
pub mod failure;
pub mod mock_ncp;
pub mod pty_transport;
pub mod scenarios;
pub mod topology;
