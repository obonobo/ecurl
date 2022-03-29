use std::net::{SocketAddr, UdpSocket};

use udp::{LOCALHOST, NINE_THOUSAND};

///
/// First we need to port our old client Rust
///

fn main() {
    // In UDP, you must bind to a local socket from which you can send data. As
    // a client, the socket port doesn't matter much - you just need to declare
    // a port to create the local end of the socket.
    //
    // Remember that UDP is connectionless, it works very simply: you bind the
    // socket and then you can send and receive from it. Everytime you send you
    // need to specify the destination. This is unlike TCP
    send_client().unwrap();
}

fn send_client() -> std::io::Result<()> {
    let addr = format!("{}:{}", LOCALHOST, NINE_THOUSAND + 1)
        .parse::<SocketAddr>()
        .unwrap();
    let socket = UdpSocket::bind(addr).unwrap();
    let msg = "Hello world!";

    // Send the message
    socket.send_to(
        msg.as_bytes().as_ref(),
        format!("{}:{}", LOCALHOST, NINE_THOUSAND)
            .parse::<SocketAddr>()
            .unwrap(),
    )?;

    // Read the response
    let mut buf = [0; 1024];
    let (n, src) = socket.recv_from(&mut buf)?;
    let buf = &buf[..n];
    eprintln!(
        "Got a response from the server ({}): {}",
        src,
        buf.iter().map(|b| char::from(*b)).collect::<String>()
    );

    Ok(())
}
