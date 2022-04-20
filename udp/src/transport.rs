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

use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::io::{self, Error, ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

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

    /// The router proxy to use
    proxy: Option<SocketAddrV4>,

    /// A set of SYN packet's so that we can avoid duplicates
    duplicate_syns: HashMap<Packet, Instant>,
}

impl Bindable<UdpxStream> for UdpxListener {
    fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Self::bind_with_proxy(addr, None)
    }
}

impl UdpxListener {
    pub fn bind_with_proxy(
        addr: impl ToSocketAddrs,
        proxy: Option<SocketAddrV4>,
    ) -> io::Result<Self> {
        Ok(Self {
            sock: UdpSocket::bind(addr)?,
            buf: packet_buffer(),
            timeout: DEFAULT_TIMEOUT,
            nonblocking: false,
            proxy,
            duplicate_syns: HashMap::with_capacity(32),
        })
    }

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
    ) -> io::Result<(Packet, u32, UdpSocket, SocketAddr)> {
        let remote = deserialize_addr(packet.data.as_ref());

        log::debug!("Beginning handshake with {}", remote);
        log::debug!("{}", packet);
        if packet.ptyp != PacketType::Syn {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "handshake failure: first packet received is not a SYN",
            ));
        }

        // We need to create a new UdpSocket for our response - let the OS
        // choose the port
        let sock = UdpSocket::bind("127.0.0.1:0")?;
        log::debug!(
            "Dispatching to new UDP socket for the rest of the conversation, new socket = {}",
            sock.local_addr()?
        );

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
            // peer: if let SocketAddr::V4(addr) = addr {
            //     addr.ip().to_owned()
            // } else {
            //     Ipv4Addr::new(127, 0, 0, 1)
            // },
            // port: addr.port(),
            peer: if let SocketAddr::V4(addr) = remote {
                addr.ip().to_owned()
            } else {
                Ipv4Addr::new(127, 0, 0, 1)
            },
            port: remote.port(),

            data: serialize_addr(sock.local_addr().unwrap()).into(),
        };

        // Send the SYN-ACK and wait for a response packet. It should be ACK,
        // but if the ACK get's dropped, we will accept the next DATA packet as
        // well
        let n = send.write_to(&mut self.buf[..])?;
        let (ack_or_data, _) = reliable_send(
            &self.buf[..n],
            &sock,
            addr,
            self.timeout(),
            PacketType::SynAck,
            &[PacketType::Ack, PacketType::Data],
            true,
            false,
            self.proxy,
        )?;

        // This packet should be an ACK or DATA packet
        // sock.connect(remote)?;
        let _debug = addr.to_string();
        sock.connect(addr)?;
        let nseq = packet.nseq + 3;
        match ack_or_data.ptyp {
            PacketType::Data => Ok((ack_or_data, nseq, sock, remote)),
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
                    Ok((ack_or_data, nseq, sock, remote))
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

        let timelimit_to_accept_another_syn = Duration::from_secs(2);
        if let Some(when) = self.duplicate_syns.get(&packet) {
            let how_long_has_it_been = Instant::now() - *when;
            if how_long_has_it_been < timelimit_to_accept_another_syn {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("duplicate SYN packet received: {}", packet),
                ));
            }
        }
        self.duplicate_syns.insert(packet.clone(), Instant::now());

        let (packet, nseq, sock, remote) = match self.handshake(addr, &packet) {
            Ok(values) => values,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                log::error!("Client unexpectedly closed connection in the middle of our handshake");
                return Err(e);
            }
            Err(e) => return Err(e),
        };

        let stream = UdpxStream::new(sock, nseq, self.proxy, {
            if let SocketAddr::V4(addr) = remote {
                Some(addr)
            } else {
                None
            }
        })
        .with_starting_data([packet]);

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
    last_nseq: Option<u32>,
    proxy: Option<SocketAddrV4>,
    handshake_ack: Option<Packet>,
    got_flush: bool,
}

impl Drop for UdpxStream {
    fn drop(&mut self) {
        // let _ = self.shutdown();
    }
}

impl Connectable for UdpxStream {
    fn connect(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Self::connect_with_proxy(addr, None)
    }
}

impl UdpxStream {
    /// The sequence number of the first DATA packet sent in a conversation
    pub const FIRST_NSEQ: u32 = 3;

    pub fn connect(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Connectable::connect(addr)
    }

    pub fn connect_with_proxy(
        addr: impl ToSocketAddrs,
        proxy: Option<SocketAddrV4>,
    ) -> io::Result<Self> {
        Self::new(Self::random_socket()?, Self::FIRST_NSEQ, proxy, None).handshake(addr)
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sock.local_addr()
    }

    pub fn shutdown(&mut self) -> io::Result<()> {
        Stream::shutdown(self, std::net::Shutdown::Both)
    }

    fn new(
        sock: UdpSocket,
        nseq: u32,
        proxy: Option<SocketAddrV4>,
        remote: Option<SocketAddrV4>,
    ) -> Self {
        let remote = remote.unwrap_or_else(|| {
            sock.peer_addr()
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
                .unwrap_or_else(|_| SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0))
        });

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
            last_nseq: None,
            proxy,
            handshake_ack: None,
            got_flush: false,
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
            data: self.my_ip_buffer().into(),
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
            // false,
            true,
            self.proxy,
        )?;

        log::debug!(
            "Received SYN-ACK response from server (remote addr = {})",
            remote,
        );

        log::debug!("Setting socket remote peer to {}", remote);
        self.remote = try_to_ipv4(remote)?;
        self.sock
            .connect(self.proxy.map(Into::into).unwrap_or(remote))?;

        // Send the ACK packet. We will just send this packet without waiting
        // for a response
        log::debug!("Sending ACK packet to complete handshake");
        let ack = Packet {
            ptyp: PacketType::Ack,
            nseq: syn_ack.nseq + 1,
            peer: self.remote.ip().to_owned(),
            port: self.remote.port(),
            ..Default::default()
        };
        self.write_packet(&ack)?;
        self.handshake_ack = Some(ack);
        log::debug!("Handshake complete! Returning UdpxStream");

        // Handshake is done!
        Ok(self)
    }

    fn my_ip_buffer(&self) -> [u8; 6] {
        let my_addr = self.local_addr().unwrap();
        serialize_addr(my_addr)
    }

    fn write_packet(&mut self, packet: &Packet) -> io::Result<()> {
        let n = packet.write_to(&mut self.buf[..])?;
        self.sock.send_to(
            &self.buf[..n],
            self.proxy.map(Into::into).unwrap_or(self.remote),
        )?;
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
        log::debug!(
            "Acknowledging packet {} is well received",
            transfer.packet.nseq
        );

        let ack = Packet {
            ptyp: PacketType::Ack,
            nseq: transfer.packet.nseq,
            peer: self.remote.ip().to_owned(),
            port: self.remote.port(),
            ..Default::default()
        };

        self.sock
            .set_write_timeout(millis(TIMEOUT))
            .expect("Failed to set write timeout");

        let n = ack.write_to(&mut self.buf[..])?;
        self.sock.send(&self.buf[..n])?;
        Ok(transfer)
    }

    fn buffer_and_ack(&mut self, transfer: PacketTransfer) -> io::Result<()> {
        self.acknowledge_packet(transfer).map(|t| {
            log::debug!(
                "Received and ACKed packet {}, placing it in receive buffer now",
                t.packet.nseq
            );
            // self.packets_received.insert(t.packet.nseq, t);

            if self.packets_received.get(&t.packet.nseq).is_none() {
                self.packets_received.insert(t.packet.nseq, t);
            }
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
        if self.is_closed() {
            Err(self.closed_err())
        } else if let Some(err) = self.copy_of_err() {
            Err(err)
        } else {
            Ok(())
        }
    }

    fn maybe_registered_err(&mut self, red: usize) -> Option<Result<usize, Error>> {
        if let Err(e) = self.registered_err() {
            if red > 0 || e.to_string().contains("UdpxStream connection closed") {
                return Some(Ok(red));
            }
            return Some(Err(e));
        }
        None
    }

    fn is_closed(&self) -> bool {
        let last = self.received_last_packet();
        self.closed && last
    }

    fn received_last_packet(&self) -> bool {
        self.last_nseq.filter(|n| self.next_nseq >= *n).is_some()
    }

    fn cannot_read_anymore(&self) -> bool {
        self.err.is_some() || self.is_closed()
    }

    /// Acknowledge that a FIN packet has been received
    fn fin_ack(&mut self) -> io::Result<()> {
        // Send fin_ack many times
        self.sock.set_write_timeout(millis(5))?;
        for _ in 0..100 {
            let fin_ack = Packet {
                ptyp: PacketType::FinAck,
                ..self.packet_defaults()
            };
            let n = fin_ack.write_to(&mut self.buf[..])?;
            let _ = self.sock.send(&self.buf[..n]); // ignore
        }
        Ok(())
    }

    pub fn do_flush(&mut self) -> io::Result<()> {
        self.sock.set_write_timeout(millis(50))?;
        let fin_ack = Packet {
            ptyp: PacketType::Flush,
            ..self.packet_defaults()
        };
        let n = fin_ack.write_to(&mut self.buf[..])?;
        for _ in 0..100 {
            let _ = self.sock.send(&self.buf[..n]); // ignore
        }
        Ok(())
    }
}

fn deserialize_addr(buf: &[u8]) -> SocketAddr {
    let ip = Ipv4Addr::from(TryInto::<[u8; 4]>::try_into(&buf[..4]).unwrap());
    let port = u16::from_le_bytes(TryInto::<[u8; 2]>::try_into(&buf[4..6]).unwrap());
    SocketAddr::V4(SocketAddrV4::new(ip, port))
}

fn serialize_addr(my_addr: SocketAddr) -> [u8; 6] {
    let local_addr = try_to_ipv4(my_addr).unwrap();
    let mut body: [u8; 6] = [0; 6];
    (&mut body[..])
        .write_all(&local_addr.ip().octets())
        .unwrap();
    (&mut body[4..])
        .write_all(local_addr.port().to_le_bytes().as_ref())
        .unwrap();
    body
}

impl Stream for UdpxStream {
    fn peer_addr(&self) -> io::Result<SocketAddr> {
        Ok(SocketAddr::V4(self.remote))
    }

    /// Sends a FIN packet and registers a `StreamClosed` error
    fn shutdown(&mut self, _: std::net::Shutdown) -> io::Result<()> {
        let _debug_peer = format!("{}", self.sock.peer_addr().unwrap());
        let _debug_remote = format!("{}", self.remote);

        log::debug!("Shutting down UpdxStream...");

        if self.cannot_read_anymore() {
            log::debug!(
                "Shutdown attempted, but this stream has already been closed from the other end"
            );
            return Ok(());
        }

        let fin = Packet {
            ptyp: PacketType::Fin,
            nseq: {
                // if let Some(n) = self.last_nseq {
                //     n
                // } else {
                let last_nseq = self.next_nseq;
                self.last_nseq = Some(last_nseq);
                self.next_nseq += 1;
                last_nseq
                // }
            },
            ..self.packet_defaults()
        };
        let mut fin_buf = packet_buffer();
        let n = fin.write_to(&mut fin_buf[..])?;
        let fin = &fin_buf[..n];

        // 10 tries to receive a FIN-ACK
        for _ in 0..30 {
            self.sock.set_write_timeout(millis(TIMEOUT))?;
            log::debug!("Sending FIN packet");

            match self.sock.send(fin) {
                Ok(_) => {
                    log::debug!("Ok");
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    log::error!("Got an error sending FIN: {}", e);
                    continue;
                }
                Err(e) => {
                    log::error!("Got an error sending FIN: {}", e);
                }
            }

            // Await FIN-ACK
            log::debug!("Awaiting FIN-ACK");
            self.sock.set_read_timeout(millis(TIMEOUT))?;
            match self.sock.recv(&mut self.buf[..]) {
                Ok(n) => {
                    let packet = Packet::try_from(&self.buf[..n])?;
                    match packet.ptyp {
                        PacketType::FinAck | PacketType::Fin => {
                            log::debug!("FIN-ACK received");
                            break;
                        }
                        _ => {
                            log::error!("Wrong packet type, got {}, expected FIN-ACK", packet);
                            continue;
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    log::error!("Got an error awaiting FIN-ACK: {}", e);
                    continue;
                }
                Err(e) => {
                    log::error!("Got an error awaiting FIN-ACK: {}", e);
                    // break;
                    continue;
                } // For now, let's say that recv errors mean we can close
            };
        }
        self.closed = true;
        Ok(())
    }
}

/// Max number of WouldBlock skips for Read/Write
pub const MAX_SKIPPED: usize = 5;
pub const TIMEOUT: u64 = 100;

impl Read for UdpxStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // // TODO: DEBUG
        // return self.do_read(buf);
        // // TODO: DEBUG

        let _debug_peer = format!("{}", self.sock.peer_addr().unwrap());
        let _debug_remote = format!("{}", self.remote);

        let mut red = 0;
        // let mut skipped = MAX_SKIPPED;
        let mut skipped = 1 << 14;
        while red < buf.len() && skipped > 0 {
            if let Some(value) = self.maybe_registered_err(red) {
                return value;
            }

            // TODO: for now we will wait forever
            // We will only try reading for a short period of time
            self.sock.set_read_timeout(millis(TIMEOUT))?;
            // self.sock.set_read_timeout(None).unwrap();

            // Grab a packet from either the received packets buffer, or a fresh
            // packet from the socket.
            let mut transfer = {
                if let Some(packet) = self.packets_received.remove(&self.next_nseq) {
                    packet
                } else {
                    let n = match self.sock.recv(&mut self.buf) {
                        Ok(n) => n,
                        Err(e) if e.kind() == ErrorKind::TimedOut => return Ok(red),
                        Err(e) if e.kind() == ErrorKind::WouldBlock => {
                            log::error!("UdpxStream::read(): {}", e);

                            if red > 0 {
                                log::error!("UdpxStream::read(): we've already read some data, returning that now");
                                return Ok(red);
                            }

                            if skipped > 1 {
                                log::error!("Skipping this error...");
                            }
                            skipped -= 1;
                            continue;
                        }
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

                    if transfer.packet.ptyp == PacketType::Flush {
                        // Then the client has sent all that he will send at
                        // this point...
                        log::debug!("Received FLUSH packet: {}", transfer.packet);
                        if self.got_flush {
                            log::debug!("This is a duplice FLUSH, discarding...");
                        } else {
                            log::debug!("Setting FLUSH state");
                            self.got_flush = true;
                            self.last_nseq = Some(transfer.packet.nseq);
                            if self.next_nseq >= transfer.packet.nseq {
                                log::debug!(
                                    "Final packet has already been consumed, returning from read"
                                );
                                return Ok(red);
                            }
                            // return Ok(red);
                        }
                    }

                    if transfer.packet.ptyp == PacketType::Fin {
                        // Then the connection was closed at the other end.
                        // Terminate this part of the connection
                        log::debug!("UdpxStream::read(): got a FIN packet ({})", transfer.packet);
                        self.last_nseq = Some(transfer.packet.nseq);
                        self.closed = true;
                        self.fin_ack()?;
                        let done = self.cannot_read_anymore();
                        if done {
                            log::debug!("UdpxStream::read(): no more data left, exiting now");
                            return Ok(red);
                        } else {
                            log::debug!(
                                "UdpxStream::read(): but there is still data to be read..."
                            );
                            continue;
                        }
                    }

                    if transfer.packet.nseq < self.next_nseq {
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
            let into = &mut buf[red..red + n];
            let from = &transfer.packet.data[..n];
            into.copy_from_slice(from);
            red += n;
            transfer.packet.data.truncate_left(n);

            if transfer.packet.data.is_empty() {
                // This packet has been fully read, we can now drop it entirely
                self.next_nseq += 1;
                if self.received_last_packet() {
                    return Ok(red);
                }
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
        // // TODO: DEBUG
        // return self.do_write(buf);
        // // TODO: DEBUG

        let _debug_peer = format!("{}", self.sock.peer_addr().unwrap());
        let _debug_remote = format!("{}", self.remote);
        self.registered_err()?;

        // Queue up the packets to be written
        self.packets_sent.extend(
            Packet::stream(buf)
                .packet_type(PacketType::Data)
                .remote(self.remote)
                .seq(self.next_nseq)
                .map(PacketTransfer::from)
                .map(|p| {
                    self.next_nseq += 1;
                    (p.packet.nseq, p)
                }),
        );

        // TODO: for now we will put the ACK-loop right in here. In the future,
        // we may move the loop somewhere else, perhaps into the `flush` method?
        let mut skipped = 1 << 14;
        let mut n = 0;
        while !self.packets_sent.is_empty() && skipped > 0 {
            // self.registered_err()?;
            if let Some(value) = self.maybe_registered_err(n) {
                return value;
            }

            // Send/resend packets
            for transfer in self.packets_sent.values() {
                log::debug!(
                    "UdpxStream::write(): Sending packet (seq={}): {}",
                    transfer.packet.nseq,
                    transfer.packet
                );

                self.sock.set_write_timeout(millis(TIMEOUT)).unwrap();
                let n = transfer.packet.write_to(&mut self.buf[..]).unwrap();
                match self.sock.send(&self.buf[..n]) {
                    Ok(_) => {}
                    Err(e) if e.kind() == ErrorKind::TimedOut => continue,
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        log::error!("UdpxStream::write(): {}", e);
                        log::error!("Hmmm... Maybe they are trying to me something");

                        // self.sock.set_read_timeout(millis(250))?;
                        // let packet = match self.sock.recv(&mut self.buf)?;

                        // break;

                        if skipped > 1 {
                            log::error!("Skipping this error, let's try a read...");
                        }
                        skipped -= 1;
                        self.sock.set_read_timeout(millis(TIMEOUT))?;
                        let packet = match self.sock.recv(&mut self.buf) {
                            Ok(n) => Packet::try_from(&self.buf[..n]).wrap_malpac()?,
                            Err(e) => {
                                log::error!("{}", e);
                                log::error!("Nope, reading didn't work either...");
                                continue;
                            }
                        };

                        match packet.ptyp {
                            PacketType::Data => {
                                log::debug!(
                                    "{}",
                                    "Got a DATA packet in UdpxStream::write(), ".to_owned()
                                        + "placing it in read-packets queue"
                                );
                                self.packets_received.insert(packet.nseq, packet.into());
                                continue;
                            }

                            PacketType::SynAck => {
                                log::error!("Got a SYN-ACK, server must have lost our handshake ACK, resending now");
                                if let Some(ack) = self.handshake_ack.borrow() {
                                    log::debug!("Resending handshake ACK: {}", ack);
                                    let mut buf = packet_buffer();
                                    let n = ack.write_to(&mut buf[..]).unwrap();
                                    self.sock.send(&buf[..n])?;
                                }
                            }

                            _ => continue,
                        }
                    }
                    Err(e) => return Err(self.register_err(e)),
                };
            }
            self.sock.set_write_timeout(None).unwrap();

            // Check for acked packets
            log::debug!(
                "Beginning wait for ACKs, unacked packets are [{}]",
                self.packets_sent.keys().join(", ")
            );

            self.sock.set_read_timeout(millis(TIMEOUT))?;
            let mut i = self.packets_sent.len();
            while i > 0 {
                i -= 1;

                // for i in 0..self.packets_sent.len() {
                log::debug!("Waiting for ACK - {}", i);
                let packet = match self.sock.recv(&mut self.buf) {
                    Ok(n) => Packet::try_from(&self.buf[..n]).wrap_malpac()?,
                    Err(e) if e.kind() == ErrorKind::TimedOut => break,
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        log::error!("UdpxStream::write(): {}", e);
                        skipped -= 1;
                        if skipped == 0 {
                            break;
                        } else {
                            log::error!("Skipping this error...");
                        }
                        thread::sleep(millis(TIMEOUT).unwrap());
                        continue;
                    }
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
                        self.buffer_and_ack(packet.into())?;
                        // self.packets_received.insert(packet.nseq, packet.into());
                        continue;
                    }

                    // Then the server failed to receive our handshake ACK, resend it
                    PacketType::SynAck => {
                        log::error!(
                            "Got a SYN-ACK, server must have lost our handshake ACK, resending now"
                        );
                        if let Some(ack) = self.handshake_ack.borrow() {
                            log::debug!("Resending handshake ACK: {}", ack);
                            let mut buf = packet_buffer();
                            let n = ack.write_to(&mut buf[..]).unwrap();
                            self.sock.send(&buf[..n])?;
                        }
                        i += 1;
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
        // Spams a FLUSH packet

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

const RELIABLE_SEND_MAX_ATTEMPTS: usize = 1 << 14;

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
    proxy: Option<SocketAddrV4>,
) -> io::Result<(Packet, SocketAddr)> {
    let timeout = Duration::from_millis(TIMEOUT);

    let send_to_addr = proxy.map(Into::into).unwrap_or(peer);
    let mut recv = packet_buffer();
    let join = |packet_types: &[PacketType]| packet_types.iter().join(" or ");
    let joined = join(recv_packet_types);
    let mut invalid_response_packets = Vec::with_capacity(5);

    let mut attempts = 1;
    let mut i = 0;

    let mut block_limit = RELIABLE_SEND_MAX_ATTEMPTS;
    if skip_would_block {
        block_limit = RELIABLE_SEND_MAX_ATTEMPTS
    }

    while i < RELIABLE_SEND_MAX_ATTEMPTS {
        i += 1;
        attempts += 1;
        {
            let packet_debug = Packet::from(send);
            log::debug!(
                "{}Sending {} packet to {}, waiting for packets of type {}",
                if attempts > 1 {
                    format!("(Attempt #{}) ", attempts)
                } else {
                    String::new()
                },
                send_packet_type,
                format!("{}:{}", packet_debug.peer, packet_debug.port),
                joined,
            );
        }

        // sock.send_to(send, peer)?; // Resend the packet
        sock.send_to(send, send_to_addr)?; // Resend the packet
        log::debug!("{} packet sent", send_packet_type);

        sock.set_read_timeout(Some(timeout))?;
        let (packet, remote) = match sock.recv_from(&mut recv) {
            Ok((_, addrr)) if skip_address_mismatch && addrr != peer => continue,
            Ok((n, addrr)) => Packet::try_from(&recv[..n]).map(|p| (p, addrr)),
            Err(e) if e.kind() == ErrorKind::TimedOut => continue,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                log::debug!("Would block ({}), block_limit = {}", e, block_limit);
                if block_limit == 0 {
                    return Err(e);
                }
                block_limit -= 1;
                thread::sleep(Duration::from_millis(10));
                i -= 1;
                continue;
            }
            Err(e) => return Err(e),
        }?;

        // Check that the packet is of (one of) the types that we expect
        if !recv_packet_types.iter().any(|t| packet.ptyp == *t) {
            invalid_response_packets.push(packet.ptyp);
            thread::sleep(Duration::from_millis(1));
            continue;
        }

        if let PacketType::Syn | PacketType::SynAck = packet.ptyp {
            // Then we need to deserialize the remote address from the message
            let remote = deserialize_addr(packet.data.as_ref());
            return Ok((packet, remote));
        }

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
