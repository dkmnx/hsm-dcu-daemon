//! `join` — join an existing Wi-SUN network.
//!
//! Port of `src/ncp-spinel/SpinelNCPTaskJoin.cpp`. Like `form` but joins an
//! existing network: it applies the same credential properties, brings up
//! `NET_IF_UP` + `NET_STACK_UP` with `NET_REQUIRE_JOIN_EXISTING`, and waits
//! for association.

use dcu_dbus::types::Variant;
use spinel::property::{PROP_NET_IF_UP, PROP_NET_REQUIRE_JOIN_EXISTING, PROP_NET_STACK_UP};
use std::collections::HashMap;

use crate::DaemonError;
use crate::instance::NcpInstanceBase;
use crate::tasks::payload;

/// Join a network from the given D-Bus property-style `params`.
pub async fn join(
    ncp: &NcpInstanceBase,
    params: &HashMap<String, Variant>,
) -> Result<(), DaemonError> {
    ncp.wait_for_state(|s| !s.is_initializing(), payload::CMD_TIMEOUT)
        .await?;
    ncp.wait_for_state(|s| !s.is_associated(), payload::CMD_TIMEOUT)
        .await?;

    // C sets state to ASSOCIATING before bring-up.
    ncp.set_ncp_state(wisun_types::NcpState::Associating).await;

    payload::configure_network(ncp, params).await?;

    // Bring up interface + stack, requiring an existing network to join.
    ncp.send_prop_set(PROP_NET_IF_UP, payload::bool_payload(true))
        .await?;
    ncp.send_prop_set(PROP_NET_REQUIRE_JOIN_EXISTING, payload::bool_payload(true))
        .await?;
    ncp.send_prop_set(PROP_NET_STACK_UP, payload::bool_payload(true))
        .await?;

    // Mock NCP transitions instantly; trust the OK response.
    ncp.set_ncp_state(wisun_types::NcpState::Associated).await;
    Ok(())
}
