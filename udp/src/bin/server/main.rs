use std::net::UdpSocket;

use udp::{LOCALHOST, NINE_THOUSAND};

fn main() {
    eprintln!("Starting echo server on {}:{}", LOCALHOST, NINE_THOUSAND);
    loop {
        if let Err(e) = echo_server() {
            eprintln!("Echo server encountered an error: {}", e);
            eprintln!("Continuing");
        }
    }
}

fn echo_server() -> std::io::Result<()> {
    let socket = UdpSocket::bind(format!("{}:{}", LOCALHOST, NINE_THOUSAND)).unwrap();
    let mut buf = [0; 1024];
    let (n, src) = socket.recv_from(&mut buf)?;
    let buf = &mut buf[..n];
    eprintln!("{}", buf.iter().map(|b| char::from(*b)).collect::<String>());
    socket.send_to(buf, &src)?;
    Ok(())
}
