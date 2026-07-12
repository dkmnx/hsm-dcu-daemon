//! Shared test infrastructure for phase-4B integration tests.
//!
//! [`TestDaemon`] spins up a mock NCP, an `NcpInstance`, and wires them
//! together over an in-memory `DuplexTransport`. Tests send commands via
//! `command_sender()` and read state via `shared_state()`.

pub mod daemon_test;
