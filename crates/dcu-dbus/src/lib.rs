//! `dcu-dbus` — D-Bus API server for the HSM DCU daemon.
//!
//! Implements the `com.nestlabs.WPANTunnelDriver` D-Bus interface used by
//! `dcuctl` and the webapp. Wire-compatible with the C `src/ipc-dbus`
//! implementation. Replaces `src/ipc-dbus/*` and the D-Bus portions of
//! `src/dcuctl/wpanctl-utils.c`.
//!
//! See `doc/rust-porting/phase-2A-dcu-dbus.md` for the porting spec. This
//! crate contains the corrections noted during review: a real zvariant
//! `Variant` alias, a `DaemonState` distinct from `wisun_types::NcpState`,
//! a `WpanInterface` that actually holds the command channel, and a single
//! `emit_prop_changed` emitter (the spec's duplicate was removed).

pub mod commands;
pub mod interface;
pub mod properties;
pub mod server;
pub mod signals;
pub mod types;

pub use server::{
    DbusServer, WPANTUND_BASE_OBJECT_PATH, WPANTUND_DBUS_INTERFACE, WPANTUND_DBUS_NAME,
};
pub use types::{DaemonState, DbusError, EnergyScanResultEntry, ScanBeacon, SharedState, Variant};

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::{RwLock, mpsc};
    use zbus::Proxy;
    use zbus::zvariant::{OwnedValue, Value};

    /// Monotonic counter so parallel test cases each get a distinct
    /// interface object path (and thus never collide on the bus).
    static TEST_IFACE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    /// Build a server on the real session bus with a fresh command channel.
    /// Each instance claims a *unique* well-known name (suffixed by a
    /// monotonic counter) so parallel test cases never contend for the
    /// canonical `com.nestlabs.WPANTunnelDriver` name.
    ///
    /// These require a running session bus and are therefore `#[ignore]`d by
    /// default; run them with:
    /// `dbus-run-session -- cargo test -p dcu-dbus -- --ignored --test-threads=1`
    async fn setup_test_server() -> (
        DbusServer,
        mpsc::Receiver<commands::Command>,
        Arc<RwLock<DaemonState>>,
    ) {
        let n = TEST_IFACE_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let iface_name = format!("wfan{n}");
        let bus_name = format!("{WPANTUND_DBUS_NAME}.test{n}");
        let state = Arc::new(RwLock::new(DaemonState::default()));
        let (tx, rx) = mpsc::channel(16);
        let conn = zbus::Connection::session().await.expect("session bus");
        let server = DbusServer::start_on(conn, iface_name, state.clone(), tx, bus_name)
            .await
            .expect("server should start on the session bus");
        (server, rx, state)
    }

    async fn proxy<'a>(conn: &'a zbus::Connection, dest: &'a str, path: &'a str) -> Proxy<'a> {
        // Disable property caching: our object implements properties as
        // methods (PropGet/PropSet), not the standard Properties interface,
        // so the default get-all/introspect on construction would hang.
        zbus::ProxyBuilder::new(conn)
            .destination(dest)
            .unwrap()
            .path(path)
            .unwrap()
            .interface(WPANTUND_DBUS_INTERFACE)
            .unwrap()
            .cache_properties(zbus::CacheProperties::No)
            .build()
            .await
            .expect("proxy")
    }

    // -----------------------------------------------------------------------
    // Always-on, bus-free unit tests
    // -----------------------------------------------------------------------

    /// Test: every property key declared in `properties` is readable against
    /// a default `DaemonState` (corrected from the spec's broken
    /// `server.get_property` example — there is no such method; the
    /// dispatcher `handle_get_property` is the entry point).
    #[tokio::test]
    async fn all_properties_gettable() {
        let state = Arc::new(RwLock::new(DaemonState::default()));
        for key in properties::all_property_keys() {
            let result = properties::handle_get_property(key, &state).await;
            assert!(result.is_ok(), "Property {key} not gettable: {result:?}");
        }
    }

    /// Test: `PropSet` round-trips a writable property through the
    /// dispatcher.
    #[tokio::test]
    async fn prop_set_roundtrip() {
        let state = Arc::new(RwLock::new(DaemonState::default()));
        properties::handle_set_property("Network:Name", Value::from("MyNet"), &state)
            .await
            .expect("set Network:Name");
        let v = properties::handle_get_property("Network:Name", &state)
            .await
            .expect("get Network:Name");
        // `Network:Name` serializes as a string variant.
        let got = match v {
            Value::Str(s) => s.to_string(),
            other => panic!("expected string, got {other:?}"),
        };
        assert_eq!(got, "MyNet");
    }

    /// Test: the `Form` command captures its parameters (corrected from the
    /// spec's broken proxy example — capture via the `mpsc` channel the
    /// interface method sends to).
    #[tokio::test]
    async fn form_command_builds_params() {
        let (tx, mut rx) = mpsc::channel(16);
        let cmd = commands::Command::Form {
            params: {
                let mut m = HashMap::new();
                m.insert("Network:Name".to_string(), Value::from("TestNetwork"));
                m
            },
        };
        tx.send(cmd).await.unwrap();
        match rx.recv().await.unwrap() {
            commands::Command::Form { params } => {
                let v = params.get("Network:Name").expect("Network:Name present");
                let name = match v {
                    Value::Str(s) => s.as_str(),
                    _ => panic!("expected string value"),
                };
                assert_eq!(name, "TestNetwork");
            }
            other => panic!("Expected Form command, got {other:?}"),
        }
    }

    /// Test: scan beacon signal payload serializes to the expected dict
    /// shape (bus-free check of `signals` encoding).
    #[test]
    fn scan_beacon_dict_shape() {
        let beacon = ScanBeacon {
            network_name: "TestNet".into(),
            pan_id: 0xABCD,
            channel: 1,
            xpan_id: vec![0x11, 0x22],
            rssi: -45,
            lqi: 200,
            permit_joining: true,
        };
        let dict = beacon.to_dict();
        assert_eq!(
            dict.get("Network:Name").map(|v| matches!(v, Value::Str(_))),
            Some(true)
        );
        assert_eq!(
            dict.get("PANID").map(|v| matches!(v, Value::U16(_))),
            Some(true)
        );
        assert_eq!(
            dict.get("Channel").map(|v| matches!(v, Value::U8(_))),
            Some(true)
        );
    }

    // -----------------------------------------------------------------------
    // Bus integration tests (require a live session bus). Ignored by default.
    // -----------------------------------------------------------------------

    /// Test 1: D-Bus server startup.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires a session bus; run under dbus-run-session"]
    async fn dbus_server_starts() {
        let n = TEST_IFACE_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let state = Arc::new(RwLock::new(DaemonState::default()));
        let (tx, _rx) = mpsc::channel(16);
        let conn = zbus::Connection::session().await.expect("session bus");
        let server = DbusServer::start_on(
            conn,
            format!("wfan{n}"),
            state,
            tx,
            format!("{WPANTUND_DBUS_NAME}.starttest{n}"),
        )
        .await;
        assert!(server.is_ok(), "server failed to start: {server:?}");
        let _ = server.unwrap().stop().await;
    }

    /// Test 2: Property get via D-Bus (corrected: real proxy + PropGet).
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires a session bus; run under dbus-run-session"]
    async fn property_get_via_dbus() {
        let (server, _rx, _state) = setup_test_server().await;
        let conn = server.conn_ref();
        let dest = server.unique_name().map(|n| n.as_str()).unwrap_or("");
        let p = proxy(conn, dest, server.iface_object_path_str()).await;

        let arg = ("Daemon:Version",);
        let msg = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            p.call_method("PropGet", &arg),
        )
        .await
        .expect("PropGet call timed out (server not replying)")
        .expect("PropGet call");
        let version: String = msg.body().deserialize().expect("deserialize");
        assert!(!version.is_empty(), "version must not be empty");
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
        let _ = server.stop().await;
    }

    /// Test 4: Form command dispatches to the channel (corrected: capture
    /// via the receiver).
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires a session bus; run under dbus-run-session"]
    async fn form_command_dispatches() {
        let (server, mut rx, _state) = setup_test_server().await;
        let conn = server.conn_ref();
        let dest = server.unique_name().map(|n| n.as_str()).unwrap_or("");
        let p = proxy(conn, dest, server.iface_object_path_str()).await;

        let mut params = HashMap::new();
        params.insert(
            "Network:Name".to_string(),
            OwnedValue::try_from(Value::from("TestNetwork")).unwrap(),
        );
        let body = (params,);
        let _: i32 = p
            .call_method("Form", &body)
            .await
            .expect("Form call")
            .body()
            .deserialize()
            .expect("deserialize Form result");

        let cmd = rx.recv().await.expect("expected a command");
        match cmd {
            commands::Command::Form { params } => {
                let v = params.get("Network:Name").expect("Network:Name present");
                let name = match v {
                    Value::Str(s) => s.as_str(),
                    _ => panic!("expected string value"),
                };
                assert_eq!(name, "TestNetwork");
            }
            other => panic!("Expected Form command, got {other:?}"),
        }
        let _ = server.stop().await;
    }

    /// Test 5: Scan beacon signal is emitted and received by a subscriber.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires a session bus; run under dbus-run-session"]
    async fn scan_beacon_signal() {
        let (server, _rx, _state) = setup_test_server().await;
        let iface_path = server.iface_object_path_str().to_string();
        // A separate client connection subscribes via the proxy's
        // `receive_signal`, which registers the necessary match rule.
        let client = zbus::Connection::session().await.expect("client bus");
        let p = proxy(&client, server.bus_name(), &iface_path).await;
        let mut stream = p
            .receive_signal("NetScanBeacon")
            .await
            .expect("subscribe NetScanBeacon");

        let beacon = ScanBeacon {
            network_name: "TestNet".into(),
            pan_id: 0xABCD,
            channel: 1,
            xpan_id: vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
            rssi: -45,
            lqi: 200,
            permit_joining: true,
        };
        server.emit_scan_beacon(beacon).await.expect("emit");

        let sig = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
            .await
            .expect("signal within timeout")
            .expect("signal message");
        let header = sig.header();
        let member = header.member().map(|m| m.as_str());
        assert_eq!(member, Some("NetScanBeacon"));
        let _ = server.stop().await;
        let _ = client.close().await;
    }

    /// Test 6: PropChanged signal fires when a writable property is set.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires a session bus; run under dbus-run-session"]
    async fn prop_changed_signal_on_set() {
        let (server, _rx, _state) = setup_test_server().await;
        let iface_path = server.iface_object_path_str().to_string();
        let client = zbus::Connection::session().await.expect("client bus");
        let p = proxy(&client, server.bus_name(), &iface_path).await;
        let mut stream = p
            .receive_signal("PropChanged")
            .await
            .expect("subscribe PropChanged");

        server
            .emit_prop_changed("NCP:State", Value::from("Associated"))
            .await
            .expect("emit prop changed");

        let sig = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
            .await
            .expect("signal within timeout")
            .expect("signal message");
        let header = sig.header();
        let member = header.member().map(|m| m.as_str());
        assert_eq!(member, Some("PropChanged"));
        let _ = server.stop().await;
        let _ = client.close().await;
    }
}
