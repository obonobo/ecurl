//! Contains code for dealing with the custom packet structure in the udp
//! assignment

use std::fmt::Display;
use std::io::{Error, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

/// The custom packet structure defined by the assignment requirements
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct Packet {
    /// Packet type. Possible value
    pub ptyp: PacketType,

    /// Sequence number, big endian
    pub nseq: u32,

    /// Peer IP address
    pub peer: Ipv4Addr,

    /// Peer port number, big endian
    pub port: u16,

    /// Packet payload
    pub data: Vec<u8>,
}

impl Packet {
    /// The size of a packet with an empty data field
    pub const MIN_PACKET_SIZE: usize = 1 + 4 + 4 + 2; // 11

    /// The size of a packet with a full data field
    pub const MAX_PACKET_SIZE: usize = Self::MIN_PACKET_SIZE + Self::PACKET_DATA_CAPACITY;

    /// The maximum size of the data field of a packet
    pub const PACKET_DATA_CAPACITY: usize = 1014;

    /// Converts a byte source to a [stream of packets](PacketStream).
    ///
    /// All packets will by of type [PacketType::Data], with a default peer and
    /// port number, nseq will auto increment with each packet
    pub fn stream<R: Read>(reader: R) -> PacketStream<R> {
        let p = Self::default();
        PacketStream {
            reader,
            packet_type: p.ptyp,
            seq: 1,
            port: p.port,
            peer: p.peer,
            active: false,
            buf: data_buffer(),
        }
    }

    /// Serializes the entire packet to a byte buffer.
    pub fn raw(&self) -> Vec<u8> {
        let mut buf = vec![0; self.len()];
        let n = self.write_to(&mut buf[..]).unwrap_or(0);
        buf.truncate(n); // should do nothing
        buf
    }

    pub fn write_to(&self, mut buf: impl Write) -> std::io::Result<usize> {
        let mut n = 0;
        n += buf.write(&[self.ptyp.into()])?;
        n += buf.write(self.nseq.to_be_bytes().as_ref())?;
        n += buf.write(self.peer.octets().as_ref())?;
        n += buf.write(self.port.to_be_bytes().as_ref())?;
        n += buf.write(self.data.as_ref())?;

        if n < self.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "partial write: buffer is too small to fit this packet",
            ));
        }

        Ok(n)
    }

    pub fn len(&self) -> usize {
        self.data.len() + Self::MIN_PACKET_SIZE
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// Converts a buffer into a Packet. The reason why this is not an
    /// implementation of [From](core::convert::From) is because that would
    /// create a blanket implementation of [Into](core::convert::Into) which
    /// creates ANOTHER blanket implementation of
    /// [TryFrom](core::convert::TryFrom) where the `Error` is set to
    /// `Infallible`...
    ///
    /// So because of the shitty `TryFrom` that is by default implemented on
    /// anything with `Into`, we have to do shitty workarounds. In this case, we
    /// are choosing to us our own custom `TryFrom` and then to just place a
    /// non-trait `from` method directly on our type.
    ///
    /// ### Panics
    ///
    /// Panics if the buffer does not contain a valid packet
    pub fn from(buf: &[u8]) -> Self {
        Self::try_from(buf).unwrap()
    }

    pub fn peer_addr(&self) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(self.peer, self.port))
    }
}

impl From<Packet> for Vec<u8> {
    fn from(p: Packet) -> Self {
        p.raw()
    }
}

impl TryFrom<&[u8]> for Packet {
    type Error = Error;

    fn try_from(buf: &[u8]) -> Result<Self, Self::Error> {
        let err = |msg| move |_| Error::new(ErrorKind::Other, msg);

        if buf.len() < Self::MIN_PACKET_SIZE {
            return Err(Error::new(
                ErrorKind::Other,
                format!(
                    "invalid packet (size = {} bytes), must be at least {} bytes",
                    buf.len(),
                    Self::MIN_PACKET_SIZE
                ),
            ));
        }

        Ok(Self {
            ptyp: buf[0].into(),
            nseq: u32::from_be_bytes(
                buf[1..5]
                    .try_into()
                    .map_err(err("invalid nseq, needs 4 bytes"))?,
            ),
            peer: Ipv4Addr::from(
                TryInto::<[u8; 4]>::try_into(&buf[5..9])
                    .map_err(err("invalid peer address, needs 4 bytes"))?,
            ),
            port: u16::from_be_bytes(
                buf[9..11]
                    .try_into()
                    .map_err(err("invalid port, needs 2 bytes"))?,
            ),
            data: buf[11..].into(),
        })
    }
}

impl Default for Packet {
    fn default() -> Self {
        Self {
            ptyp: Default::default(),
            nseq: Default::default(),
            peer: Ipv4Addr::new(127, 0, 0, 1),
            port: Default::default(),
            data: Default::default(),
        }
    }
}

impl Display for Packet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Packet[ptyp={}, nseq={}, peer={}, data=[{}]]",
            self.ptyp,
            self.nseq,
            self.peer_addr(),
            if self.data.is_empty() { "" } else { "..." }
        )
    }
}

pub struct PacketStream<R: Read> {
    reader: R,
    packet_type: PacketType,
    seq: u32,
    port: u16,
    peer: Ipv4Addr,
    active: bool,
    buf: [u8; Packet::PACKET_DATA_CAPACITY],
}

impl<R: Read> Iterator for PacketStream<R> {
    type Item = Packet;

    /// Reads from the [PacketStream's](PacketStream) inner reader in
    /// [Packet](Packet) sized chunks
    fn next(&mut self) -> Option<Self::Item> {
        self.active = true;

        // Read some data from the inner reader
        let data = {
            let n = self.reader.read(&mut self.buf).ok().filter(|n| *n != 0)?;
            &self.buf[..n]
        };

        let p = Packet {
            data: data.into(),
            ptyp: self.packet_type,
            nseq: self.seq,
            peer: self.peer,
            port: self.port,
        };

        self.seq += 1;
        Some(p)
    }
}

/// A macro for generating [PacketStream] setter functions
macro_rules! packet_stream_setter {
    ($name:ident, $type:ty) => {
        pub fn $name(mut self, $name: $type) -> Self {
            if self.active {
                return self;
            }
            self.$name = $name;
            self
        }
    };
    ($name:ident, $type:ty, $does_not_need_active:expr) => {
        pub fn $name(mut self, $name: $type) -> Self {
            self.$name = $name;
            self
        }
    };
}

impl<R: Read> PacketStream<R> {
    packet_stream_setter!(seq, u32);
    packet_stream_setter!(port, u16, false);
    packet_stream_setter!(peer, Ipv4Addr, false);
    packet_stream_setter!(packet_type, PacketType, false);

    /// Sets both the port and the ip fields of the packets
    pub fn remote(self, addr: SocketAddrV4) -> Self {
        self.peer(*addr.ip()).port(addr.port())
    }

    /// Returns the current sequence number o
    pub fn current_seq(&self) -> u32 {
        self.seq
    }
}

/// The type of a packet
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum PacketType {
    Ack,
    Syn,
    SynAck,
    Nak,
    Data,
    Fin,
    FinAck,
    Invalid,
}

impl PacketType {}

impl From<String> for PacketType {
    fn from(s: String) -> Self {
        Self::from(s.as_ref())
    }
}

impl From<&str> for PacketType {
    fn from(s: &str) -> Self {
        match str::to_lowercase(s).as_ref() {
            "ack" => Self::Ack,
            "syn" => Self::Syn,
            "synack" | "syn-ack" => Self::SynAck,
            "nak" => Self::Nak,
            "fin" => Self::Fin,
            "finack" | "fin-ack" => Self::FinAck,
            _ => Self::Data,
        }
    }
}

impl From<u8> for PacketType {
    fn from(b: u8) -> Self {
        match b {
            0 => Self::Ack,
            1 => Self::Syn,
            2 => Self::SynAck,
            3 => Self::Nak,
            4 => Self::Data,
            5 => Self::Fin,
            6 => Self::FinAck,
            _ => Self::Invalid,
        }
    }
}

impl From<PacketType> for u8 {
    fn from(p: PacketType) -> u8 {
        match p {
            PacketType::Ack => 0,
            PacketType::Syn => 1,
            PacketType::SynAck => 2,
            PacketType::Nak => 3,
            PacketType::Data => 4,
            PacketType::Fin => 5,
            PacketType::FinAck => 6,
            PacketType::Invalid => u8::MAX,
        }
    }
}

impl Display for PacketType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Default for PacketType {
    fn default() -> Self {
        Self::Data
    }
}

#[cfg(test)]
mod tests {
    use super::{Packet, PacketType};
    use rand::distributions::Alphanumeric;
    use rand::{thread_rng, Rng};
    use std::net::Ipv4Addr;

    #[test]
    fn test_packet_stream_empty() {
        assert_packet_stream(Packet::stream("".as_bytes()), &[]);
    }

    #[test]
    fn test_packet_stream_simple() {
        let peer = default_peer();
        let data = "Hello world!";
        assert_packet_stream(
            Packet::stream(data.as_bytes())
                .seq(1)
                .port(8080)
                .peer(peer)
                .packet_type(PacketType::Data),
            &[Packet {
                peer,
                port: 8080,
                ptyp: PacketType::Data,
                nseq: 1,
                data: data.as_bytes().into(),
            }],
        )
    }

    #[test]
    fn test_packet_stream_two_packets() {
        let peer = default_peer();
        let data = "a".repeat(Packet::PACKET_DATA_CAPACITY).repeat(2);
        assert_packet_stream(
            Packet::stream(data.as_bytes()).peer(peer),
            &to_packet_chunks(&data),
        )
    }

    /// Tests the ability of the [Packet::stream] function to chunk up data,
    macro_rules! test_packet_stream_chunkability {
        ($($name:ident: $length:expr,)*) => {$(
            #[test]
            fn $name() {
                let data = thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take($length)
                    .map(char::from)
                    .collect::<String>();

                assert_packet_stream(
                    Packet::stream(data.as_bytes()).peer(default_peer()),
                    &to_packet_chunks(&data),
                )
            }
        )*};
    }

    test_packet_stream_chunkability! {
        test_stream_chunking_size_2: 2,
        test_stream_chunking_size_4: 4,
        test_stream_chunking_size_8: 8,
        test_stream_chunking_size_16: 16,
        test_stream_chunking_size_32: 32,
        test_stream_chunking_size_64: 64,
        test_stream_chunking_size_128: 128,
        test_stream_chunking_size_256: 256,
        test_stream_chunking_size_512: 512,
        test_stream_chunking_size_1024: 1024,
        test_stream_chunking_size_2048: 2048,
        test_stream_chunking_size_4096: 4096,
        test_stream_chunking_size_8192: 8192,
        test_stream_chunking_size_16384: 16384,
        test_stream_chunking_size_32768: 32768,
        test_stream_chunking_size_65536: 65536,
        test_stream_chunking_size_131072: 131072,
        test_stream_chunking_size_262144: 262144,
        test_stream_chunking_size_524288: 524288,
        test_stream_chunking_size_1048576: 1048576,
    }

    /// Tests that packet serialization and deserialization works as expected by
    /// creating random packets then serializing and deserializing them
    #[test]
    fn test_packet_serialization() {
        for packet in (0..100).map(|_| random_packet()) {
            let packet2 = Packet::from(&packet.raw());
            assert_eq!(packet, packet2);
        }
    }

    /// Creates a [Packet] with randomized fields
    fn random_packet() -> Packet {
        let r = || thread_rng().gen();
        Packet {
            ptyp: thread_rng().gen_range(0..=5).into(),
            nseq: thread_rng().gen(),
            peer: Ipv4Addr::new(r(), r(), r(), r()),
            port: thread_rng().gen(),
            data: thread_rng()
                .sample_iter(&Alphanumeric)
                .take(Packet::PACKET_DATA_CAPACITY)
                .collect(),
        }
    }

    fn default_peer() -> Ipv4Addr {
        Ipv4Addr::new(192, 168, 2, 1)
    }

    /// Asserts that a packet stream has the specified contents
    fn assert_packet_stream(packets: impl Iterator<Item = Packet>, expected: &[Packet]) {
        assert_eq!(expected, packets.collect::<Vec<_>>());
    }

    /// Converts a string to a Packet buffer
    fn to_packet_chunks(data: &str) -> Vec<Packet> {
        let mut seq = 1;
        data.as_bytes()
            .chunks(Packet::PACKET_DATA_CAPACITY)
            .map(|chunk| Packet {
                nseq: {
                    seq += 1;
                    seq - 1
                },
                peer: default_peer(),
                data: chunk.into(),
                ..Default::default()
            })
            .collect()
    }
}

pub use packet_buffer::*;
mod packet_buffer {
    use super::Packet;

    pub type PacketBuffer = [u8; Packet::MAX_PACKET_SIZE];
    pub type PacketDataBuffer = [u8; Packet::PACKET_DATA_CAPACITY];

    pub const fn buffer<const S: usize>() -> [u8; S] {
        [0; S]
    }

    pub fn packet_buffer() -> PacketBuffer {
        buffer::<{ Packet::MAX_PACKET_SIZE }>()
    }

    pub fn data_buffer() -> PacketDataBuffer {
        buffer::<{ Packet::PACKET_DATA_CAPACITY }>()
    }
}
