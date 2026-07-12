//! `leave` — tear down the current network association.
//!
//! Port of `src/ncp-spinel/SpinelNCPTaskLeave.cpp`. The C protothread becomes
//! a straight async function: each `EH_SPAWN(vprocess_send_command)` is a
//! `.await` on `send_command()`, and the two `EH_REQUIRE_WITHIN` state waits
//! become `wait_for_state` / `wait_for_driver_ready`.

use std::time::Duration;

use spinel::command::{CMD_NET_CLEAR, CMD_RESET};
use spinel::property::{PROP_NET_IF_UP, PROP_NET_STACK_UP};

use crate::DaemonError;
use crate::instance::NcpInstanceBase;
use crate::tasks::payload;

/// Command/state-wait timeout (`NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT`).
const TIMEOUT: Duration = Duration::from_secs(5);

/// Leave the current network: bring down stack + interface, clear settings,
/// reset the NCP, then wait for re-initialization to complete.
pub async fn leave(ncp: &NcpInstanceBase) -> Result<(), DaemonError> {
    // C: EH_REQUIRE_WITHIN(!ncp_state_is_initializing && !is_initializing_ncp)
    ncp.wait_for_state(|s| !s.is_initializing(), TIMEOUT)
        .await?;

    ncp.send_prop_set(PROP_NET_STACK_UP, payload::bool_payload(false))
        .await?;
    ncp.send_prop_set(PROP_NET_IF_UP, payload::bool_payload(false))
        .await?;
    ncp.send_command(CMD_NET_CLEAR, Vec::new()).await?;
    ncp.send_command(CMD_RESET, Vec::new()).await?;

    // C: EH_REQUIRE_WITHIN(ncp_state_is_initializing)
    ncp.wait_for_state(|s| s.is_initializing(), TIMEOUT).await?;
    // C: EH_REQUIRE_WITHIN(!initializing && mDriverState == NORMAL_OPERATION)
    ncp.wait_for_state(|s| !s.is_initializing(), TIMEOUT)
        .await?;
    ncp.wait_for_driver_ready(TIMEOUT).await?;
    Ok(())
}
