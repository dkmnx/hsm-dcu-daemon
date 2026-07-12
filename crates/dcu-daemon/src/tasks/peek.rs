//! `peek` — read raw NCP memory.
//!
//! Port of `src/ncp-spinel/SpinelNCPTaskPeek.cpp`. Sends `CMD_PEEK` with the
//! address + count and awaits the matching `CMD_PEEK_RET` response (matched by
//! TID through the shared response table).

use std::time::Duration;

use spinel::command::CMD_PEEK;
use spinel::pack::{PackReader, PackWriter};

use crate::DaemonError;
use crate::instance::NcpInstanceBase;

/// Entry-guard / response timeout (`NCP_DEFAULT_COMMAND_RESPONSE_TIMEOUT`).
const TIMEOUT: Duration = Duration::from_secs(5);

/// Peek `count` bytes starting at `address` from NCP memory.
pub async fn peek(ncp: &NcpInstanceBase, address: u32, count: u16) -> Result<Vec<u8>, DaemonError> {
    // C guards: !is_initializing && state != UPGRADING && mEnabled.
    ncp.wait_for_state(|s| !s.is_initializing(), TIMEOUT)
        .await?;

    let mut w = PackWriter::new();
    w.write_uint32(address);
    w.write_uint16(count);
    let resp = ncp.send_command(CMD_PEEK, w.into_bytes()).await?;

    // PEEK_RET payload is "CiLSD": a leading uint8 + uint_packed (both ignored
    // by the C), then uint32 address, uint16 count, and the raw data bytes.
    let mut reader = PackReader::new(&resp.payload);
    let _ = reader.read_uint8();
    let _ = reader.read_uint_packed();
    let ret_addr = reader.read_uint32().unwrap_or(0);
    let ret_count = reader.read_uint16().unwrap_or(0);
    let data = reader
        .read_bytes(ret_count as usize)
        .map(|b| b.to_vec())
        .unwrap_or_default();

    // C validates the echoed address/count match the request.
    if ret_addr != address || ret_count != count {
        return Err(DaemonError::Ncp(format!(
            "peek mismatch: requested {address:#x}/{count}, got {ret_addr:#x}/{ret_count}"
        )));
    }
    Ok(data)
}
