use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::str::FromStr;

use udpx::packet::{Packet, PacketType};
use udpx::transport::UdpxStream;
use udpx::util::constants::EXIT_NOT_OKAY;
use udpx::util::Chug;
use udpx::util::{config::err_to_exit_code, constants::EXIT_OKAY};

udpx::cli_binary!(ClientConfig, client_main);

fn client_main(cfg: ClientConfig) -> Result<i32, i32> {
    let (addr, file) = parse_args(&cfg.args)?;
    log::info!("Sending request to: {}{}", addr, file);
    let got = get(&cfg, addr, file).map_err(|_| EXIT_NOT_OKAY)?;
    println!("{}", got);
    Ok(EXIT_OKAY)
}

fn get(cfg: &ClientConfig, addr: SocketAddrV4, file: String) -> std::io::Result<String> {
    // let remote = SocketAddrV4::from_str("127.0.0.1:8080").unwrap();
    let remote = addr;

    log::debug!("remote: {}", remote);

    let mut conn = UdpxStream::connect_with_proxy(remote, cfg.proxy)?;

    conn.write_all(format!("GET {} HTTP/1.1\r\n\r\n", file).as_bytes())?;
    // conn.write_all(b"GET /Makefile HTTP/1.1\r\n\r\n")?;

    let got = conn.borrow_chug()?;
    conn.shutdown()?;
    Ok(got)
}

fn post() -> std::io::Result<String> {
    todo!()
}

/// Parses remaining args
fn parse_args(args: &[String]) -> Result<(SocketAddrV4, String), i32> {
    if args.is_empty() {
        eprintln!("Please provide a url...");
        return Err(EXIT_NOT_OKAY);
    }

    let mut url: String = (&args[0]).to_owned();
    if !url.contains('/') {
        url += "/";
    }

    if let &[host, rest] = &url.splitn(2, '/').collect::<Vec<_>>()[..] {
        Ok((
            SocketAddrV4::from_str(host).map_err(|_| {
                eprintln!("Malformed request URL");
                EXIT_NOT_OKAY
            })?,
            String::from("/") + rest,
        ))
    } else {
        eprintln!("Malformed request URL");
        Err(EXIT_NOT_OKAY)
    }
}

fn client_main2(_: ClientConfig) -> Result<i32, i32> {
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
