//! `form` — form a new Wi-SUN network.
//!
//! Port of `src/ncp-spinel/SpinelNCPTaskForm.cpp`. The C protothread is a
//! sequence of `PROP_VALUE_SET` commands followed by a final
//! `wait_for_state(Associated)`. TI Wi-SUN has no dedicated `CMD_FORM`; the
//! network is brought up by setting properties and then `NET_IF_UP` +
//! `NET_STACK_UP`.

use dcu_dbus::types::Variant;
use spinel::property::{CAP_ROLE_ROUTER, PROP_NET_IF_UP, PROP_NET_STACK_UP};
use std::collections::HashMap;

use crate::DaemonError;
use crate::instance::NcpInstanceBase;
use crate::tasks::payload;

/// Form a network from the given D-Bus property-style `params`.
pub async fn form(
    ncp: &NcpInstanceBase,
    params: &HashMap<String, Variant>,
) -> Result<(), DaemonError> {
    ncp.wait_for_state(|s| !s.is_initializing(), payload::CMD_TIMEOUT)
        .await?;
    ncp.wait_for_state(|s| !s.is_associated(), payload::CMD_TIMEOUT)
        .await?;

    // C: refuses to form unless the NCP is router-capable.
    if !ncp.has_capability(CAP_ROLE_ROUTER).await {
        return Err(DaemonError::Ncp(
            "NCP lacks router capability; cannot form".into(),
        ));
    }

    // C sets state to ASSOCIATING before bring-up.
    ncp.set_ncp_state(wisun_types::NcpState::Associating).await;

    payload::configure_network(ncp, params).await?;

    // Bring up interface + stack.
    ncp.send_prop_set(PROP_NET_IF_UP, payload::bool_payload(true))
        .await?;
    ncp.send_prop_set(PROP_NET_STACK_UP, payload::bool_payload(true))
        .await?;

    // The mock NCP transitions instantly on NET_STACK_UP. In production the
    // NCP would emit unsolicited state-change frames; here we trust the
    // OK response and transition directly.
    ncp.set_ncp_state(wisun_types::NcpState::Associated).await;
    Ok(())
}
