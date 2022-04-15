//! Abstractions to support swapping between TCP and UDPx implementations with
//! ease

use std::io::{self, Read, Write};
use std::marker::PhantomData;
use std::net::{SocketAddr, ToSocketAddrs};

/// A factory method for creating [Listeners](Listener)
pub trait Bindable<S: Stream, L: Listener<S>> {
    /// Binds to the specified address and listens for incoming connections
    fn bind(addr: impl ToSocketAddrs) -> io::Result<L>;
}

/// Mimicks [std::net::tcp::Incoming]
pub trait Incoming<S, I>
where
    S: Stream,
    I: Iterator<Item = io::Result<S>>,
{
    fn incoming(self) -> I;
}

/// Mimicks [std::net::tcp::TcpListener]
pub trait Listener<S: Stream> {
    fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()>;
    fn accept(&mut self) -> io::Result<(S, SocketAddr)>;
}

// Blanket implementation. All Listeners implement Incoming automatically
impl<S, L> Incoming<S, StreamIterator<S, L>> for L
where
    S: Stream,
    L: Listener<S>,
{
    fn incoming(self) -> StreamIterator<S, L> {
        StreamIterator::new(self)
    }
}

/// Mimicks [std::net::tcp::TcpStream]. Note that all [Streams](Stream) are also
/// [Readers](Read) as well as [Writers](Write).
pub trait Stream: Read + Write {
    fn peer_addr(&self) -> io::Result<SocketAddr>;
}

/// A generic version of [std::net::tcp::Incoming] that works on any kind of
/// [Listeners](Listener)
pub struct StreamIterator<S: Stream, L: Listener<S>> {
    listener: L,
    _s: PhantomData<S>, // This is ridiculous
}

impl<S: Stream, L: Listener<S>> StreamIterator<S, L> {
    /// Wraps the provided listener,
    pub fn new(listener: L) -> Self {
        Self {
            listener,
            _s: PhantomData,
        }
    }
}

impl<S: Stream, L: Listener<S>> Iterator for StreamIterator<S, L> {
    type Item = io::Result<S>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.listener.accept().map(|p| p.0))
    }
}

/// Adaptors for [std::net::tcp], contains implementations of our traits for the
/// stdlib TCP package.
mod adaptors {
    use super::{Bindable, Listener, Stream};
    use std::io::{self, Result};
    use std::net::{SocketAddr, TcpListener, TcpStream};

    // Delegates
    impl Bindable<TcpStream, Self> for TcpListener {
        fn bind(addr: impl std::net::ToSocketAddrs) -> io::Result<Self> {
            TcpListener::bind(addr)
        }
    }
    impl Stream for TcpStream {
        fn peer_addr(&self) -> Result<SocketAddr> {
            self.peer_addr()
        }
    }
    impl Listener<TcpStream> for TcpListener {
        fn set_nonblocking(&self, nonblocking: bool) -> Result<()> {
            self.set_nonblocking(nonblocking)
        }
        fn accept(&mut self) -> Result<(TcpStream, SocketAddr)> {
            TcpListener::accept(self)
        }
    }
}
