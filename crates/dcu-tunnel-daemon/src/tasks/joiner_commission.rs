//! `joiner_commission` — start/stop Thread/Win-SUN joiner commissioning.
//!
//! Port of `src/ncp-spinel/SpinelNCPTaskJoinerCommissioning.cpp`. When
//! `action` is true, sets the joiner credentials (`MESHCOP_JOINER_*`) and
//! brings up `NET_IF_UP` + `NET_STACK_UP`; when false, tears them down by
//! bringing `NET_IF_UP` down and clearing `MESHCOP_JOINER_COMMISSIONING`.

use std::time::Duration;

use dcu_dbus::types::Variant;
use spinel::pack::PackWriter;
use spinel::property::{
    PROP_MAC_PROMISCUOUS_MODE, PROP_MESHCOP_JOINER_COMMISSIONING, PROP_NET_IF_UP, PROP_NET_STACK_UP,
};
use std::collections::HashMap;

use crate::DaemonError;
use crate::instance::NcpInstanceBase;
use crate::tasks::params;
use crate::tasks::payload;

/// Joiner timeout (`NCP_JOINER_TIMEOUT` = 60s).
const JOINER_TIMEOUT: Duration = Duration::from_secs(60);

/// Start or stop joiner commissioning.
///
/// `params` carries the joiner credentials (`PSKd`, optional provisioning URL,
/// vendor name/model/sw-version/data). `action == true` starts commissioning;
/// `action == false` stops it.
pub async fn joiner_commission(
    ncp: &NcpInstanceBase,
    action: bool,
    params: &HashMap<String, Variant>,
) -> Result<(), DaemonError> {
    ncp.wait_for_state(|s| !s.is_initializing(), payload::CMD_TIMEOUT)
        .await?;

    if !action {
        // Stop: bring interface down, clear commissioning flag.
        ncp.send_prop_set(PROP_NET_IF_UP, payload::bool_payload(false))
            .await?;
        ncp.send_prop_set(
            PROP_MESHCOP_JOINER_COMMISSIONING,
            payload::bool_payload(false),
        )
        .await?;
        return Ok(());
    }

    // Start: PSKd is mandatory.
    let pskd = params::get_str(params, "Network:PSKd")
        .ok_or_else(|| DaemonError::Ncp("joiner commissioning requires PSKd".into()))?;

    // Promiscuous mode off.
    ncp.send_prop_set(PROP_MAC_PROMISCUOUS_MODE, payload::u8_payload(0))
        .await?;

    // Bring interface up.
    ncp.send_prop_set(PROP_NET_IF_UP, payload::bool_payload(true))
        .await?;

    // Commissioning payload: action(bool) + PSKd + optional fields (utf8),
    // matching the C field order (SPINEL_DATATYPE_BOOL_S + 6×UTF8_S).
    let provisioning_url = params::get_str(params, "Network:ProvisioningUrl").unwrap_or_default();
    let vendor_name = params::get_str(params, "Network:VendorName").unwrap_or_default();
    let vendor_model = params::get_str(params, "Network:VendorModel").unwrap_or_default();
    let vendor_sw_version = params::get_str(params, "Network:VendorSwVersion").unwrap_or_default();
    let vendor_data = params::get_str(params, "Network:VendorData").unwrap_or_default();

    let mut w = PackWriter::new();
    w.write_bool(action);
    w.write_utf8(&pskd);
    w.write_utf8(&provisioning_url);
    w.write_utf8(&vendor_name);
    w.write_utf8(&vendor_model);
    w.write_utf8(&vendor_sw_version);
    w.write_utf8(&vendor_data);
    ncp.send_prop_set(PROP_MESHCOP_JOINER_COMMISSIONING, w.into_bytes())
        .await?;

    ncp.send_prop_set(PROP_NET_STACK_UP, payload::bool_payload(true))
        .await?;

    // The C blocks until a LAST_STATUS JOIN response arrives. In the async
    // port we surface success once the stack is up; the daemon's event loop
    // will deliver the join status via the response table / unsolicited path.
    ncp.wait_for_state(
        |s| s.is_associated() || matches!(s, wisun_types::NcpState::CredentialsNeeded),
        JOINER_TIMEOUT,
    )
    .await?;
    Ok(())
}
