//! `deep_sleep`, `wake`, `host_did_wake` — NCP power-state management.
//!
//! Ports of `SpinelNCPTaskDeepSleep.cpp`, `SpinelNCPTaskWake.cpp`,
//! `SpinelNCPTaskHostDidWake.cpp`.
//!
//! The C code has two paths: a hardware power-cut (`set_ncp_power`) and an
//! MCU-power-state property (`PROP_MCU_POWER_STATE`, gated on
//! `SPINEL_CAP_MCU_POWER_STATE`). The Rust port has no hardware power API, so
//! these functions use the MCU-power-state property path, which is the
//! capability-guarded branch and works over the Spinel wire.

use std::time::Duration;

use spinel::command::{CMD_NOOP, CMD_PROP_VALUE_SET};
use spinel::property::prop_value_set;
use spinel::property::{
    CAP_MCU_POWER_STATE, MCU_POWER_STATE_LOW_POWER, MCU_POWER_STATE_ON, PROP_MCU_POWER_STATE,
};

use crate::DaemonError;
use crate::instance::NcpInstanceBase;
use crate::tasks::payload;

/// Command/state-wait timeout (`NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT`).
const TIMEOUT: Duration = Duration::from_secs(5);

/// Put the NCP into a low-power (deep sleep) state via `PROP_MCU_POWER_STATE`.
pub async fn deep_sleep(ncp: &NcpInstanceBase) -> Result<(), DaemonError> {
    // C: EH_WAIT_UNTIL_WITH_TIMEOUT(mDriverState == NORMAL_OPERATION, …)
    ncp.wait_for_driver_ready(TIMEOUT).await?;

    // C: only act if not already in DEEP_SLEEP (SpinelNCPTaskDeepSleep.cpp:94)
    if ncp.get_ncp_state().await == wisun_types::NcpState::DeepSleep {
        return Ok(());
    }

    if ncp.has_capability(CAP_MCU_POWER_STATE).await {
        ncp.send_command(
            CMD_PROP_VALUE_SET,
            prop_value_set(
                PROP_MCU_POWER_STATE,
                payload::u8_payload(MCU_POWER_STATE_LOW_POWER),
            )
            .payload,
        )
        .await?;
        ncp.set_ncp_state(wisun_types::NcpState::DeepSleep).await;
    } else {
        tracing::warn!("NCP lacks CAP_MCU_POWER_STATE; cannot deep sleep");
        return Err(DaemonError::Ncp(
            "NCP lacks MCU power-state capability".into(),
        ));
    }
    Ok(())
}

/// Wake the NCP from low-power state.
pub async fn wake(ncp: &NcpInstanceBase) -> Result<(), DaemonError> {
    if ncp.has_capability(CAP_MCU_POWER_STATE).await {
        ncp.send_command(
            CMD_PROP_VALUE_SET,
            prop_value_set(
                PROP_MCU_POWER_STATE,
                payload::u8_payload(MCU_POWER_STATE_ON),
            )
            .payload,
        )
        .await?;
    } else {
        ncp.send_command(CMD_NOOP, Vec::new()).await?;
        ncp.wait_for_state(
            |s| {
                !matches!(
                    s,
                    wisun_types::NcpState::NetWakeAsleep | wisun_types::NcpState::DeepSleep
                )
            },
            TIMEOUT,
        )
        .await?;
    }
    Ok(())
}

/// Notify the NCP that the host has woken. Optionally sends a `NOOP` tickle.
pub async fn host_did_wake(ncp: &NcpInstanceBase, tickle: bool) -> Result<(), DaemonError> {
    ncp.wait_for_state(|s| !s.is_initializing(), TIMEOUT)
        .await?;
    if tickle {
        ncp.send_command(CMD_NOOP, Vec::new()).await?;
    }
    Ok(())
}
