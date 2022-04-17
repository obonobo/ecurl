mod utils;
use std::{io::Write, sync::mpsc, thread, time::Duration};
use udpx::{
    transport::{UdpxListener, UdpxStream},
    util::{self, read_all},
    Bindable, Listener,
};
pub use utils::*;

/// Tests the UDPx handshake. This test spins up a ServerDropper and attempts to
/// start a handshake
#[test]
fn test_handshake() {
    LOGS.initialize();
    let handle = ServerDropper::udpxserver();
    let addr = handle.addr();
    UdpxStream::connect(addr).unwrap();
}

/// Tests the UPDx handshake without trying to read data from the socket (like
/// the [ServerDropper] does)
#[test]
fn test_handshake_raw() {
    LOGS.initialize();

    let (addrsend, addrrecv) = mpsc::channel();
    let (errsend, errecv) = mpsc::channel();
    thread::spawn(move || {
        let server_error = UdpxListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr().map(|addr| (l, addr)))
            .and_then(|(l, a)| addrsend.send(a).map(|_| l).map_err(util::InTwo::intwo))
            .expect("Server failed to start correctly")
            .accept()
            .err();
        errsend.send(server_error).expect("Server side error");
    });

    // Assert no client errors
    let addr = addrrecv
        .recv_timeout(Duration::from_millis(100))
        .expect("Server failed to report its address within timeout window");
    UdpxStream::connect(addr).expect("Client side error");

    // Assert no server errors
    assert!(errecv
        .recv_timeout(Duration::from_millis(100))
        .expect("Server failed to report connection error")
        .is_none());
}

/// Tests the UPDx handshake with many clients all trying to connect at the same
/// time
#[test]
fn test_concurrent_handshakes() {
    LOGS.initialize();
    let handle = ServerDropper::udpxserver();
    let addr = handle.addr();
    let (resin, resout) = mpsc::channel();

    // Spawn threads
    let n = 25;
    for _ in 0..n {
        let (resin, addr) = (resin.clone(), addr.clone());
        thread::spawn(move || {
            resin.send(UdpxStream::connect(addr)).unwrap();
        });
    }
    drop(resin);

    // Assert results
    for res in resout {
        assert!(
            res.is_ok(),
            "Expected no connection errors, but got: {}",
            DisplayResult(res)
        );
    }
}

/// Tests some basic reading from a socket server side
#[test]
fn test_read() {
    LOGS.initialize();

    // Start a server thread
    let (addrsend, addrrecv) = mpsc::channel();
    let (msgsend, msgrecv) = mpsc::channel();
    thread::spawn(move || {
        let msg = UdpxListener::bind("127.0.0.1:0")
            .and_then(|mut l| {
                addrsend.send(l.local_addr()?).map_err(util::InTwo::intwo)?;
                l.accept()
            })
            .map(|s| s.0)
            .map(read_all);

        msgsend
            .send(msg)
            .expect("Failed to send server thread results");
    });

    // First grab the address reported by the server
    let server_addr = addrrecv
        .recv_timeout(Duration::from_millis(100))
        .expect("Server did not report its address within the timeout window");

    // Try to write a message to the server
    let mut sock = UdpxStream::connect(server_addr).unwrap();
    let msg = b"Hello world!";
    sock.write_all(msg).unwrap();

    // The server should now have reported the message it read
    let server_msg = msgrecv
        .recv_timeout(Duration::from_millis(100))
        .expect("Server did not report its received message within the timeout window")
        .expect("Server failed to properly receive the message");

    assert_eq!(msg, server_msg.as_bytes());
}

/// Tests some basic writing from client side
#[test]
fn test_write() {}
