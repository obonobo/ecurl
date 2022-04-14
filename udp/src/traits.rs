//! Abstractions to support swapping between TCP and UDPx implementations with
//! ease

use std::io::{self, Read, Write};
use std::net::SocketAddr;

/// Mimicks [std::net::tcp::TcpListener::incoming()]
pub trait Incoming<'a, S, I>
where
    S: Stream,
    I: Iterator<Item = io::Result<S>> + 'a,
{
    fn incoming(&'a mut self) -> I;
}

/// Mimicks [std::net::tcp::TcpListener]
pub trait Listener<'a, S>
where
    S: Stream,
{
    fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()>;
    fn accept(&mut self) -> io::Result<(S, SocketAddr)>;
}

// Blanket implementation. All Listeners implement Incoming automatically
impl<'a, S, L> Incoming<'a, S, StreamIterator<'a, S>> for L
where
    S: Stream,
    L: Listener<'a, S>,
{
    fn incoming(&'a mut self) -> StreamIterator<'a, S> {
        StreamIterator { listener: self }
    }
}

/// Mimicks [std::net::tcp::TcpStream]. Note that all [Streams](Stream) are also
/// [Readers](Read) as well as [Writers](Write).
pub trait Stream: Read + Write {
    fn peer_addr(&self) -> io::Result<SocketAddr>;
}

/// A generic version of [std::net::tcp::Incoming] that works on any kind of
/// [Listeners](Listener)
pub struct StreamIterator<'a, S: Stream> {
    listener: &'a mut dyn Listener<'a, S>,
}

impl<'a, S: Stream> StreamIterator<'a, S> {
    /// Wraps the provided listener,
    pub fn new(listener: &'a mut dyn Listener<'a, S>) -> Self {
        Self { listener }
    }
}

impl<'a, S: Stream> Iterator for StreamIterator<'a, S> {
    type Item = io::Result<S>;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.listener.accept().map(|p| p.0))
    }
}

/// Adaptors for [std::net::tcp], contains implementations of our traits for the
/// stdlib TCP package.
mod adaptors {
    use super::{Listener, Stream};
    use std::io::Result;
    use std::net::{SocketAddr, TcpListener, TcpStream};

    // Delegates
    #[rustfmt::skip]
    impl Stream for TcpStream {
        fn peer_addr(&self) -> Result<SocketAddr> { self.peer_addr() }
    }
    #[rustfmt::skip]
    impl<'a> Listener<'a, TcpStream> for TcpListener {
        fn set_nonblocking(&self, nonblocking: bool) -> Result<()> { self.set_nonblocking(nonblocking) }
        fn accept(&mut self) -> Result<(TcpStream, SocketAddr)> { TcpListener::accept(self) }
    }
}
