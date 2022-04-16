#[cfg(test)]
mod test_utils;

use test_utils::*;
use udpx::{transport::UdpxStream, util::logging::init_logging};

/// Test the UDPx handshake. This test spins up a ServerDropper and attempts to
/// start a handshake
#[test]
fn test_handshake() {
    init_logging(true); // For better debugging info
    let handle = ServerDropper::udpxserver();
    let addr = handle.addr();
    UdpxStream::connect(addr).unwrap();
}
