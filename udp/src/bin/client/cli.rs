use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::str::FromStr;

use udpx::packet::{Packet, PacketType};
use udpx::transport::UdpxStream;
use udpx::util::Chug;
use udpx::util::{config::err_to_exit_code, constants::EXIT_OKAY};

udpx::cli_binary!(ClientConfig, client_main);
fn client_main(_: ClientConfig) -> Result<i32, i32> {
    // let sock = UdpSocket::bind("localhost:0").unwrap();
    // let packet = Packet {
    //     ptyp: PacketType::Data,
    //     nseq: 0,
    //     peer: Ipv4Addr::new(127, 0, 0, 1),
    //     port: 8080,
    //     data: "Hello world!".into(),
    // };

    // let mut buf = [0; 2048];
    // let n = packet.write_to(&mut buf[..]).unwrap();
    // sock.send_to(&buf[..n], "localhost:3000").unwrap();

    let _err = err_to_exit_code;
    let mut conn = UdpxStream::connect_with_proxy(
        "localhost:8080",
        Some(SocketAddrV4::from_str("127.0.0.1:3000").unwrap()),
    )
    .unwrap();

    conn.write_all(b"Hello world!").unwrap();
    conn.shutdown().unwrap();

    // let mut conn = UdpxStream::connect("localhost:8080").unwrap();
    // conn.write_all("Hello world!".as_bytes()).unwrap();
    // conn.shutdown().unwrap();

    // let response = conn.must_chug();
    // log::info!("CLIENT: {}", response);

    Ok(EXIT_OKAY)
}
