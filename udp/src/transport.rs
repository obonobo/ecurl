//! # Transport Layer
//!
//! Handshakes (opening + closing), connection management, etc.
//!
//! The API in this module has been designed to mimick the functionality of the
//! [std::net::tcp] package. Since this API will be used to implement HTTP, it
//! needs to be easy to swap between the two transports.

use crate::packet::{data_buffer, packet_buffer, Packet, PacketType};
use crate::{Listener, Stream, StreamIterator};

use std::io::{self, Error, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use super::packet::PacketBuffer;

pub type UdpxIncoming<'a> = StreamIterator<'a, UdpxStream>;

pub struct UdpxListener {
    sock: UdpSocket,

    /// Buffer for handshaking
    buf: PacketBuffer,

    /// In millis
    timeout: u64,
}

impl UdpxListener {
    pub const DEFAULT_TIMEOUT: u64 = 50;

    pub fn bind(addr: impl ToSocketAddrs) -> io::Result<UdpxListener> {
        Ok(Self {
            sock: UdpSocket::bind(addr)?,
            buf: packet_buffer(),
            timeout: Self::DEFAULT_TIMEOUT,
        })
    }

    /// Does a UDPx open connection handshake
    fn handshake(&mut self, addr: SocketAddr, packet: &Packet) -> io::Result<()> {
        if packet.ptyp != PacketType::Syn {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "handshake failure: first packet received is not a SYN",
            ));
        }

        // The handshake needs to send a SYN-ACK packet in response and then
        // wait for the ACK to this packet. Do a timeout as well if the packet
        // SYN-ACK is not received
        let send = Packet {
            ptyp: PacketType::SynAck,

            // Normally, we would set the ack number to A + 1, and
            // the seq to B, but we don't have an ack field in these
            // packets, so we will simply increment the seq for our
            // response packet (i.e. seq is A + 1)
            nseq: packet.nseq + 1,

            // We technically only support ipv4 addresses
            peer: if let SocketAddr::V4(addr) = addr {
                addr.ip().to_owned()
            } else {
                Ipv4Addr::new(127, 0, 0, 1)
            },

            port: packet.port,
            ..Default::default()
        };

        let n = send.write_to(&mut self.buf)?;
        self.sock.send_to(&self.buf[..n], addr)?;

        // Wait for the response - 5 tries
        let packett = (|| {
            for _ in 0..5 {
                self.sock.set_read_timeout(Some(self.timeout()))?;
                return match self.sock.recv_from(&mut self.buf) {
                    Ok((_, addrr)) if addrr != addr => continue,
                    Ok((n, _)) => Packet::try_from(&self.buf[..n]),
                    Err(e) if e.kind() == ErrorKind::TimedOut => continue,
                    Err(e) => return Err(e),
                };
            }
            Err(Error::new(
                ErrorKind::TimedOut,
                "timed out waiting for ACK to my SYN-ACK",
            ))
        })()?;

        // This packet should be an ACK
        if packett.ptyp != PacketType::Ack {
            return Err(Error::new(
                ErrorKind::Other,
                format!(
                    "{} packet has type {} but should be ACK",
                    "received a non-ACK in response to my SYN-ACK, ", packet.ptyp,
                ),
            ));
        } else if packett.nseq != packet.nseq + 2 {
            return Err(Error::new(
                ErrorKind::Other,
                format!(
                    "bad sequence number on ACK response to SYN-ACK, got {} but expected {}",
                    packett.nseq,
                    packet.nseq + 2
                ),
            ));
        }

        Ok(())
    }

    fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout)
    }
}

impl<'a> Listener<'a, UdpxStream, UdpxIncoming<'a>> for UdpxListener {
    fn set_nonblocking(&self, _: bool) -> io::Result<()> {
        todo!()
    }

    /// Returns a new UDPX stream as well as the address of the remote peer
    fn accept(&mut self) -> io::Result<(UdpxStream, SocketAddr)> {
        // Do a handshake
        let (n, addr) = self.sock.recv_from(&mut self.buf)?;
        let packet = Packet::try_from(&self.buf[..n])?;
        self.handshake(addr, &packet)?;

        // TODO: debug
        eprintln!("DEBUG: handshake completed with addr {}", addr);

        Ok((UdpxStream::new(self.sock.try_clone()?), addr))
    }

    // Returns an iterator on the incoming connections
    fn incoming(&'a self) -> UdpxIncoming<'_> {
        todo!()
    }
}

pub struct UdpxStream {
    sock: UdpSocket,
    buf: PacketBuffer,
}

impl UdpxStream {
    fn new(sock: UdpSocket) -> UdpxStream {
        UdpxStream {
            sock,
            buf: packet_buffer(),
        }
    }

    pub fn connect(addr: impl ToSocketAddrs) -> io::Result<UdpxStream> {
        Self::new(Self::random_socket()?).handshake(addr)
    }

    /// Performs the client side of the handshake
    fn handshake(self, addr: impl ToSocketAddrs) -> io::Result<Self> {
        self.sock.connect(addr)?;

        // Send the SYN packet
        let packet = Packet {
            ptyp: todo!(),
            nseq: todo!(),
            peer: todo!(),
            port: todo!(),
            ..Default::default()
        };

        packet.write_to(&mut self.buf)?;
        self.sock.send(&self.buf)?;

        todo!()
    }

    /// Binds to a random UDP socket for the client to use
    pub fn random_socket() -> io::Result<UdpSocket> {
        UdpSocket::bind(super::random_udp_socket_addr())
    }
}

impl Stream for UdpxStream {
    fn peer_addr(&self) -> io::Result<SocketAddr> {
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
