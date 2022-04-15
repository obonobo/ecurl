//! # Transport Layer
//!
//! Handshakes (opening + closing), connection management, etc.
//!
//! The API in this module has been designed to mimick the functionality of the
//! [std::net::tcp] package. Since this API will be used to implement HTTP, it
//! needs to be easy to swap between the two transports.

use crate::packet::{packet_buffer, Packet, PacketType};
use crate::util::random_udp_socket_addr;
use crate::{Bindable, Listener, Stream, StreamIterator};

use std::fmt::Display;
use std::io::{self, Error, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use super::packet::PacketBuffer;

pub const DEFAULT_TIMEOUT: u64 = 50;

pub type UdpxIncoming<'a> = StreamIterator<'a, UdpxStream, UdpxListener>;

pub struct UdpxListener {
    /// Dispatch socket or completing handshakes. After performing the
    /// handshake, the listener will bind a new socket for the rest of the
    /// conversation, leaving this socket free to accept more connections.
    sock: UdpSocket,

    /// Buffer for handshaking
    buf: PacketBuffer,

    /// In millis
    timeout: u64,
}

impl<'a> Bindable<'a, UdpxStream, Self> for UdpxListener {
    fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Ok(Self {
            sock: UdpSocket::bind(addr)?,
            buf: packet_buffer(),
            timeout: DEFAULT_TIMEOUT,
        })
    }
}

impl UdpxListener {
    pub fn with_timeout(self, timeout: u64) -> Self {
        Self { timeout, ..self }
    }

    /// Does a UDPx open connection handshake. Returns the response packet, the
    /// starting sequence number for future received data packets as well as the
    /// negotiated [UdpSocket].
    ///
    /// The handshake spawns a new UdpSocket and sends the SYN-ACK from that new
    /// address. Clients must send future messages to the remote address of the
    /// SYN-ACK. The main [Listener] socket is only for accepting handshakes and
    /// starting new connections - the rest of the conversation happens on the
    /// dispatched socket.
    fn handshake(
        &mut self,
        addr: SocketAddr,
        packet: &Packet,
    ) -> io::Result<(Packet, u32, UdpSocket)> {
        log::debug!("Beginning handshake with {}", addr);
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

            port: addr.port(),
            ..Default::default()
        };

        // We need to create a new UdpSocket for our response - let the OS
        // choose the port
        let sock = UdpSocket::bind("localhost:0")?;
        log::debug!(
            "Dispatching to new UDP socket for the rest of the conversation, new socket = {}",
            sock.local_addr()?
        );

        // Send the SYN-ACK and wait for a response packet. It should be ACK,
        // but if the ACK get's dropped, we will accept the next DATA packet as
        // well
        let n = send.write_to(&mut self.buf[..])?;
        let (ack_or_data, remote) = reliable_send(
            &self.buf[..n],
            // &self.sock, // TODO: main socket, remove this once you have the dispatch working
            &sock,
            addr,
            self.timeout(),
            PacketType::SynAck,
            &[PacketType::Ack, PacketType::Data],
            true,
        )?;

        // This packet should be an ACK or DATA packet
        sock.connect(remote)?;
        let nseq = packet.nseq + 3;
        match ack_or_data.ptyp {
            PacketType::Data => Ok((ack_or_data, nseq, sock)),
            PacketType::Ack => {
                // If it's an ACK, check the seq number, otherwise return
                if ack_or_data.nseq != packet.nseq + 2 {
                    Err(Error::new(
                        ErrorKind::Other,
                        format!(
                        "bad sequence number on ACK response to SYN-ACK, got {} but expected {}",
                        ack_or_data.nseq,
                        packet.nseq + 2),
                    ))
                } else {
                    Ok((ack_or_data, nseq, sock))
                }
            }
            _ => Err(Error::new(
                ErrorKind::Other,
                [
                    "received a non-ACK in response to my SYN-ACK, ",
                    &format!("packet has type {} but should be ACK", ack_or_data.ptyp),
                ]
                .join(""),
            )),
        }
    }

    fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout)
    }
}

impl<'a> Listener<'a, UdpxStream> for UdpxListener {
    // TODO: This function needs to set the underlying UDP socket of the server
    // to be nonblocking. Remember that this socket is the one accepting
    // connections.
    fn set_nonblocking(&self, _: bool) -> io::Result<()> {
        todo!()
    }

    /// Returns a new UDPX stream as well as the address of the remote peer
    fn accept(&mut self) -> io::Result<(UdpxStream, SocketAddr)> {
        // Do a handshake
        let (n, addr) = self.sock.recv_from(&mut self.buf)?;
        let packet = Packet::try_from(&self.buf[..n])?;
        let (packet, nseq, sock) = self.handshake(addr, &packet)?;
        let stream = UdpxStream::new(sock, nseq).with_starting_data([packet]);
        log::debug!("handshake completed with addr {}", addr);
        Ok((stream, addr))
    }
}

pub struct UdpxStream {
    sock: UdpSocket,
    buf: PacketBuffer,
    remote: SocketAddrV4,
    timeout: u64,
    packets_received: Vec<Packet>,
    packets_sent: Vec<Packet>,
    next_nseq: u32,
}

impl UdpxStream {
    /// The sequence number of the first DATA packet sent in a conversation
    pub const FIRST_NSEQ: u32 = 4;

    /// Returns an error if the ip is not ipv4
    pub fn connect(addr: impl ToSocketAddrs) -> io::Result<UdpxStream> {
        Self::new(Self::random_socket()?, Self::FIRST_NSEQ).handshake(addr)
    }

    fn new(sock: UdpSocket, nseq: u32) -> Self {
        let remote = sock
            .peer_addr()
            .and_then(|ip| match ip {
                SocketAddr::V4(addr) => Ok(addr),
                SocketAddr::V6(addr) => {
                    let err = io::Error::new(
                        io::ErrorKind::Other,
                        format!("not an ipv4 address ({})", addr),
                    );
                    log::error!("Bad remote addr: {}", err);
                    log::error!("Using default addr as remote...");
                    Err(err)
                }
            })
            .unwrap_or_else(|_| SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0));

        Self {
            sock,
            timeout: DEFAULT_TIMEOUT,
            buf: packet_buffer(),
            remote,
            next_nseq: nseq,
            packets_received: Vec::with_capacity(32),
            packets_sent: Vec::with_capacity(32),
        }
    }

    /// Used serverside to load initial data as received packets in some cases
    fn load_initial_data_packets(mut self, initial_data: impl IntoIterator<Item = Packet>) -> Self {
        self.packets_received.extend(initial_data.into_iter());
        self
    }

    fn with_starting_data(self, initial_data: impl IntoIterator<Item = Packet>) -> Self {
        self.load_initial_data_packets(
            initial_data
                .into_iter()
                .filter(|p| p.ptyp == PacketType::Data),
        )
    }

    fn with_timeout(self, timeout: u64) -> Self {
        Self { timeout, ..self }
    }

    /// Performs the client side of the handshake
    fn handshake(mut self, addr: impl ToSocketAddrs) -> io::Result<Self> {
        let addr = to_ipv4(addr)?;
        log::debug!(
            "Initiating UDPx handshake with {}, local_addr = {}",
            addr,
            self.sock.local_addr()?
        );

        // Send the SYN packet
        let packet = Packet {
            ptyp: PacketType::Syn,
            nseq: 0,
            peer: *addr.ip(),
            port: addr.port(),
            ..Default::default()
        };
        let n = packet.write_to(&mut self.buf[..])?;

        let (syn_ack, remote) = reliable_send(
            &self.buf[..n],
            &self.sock,
            SocketAddr::V4(addr),
            self.timeout(),
            PacketType::Syn,
            &[PacketType::SynAck],
            false,
        )?;

        log::debug!(
            "Received SYN-ACK response from server (remote addr = {}): {:?}",
            remote,
            syn_ack
        );

        log::debug!("Setting socket remote peer to {}", remote);
        self.remote = to_ipv4(remote)?;
        self.sock.connect(remote)?;

        // Send the ACK packet. We will just send this packet without waiting
        // for a response
        log::debug!("Sending ACK packet to complete handshake");
        self.write_packet(&Packet {
            ptyp: PacketType::Ack,
            nseq: syn_ack.nseq + 1,
            peer: self.remote.ip().to_owned(),
            port: self.remote.port(),
            ..Default::default()
        })?;

        // Handshake is done!
        Ok(self)
    }

    fn write_packet(&mut self, packet: &Packet) -> io::Result<()> {
        packet.write_to(&mut self.buf[..])?;
        self.sock.send(&self.buf)?;
        Ok(())
    }

    /// Binds to a random UDP socket for the client to use
    pub fn random_socket() -> io::Result<UdpSocket> {
        UdpSocket::bind(random_udp_socket_addr())
    }

    fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout)
    }
}

impl Stream for UdpxStream {
    fn peer_addr(&self) -> io::Result<SocketAddr> {
        Ok(SocketAddr::V4(self.remote))
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

/// Extracts the next ipv4 address from the given address(es). If no ipv4
/// address exists, then an error is returned.
pub fn to_ipv4(addr: impl ToSocketAddrs) -> io::Result<SocketAddrV4> {
    addr.to_socket_addrs()?
        .flat_map(|a| match a {
            SocketAddr::V4(addr) => Some(addr),
            _ => None,
        })
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "only ipv4 addresses are supported"))
}

/// An extensions to [Iterator] that allows you to join anything with a
/// [ToString]
pub trait JoinIter {
    fn join(self, sep: &str) -> String;
}

impl<T: Display, I: Iterator<Item = T>> JoinIter for I {
    fn join(self, sep: &str) -> String {
        self.map(|e| e.to_string()).collect::<Vec<_>>().join(sep)
    }
}

/// Sends a packet (potentially multiple times) in a loop with a timeout and
/// waits for the response. Used for handshakes.
pub fn reliable_send(
    send: &[u8],
    sock: &UdpSocket,
    peer: SocketAddr,
    timeout: Duration,
    send_packet_type: PacketType,
    recv_packet_types: &[PacketType],
    skip_address_mismatch: bool,
) -> io::Result<(Packet, SocketAddr)> {
    // TODO: DEBUG
    let timeout = Duration::from_secs(100000);
    // TODO: DEBUG

    let mut recv = packet_buffer();
    let join = |packet_types: &[PacketType]| packet_types.iter().join(" or ");
    let joined = join(recv_packet_types);
    let mut invalid_response_packets = Vec::with_capacity(5);

    for i in 0..5 {
        log::debug!(
            "{}Sending {} packet, waiting for packets of type {}",
            if i > 0 {
                format!("(Attempt #{}) ", i)
            } else {
                String::new()
            },
            send_packet_type,
            joined,
        );

        sock.send_to(send, peer)?; // Resend the packet
        sock.set_read_timeout(Some(timeout))?;
        let (packet, remote) = match sock.recv_from(&mut recv) {
            Ok((_, addrr)) if skip_address_mismatch && addrr != peer => continue,
            Ok((n, addrr)) => Packet::try_from(&recv[..n]).map(|p| (p, addrr)),
            Err(e) if e.kind() == ErrorKind::TimedOut => continue,
            Err(e) => return Err(e),
        }?;

        // Check that the packet is of (one of) the types that we expect
        if !recv_packet_types.iter().any(|t| packet.ptyp == *t) {
            invalid_response_packets.push(packet.ptyp);
            continue;
        }

        return Ok((packet, remote));
    }

    Err(if !invalid_response_packets.is_empty() {
        Error::new(
            ErrorKind::Other,
            [
                "invalid response packets, expected to receive a packet of one of these types:",
                &format!(
                    "{}, but received only the following packets: {}",
                    joined,
                    join(&invalid_response_packets)
                ),
            ]
            .join(""),
        )
    } else {
        Error::new(
            ErrorKind::TimedOut,
            format!(
                "timed out waiting for valid response: send_packet={}, recv_packet={}",
                send_packet_type, joined
            ),
        )
    })
}
