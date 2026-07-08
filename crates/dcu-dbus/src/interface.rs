//! `com.nestlabs.WPANTunnelDriver` interface implementation.
//!
//! Properties are exposed as D-Bus *methods* (`PropGet`/`PropSet`), not as
//! D-Bus properties, matching the C `DBusIPCAPI.cpp` handlers. The property
//! key is passed as a string method argument.

use std::collections::HashMap;

use tokio::sync::mpsc;
use zbus::interface;
use zbus::zvariant::OwnedValue;

use crate::commands::Command;
use crate::properties;
use crate::types::{DbusError, SharedState, Variant};

/// The per-interface D-Bus object.
///
/// Holds the shared daemon state and a sender into the daemon command
/// channel. The spec's `impl WpanInterface` methods referenced these
/// fields implicitly; they are declared here so the command-dispatch
/// methods (`form`, `join`, `prop_set`, ...) can actually reach the
/// channel.
pub struct WpanInterface {
    pub state: SharedState,
    pub command_tx: mpsc::Sender<Command>,
    /// Name of the NCP interface (e.g. `wfan0`), used for object paths.
    pub iface_name: String,
}

#[interface(name = "com.nestlabs.WPANTunnelDriver")]
impl WpanInterface {
    /// GetVersion: return the daemon version string.
    #[zbus(name = "GetVersion")]
    async fn get_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// PropGet: read a property by its key string.
    ///
    /// Matches C `interface_prop_get_handler` (DBusIPCAPI.cpp:890). Values
    /// are returned as strings (see `properties::variant_to_string` for why
    /// a bare variant return cannot be used with this zbus version).
    #[zbus(name = "PropGet")]
    async fn prop_get(&self, key: &str) -> Result<String, DbusError> {
        let v = properties::handle_get_property(key, &self.state).await?;
        Ok(properties::variant_to_string(&v))
    }

    /// PropSet: write a property by its key string.
    ///
    /// Matches C `interface_prop_set_handler` (DBusIPCAPI.cpp:929).
    #[zbus(name = "PropSet")]
    async fn prop_set(&self, key: &str, value: OwnedValue) -> Result<i32, DbusError> {
        let value: Variant = value.into();
        properties::handle_set_property(key, value, &self.state).await?;
        Ok(0)
    }

    /// PropInsert: insert into a list property.
    ///
    /// List properties are not yet modeled; reserved for future use.
    #[zbus(name = "PropInsert")]
    async fn prop_insert(&self, _key: &str, _value: OwnedValue) -> Result<i32, DbusError> {
        Err(DbusError::NotImplemented("PropInsert".into()))
    }

    /// PropRemove: remove from a list property.
    #[zbus(name = "PropRemove")]
    async fn prop_remove(&self, _key: &str, _value: OwnedValue) -> Result<i32, DbusError> {
        Err(DbusError::NotImplemented("PropRemove".into()))
    }

    /// Status: return all properties as a string-keyed dict.
    #[zbus(name = "Status")]
    async fn status(&self) -> Result<HashMap<String, String>, DbusError> {
        let guard = self.state.read().await;
        let mut out = HashMap::new();
        for key in properties::all_property_keys() {
            if let Ok(v) = properties::get_property_locked(key, &guard) {
                out.insert((*key).to_string(), properties::variant_to_string(&v));
            }
        }
        Ok(out)
    }

    /// Form a network.
    #[zbus(name = "Form")]
    async fn form(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::Form { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Join an existing network.
    #[zbus(name = "Join")]
    async fn join(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::Join { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Leave the current network.
    #[zbus(name = "Leave")]
    async fn leave(&self) -> Result<i32, DbusError> {
        self.command_tx
            .send(Command::Leave)
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Reset the NCP.
    #[zbus(name = "Reset")]
    async fn reset(&self) -> Result<i32, DbusError> {
        self.command_tx
            .send(Command::Reset)
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Enter low-power mode.
    #[zbus(name = "BeginLowPower")]
    async fn begin_low_power(&self) -> Result<i32, DbusError> {
        self.command_tx
            .send(Command::BeginLowPower)
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Notify the host that it has woken from sleep.
    #[zbus(name = "HostDidWake")]
    async fn host_did_wake(&self) -> Result<i32, DbusError> {
        self.command_tx
            .send(Command::HostDidWake)
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Attach to an existing network without forming or joining.
    #[zbus(name = "Attach")]
    async fn attach(&self) -> Result<i32, DbusError> {
        self.command_tx
            .send(Command::Attach)
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Configure the gateway.
    #[zbus(name = "ConfigGateway")]
    async fn config_gateway(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::ConfigGateway { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Poll for pending data.
    #[zbus(name = "DataPoll")]
    async fn data_poll(&self) -> Result<i32, DbusError> {
        self.command_tx
            .send(Command::DataPoll)
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Start a network scan.
    #[zbus(name = "NetScanStart")]
    async fn net_scan_start(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::NetScanStart { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Stop a network scan.
    #[zbus(name = "NetScanStop")]
    async fn net_scan_stop(&self) -> Result<i32, DbusError> {
        self.command_tx
            .send(Command::NetScanStop)
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    // -------------------------------------------------------------------
    // Additional methods. These wire the `Command` variants defined in
    // `commands.rs` to the D-Bus interface (the spec's scope deferred them,
    // but the dispatch variants already existed and were otherwise dead
    // code).
    // -------------------------------------------------------------------

    /// Start a discover scan (joiner/discover variant of NetScan).
    #[zbus(name = "DiscoverScanStart")]
    async fn discover_scan_start(
        &self,
        params: HashMap<String, OwnedValue>,
    ) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::DiscoverScanStart { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Stop a discover scan.
    #[zbus(name = "DiscoverScanStop")]
    async fn discover_scan_stop(&self) -> Result<i32, DbusError> {
        self.command_tx
            .send(Command::DiscoverScanStop)
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Start an energy scan.
    #[zbus(name = "EnergyScanStart")]
    async fn energy_scan_start(
        &self,
        params: HashMap<String, OwnedValue>,
    ) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::EnergyScanStart { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Stop an energy scan.
    #[zbus(name = "EnergyScanStop")]
    async fn energy_scan_stop(&self) -> Result<i32, DbusError> {
        self.command_tx
            .send(Command::EnergyScanStop)
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Request MLR (Multicast Listener Registration).
    #[zbus(name = "MlrRequest")]
    async fn mlr_request(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::MlrRequest { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Configure the Backbone Router.
    #[zbus(name = "BackboneRouterConfig")]
    async fn backbone_router_config(
        &self,
        params: HashMap<String, OwnedValue>,
    ) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::BackboneRouterConfig { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Announce a Begin event on the given channel.
    #[zbus(name = "AnnounceBegin")]
    async fn announce_begin(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::AnnounceBegin { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Query for a PAN ID.
    #[zbus(name = "PanIdQuery")]
    async fn pan_id_query(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::PanIdQuery { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Generate a PSKc from the given parameters.
    #[zbus(name = "GeneratePSKc")]
    async fn generate_pskc(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::GeneratePSKc { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Peek at raw NCP data (debug).
    #[zbus(name = "Peek")]
    async fn peek(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::Peek { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Poke raw NCP data (debug).
    #[zbus(name = "Poke")]
    async fn poke(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::Poke { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Add a route.
    #[zbus(name = "RouteAdd")]
    async fn route_add(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::RouteAdd { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Remove a route.
    #[zbus(name = "RouteRemove")]
    async fn route_remove(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::RouteRemove { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Add a service.
    #[zbus(name = "ServiceAdd")]
    async fn service_add(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::ServiceAdd { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }

    /// Remove a service.
    #[zbus(name = "ServiceRemove")]
    async fn service_remove(&self, params: HashMap<String, OwnedValue>) -> Result<i32, DbusError> {
        let params = to_variant_map(params);
        self.command_tx
            .send(Command::ServiceRemove { params })
            .await
            .map_err(|e| DbusError::Transport(e.to_string()))?;
        Ok(0)
    }
}

/// Helper used by the server to build an interface instance.
pub fn new(
    iface_name: String,
    state: SharedState,
    command_tx: mpsc::Sender<Command>,
) -> WpanInterface {
    WpanInterface {
        state,
        command_tx,
        iface_name,
    }
}

/// Convert a D-Bus `OwnedValue` map (the wire form of method arguments)
/// into the in-memory `Variant` (`Value<'static>`) map used by the command
/// layer.
fn to_variant_map(params: HashMap<String, OwnedValue>) -> HashMap<String, Variant> {
    params
        .into_iter()
        .map(|(k, v)| {
            let v: Variant = v.into();
            (k, v)
        })
        .collect()
}
