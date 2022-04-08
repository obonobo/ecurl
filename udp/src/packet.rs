use std::{fmt::Display, net::Ipv4Addr};

/// The custom packet structure defined by the assignment requirements
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct Packet {
    /// Packet type. Possible value
    pub ptyp: u8,

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
    pub const MIN_PACKET_SIZE: usize = 1 + 4 + 4 + 2;

    /// Serializes the entire packet to a byte buffer.
    pub fn raw(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.data.len() + Self::MIN_PACKET_SIZE);
        buf.append(&mut self.nseq.to_be_bytes().into());
        buf.append(&mut self.peer.octets().into());
        buf.append(&mut self.port.to_be_bytes().into());
        buf.extend(self.data.iter());
        buf
    }
}

// impl Into<Vec<u8>> for Packet {
//     fn into(self) -> Vec<u8> {
//         self.raw()
//     }
// }

impl From<Vec<u8>> for Packet {
    /// Converts a buffer into a Packet
    fn from(buf: Vec<u8>) -> Self {
        if buf.len() < Self::MIN_PACKET_SIZE {
            panic!(
                "invalid packet (size = {} bytes), must be at least {} bytes",
                buf.len(),
                Self::MIN_PACKET_SIZE
            )
        }

        Self {
            ptyp: buf[0],
            nseq: u32::from_be_bytes(buf[1..5].try_into().unwrap_or([0; 4])),
            peer: Ipv4Addr::from(buf[5..9].try_into().unwrap_or([0; 4])),
            port: u16::from_be_bytes(buf[9..11].try_into().unwrap_or([0; 2])),
            data: buf[11..].into(),
        }
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

/// The type of a packet
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

impl Display for PacketType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
