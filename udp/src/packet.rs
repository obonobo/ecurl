use std::{fmt::Display, net::Ipv4Addr};

/// The custom packet structure defined by the assignment requirements
pub struct Packet {
    /// Packet type. Possible value
    ptyp: u8,

    /// Sequence number, big endian
    nseq: u32,

    /// Peer IP address
    peer: Ipv4Addr,

    /// Peer port number, big endian
    port: u16,

    /// Packet payload
    data: Vec<u8>,
}

#[derive(Debug)]
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
