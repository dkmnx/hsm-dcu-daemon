# Phase 2A: `dcu-dbus` — D-Bus API Server

## Overview

Implement the D-Bus API that `dcuctl` and the webapp use to communicate with the daemon. Must be wire-compatible with the C version.

**Replaces**: `src/ipc-dbus/*`, `src/dcuctl/wpanctl-utils.c` (D-Bus parts)

**Effort**: 5-7 days

## Source Files to Port

| C/C++ File                        | LOC  | What to Extract                                    |
| --------------------------------- | ---- | -------------------------------------------------- |
| `src/ipc-dbus/DBusIPCAPI.cpp`     | 2453 | All D-Bus method/property handlers                 |
| `src/ipc-dbus/DBusIPCAPI.h`       | ~80  | API class definition                               |
| `src/ipc-dbus/DBUSIPCServer.cpp`  | ~600 | D-Bus connection, object registration              |
| `src/ipc-dbus/DBUSIPCServer.h`    | ~50  | Server class definition                            |
| `src/ipc-dbus/wpan-dbus.h`        | ~200 | D-Bus interface XML definitions                    |
| `src/ipc-dbus/Makefile.am`        | ~70  | Build config (to extract interface names)          |

**Total C/C++ code**: ~3,453 LOC

## Crate Structure

```text
dcu-dbus/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── server.rs           # D-Bus server lifecycle
    ├── interface.rs        # com.nestlabs.WPANTunnelDriver interface implementation
    ├── properties.rs       # Property get/set dispatch
    ├── commands.rs         # Form, Join, Leave, Reset, Status, etc.
    ├── signals.rs          # NetScanBeacon, EnergyScanResult, PropChanged, etc.
    └── types.rs            # D-Bus type conversions
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

### `server.rs`

```rust
use zbus::connection::Builder;

pub struct DbusServer {
    // zbus server handles connection lifecycle
}

impl DbusServer {
    /// Start D-Bus server with the wpantund interface.
    pub async fn start(
        iface_name: String,
        state: Arc<RwLock<NcpState>>,
        command_tx: mpsc::Sender<Command>,
    ) -> Result<Self, DbusError>;

    /// Stop the D-Bus server.
    pub async fn stop(self) -> Result<(), DbusError>;

    /// Emit a property changed signal.
    pub async fn emit_property_changed(&self, prop: &str, value: Variant) -> Result<(), DbusError>;

    /// Emit NetScanBeacon signal.
    pub async fn emit_scan_beacon(&self, beacon: ScanBeacon) -> Result<(), DbusError>;

    /// Emit EnergyScanResult signal.
    pub async fn emit_energy_scan_result(&self, result: EnergyScanResultEntry) -> Result<(), DbusError>;

    /// Emit PropChanged signal.
    pub async fn emit_prop_changed(&self, key: &str, value: Variant) -> Result<(), DbusError>;
}
```

### `interface.rs`

Properties are exposed as D-Bus **methods** (`PropGet`/`PropSet`), NOT as D-Bus properties. This matches the C implementation in `DBusIPCAPI.cpp:890-967`.

```rust
use zbus::interface;

#[interface(name = "com.nestlabs.WPANTunnelDriver")]
impl WpanInterface {
    /// GetVersion: return daemon version string.
    #[zbus(name = "GetVersion")]
    async fn get_version(&self) -> String;

    /// PropGet: get a property by key string.
    /// Matches C: interface_prop_get_handler (DBusIPCAPI.cpp:890)
    #[zbus(name = "PropGet")]
    async fn prop_get(&self, key: &str) -> Result<Variant, DbusError>;

    /// PropSet: set a property by key string.
    /// Matches C: interface_prop_set_handler (DBusIPCAPI.cpp:929)
    #[zbus(name = "PropSet")]
    async fn prop_set(&self, key: &str, value: Variant) -> Result<i32, DbusError>;

    /// PropInsert: insert into a property (for list properties).
    #[zbus(name = "PropInsert")]
    async fn prop_insert(&self, key: &str, value: Variant) -> Result<i32, DbusError>;

    /// PropRemove: remove from a property (for list properties).
    #[zbus(name = "PropRemove")]
    async fn prop_remove(&self, key: &str, value: Variant) -> Result<i32, DbusError>;

    /// Status: return all properties as a dict.
    #[zbus(name = "Status")]
    async fn status(&self) -> Result<HashMap<String, Variant>, DbusError>;

    #[zbus(name = "Form")]
    async fn form(&self, params: HashMap<String, Variant>) -> Result<i32, DbusError>;

    #[zbus(name = "Join")]
    async fn join(&self, params: HashMap<String, Variant>) -> Result<i32, DbusError>;

    #[zbus(name = "Leave")]
    async fn leave(&self) -> Result<i32, DbusError>;

    #[zbus(name = "Reset")]
    async fn reset(&self) -> Result<i32, DbusError>;

    #[zbus(name = "BeginLowPower")]
    async fn begin_low_power(&self) -> Result<i32, DbusError>;

    #[zbus(name = "HostDidWake")]
    async fn host_did_wake(&self) -> Result<i32, DbusError>;

    #[zbus(name = "ConfigGateway")]
    async fn config_gateway(&self, params: HashMap<String, Variant>) -> Result<i32, DbusError>;

    #[zbus(name = "DataPoll")]
    async fn data_poll(&self) -> Result<i32, DbusError>;

    #[zbus(name = "NetScanStart")]
    async fn net_scan_start(&self, params: HashMap<String, Variant>) -> Result<i32, DbusError>;

    #[zbus(name = "NetScanStop")]
    async fn net_scan_stop(&self) -> Result<i32, DbusError>;
}
```

Note: `PropGet`/`PropSet` are method calls with a string key, NOT D-Bus property accessors. The property key is a string like `"NCP:State"`, `"Network:PANID"`, etc. — passed as a method argument, not as a D-Bus property name.

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
    SetProperty { name: String, value: Variant },
    GetProperty { name: String, reply: oneshot::Sender<Variant> },
}
```

### `properties.rs`

Property dispatch mapping every property name to its handler:

```rust
pub fn handle_get_property(name: &str, state: &NcpState) -> Result<Variant, DbusError> {
    match name {
        "NCP:State" => Ok(state.ncp_state.to_string().into()),
        "NCP:Version" => Ok(state.ncp_version.clone().into()),
        "Daemon:Version" => Ok(env!("CARGO_PKG_VERSION").into()),
        "NCP:HardwareAddress" => Ok(state.hardware_address.to_string().into()),
        "Network:PANID" => Ok(state.pan_id.into()),
        "Network:NodeType" => Ok(state.node_type.to_string().into()),
        // ... all properties
        _ => Err(DbusError::UnknownProperty(name.into())),
    }
}
```

## Tests

### Test 1: D-Bus Server Startup

```rust
#[tokio::test]
async fn dbus_server_starts() {
    let state = Arc::new(RwLock::new(NcpState::default()));
    let (tx, _rx) = mpsc::channel(16);
    let server = DbusServer::start("wfan0".into(), state, tx).await;
    assert!(server.is_ok());
}
```

### Test 2: Property Get via D-Bus

```rust
#[tokio::test]
async fn property_get_via_dbus() {
    let server = setup_test_server().await;
    let conn = zbus::Connection::session().await.unwrap();
    let proxy = WpanProxy::new(&conn, &server bus_name(), "/com/nestlabs/WPANTunnelDriver/wfan0").await.unwrap();

    let version = proxy.daemon_version().await.unwrap();
    assert!(!version.is_empty());
}
```

### Test 3: All Properties Round-Trip

```rust
#[tokio::test]
async fn all_properties_gettable() {
    let props = vec![
        "NCP:State", "NCP:Version", "NCP:HardwareAddress",
        "NCP:InterfaceType", "NCP:CCAThreshold", "NCP:Region",
        "Network:Name", "Network:PANID", "Network:XPANID",
        "Network:NodeType", "IPv6:LinkLocalAddress",
        "Interface:Up", "Stack:Up",
        // ... all 40+
    ];
    let server = setup_test_server().await;
    for prop in props {
        let result = server.get_property(prop).await;
        assert!(result.is_ok(), "Property {prop} not found");
    }
}
```

### Test 4: Form Command

```rust
#[tokio::test]
async fn form_command_dispatches() {
    let (tx, mut rx) = mpsc::channel(16);
    let server = setup_test_server_with_tx(tx).await;

    let mut params = HashMap::new();
    params.insert("Network:Name".into(), "TestNetwork".into());
    server.form(params).await.unwrap();

    let cmd = rx.recv().await.unwrap();
    match cmd {
        Command::Form { params } => {
            assert_eq!(params.get("Network:Name").unwrap(), "TestNetwork");
        }
        _ => panic!("Expected Form command"),
    }
}
```

### Test 5: Signal Emission

```rust
#[tokio::test]
async fn scan_beacon_signal() {
    let server = setup_test_server().await;
    let beacon = ScanBeacon {
        network_name: "TestNet".into(),
        pan_id: 0xABCD,
        channel: 1,
        // ...
    };
    server.emit_scan_beacon(beacon).await.unwrap();
    // Verify signal was emitted (check signal history)
}
```

### Test 6: Conformance Against C D-Bus Interface

```rust
#[test]
fn dbus_interface_matches_c_version() {
    // Introspect C daemon's D-Bus interface
    // Compare method signatures, property names, signal names
    // This is a snapshot test run once to capture C version's interface
}
```

## Dependencies

```toml
[dependencies]
zbus = { version = "4", features = ["tokio"] }
wisun-types = { path = "../wisun-types" }
tokio = { version = "1", features = ["sync", "rt"] }
serde = "1"
serde_json = "1"
tracing = "0.1"
thiserror = "2"
```

## Verification Checklist

- [ ] All methods from `doc/wpan-dbus-protocol.md` are implemented
- [ ] All 40+ properties from `ti_wisun_commands.md` are handled
- [ ] D-Bus introspection XML matches C version
- [ ] Signals fire on state changes
- [ ] Property changed signals fire on set
- [ ] `cargo test` passes
- [ ] `cargo clippy` produces zero warnings
- [ ] No `unsafe` code in this crate
