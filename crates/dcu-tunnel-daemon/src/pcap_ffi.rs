//! Raw-kernel `unsafe` shims for pcap capture.
//!
//! `File::from_raw_fd`, struct-to-byte casts, and `libc::close` are
//! isolated here. The rest of `pcap.rs` sees only safe wrappers.

#![allow(unsafe_code)]

use std::io::Write;
use std::os::fd::FromRawFd;
use std::os::unix::io::RawFd;

/// pcap global file header (little-endian).
#[repr(C, packed)]
pub struct PcapGlobalHeader {
    pub magic: u32,
    pub version_major: u16,
    pub version_minor: u16,
    pub gmt_offset: i32,
    pub accuracy: u32,
    pub snap_len: u32,
    pub dlt: u32,
}

/// pcap per-packet record header (little-endian).
#[repr(C, packed)]
pub struct PcapRecordHeader {
    pub ts_sec: u32,
    pub ts_usec: u32,
    pub incl_len: u32,
    pub orig_len: u32,
}

/// PPI (Per-Packet Information) header for 802.15.4 captures.
#[repr(C, packed)]
pub struct PpiHeader {
    pub version: u8,
    pub flags: u8,
    pub size: u16,
    pub dlt: u32,
}

/// Write a pcap global header to a raw fd.
pub fn write_pcap_header(fd: RawFd, header: &PcapGlobalHeader) -> std::io::Result<()> {
    let buf = unsafe {
        std::slice::from_raw_parts(
            header as *const _ as *const u8,
            std::mem::size_of::<PcapGlobalHeader>(),
        )
    };
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.write_all(buf)?;
    std::mem::forget(file);
    Ok(())
}

/// Write a pcap record (record header + PPI header + payload) to a raw fd.
pub fn write_pcap_record(
    fd: RawFd,
    record: &PcapRecordHeader,
    ppi: &PpiHeader,
    payload: &[u8],
) -> std::io::Result<()> {
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.write_all(unsafe {
        std::slice::from_raw_parts(
            record as *const _ as *const u8,
            std::mem::size_of::<PcapRecordHeader>(),
        )
    })?;
    file.write_all(unsafe {
        std::slice::from_raw_parts(
            ppi as *const _ as *const u8,
            std::mem::size_of::<PpiHeader>(),
        )
    })?;
    file.write_all(payload)?;
    std::mem::forget(file);
    Ok(())
}

/// `close(fd)` — close a raw file descriptor.
pub fn close_fd(fd: RawFd) {
    unsafe {
        libc::close(fd);
    }
}
