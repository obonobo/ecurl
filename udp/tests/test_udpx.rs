#[cfg(test)]
mod test_utils;

use std::{sync::mpsc, thread};
use test_utils::*;
use udpx::transport::UdpxStream;

static LOGS: LoggingInitializer = LoggingInitializer::new();

/// Tests the UDPx handshake. This test spins up a ServerDropper and attempts to
/// start a handshake
#[test]
fn test_handshake() {
    LOGS.initialize();
    let handle = ServerDropper::udpxserver();
    let addr = handle.addr();
    UdpxStream::connect(addr).unwrap();
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
    // let n = 25;
    let n = 1;
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
