use std::net::Ipv4Addr;

use udp::packet::{Packet, PacketType};

mod cli;

fn main() {
    let p: Packet = Default::default();
    println!("{}", std::mem::size_of::<PacketType>());
    println!("{}", std::mem::size_of::<u32>());
    println!("{}", std::mem::size_of::<Ipv4Addr>());
    println!("{}", std::mem::size_of::<u16>());
    println!("{}", Packet::MIN_PACKET_SIZE);
    println!(
        "{}, {}",
        std::mem::size_of::<Packet>(),
        std::mem::size_of::<Vec<u8>>()
    );

    let buf: &[u8] = &[0, 0, 0];
    let packet = Packet::try_from(buf);

    // cli::run_and_exit()
}
