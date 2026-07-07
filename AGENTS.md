# AGENTS.md

## Commands

### C/C++ (autotools)

```bash
./bootstrap.sh && ./configure && make -j$(nproc)
sudo make install    # requires sudo for /usr/local
```

### Rust (wisun-types crate, phase 1A of porting)

```bash
cd wisun-types && cargo test && cargo clippy
cd wisun-types && cargo fmt --check
```

### CI (GitHub Actions)

```bash
./bootstrap.sh && ./configure && sudo make -j2
sudo make install
```

## Code Style

- **C/C++**: Uncrustify enforced (`.uncrustify.cfg`).
  Do not manually format C code.
- **Rust**: `cargo fmt` + `cargo clippy -- -D warnings`
  enforced via `[lints]` in `wisun-types/Cargo.toml`.
- **JS (webapp only)**: ESLint + Prettier in
  `ti-wisun-webapp/`. Webapp is NOT being ported.
- Property key constants must match D-Bus wire format
  exactly (case-sensitive).

## Architecture

TI Wi-SUN FAN Border Router daemon derived from Nest's
`wpantund`. Single-threaded, async I/O, plugin-based
NCP communication.

```text
src/dcud/           Main daemon binary
src/ncp-spinel/     Spinel protocol plugin
src/dcuctl/         CLI control tool
src/ipc-dbus/       D-Bus API server
src/ncp-dummy/      Template for new NCP plugins
src/util/           Serial/socket utilities
wisun-types/        Rust port: foundational types
```

See `doc/rust-porting/README.md` for the full Rust
porting plan, crate architecture, and dependency map.

## Testing

- C: `strlcpy_test`/`strlcat_test` + 2 fuzz harnesses
  under `etc/fuzz-corpus/`
- Rust: `cargo test` in `wisun-types/` — 41 tests
  covering round-trip conversions, error codes,
  property keys
- Integration: OpenThread `toranj` tests (requires NCP
  hardware or mock)
- No hardware dependency for Rust CI (all types are
  pure data, no I/O)

## Boundaries

- `ti-wisun-webapp/` — Node.js webapp, not part of
  the Rust port.
- `connman-plugin/` — optional, deferred from porting
- `third_party/openthread/` — vendored OpenThread,
  do not modify
- `wisun-types/target/` — Rust build artifacts,
  gitignored. Never commit.
- NCP wire protocol (Spinel) must stay
  binary-compatible with TI CC13xx firmware

## Patterns

- **Rust port phases**: 10 phases from `wisun-types`
  → `spinel` → `dcu-tun`/`dcu-serial` → `dcu-dbus`/
  `dcuctl` → `dcu-daemon` core → tasks → mock → e2e.
  See `doc/rust-porting/` for per-phase specs.
- **Property key constants**: Defined once via
  `declare_property_keys!` macro in
  `wisun-types/src/property_key.rs`. Add new properties
  in the macro invocation, not separately.
- **No unsafe in Rust crates** except `dcu-tun`
  (ioctl) and `dcu-serial` (serial port) — enforced
  by `[lints]` section.
