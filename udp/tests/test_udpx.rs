#[cfg(test)]
mod test_utils;

use std::{sync::mpsc, thread};

use test_utils::*;
use udpx::{transport::UdpxStream, util::logging::init_logging};

/// Tests the UDPx handshake. This test spins up a ServerDropper and attempts to
/// start a handshake
#[test]
fn test_handshake() {
    init_logging(true); // For better debugging info
    let handle = ServerDropper::udpxserver();
    let addr = handle.addr();
    UdpxStream::connect(addr).unwrap();
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
    for _ in 0..25 {
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
