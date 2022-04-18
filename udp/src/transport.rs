//! # Transport Layer
//!
//! Handshakes (opening + closing), connection management, etc.
//!
//! The API in this module has been designed to mimick the functionality of the
//! [std::net::tcp] package. Since this API will be used to implement HTTP, it
//! needs to be easy to swap between the two transports.

use crate::packet::{packet_buffer, Packet, PacketType};
use crate::util::{millis, random_udp_socket_addr, TruncateLeft};
use crate::{Bindable, Connectable, Listener, Stream, StreamIterator};

use std::collections::HashMap;
use std::fmt::Display;
use std::io::{self, Error, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::thread;
use std::time::Duration;

use super::packet::PacketBuffer;

pub const DEFAULT_TIMEOUT: u64 = 50;

pub type UdpxIncoming<'a> = StreamIterator<UdpxStream, UdpxListener>;

pub struct UdpxListener {
    /// Dispatch socket or completing handshakes. After performing the
    /// handshake, the listener will bind a new socket for the rest of the
    /// conversation, leaving this socket free to accept more connections.
    sock: UdpSocket,

    /// Buffer for handshaking
    buf: PacketBuffer,

    /// In millis
    timeout: u64,

    /// Whether the listener has been set to be nonblocking
    nonblocking: bool,
}

impl Bindable<UdpxStream> for UdpxListener {
    fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Ok(Self {
            sock: UdpSocket::bind(addr)?,
            buf: packet_buffer(),
            timeout: DEFAULT_TIMEOUT,
            nonblocking: false,
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
        let sock = UdpSocket::bind("127.0.0.1:0")?;
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
            &sock,
            addr,
            self.timeout(),
            PacketType::SynAck,
            &[PacketType::Ack, PacketType::Data],
            true,
            self.nonblocking,
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
                "received a non-ACK in response to my SYN-ACK, ".to_owned()
                    + &format!("packet has type {} but should be ACK", ack_or_data.ptyp),
            )),
        }
    }

    fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout)
    }
}

impl Listener<UdpxStream> for UdpxListener {
    // TODO: This function needs to set the underlying UDP socket of the server
    // to be nonblocking. Remember that this socket is the one accepting
    // connections.
    fn set_nonblocking(&mut self, nonblocking: bool) -> io::Result<()> {
        self.nonblocking = nonblocking;
        self.sock.set_nonblocking(nonblocking)
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

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sock.local_addr()
    }
}

/// A struct for keeping track of sent/received packets
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug)]
struct PacketTransfer {
    acked: bool,
    packet: Packet,
}

impl From<&PacketTransfer> for Packet {
    fn from(transfer: &PacketTransfer) -> Self {
        Self {
            ..transfer.packet.to_owned()
        }
    }
}

impl From<&Packet> for PacketTransfer {
    fn from(packet: &Packet) -> Self {
        Self {
            acked: false,
            packet: packet.to_owned(),
        }
    }
}

impl From<PacketTransfer> for Packet {
    fn from(transfer: PacketTransfer) -> Self {
        transfer.packet
    }
}

impl From<Packet> for PacketTransfer {
    fn from(packet: Packet) -> Self {
        Self {
            packet,
            ..Default::default()
        }
    }
}

/// Represents one side of a UDPx connection.
#[derive(Debug)]
pub struct UdpxStream {
    sock: UdpSocket,
    buf: PacketBuffer,
    remote: SocketAddrV4,
    timeout: u64,
    packets_received: HashMap<u32, PacketTransfer>,
    packets_sent: HashMap<u32, PacketTransfer>,
    next_nseq: u32,
    closed: bool,           // Whether the connection has been closed at the other end
    err: Option<io::Error>, // Socket error that has been registered during a read/write
}

impl Drop for UdpxStream {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

impl Connectable for UdpxStream {
    fn connect(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Self::new(Self::random_socket()?, Self::FIRST_NSEQ).handshake(addr)
    }
}

impl UdpxStream {
    /// The sequence number of the first DATA packet sent in a conversation
    pub const FIRST_NSEQ: u32 = 3;

    pub fn connect(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Connectable::connect(addr)
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sock.local_addr()
    }

    pub fn shutdown(&mut self) -> io::Result<()> {
        Stream::shutdown(self, std::net::Shutdown::Both)
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
            packets_received: HashMap::with_capacity(32),
            packets_sent: HashMap::with_capacity(32),
            err: None,
            closed: false,
        }
    }

    /// Used serverside to load initial data as received packets in some cases
    fn load_initial_data_packets(mut self, initial_data: impl IntoIterator<Item = Packet>) -> Self {
        self.packets_received
            .extend(initial_data.into_iter().map(|p| (p.nseq, p.into())));
        self
    }

    fn with_starting_data(self, initial_data: impl IntoIterator<Item = Packet>) -> Self {
        self.load_initial_data_packets(
            initial_data
                .into_iter()
                .filter(|p| p.ptyp == PacketType::Data),
        )
    }

    /// Performs the client side of the handshake
    fn handshake(mut self, addr: impl ToSocketAddrs) -> io::Result<Self> {
        let addr = try_to_ipv4(addr)?;
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
            false,
        )?;

        log::debug!(
            "Received SYN-ACK response from server (remote addr = {})",
            remote,
        );

        log::debug!("Setting socket remote peer to {}", remote);
        self.remote = try_to_ipv4(remote)?;
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
        log::debug!("Handshake complete! Returning UdpxStream");

        // Handshake is done!
        Ok(self)
    }

    fn write_packet(&mut self, packet: &Packet) -> io::Result<()> {
        let n = packet.write_to(&mut self.buf[..])?;
        self.sock.send(&self.buf[..n])?;
        Ok(())
    }

    /// Binds to a random UDP socket for the client to use
    fn random_socket() -> io::Result<UdpSocket> {
        UdpSocket::bind(random_udp_socket_addr())
    }

    fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout)
    }

    /// Sends an ACK for this packet
    fn acknowledge_packet(&mut self, transfer: PacketTransfer) -> io::Result<PacketTransfer> {
        let ack = Packet {
            ptyp: PacketType::Ack,
            nseq: transfer.packet.nseq,
            peer: self.remote.ip().to_owned(),
            port: self.remote.port(),
            ..Default::default()
        };

        self.sock
            .set_write_timeout(millis(30))
            .expect("Failed to set write timeout");

        let n = ack.write_to(&mut self.buf[..])?;
        self.sock.send(&self.buf[..n])?;
        Ok(transfer)
    }

    fn buffer_and_ack(&mut self, transfer: PacketTransfer) -> io::Result<()> {
        self.acknowledge_packet(transfer).map(|t| {
            self.packets_received.insert(t.packet.nseq, t);
        })
    }

    /// "Clones" the error registered on this stream
    fn copy_of_err(&self) -> Option<io::Error> {
        self.err
            .as_ref()
            .map(|e| io::Error::new(e.kind(), e.to_string()))
    }

    fn register_err(&mut self, err: io::Error) -> io::Error {
        self.err = Some(err);
        self.copy_of_err().unwrap()
    }

    fn packet_defaults(&self) -> Packet {
        Packet {
            peer: self.remote.ip().to_owned(),
            port: self.remote.port(),
            ..Default::default()
        }
    }

    fn closed_err(&self) -> io::Error {
        io::Error::new(io::ErrorKind::Other, "UdpxStream connection closed")
    }

    /// Returns the error, if any, that has been registered on the stream
    fn registered_err(&self) -> io::Result<()> {
        if self.closed {
            Err(self.closed_err())
        } else if let Some(err) = self.copy_of_err() {
            Err(err)
        } else {
            Ok(())
        }
    }

    /// Acknowledge that a FIN packet has been received
    fn fin_ack(&mut self) -> io::Result<()> {
        let fin_ack = Packet {
            ptyp: PacketType::FinAck,
            ..self.packet_defaults()
        };
        self.sock.set_write_timeout(millis(250))?;
        let n = fin_ack.write_to(&mut self.buf[..])?;
        let _ = self.sock.send(&self.buf[..n]); // ignore
        Ok(())
    }
}

impl Stream for UdpxStream {
    fn peer_addr(&self) -> io::Result<SocketAddr> {
        Ok(SocketAddr::V4(self.remote))
    }

    /// Sends a FIN packet and registers a `StreamClosed` error
    fn shutdown(&mut self, _: std::net::Shutdown) -> io::Result<()> {
        let fin = Packet {
            ptyp: PacketType::Fin,
            ..self.packet_defaults()
        };
        let mut fin_buf = packet_buffer();
        let n = fin.write_to(&mut fin_buf[..])?;
        let fin = &fin_buf[..n];

        // 10 tries to receive a FIN-ACK
        for _ in 0..10 {
            self.sock.set_write_timeout(millis(100))?;
            match self.sock.send(fin) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
                Err(e) => return Err(e),
            }

            // Await FIN-ACK
            self.sock.set_read_timeout(millis(30))?;
            match self.sock.recv(&mut self.buf[..]) {
                Ok(n) => match Packet::try_from(&self.buf[..n])?.ptyp {
                    PacketType::FinAck | PacketType::Fin => break,
                    _ => continue,
                },
                Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
                // Err(e) => return Err(e),
                _ => break, // For now, let's say that recv errors mean we can close
            };
        }
        self.closed = true;
        Ok(())
    }
}

impl Read for UdpxStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut red = 0;
        while red < buf.len() {
            self.registered_err()?;

            // TODO: for now we will wait forever
            // We will only try reading for a short period of time
            // self.sock.set_read_timeout(millis(50))?;
            self.sock.set_read_timeout(None).unwrap();

            // Grab a packet from either the received packets buffer, or a fresh
            // packet from the socket.
            let mut transfer = {
                if let Some(packet) = self.packets_received.remove(&self.next_nseq) {
                    packet
                } else {
                    let n = match self.sock.recv(&mut self.buf) {
                        Ok(n) => n,
                        Err(e) if e.kind() == ErrorKind::TimedOut => return Ok(red),
                        // Err(e) if e.kind() == ErrorKind::WouldBlock => return Err(e),
                        Err(e) => {
                            log::error!("UdpxStream::read(): {}", e);
                            log::error!("self = {}", self);
                            log::error!("sock = {:?}", self.sock);

                            // The behaviour we want here is that if the
                            // connection gets closed or something, then that is
                            // treated as EOF. In Rust, EOF is simply when you
                            // return Ok(0) from a read operation. We will
                            // register the error and return amount read. Next
                            // time this function is called, return the
                            // registered error.
                            self.register_err(e);
                            return Ok(red);
                        }
                    };

                    let transfer: PacketTransfer =
                        Packet::try_from(&self.buf[..n]).wrap_malpac()?.into();

                    if transfer.packet.ptyp == PacketType::Fin {
                        // Then the connection was closed at the other end.
                        // Terminate this part of the connection
                        self.closed = true;
                        self.fin_ack()?;
                        return Ok(red);
                    } else if transfer.packet.nseq < self.next_nseq {
                        // Then we have already acked this packet, this is a
                        // resent packet and our ack got dropped
                        self.acknowledge_packet(transfer)?;
                        continue;
                    } else if transfer.packet.nseq != self.next_nseq {
                        // Then buffer this packet, and try to read another
                        // one
                        self.buffer_and_ack(transfer)?;
                        continue;
                    }
                    self.acknowledge_packet(transfer)?
                }
            };

            // We now have the next packet in the sequence in hand, read as much
            // as possible into the buffer. If there is still data in the
            // packet, return it back to the queue and don't increment
            // next_seq
            let n = std::cmp::min(transfer.packet.data.len(), buf.len() - red);
            let into = &mut buf[red..n];
            let from = &transfer.packet.data[..n];
            into.copy_from_slice(from);
            red += n;
            transfer.packet.data.truncate_left(n);

            if transfer.packet.data.is_empty() {
                // This packet has been fully read, we can now drop it entirely
                self.next_nseq += 1;
            } else {
                // Then return this packet to the queue, we are not finished
                // reading it
                self.packets_received.insert(transfer.packet.nseq, transfer);
            }
        }
        Ok(red)
    }
}

impl Write for UdpxStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.registered_err()?;

        // Queue up the packets to be written
        self.packets_sent.extend(
            Packet::stream(buf)
                .packet_type(PacketType::Data)
                .remote(self.remote)
                .seq(self.next_nseq)
                .map(PacketTransfer::from)
                .map(|p| (p.packet.nseq, p)),
        );

        // TODO: for now we will put the ACK-loop right in here. In the future,
        // we may move the loop somewhere else, perhaps into the `flush` method?
        let mut n = 0;
        while !self.packets_sent.is_empty() {
            // Send/resend packets
            for transfer in self.packets_sent.values() {
                log::debug!(
                    "UdpxStream::write(): Sending packet (seq={}): {}",
                    transfer.packet.nseq,
                    transfer.packet
                );

                self.sock.set_write_timeout(millis(30)).unwrap();
                let n = transfer.packet.write_to(&mut self.buf[..]).unwrap();
                match self.sock.send(&self.buf[..n]) {
                    Ok(_) => {}
                    Err(e) if e.kind() == ErrorKind::TimedOut => continue,
                    Err(e) => return Err(self.register_err(e)),
                };
            }
            self.sock.set_write_timeout(None).unwrap();

            // Check for acked packets
            log::debug!(
                "Beginning wait for ACKs, unacked packets are [{}]",
                self.packets_sent.keys().join(", ")
            );

            self.sock.set_read_timeout(millis(30))?;
            for i in 0..self.packets_sent.len() {
                log::debug!("Waiting for ACK - {}", i);
                let packet = match self.sock.recv(&mut self.buf) {
                    Ok(n) => Packet::try_from(&self.buf[..n]).wrap_malpac()?,
                    Err(e) if e.kind() == ErrorKind::TimedOut => break,
                    Err(e) => {
                        log::error!("UdpxStream::write(): ({:?}) {}", e.kind(), e);
                        log::error!("self = {}", self);
                        log::error!("sock = {:?}", self.sock);
                        return Err(self.register_err(e));
                    }
                };

                // Skip non-ACK packet's (add them to our received-packets
                // buffer if they are DATA packets)
                match packet.ptyp {
                    PacketType::Ack => {
                        log::debug!("Got an ACK for seq {}", packet.nseq);
                        if let Some(p) = self.packets_sent.remove(&packet.nseq) {
                            log::debug!(
                                "Marking packet {} as ACKed, will not resend, removing from queue",
                                packet.nseq
                            );
                            log::debug!(
                                "{} remaining packets: [{}]",
                                self.packets_sent.len(),
                                self.packets_sent.iter().map(|p| p.0.to_string()).join(", ")
                            );
                            n += p.packet.data.len();
                        }
                    }
                    PacketType::Data => {
                        log::debug!(
                            "{}",
                            "Got a DATA packet in UdpxStream::write(), ".to_owned()
                                + "placing it in read-packets queue"
                        );
                        self.packets_received.insert(packet.nseq, packet.into());
                        continue;
                    }

                    // Drop packet otherwise; at this point in the conversation
                    // we should only be dealing with ACK or DATA packets
                    _ => continue,
                }
            }
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Display for UdpxStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let packets_string = |packets: &HashMap<u32, PacketTransfer>| {
            let joined = packets.iter().map(|p| p.0.to_string()).join(", ");
            if !joined.is_empty() {
                format!(" (ids are {})", joined)
            } else {
                joined
            }
        };

        write!(
            f,
            "{}",
            format!("UdpxStream[local_addr={}, ", to_ipv4(&self.sock))
                + &format!("remote_addr={}, ", self.remote)
                + &format!("nseq={}, ", self.next_nseq)
                + &format!(
                    "{} recv packets{}, {} send packets{}]",
                    self.packets_received.len(),
                    packets_string(&self.packets_received),
                    self.packets_sent.len(),
                    packets_string(&self.packets_sent)
                )
        )
    }
}

/// Extracts the next ipv4 address from the given address(es). If no ipv4
/// address exists, then an error is returned.
pub fn try_to_ipv4(addr: impl ToSocketAddrs) -> io::Result<SocketAddrV4> {
    let to_v4 = |a| match a {
        SocketAddr::V4(addr) => Some(addr),
        _ => None,
    };

    addr.to_socket_addrs()?
        .flat_map(to_v4)
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "only ipv4 addresses are supported"))
}

pub fn to_ipv4(sock: &UdpSocket) -> SocketAddrV4 {
    sock.local_addr()
        .and_then(try_to_ipv4)
        .unwrap_or_else(|_| SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0))
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

trait MalformedPacketError: Sized {
    fn wrap_malpac(self) -> Self;
}

impl MalformedPacketError for Result<Packet, io::Error> {
    fn wrap_malpac(self) -> Self {
        self.map_err(|e| {
            io::Error::new(
                ErrorKind::Other,
                format!("received a malformed packet: {}", e),
            )
        })
    }
}

const RELIABLE_SEND_MAX_ATTEMPTS: u8 = 5;

/// Sends a packet (potentially multiple times) in a loop with a timeout and
/// waits for the response. Used for handshakes.
///
/// TODO: refactor this function to reduce the number of arguments
#[allow(clippy::too_many_arguments)]
pub fn reliable_send(
    send: &[u8],
    sock: &UdpSocket,
    peer: SocketAddr,
    timeout: Duration,
    send_packet_type: PacketType,
    recv_packet_types: &[PacketType],
    skip_address_mismatch: bool,
    skip_would_block: bool,
) -> io::Result<(Packet, SocketAddr)> {
    // // TODO: DEBUG
    // let timeout = Duration::from_secs(100000);
    // // TODO: DEBUG

    let mut recv = packet_buffer();
    let join = |packet_types: &[PacketType]| packet_types.iter().join(" or ");
    let joined = join(recv_packet_types);
    let mut invalid_response_packets = Vec::with_capacity(5);

    let mut i = 0;
    let mut block_limit = 50;
    while i < RELIABLE_SEND_MAX_ATTEMPTS {
        i += 1;
        log::debug!(
            "{}Sending {} packet, waiting for packets of type {}",
            if i > 1 {
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
            Err(e) if skip_would_block && e.kind() == ErrorKind::WouldBlock => {
                log::debug!("Would block ({}), block_limit = {}", e, block_limit);
                if block_limit == 0 {
                    return Err(e);
                }
                block_limit -= 1;
                thread::sleep(Duration::from_millis(1));
                i -= 1;
                continue;
            }
            Err(e) => return Err(e),
        }?;

        // Check that the packet is of (one of) the types that we expect
        if !recv_packet_types.iter().any(|t| packet.ptyp == *t) {
            invalid_response_packets.push(packet.ptyp);
            continue;
        }

        // let remote = packet.peer_addr();
        return Ok((packet, remote));
    }

    Err(if !invalid_response_packets.is_empty() {
        Error::new(
            ErrorKind::Other,
            "invalid response packets, ".to_owned()
                + "expected to receive a packet of one of these types:"
                + &format!(
                    "{}, but received only the following packets: {}",
                    joined,
                    join(&invalid_response_packets)
                ),
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
