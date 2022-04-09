//! Abstractions to support swapping between TCP and UDPx implementations with
//! ease

use std::io::{self, Read, Write};
use std::net::SocketAddr;

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

/// A generic version of [std::net::tcp::Incoming] that works on any kind of
/// [Listeners](Listener)
pub struct StreamIterator<'a, S: Stream> {
    listener: &'a dyn Listener<'a, S, Self>,
}

impl<'a, S: Stream> StreamIterator<'a, S> {
    /// Wraps the provided listener,
    pub fn new(listener: &'a dyn Listener<'a, S, Self>) -> Self {
        Self { listener }
    }
}

impl<'a, S: Stream> Iterator for StreamIterator<'a, S> {
    type Item = io::Result<S>;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.listener.accept().map(|p| p.0))
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
