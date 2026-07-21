//! End-to-end exercise of the `fd:` NCP transport.
//!
//! The `fd:` transport (`FdTransport`) wraps a caller-supplied file
//! descriptor — the daemon `dup()`s it, sets it non-blocking, and owns the
//! copy. It was implemented but never exercised against real I/O; these tests
//! drive it through the same public `dispatch::open_transport` entry point the
//! daemon uses, over a real Unix socketpair, and assert bytes round-trip in
//! both directions.

use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream as TokioUnixStream;

use dcu_serial::Transport;
use dcu_serial::dispatch::open_transport;

/// Bytes that look like a Spinel frame: header + property + payload. They have
/// no special meaning here; we only assert the transport carries them verbatim.
const HOST_TO_NCP: &[u8] = b"\x80\x01\x02\x71\x00hello-ncp";
const NCP_TO_HOST: &[u8] = b"\x80\x02\x06\x71reply";

#[tokio::test]
async fn fd_transport_round_trips_over_a_real_socketpair() {
    // One socketpair: `transport_side` is handed to the transport by fd number,
    // `peer` stands in for the NCP process on the other end of the wire.
    let (peer, transport_side) = UnixStream::pair().expect("socketpair");
    let fd = transport_side.as_raw_fd();

    // Open through the public dispatch path the daemon uses (`fd:<N>`).
    let mut transport = open_transport(&format!("fd:{fd}"), 115200)
        .await
        .expect("open fd: transport");

    // The transport must own an independent dup(): closing the original fd
    // here must NOT break subsequent I/O on the transport.
    drop(transport_side);

    let mut peer = {
        // tokio refuses to register a blocking fd; the transport sets its dup
        // non-blocking internally, so mirror that on the peer before converting.
        peer.set_nonblocking(true).expect("set peer non-blocking");
        TokioUnixStream::from_std(peer).expect("async peer")
    };

    // host -> NCP
    transport
        .write_all(HOST_TO_NCP)
        .await
        .expect("transport write");
    transport.flush().await.expect("transport flush");
    let mut got = vec![0u8; HOST_TO_NCP.len()];
    peer.read_exact(&mut got).await.expect("peer read");
    assert_eq!(
        got, HOST_TO_NCP,
        "bytes written by the transport must reach the NCP side"
    );

    // NCP -> host
    peer.write_all(NCP_TO_HOST).await.expect("peer write");
    peer.flush().await.expect("peer flush");
    let mut got = vec![0u8; NCP_TO_HOST.len()];
    transport
        .read_exact(&mut got)
        .await
        .expect("transport read");
    assert_eq!(
        got, NCP_TO_HOST,
        "bytes written by the NCP side must reach the transport"
    );

    assert!(
        transport.info().starts_with("fd:"),
        "info() should report the fd: prefix, got {}",
        transport.info()
    );
}

#[tokio::test]
async fn fd_transport_rejects_a_non_numeric_descriptor() {
    let err = match open_transport("fd:not-a-number", 115200).await {
        Ok(_) => panic!("a non-numeric fd must fail"),
        Err(e) => e,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("invalid descriptor"),
        "unexpected error message: {msg}"
    );
}

#[tokio::test]
async fn fd_transport_rejects_a_closed_descriptor() {
    // A descriptor this large is not open in the test process; dup() -> EBADF.
    let err = match open_transport("fd:99999", 115200).await {
        Ok(_) => panic!("a closed fd must fail"),
        Err(e) => e,
    };
    assert!(
        !err.to_string().is_empty(),
        "the error should carry a message"
    );
}
