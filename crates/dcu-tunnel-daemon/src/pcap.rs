//! Pcap capture manager.
//!
//! Port of `src/wfantund/Pcap.cpp`. Manages a set of file descriptors
//! that receive pcap-formatted Spinel frames. Each `PcapToFd` call adds
//! an FD; `PcapTerminate` closes them all.

use std::io::Write;
use std::os::unix::io::{FromRawFd, RawFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::RwLock;

/// pcap global file header (little-endian).
#[repr(C, packed)]
struct PcapGlobalHeader {
    magic: u32,
    version_major: u16,
    version_minor: u16,
    _gmt_offset: i32,
    _accuracy: u32,
    snap_len: u32,
    dlt: u32,
}

/// pcap per-packet record header (little-endian).
#[repr(C, packed)]
struct PcapRecordHeader {
    ts_sec: u32,
    ts_usec: u32,
    incl_len: u32,
    orig_len: u32,
}

/// PPI (Per-Packet Information) header for 802.15.4 captures.
#[repr(C, packed)]
struct PpiHeader {
    version: u8,
    _flags: u8,
    size: u16,
    dlt: u32,
}

const PCAP_MAGIC: u32 = 0xa1b2c3d4;
const PCAP_VERSION_MAJOR: u16 = 2;
const PCAP_VERSION_MINOR: u16 = 4;
const PCAP_SNAP_LEN: u32 = 0x0004_0000;
const PCAP_DLT_PPI: u32 = 192;
const PPI_HEADER_SIZE: u16 = 8;

#[allow(unsafe_code)]
fn write_pcap_header(fd: RawFd) -> std::io::Result<()> {
    let header = PcapGlobalHeader {
        magic: PCAP_MAGIC,
        version_major: PCAP_VERSION_MAJOR,
        version_minor: PCAP_VERSION_MINOR,
        _gmt_offset: 0,
        _accuracy: 0,
        snap_len: PCAP_SNAP_LEN,
        dlt: PCAP_DLT_PPI,
    };
    let buf = unsafe {
        std::slice::from_raw_parts(
            &header as *const _ as *const u8,
            size_of::<PcapGlobalHeader>(),
        )
    };
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.write_all(buf)?;
    std::mem::forget(file);
    Ok(())
}

#[allow(unsafe_code)]
fn write_pcap_record(fd: RawFd, payload: &[u8]) -> std::io::Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let record = PcapRecordHeader {
        ts_sec: now.as_secs() as u32,
        ts_usec: now.subsec_micros(),
        incl_len: PPI_HEADER_SIZE as u32 + payload.len() as u32,
        orig_len: PPI_HEADER_SIZE as u32 + payload.len() as u32,
    };
    let ppi = PpiHeader {
        version: 0,
        _flags: 0,
        size: PPI_HEADER_SIZE,
        dlt: PCAP_DLT_PPI,
    };
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.write_all(unsafe {
        std::slice::from_raw_parts(
            &record as *const _ as *const u8,
            size_of::<PcapRecordHeader>(),
        )
    })?;
    file.write_all(unsafe {
        std::slice::from_raw_parts(&ppi as *const _ as *const u8, size_of::<PpiHeader>())
    })?;
    file.write_all(payload)?;
    std::mem::forget(file);
    Ok(())
}

/// Pcap capture state. Uses `AtomicBool` for fast O(1) `is_enabled` check
/// on the hot path, and `spawn_blocking` for writes so a slow capture
/// consumer cannot block the async worker.
#[derive(Clone)]
pub struct PcapManager {
    fds: Arc<RwLock<Vec<RawFd>>>,
    active: Arc<AtomicBool>,
}

impl Default for PcapManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PcapManager {
    pub fn new() -> Self {
        Self {
            fds: Arc::new(RwLock::new(Vec::new())),
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn insert_fd(&self, fd: RawFd) -> Result<(), String> {
        write_pcap_header(fd).map_err(|e| format!("pcap header write: {e}"))?;
        let mut fds = self.fds.write().await;
        fds.push(fd);
        self.active.store(true, Ordering::Relaxed);
        tracing::info!("Pcap: FD {fd} added ({} streams active)", fds.len());
        Ok(())
    }

    #[allow(unsafe_code)]
    pub async fn terminate(&self) {
        let mut fds = self.fds.write().await;
        for fd in fds.drain(..) {
            tracing::info!("Pcap: closing FD {fd}");
            unsafe {
                libc::close(fd);
            }
        }
        self.active.store(false, Ordering::Relaxed);
    }

    /// O(1) fast check — no async lock, suitable for the hot path.
    pub fn is_enabled(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// Push a pcap packet to all active capture FDs via `spawn_blocking`.
    pub fn push_packet(&self, payload: Vec<u8>) {
        if !self.is_enabled() {
            return;
        }
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let fds = this.fds.blocking_read();
            let mut remove = Vec::new();
            for &fd in fds.iter() {
                if write_pcap_record(fd, &payload).is_err() {
                    remove.push(fd);
                }
            }
            drop(fds);
            if !remove.is_empty() {
                let mut fds = this.fds.blocking_write();
                for fd in &remove {
                    fds.retain(|&f| f != *fd);
                    #[allow(unsafe_code)]
                    unsafe {
                        libc::close(*fd);
                    }
                    tracing::warn!("Pcap: removed broken FD {fd}");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcap_header_size() {
        assert_eq!(size_of::<PcapGlobalHeader>(), 24);
    }

    #[test]
    fn pcap_record_header_size() {
        assert_eq!(size_of::<PcapRecordHeader>(), 16);
    }

    #[test]
    fn ppi_header_size() {
        assert_eq!(size_of::<PpiHeader>(), 8);
    }
}
