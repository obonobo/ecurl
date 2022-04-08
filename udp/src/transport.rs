//! Handshakes (opening + closing), as well as transfer
//!
//! The API in this module has been designed to mimick the functionality of the
//! [std::net::tcp] package. Since this API will be used to implement HTTP, it
//! needs to be easy to swap between the two transports.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, ToSocketAddrs};

/// Mimicks [std::net::tcp::TcpListener]
pub trait Listener<'a, S, I>
where
    S: Stream,
    I: Iterator<Item = io::Result<S>> + 'a,
{
    fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()>;
    fn accept(&self) -> io::Result<(S, SocketAddr)>;
    fn incoming(&'a self) -> I;
}

/// Mimicks [std::net::tcp::TcpStream]. Note that all [Streams](Stream) are also
/// [Readers](Read) as well as [Writers](Write).
pub trait Stream: Read + Write {
    fn peer_addr(&self) -> io::Result<SocketAddr>;
}

pub struct UdpxListener {}

impl UdpxListener {
    pub fn bind(addr: impl ToSocketAddrs) -> io::Result<UdpxListener> {
        todo!()
    }
}

/// Adaptors for [std::net::tcp]
mod adaptors {
    use super::{Listener, Stream};
    use std::io::Result;
    use std::net::{Incoming, SocketAddr, TcpListener, TcpStream};

    #[rustfmt::skip]
    impl Stream for TcpStream {
        fn peer_addr(&self) -> Result<SocketAddr> { self.peer_addr() }
    }

    #[rustfmt::skip]
    impl<'a> Listener<'a, TcpStream, Incoming<'a>> for TcpListener {
        fn set_nonblocking(&self, nonblocking: bool) -> Result<()> { self.set_nonblocking(nonblocking) }
        fn accept(&self) -> Result<(TcpStream, SocketAddr)> { self.accept() }
        fn incoming(&self) -> Incoming<'_> { self.incoming() }
    }
}
