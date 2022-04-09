//! # Transport Layer
//!
//! Handshakes (opening + closing), connection management, etc.
//!
//! The API in this module has been designed to mimick the functionality of the
//! [std::net::tcp] package. Since this API will be used to implement HTTP, it
//! needs to be easy to swap between the two transports.

use crate::{Listener, Stream, StreamIterator};

use std::io::{self, Read, Write};
use std::net::{ToSocketAddrs, UdpSocket};

pub type UdpxIncoming<'a> = StreamIterator<'a, UdpxStream>;

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

impl<'a> Listener<'a, UdpxStream, UdpxIncoming<'a>> for UdpxListener {
    fn set_nonblocking(&self, _: bool) -> io::Result<()> {
        todo!()
    }

    /// Returns a new UDPX stream as well as the address of the remote peer
    fn accept(&self) -> io::Result<(UdpxStream, std::net::SocketAddr)> {
        todo!()
    }

    // Returns an iterator on the incoming connections
    fn incoming(&'a self) -> UdpxIncoming<'_> {
        todo!()
    }
}

pub struct UdpxStream {
    sock: UdpSocket,
}

impl Stream for UdpxStream {
    fn peer_addr(&self) -> io::Result<std::net::SocketAddr> {
        todo!()
    }
}

impl Read for UdpxStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        todo!()
    }
}

impl Write for UdpxStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        todo!()
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
