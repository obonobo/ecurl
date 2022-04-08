use std::{
    fmt::Display,
    io::{Error, Read},
    net::Ipv4Addr,
};

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

    /// The maximum size of the data field of a packet
    pub const PACKET_DATA_CAPACITY: usize = 1014;

    /// Serializes the entire packet to a byte buffer.
    pub fn raw(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.data.len() + Self::MIN_PACKET_SIZE);
        buf.push(self.ptyp.clone().into());
        buf.append(&mut self.nseq.to_be_bytes().into());
        buf.append(&mut self.peer.octets().into());
        buf.append(&mut self.port.to_be_bytes().into());
        buf.extend(self.data.iter());
        buf
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

    /// Converts a byte source to a [stream of packets](PacketStream).
    ///
    /// All packets will by of type [PacketType::Data], with a default peer and
    /// port number, nseq will auto increment with each packet. You can set the
    /// starting nseq by calling [PacketStream::]
    pub fn stream<R: Read>(reader: R) -> PacketStream<R> {
        todo!()
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
        use std::io::ErrorKind;

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

        let err = |msg| move |_| Error::new(ErrorKind::Other, msg);
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

pub struct PacketStream<R: Read> {
    reader: R,
    seq: u32,
    port: u16,
    peer: Ipv4Addr,
    active: bool,
}

impl<R: Read> PacketStream<R> {
    /// Sets the starting sequence number for your packet stream. If items have
    /// already been taken from the stream, this function does nothing.
    pub fn seq(mut self, seq: u32) -> Self {
        if self.active {
            return self;
        }
        self.seq = seq;
        self
    }

    /// Sets the port number for your packet stream. If items have already been
    /// taken from the stream, this function does nothing.
    pub fn port(mut self, port: u16) -> Self {
        if self.active {
            return self;
        }
        self.port = port;
        self
    }

    /// Sets the peer ip address for your packet stream. If items have already
    /// been taken from the stream, this function does nothing.
    pub fn peer(mut self, peer: Ipv4Addr) -> Self {
        if self.active {
            return self;
        }
        self.peer = peer;
        self
    }
}

impl<R: Read> Iterator for PacketStream<R> {
    type Item = Packet;

    fn next(&mut self) -> Option<Self::Item> {
        self.active = true;
        todo!()
    }
}

/// The type of a packet
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub enum PacketType {
    Ack,
    Syn,
    SynAck,
    Nak,
    Data,
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
            PacketType::Invalid => 5,
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
        Self::Ack
    }
}
