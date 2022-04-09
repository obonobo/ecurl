//! Handshakes (opening + closing), as well as transfer
//!
//! The API in this module has been designed to mimick the functionality of the
//! [std::net::tcp] package. Since this API will be used to implement HTTP, it
//! needs to be easy to swap between the two transports.

use std::io;
use std::net::{ToSocketAddrs, UdpSocket};

pub struct UdpxListener {
    sock: UdpSocket,
}

impl UdpxListener {
    pub fn bind(addr: impl ToSocketAddrs) -> io::Result<UdpxListener> {
        Ok(Self {
            sock: UdpSocket::bind(addr)?,
        })
    }
}
