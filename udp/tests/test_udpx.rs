mod utils;
use std::{io::Write, sync::mpsc, thread, time::Duration};
use udpx::{transport::UdpxStream, util::logging::init_logging, Listener};
pub use utils::*;

/// Tests the UDPx handshake. This test spins up a ServerDropper and attempts to
/// start a handshake
#[test]
fn test_handshake() {
    init_logging(true);
    let handle = ServerDropper::udpxserver();
    let addr = handle.addr();
    UdpxStream::connect(addr).unwrap();
}

/// Tests the UPDx handshake without trying to read data from the socket (like
/// the [ServerDropper] does)
#[test]
fn test_handshake_raw() {
    init_logging(true);

    // Spin up a UDPx server that does 1 handshake
    let (errsend, errecv) = mpsc::channel();
    let addr = simple_udpx::serve(move |mut l| {
        errsend
            .send(l.accept().err())
            .expect("Server: failed to send accept error")
    });

    // Assert no client errors
    UdpxStream::connect(addr).expect("Client side error");

    // Assert no server errors
    let server_error = errecv
        .recv_timeout(Duration::from_millis(100))
        .expect("Server failed to report connection error");
    assert!(server_error.is_none(), "Expecting no server errors");
}

/// Tests the UPDx handshake with many clients all trying to connect at the same
/// time
#[test]
fn test_concurrent_handshakes() {
    init_logging(true);
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

/// Codegen for [assert_echo] tests
///
/// # Examples
/// ```
/// test_echo! { test_echo_small: "Hello world!" }
/// ```
macro_rules! test_echo {($($name:ident: $msg:expr,)*) => {$(
    #[test]
    fn $name() {
        init_logging(true);
        let (msg, chunkify) = $msg;
        assert_echo(&msg, chunkify);
    }
)*};}

test_echo! {
    test_echo_small: ("Hello world!", 0),
    test_echo_big: ("Hello world!".repeat(1024), 0),
    test_echo_very_big: (random_string(1<<20), 2048),
}

/// A parameterized test function that does one round trip sending the provided
/// message
fn assert_echo(msg: &str, chunkify: usize) {
    let (addr, msgrecv) = simple_udpx::echo_server();

    // Try to write a message to the server
    let mut sock = UdpxStream::connect(addr).unwrap();
    let msg = msg.as_bytes();
    if chunkify > 0 {
        msg.chunks(1 << 10).for_each(|b| sock.write_all(b).unwrap());
    } else {
        sock.write_all(msg).unwrap();
    }
    sock.shutdown().unwrap();

    // The server should now have reported the message it read
    let server_msg = msgrecv
        .recv_timeout(Duration::from_millis(1000))
        .expect("Server did not report its received message within the timeout window")
        .expect("Server failed to properly receive the message");

    let msg_debug = msg.iter().map(|b| char::from(*b)).collect::<String>();
    if msg != server_msg.as_bytes() {
        println!("original msg: {}", msg_debug);
        println!("new msg: {}", server_msg);
        println!("equal? {}", msg_debug == server_msg);
        println!(
            "original len = {}, new len = {}",
            msg_debug.len(),
            server_msg.len()
        );
    }
    assert_eq!(msg, server_msg.as_bytes());
}
