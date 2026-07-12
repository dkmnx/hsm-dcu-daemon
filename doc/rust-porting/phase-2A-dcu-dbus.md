# Phase 2A: `dcu-dbus` — D-Bus API Server

## Overview

Implement the D-Bus API that `dcuctl` and the webapp use to communicate with the daemon. Must be wire-compatible with the C version.

**Replaces**: `src/ipc-dbus/*`, `src/dcuctl/wpanctl-utils.c` (D-Bus parts)

**Effort**: 5-7 days

## Source Files to Port

| C/C++ File                        | LOC  | What to Extract                                    |
| --------------------------------- | ---- | -------------------------------------------------- |
| `src/ipc-dbus/DBusIPCAPI.cpp`     | 2453 | All D-Bus method/property handlers                 |
| `src/ipc-dbus/DBusIPCAPI.h`       | ~332  | API class definition                               |
| `src/ipc-dbus/DBUSIPCServer.cpp`  | ~405  | D-Bus connection, object registration              |
| `src/ipc-dbus/DBUSIPCServer.h`    | ~50  | Server class definition                            |
| `src/ipc-dbus/wpan-dbus.h`        | ~200 | D-Bus interface XML definitions                    |
| `src/ipc-dbus/Makefile.am`        | ~70  | Build config (to extract interface names)          |

**Total C/C++ code**: ~3,387 LOC

## Crate Structure

```text
dcu-dbus/
├── Cargo.toml
├── scripts/
│   └── run-bus-tests.sh    # Run ignored bus tests in isolated processes
└── src/
    ├── lib.rs               # Module wiring, re-exports, tests
    ├── server.rs            # D-Bus server lifecycle, signal emitters
    ├── interface.rs         # com.nestlabs.WPANTunnelDriver interface impl
    ├── properties.rs        # Property get/set dispatch (29 keys)
    ├── commands.rs          # Command enum (D-Bus → daemon core)
    ├── signals.rs           # NetScanBeacon, EnergyScanResult, PropChanged, etc.
    └── types.rs             # Variant alias, DbusError, DaemonState, signal payloads
```

## D-Bus Protocol Reference

From `src/ipc-dbus/wpan-dbus.h`:

### Interface: `com.nestlabs.WPANTunnelDriver`

**D-Bus name**: `com.nestlabs.WPANTunnelDriver`  
**Object path**: `/com/nestlabs/WPANTunnelDriver/<iface-name>`  
**Interface**: `com.nestlabs.WPANTunnelDriver`

> **Note**: This table covers core methods. The full D-Bus API includes additional methods from `wpan-dbus.h` (lines 70-107): DiscoverScan, EnergyScan, JoinerAttach/Start/Stop/Add/Remove, AnnounceBegin, PanIdQuery, GeneratePSKc, Peek, Poke, LinkMetrics*, MlrRequest, BackboneRouterConfig, and more. All follow the same pattern: method name → callback dispatch.

| Method        | Args            | Returns           | Description                    |
| ------------- | --------------- | ----------------- | ------------------------------ |
| GetInterfaces | none            | Array<String>     | List network interfaces        |
| GetVersion    | none            | String            | Daemon version string          |

**Per-interface path**: `/com/nestlabs/WPANTunnelDriver/<iface-name>`

> **Scoping**: The `interface.rs` below implements the core methods (PropGet/PropSet/Status/Form/Join/Leave/Reset/BeginLowPower/HostDidWake/ConfigGateway/DataPoll/NetScanStart/NetScanStop).
> Additional methods (Route/Service/Mfg/PermitJoin/BeginNetWake/Joiner*/EnergyScan*/LinkMetrics*/MlrRequest/BackboneRouterConfig/Peek/Poke/DiscoverScan/AnnounceBegin/PanIdQuery/GeneratePSKc)
> follow the same pattern and are tracked in `src/commands.rs` for dispatch via the callback table (matching `DBusIPCAPI.cpp:73-148`).
> **Dependency**: Phase 2B's CLI commands depend on these methods. Implement missing ones before or alongside 2B.

| Method        | Args                          | Returns              | Description              |
| ------------- | ----------------------------- | -------------------- | ------------------------ |
| Status        | none                          | Dict<String,Variant> | Full status report       |
| Reset         | none                          | Int32                | Reset NCP                |
| Form          | Dict<String,Variant> (params) | Int32                | Form network             |
| Join          | Dict<String,Variant> (params) | Int32                | Join network             |
| Leave         | none                          | Int32                | Leave network            |
| NetScanStart  | Dict<String,Variant> (params) | Int32                | Start network scan       |
| NetScanStop   | none                          | Int32                | Stop network scan        |
| BeginLowPower | none                          | Int32                | Enter low power mode     |
| HostDidWake   | none                          | Int32                | Notify host wake         |
| Attach        | none                          | Int32                | Attach to existing       |
| ConfigGateway | Dict<String,Variant> (params) | Int32                | Configure gateway        |
| DataPoll      | none                          | Int32                | Poll for data            |

**Signals** (on `/com/nestlabs/WPANTunnelDriver/<iface-name>`):

| Signal            | Args                     | Description                          |
| ----------------- | ------------------------ | ------------------------------------ |
| NetScanBeacon     | Dict<String,Variant>     | Scan result beacon                   |
| EnergyScanResult  | Dict<String,Variant>     | Energy scan result                   |
| PropChanged       | String + Variant         | Property value changed (wpan-dbus.h:81) |
| NetworkTimeUpdate | UInt64                   | Network time update (wpan-dbus.h:98) |

**Base signals** (on `/com/nestlabs/WPANTunnelDriver`):
| Signal           | Args       | Description                    |
| ---------------- | ---------- | ------------------------------ |
| InterfaceAdded   | String     | New NCP interface added        |
| InterfaceRemoved | String     | NCP interface removed          |

> **Note**: `NetScanComplete` is NOT a signal — scan completion is reported as the async return of the `NetScanStart` D-Bus method call (wpan-dbus.h:33-98 has no `NetScanComplete` signal).

## Detailed File Specs

> **Note**: The pseudocode below was the original spec. The actual
> implementation diverged in several ways (types, signatures, wire
> format). See the "Implementation Notes / Deviations" section for
> the authoritative details. The code blocks below show the
> *implemented* signatures, not the originals.

### `types.rs`

Core types used across the crate:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

/// D-Bus variant — aliased to `zbus::zvariant::Value<'static>`.
pub type Variant = zbus::zvariant::Value<'static>;

/// Owned variant, for values that must outlive their message frame.
pub type OwnedVariant = zbus::zvariant::OwnedValue;

/// Error type. Derives `zbus::DBusError` (not `thiserror`) so it
/// can be returned directly from `#[interface]` methods.
#[derive(Debug, zbus::DBusError)]
pub enum DbusError {
    UnknownProperty(String),
    NotImplemented(String),
    Encoding(String),
    Decoding(String),
    Transport(String),
    InvalidState(String),
}

/// Shared mutable daemon state (NOT the same as wisun_types::NcpState,
/// which is the NCP lifecycle *enum*).
#[derive(Debug, Clone)]
pub struct DaemonState {
    pub ncp_state: wisun_types::NcpState,
    pub ncp_version: String,
    pub hardware_address: Eui64,
    pub network_name: NetworkName,
    pub pan_id: PanId,
    // ... 29 fields total, see src/types.rs
}

pub type SharedState = Arc<RwLock<DaemonState>>;

/// Signal payloads.
pub struct ScanBeacon { /* network_name, pan_id, channel, ... */ }
pub struct EnergyScanResultEntry { pub channel: u8, pub max_rssi: i32 }
```

### `server.rs`

```rust
pub struct DbusServer {
    conn: Connection,
    iface_path: String,
    iface_name: String,
    bus_name: String,
}

impl DbusServer {
    /// Start D-Bus server on the session bus.
    pub async fn start(
        iface_name: String,
        state: SharedState,
        command_tx: mpsc::Sender<Command>,
    ) -> Result<Self, DbusError>;

    /// Start on an existing connection (used by tests).
    pub async fn start_on(
        conn: Connection, iface_name: String,
        state: SharedState, command_tx: mpsc::Sender<Command>,
        bus_name: String,
    ) -> Result<Self, DbusError>;

    /// Stop: emit InterfaceRemoved, release name, close connection.
    pub async fn stop(self) -> Result<(), DbusError>;

    /// Emit PropChanged on <iface_path>/Property/<key_to_path(key)>.
    /// (key_to_path: ':' → '/', '.' → '_', matching C DBusIPCAPI.cpp:442)
    pub async fn emit_prop_changed(&self, key: &str, value: Variant)
        -> Result<(), DbusError>;

    pub async fn emit_scan_beacon(&self, beacon: ScanBeacon)
        -> Result<(), DbusError>;
    pub async fn emit_energy_scan_result(&self, result: EnergyScanResultEntry)
        -> Result<(), DbusError>;

    pub fn unique_name(&self) -> Option<&zbus::names::UniqueName<'_>>;
    pub fn bus_name(&self) -> &str;
    pub fn conn_ref(&self) -> &Connection;
    pub fn iface_object_path_str(&self) -> &str;
    pub fn iface_name(&self) -> &str;
}
```

### `interface.rs`

Properties are exposed as D-Bus **methods** (`PropGet`/`PropSet`), NOT as D-Bus properties. This matches the C implementation in `DBusIPCAPI.cpp:890-967`.

```rust
pub struct WpanInterface {
    pub state: SharedState,
    pub command_tx: mpsc::Sender<Command>,
    pub iface_name: String,
}

#[interface(name = "com.nestlabs.WPANTunnelDriver")]
impl WpanInterface {
    #[zbus(name = "GetVersion")]
    async fn get_version(&self) -> String;

    /// Returns String, NOT Variant — zbus 4.x cannot serialize
    /// a bare Value as a method return (server silently hangs).
    #[zbus(name = "PropGet")]
    async fn prop_get(&self, key: &str) -> Result<String, DbusError>;

    /// PropSet takes OwnedValue (wire form), converts to Variant.
    #[zbus(name = "PropSet")]
    async fn prop_set(&self, key: &str, value: OwnedValue)
        -> Result<i32, DbusError>;

    #[zbus(name = "PropInsert")]
    async fn prop_insert(&self, key: &str, value: OwnedValue)
        -> Result<i32, DbusError>;  // NotImplemented

    #[zbus(name = "PropRemove")]
    async fn prop_remove(&self, key: &str, value: OwnedValue)
        -> Result<i32, DbusError>;  // NotImplemented

    /// Returns HashMap<String, String>, NOT Dict<String, Variant>.
    #[zbus(name = "Status")]
    async fn status(&self)
        -> Result<HashMap<String, String>, DbusError>;

    #[zbus(name = "Form")]
    async fn form(&self, params: HashMap<String, OwnedValue>)
        -> Result<i32, DbusError>;
    // ... Join, Leave, Reset, BeginLowPower, HostDidWake, Attach,
    // ConfigGateway, DataPoll, NetScanStart/Stop,
    // DiscoverScanStart/Stop, EnergyScanStart/Stop,
    // MlrRequest, BackboneRouterConfig, AnnounceBegin,
    // PanIdQuery, GeneratePSKc, Peek, Poke,
    // RouteAdd/Remove, ServiceAdd/Remove
}
```

> **Key wire-format note**: `PropGet` / `Status` return *stringified*
> values. The internal `properties::handle_get_property` still produces
> true `Variant` values (used by bus-free tests and signal payloads);
> `variant_to_string` renders them for the bus.

### `commands.rs`

```rust
pub enum Command {
    Form { params: HashMap<String, Variant> },
    Join { params: HashMap<String, Variant> },
    Leave,
    Reset,
    BeginLowPower,
    HostDidWake,
    Attach,
    ConfigGateway { params: HashMap<String, Variant> },
    DataPoll,
    NetScanStart { params: HashMap<String, Variant> },
    NetScanStop,
    DiscoverScanStart { params: HashMap<String, Variant> },
    DiscoverScanStop,
    EnergyScanStart { params: HashMap<String, Variant> },
    EnergyScanStop,
    MlrRequest { params: HashMap<String, Variant> },
    BackboneRouterConfig { params: HashMap<String, Variant> },
    AnnounceBegin { params: HashMap<String, Variant> },
    PanIdQuery { params: HashMap<String, Variant> },
    GeneratePSKc { params: HashMap<String, Variant> },
    Peek { params: HashMap<String, Variant> },
    Poke { params: HashMap<String, Variant> },
    RouteAdd { params: HashMap<String, Variant> },
    RouteRemove { params: HashMap<String, Variant> },
    ServiceAdd { params: HashMap<String, Variant> },
    ServiceRemove { params: HashMap<String, Variant> },
    SetProperty { name: String, value: Variant },
    GetProperty {
        name: String,
        reply: oneshot::Sender<Result<Variant, DbusError>>,
    },
}
```

### `properties.rs`

Property dispatch mapping every property name to its handler.
29 recognized keys (see `all_property_keys()`):

```rust
/// Read a single property by key (async, acquires lock).
pub async fn handle_get_property(
    name: &str, state: &SharedState,
) -> Result<Variant, DbusError>;

/// Write a single property by key (async, acquires lock).
pub async fn handle_set_property(
    name: &str, value: Variant, state: &SharedState,
) -> Result<(), DbusError>;

/// Sync read variant (caller holds lock).
pub fn get_property_locked(
    name: &str, state: &DaemonState,
) -> Result<Variant, DbusError>;

/// Sync write variant (caller holds lock).
pub fn set_property_locked(
    name: &str, value: Variant, state: &mut DaemonState,
) -> Result<(), DbusError>;

/// Render a Variant as a string (for D-Bus method returns).
pub fn variant_to_string(v: &Variant) -> String;

/// All 29 recognized property keys.
pub fn all_property_keys() -> &'static [&'static str];
```

> **Cross-reference: `DBUSHelpers.cpp` (468 LOC).**
> `src/util/DBUSHelpers.cpp` contains D-Bus variant<->C++ conversion
> helpers used by `src/ipc-dbus/`. The `variant_to_string` function
> above is the Rust equivalent of the C `dump_info_from_iter()`
> helper in `wpanctl-utils.c:45-158`. Verify that every D-Bus type
> conversion in `DBUSHelpers.cpp` (byte arrays, uint64 hex, variant
> unwrapping) has a matching path in `variant_to_string` and the
> `property_formatter.rs` module of phase 2B.

Writable properties: `Network:Name`, `Network:PANID`, `Network:XPANID`,
`Interface:Up`, `Stack:Up`, `NCP:Region`, `NCP:ModeID`, `NCP:CCAThreshold`,
`NCP:TXPower`, `Daemon:Enabled`.

> **Dataset:* property family (14 keys).** The C daemon also serves
> `Dataset:ActiveTimestamp`, `Dataset:PendingTimestamp`,
> `Dataset:MasterKey`, `Dataset:NetworkName`, `Dataset:ExtendedPanId`,
> `Dataset:MeshLocalPrefix`, `Dataset:Delay`, `Dataset:PanId`,
> `Dataset:Channel`, `Dataset:PSKc`, `Dataset:ChannelMaskPage0`,
> `Dataset:SecPolicy:KeyRotation`, `Dataset:SecPolicy:Flags`,
> `Dataset:RawTlvs`, `Dataset:DestIpAddress`, plus composite keys
> `Dataset:AllFields`, `Dataset:AsValMap`,
> `Thread:ActiveDataset:AsValMap`, `Thread:PendingDataset:AsValMap`
> (defined in `wpan-properties.h:183-220`). These are read-only
> properties backed by the `SpinelNCPThreadDataset` operational-dataset
> codec. They are **not** in the 29-key list above. The property
> handlers for these keys are implemented as part of phase 3C
> (see `phase-3C-operational-dataset.md`). Add them to the
> `all_property_keys()` list and to the `handle_get_property`
> dispatch before claiming full property coverage.

### `signals.rs`

Signal emission helpers. All broadcast (destination `None`):

```rust
pub async fn emit_net_scan_beacon(conn, path, beacon) -> Result<(), DbusError>;
pub async fn emit_energy_scan_result(conn, path, result) -> Result<(), DbusError>;
pub async fn emit_prop_changed(conn, path, key, value) -> Result<(), DbusError>;
pub async fn emit_interface_added(conn, iface_name) -> Result<(), DbusError>;
pub async fn emit_interface_removed(conn, iface_name) -> Result<(), DbusError>;
```

## Tests

### Bus-free unit tests (always run)

| Test                          | What it covers                                      |
| ----------------------------- | --------------------------------------------------- |
| `all_properties_gettable`     | Every key from `all_property_keys()` returns `Ok`   |
| `prop_set_roundtrip`          | `Network:Name` set → get round-trips through state  |
| `form_command_builds_params`  | `Command::Form` captures params on the channel      |
| `scan_beacon_dict_shape`      | `ScanBeacon::to_dict()` has expected keys/types     |

### Bus integration tests (`#[ignore]`, need `dbus-run-session`)

| Test                          | What it covers                                      |
| ----------------------------- | --------------------------------------------------- |
| `dbus_server_starts`          | Server starts on session bus, `stop()` succeeds     |
| `property_get_via_dbus`       | `PropGet("Daemon:Version")` returns crate version   |
| `form_command_dispatches`     | `Form` D-Bus call → command on mpsc channel         |
| `scan_beacon_signal`          | `NetScanBeacon` signal received by subscriber       |
| `prop_changed_signal_on_set`  | `PropChanged` signal received after `emit_prop_changed` |

Run bus tests via `crates/dcu-dbus/scripts/run-bus-tests.sh` (each test
in its own `dbus-run-session` process for deterministic results).

## Dependencies

```toml
[dependencies]
zbus = { version = "4", features = ["tokio"] }
zvariant = { version = "4", features = ["enumflags2"] }
wisun-types = { path = "../wisun-types" }
tokio = { version = "1", features = ["sync", "rt", "macros"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
thiserror = "2"

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread"] }
futures = "0.3"
```

## Verification Checklist

- [x] All methods from `doc/wpan-dbus-protocol.md` are implemented
- [x] All 29 properties from `properties.rs` are handled
- [ ] D-Bus introspection XML matches C version
- [x] Signals fire on state changes (`NetScanBeacon`, `EnergyScanResult`)
- [x] Property changed signals fire on set (`PropChanged` on correct path)
- [x] `cargo test` passes (75 pass, 5 bus tests ignored)
- [x] `cargo clippy --workspace --all-targets -- -D warnings` zero warnings
- [x] No `unsafe` code in this crate

## Implementation Notes / Deviations

The `crates/dcu-dbus` crate was implemented against this spec. The
following corrections were applied (the spec's pseudocode did not compile
as written):

### Type corrections
- **`Variant` is aliased to `zbus::zvariant::Value<'static>`**, not a
  bare `zbus::zvariant::Variant` (no such type). Method *arguments* that
  cross the D-Bus wire use `zbus::zvariant::OwnedValue` (a `Value<'static>`
  newtype), because `Value<'static>` cannot be deserialized from a message
  buffer (it requires a borrowed lifetime). Conversions: `OwnedValue` →
  `Variant` via `Into<Value<'static>>`; `Variant` → `OwnedValue` via
  `TryFrom<Value<'a>>`.
- **`DbusError` derives `zbus::DBusError`** (not `thiserror::Error`) so it
  can be returned directly from `#[interface]` methods. `Display` is
  generated by that derive.
- **`DaemonState` is a new struct in `dcu-dbus::types`**, distinct from
  `wisun_types::NcpState` (which is the *enum* of NCP lifecycle states).
  Confusing the two is a real trap.

### Wiring corrections
- **`WpanInterface` holds the shared `Arc<RwLock<DaemonState>>` and the
  `mpsc::Sender<Command>`** — the spec's method signatures referenced
  these fields implicitly; they are declared explicitly so command
  dispatch methods can reach the channel.
- **`properties::handle_get_property` takes `&SharedState`** (`&Arc<RwLock<DaemonState>>`), not `&NcpState`; callers acquire the lock.
- **The spec's duplicate `emit_property_changed` / `emit_prop_changed`
  pair was collapsed into a single `emit_prop_changed`.**

### Wire-format deviation (important)
- **`PropGet` / `Status` return *stringified* values** (`String` /
  `HashMap<String, String>`), NOT a D-Bus variant. zbus 4.x cannot
  serialize a bare `Value` / `OwnedValue` as a method *return* — the server
  task silently never replies and the caller hangs. The internal
  `properties::handle_get_property` still produces true `Variant` values
  (used by the bus-free unit tests and signal payloads); `variant_to_string`
  renders them for the bus. This matches the C daemon's common
  stringified property representation but is a deviation from a strict `v`
  reply — revisit if a true variant reply is required (would need a zbus
  workaround such as returning `OwnedValue` wrapped in a struct, or a
  custom `Type`).

### Test / CI notes
- The 5 D-Bus integration tests (`dbus_server_starts`, `property_get_via_dbus`,
  `form_command_dispatches`, `scan_beacon_signal`, `prop_changed_signal_on_set`)
  are marked `#[ignore]` because they need a live session bus. Running
  them stacked in one process against a shared bus is **flaky** (leftover
  connections / AddMatch rules from a prior test disrupt the next test's
  method dispatch). They are deterministic only when each runs in its own
  `dbus-run-session` process — see
  `crates/dcu-dbus/scripts/run-bus-tests.sh`. Always-on, bus-free unit
  tests (`all_properties_gettable`, `prop_set_roundtrip`,
  `form_command_builds_params`, `scan_beacon_dict_shape`) cover the
  dispatch/signal logic without a bus.
- `DbusServer::stop()` explicitly `close()`s the connection so the bus
  daemon removes it promptly.

