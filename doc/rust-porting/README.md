# Rust Porting Plan: hsm-dcu-daemon

## Overview

Port `dcud` (Wi-SUN FAN Border Router daemon) and `dcuctl` (CLI tool) from C/C++ to Rust.
~59,000 LOC across 220 source files. Existing tests: `strlcpy_test`/`strlcat_test` (portability shims) and 2 fuzz harnesses (`wpantund-fuzz`, `ncp-spinel-fuzz`). Derived from Nest's `wpantund`.

## Goals

- Memory safety, fearless concurrency, proper error handling
- Maintain wire-protocol compatibility with existing TI NCP firmware
- Maintain D-Bus API compatibility (dcuctl and webapp must keep working)
- Testable from day one — unit tests, integration tests, property-based tests
- No hardware dependency for CI (mock NCP for testing)

## Non-Goals

- Port `ti-wisun-webapp` (already Node.js, separate concern)
- Port `connman-plugin` (optional, defer)
- Change the Spinel wire protocol
- Change D-Bus interface names or property names

## Crate Architecture

```text
wisun-dcu/                    (workspace root)
├── Cargo.toml                (workspace)
├── crates/
│   ├── spinel/               (Spinel protocol types + encode/decode)
│   ├── wisun-types/          (NCP state, property keys, constants)
│   ├── dcu-tun/              (TUN interface management)
│   ├── dcu-serial/           (UART/serial transport)
│   ├── dcu-dbus/             (D-Bus API server)
│   ├── dcu-daemon/           (main daemon binary)
│   ├── dcuctl/               (CLI binary)
│   └── dcu-mock/             (mock NCP for testing)
└── doc/
    └── rust-porting/         (this plan)
```

## Phases

| Phase   | File                                                               | Crate(s)             | Duration   | Status                    |
| ------- | ------------------------------------------------------------------ | -------------------- | ---------- | ------------------------- |
| 1A      | [phase-1A-wisun-types.md](phase-1A-wisun-types.md)                 | `wisun-types`        | 2-3 days   | Done (154db77)            |
| 1B      | [phase-1B-spinel.md](phase-1B-spinel.md)                           | `spinel`             | 5-7 days   | Done (0d2c491)            |
| 1C      | [phase-1C-dcu-tun.md](phase-1C-dcu-tun.md)                         | `dcu-tun`            | 3-4 days   | Done (e0153e9)            |
| 1D      | [phase-1D-dcu-serial.md](phase-1D-dcu-serial.md)                   | `dcu-serial`         | 3-4 days   | Done (2d24fe3)            |
| 2A      | [phase-2A-dcu-dbus.md](phase-2A-dcu-dbus.md)                       | `dcu-dbus`           | 5-7 days   | Done (fd357ce)            |
| 2B      | [phase-2B-dcuctl.md](phase-2B-dcuctl.md)                           | `dcuctl`             | 5-7 days   | Done (303ef83)            |
| 3A      | [phase-3A-daemon-core.md](phase-3A-daemon-core.md)                 | `dcu-daemon`         | 14-21 days | Done (2d24fe3)            |
| 3B      | [phase-3B-spinel-tasks.md](phase-3B-spinel-tasks.md)               | `dcu-daemon` tasks   | 7-10 days  | Done (ffc8b81)            |
| 3C      | [phase-3C-operational-dataset.md](phase-3C-operational-dataset.md) | `dcu-daemon` dataset | 3-4 days   | Implemented (uncommitted) |
| 4A      | [phase-4A-mock-ncp.md](phase-4A-mock-ncp.md)                       | `dcu-mock`           | 3-5 days   | Done (d1b8e80)            |
| 4B      | [phase-4B-e2e-tests.md](phase-4B-e2e-tests.md)                     | Integration tests    | 3-5 days   | Done (91fa0c0)            |

**Total: ~17 weeks (estimate — the critical-path daemon core alone is 14-21 days)**

> **Schedule risk**: LOC-based estimates for C→Rust translation vary widely. The phase-3A daemon core (protothread→async) is the single biggest unknown. Consider a spike week early in the project to validate the async conversion approach on a representative protothread sample before committing the full schedule.

## Dependency Map

```text
wisun-types ─────────────────────────────────────┐
    │                                            │
    ├── spinel                                   │
    │    │                                       │
    │    ├── dcu-serial                          │
    │    │    │                                  │
    │    │    └── dcu-daemon ──── dcu-dbus ── dcuctl
    │    │         │
    │    │         ├── dcu-tun
    │    │         └── dcu-mock (dev-dependency)
    │    │
    │    └── dcu-mock
    │
    └── dcu-dbus
```

## External Crates

| Crate          | Version   | Purpose                                                      |
| -------------- | --------- | ------------------------------------------------------------ |
| `tokio`        | 1.x       | Async runtime                                                |
| `tokio-serial` | 5.x       | Async serial port                                            |
| `tokio-util`   | 0.7.x     | CancellationToken, time utilities                            |
| `zbus`         | 4.x       | D-Bus (pure Rust)                                            |
| `clap`         | 4.x       | CLI argument parsing                                         |
| `rustyline`    | 14.x      | REPL (may be optional; C dcuctl also supports one-shot mode) |
| `nix`          | 0.29.x    | Unix ioctls, signals, fork                                   |
| `bytes`        | 1.x       | Buffer management                                            |
| `tracing`      | 0.1.x     | Structured logging (replaces syslog)                         |
| `serde`        | 1.x       | Config serialization (for debug/mock, NOT for wpantund.conf) |
| `hex`          | 0.4.x     | Hex encoding for EUI-64, addresses                           |
| `criterion`    | 0.5.x     | Benchmarks (used by crate benches/)                          |
| `thiserror`    | 2.x       | Error boilerplate (used by every crate)                      |
| `portable-pty` | 0.8.x     | PTY for mock NCP (dev-dependency)                            |

## Success Criteria

1. `dcuctl status` output matches C version character-for-character
2. `dcuctl get <property>` returns identical values for all 40+ properties listed in `ti_wisun_commands.md`
3. Daemon forms a Wi-SUN network with mock NCP
4. Daemon joins an existing Wi-SUN network with mock NCP
5. All D-Bus signals fire correctly (NetScanBeacon, state changes)
6. CI runs `cargo test --workspace` with zero failures
7. `cargo clippy` produces zero warnings
8. Fuzz targets run for 60+ seconds with no crashes
9. `Cargo.toml` has `[profile.release]` with `strip = true`; binary size < 6 MB (tokio+zbus+clap unstripped ~5 MB; stripped ~3 MB)
10. No `unsafe` blocks outside of `dcu-tun` and `dcu-serial` (ioctl/serial only)
