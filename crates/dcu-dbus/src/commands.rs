//! Command dispatch between the D-Bus interface and the daemon core.
//!
//! Each D-Bus method that mutates NCP/network state produces a `Command`
//! that is sent over an `mpsc` channel to the daemon task (Phase 2C+/3).
//! This crate only *produces* these commands; it does not execute them.
//!
//! NOTE: This is distinct from `wisun_types::command` (Spinel protocol
//! command IDs). The two layers never mix.

use std::collections::HashMap;

use tokio::sync::oneshot;

use crate::types::Variant;

/// A request from a D-Bus client to change daemon/NCP behavior.
#[derive(Debug)]
pub enum Command {
    /// Form a new network. `params` are D-Bus property-style overrides.
    Form { params: HashMap<String, Variant> },
    /// Join an existing network.
    Join { params: HashMap<String, Variant> },
    /// Leave the current network.
    Leave,
    /// Reset the NCP.
    Reset,
    /// Enter low-power mode.
    BeginLowPower,
    /// Notify the host that it has woken from sleep.
    HostDidWake,
    /// Attach to an existing network/pan without forming or joining.
    Attach,
    /// Configure the gateway.
    ConfigGateway { params: HashMap<String, Variant> },
    /// Poll for pending data.
    DataPoll,
    /// Begin a network scan.
    NetScanStart { params: HashMap<String, Variant> },
    /// Stop a network scan.
    NetScanStop,
    /// Discover-scan start (joiner/discover variant).
    DiscoverScanStart { params: HashMap<String, Variant> },
    /// Discover-scan stop.
    DiscoverScanStop,
    /// Energy scan start.
    EnergyScanStart { params: HashMap<String, Variant> },
    /// Energy scan stop.
    EnergyScanStop,
    /// Request Multicast Listener Registration.
    MlrRequest { params: HashMap<String, Variant> },
    /// Configure the Backbone Router.
    BackboneRouterConfig { params: HashMap<String, Variant> },
    /// Announce a Begin event on a channel (params: channel mask, etc.).
    AnnounceBegin { params: HashMap<String, Variant> },
    /// Query for a PAN ID (params: panid, channel mask, etc.).
    PanIdQuery { params: HashMap<String, Variant> },
    /// Generate a PSKc from the given parameters.
    GeneratePSKc { params: HashMap<String, Variant> },
    /// Peek at raw NCP data (debug).
    Peek { params: HashMap<String, Variant> },
    /// Poke raw NCP data (debug).
    Poke { params: HashMap<String, Variant> },
    /// Add a route.
    RouteAdd { params: HashMap<String, Variant> },
    /// Remove a route.
    RouteRemove { params: HashMap<String, Variant> },
    /// Add a service.
    ServiceAdd { params: HashMap<String, Variant> },
    /// Remove a service.
    ServiceRemove { params: HashMap<String, Variant> },
    /// Set a property on the NCP/daemon.
    SetProperty { name: String, value: Variant },
    /// Get a property. The result is delivered over the oneshot sender.
    GetProperty {
        name: String,
        reply: oneshot::Sender<Result<Variant, crate::types::DbusError>>,
    },
}
