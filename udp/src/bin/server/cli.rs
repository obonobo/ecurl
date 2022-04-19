use std::net::UdpSocket;

use udpx::{
    packet::Packet,
    transport::UdpxListener,
    util::{config::err_to_exit_code, constants::EXIT_OKAY, logging::init_logging, Chug},
    Bindable, Incoming, Stream,
};

udpx::cli_binary!(ServerConfig, server_main);
fn server_main(_: ServerConfig) -> Result<i32, i32> {
    let sock = UdpSocket::bind("localhost:8080").unwrap();
    let mut buf = [0; 1024];
    let (n, addr) = sock.recv_from(&mut buf).unwrap();
    let packet: Packet = (&buf[..n]).try_into().unwrap();
    println!("addr = {}", addr);
    println!("packet = {}", packet);
    println!("{}", std::str::from_utf8(&packet.data).unwrap());

    // let err = err_to_exit_code;
    // let listener = UdpxListener::bind("localhost:8080").map_err(err())?;
    // for stream in listener.incoming() {
    //     let stream = stream.unwrap();
    //     let peer_addr = stream.peer_addr().unwrap();
    //     log::info!("SERVER: Made a connection with {}", peer_addr);
    //     let red = stream.must_chug();
    //     log::info!("SERVER: {}", red)
    // }
    Ok(EXIT_OKAY)
}
