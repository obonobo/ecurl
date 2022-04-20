use std::{net::SocketAddrV4, str::FromStr, time::Instant};

use udpx::{
    server::{Handle, Server},
    transport::UdpxListener,
    util::{
        config::err_to_exit_code,
        constants::{EXIT_NOT_OKAY, EXIT_OKAY},
        Chug,
    },
    Incoming, Stream,
};

udpx::cli_binary!(ServerConfig, server_main);

fn server_main(cfg: ServerConfig) -> Result<i32, i32> {
    // return Ok(EXIT_OKAY);

    fn server(cfg: ServerConfig) -> Server {
        Server {
            dir: cfg.dir,
            port: cfg.port as u32,
            n_workers: num_cpus::get(),
            ..Default::default()
        }
    }

    fn set_at_exit_handler(mut handle: Handle) {
        let now = Instant::now();
        let set_handler = ctrlc::set_handler(move || {
            log::info!("Server shutting down...");
            handle.shutdown();
            log::debug!("Server ran for {} seconds...", now.elapsed().as_secs());
        });
        if set_handler.is_err() {
            log::debug!(concat!(
                "Failed to set ctrl-c handler, ",
                "no program exit handler has been registered..."
            ))
        }
    }

    let proxy = cfg.proxy;
    let srv = server(cfg);

    match srv.serve_udpx_with_proxy(proxy) {
        Ok(handle) => {
            log::debug!("Got a server handle: {:?}", handle);
            set_at_exit_handler(handle.clone());
            handle.join();
            Ok(EXIT_OKAY)
        }
        Err(e) => {
            log::error!("{}", e);
            Err(EXIT_NOT_OKAY)
        }
    }
}

fn server_main2(_: ServerConfig) -> Result<i32, i32> {
    // let sock = UdpSocket::bind("localhost:8080").unwrap();
    // let sock = UdpSocket::bind("localhost:8080").unwrap();
    // let mut buf = [0; 1024];
    // let (n, addr) = sock.recv_from(&mut buf).unwrap();
    // let packet: Packet = (&buf[..n]).try_into().unwrap();
    // println!("addr = {}", addr);
    // println!("packet = {}", packet);
    // println!("{}", std::str::from_utf8(&packet.data).unwrap());

    let err = err_to_exit_code;
    let listener = UdpxListener::bind_with_proxy(
        "localhost:8080",
        Some(SocketAddrV4::from_str("127.0.0.1:3000").unwrap()),
    )
    .map_err(err())?;

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                log::error!("{}", e);
                continue;
            }
        };
        let peer_addr = stream.peer_addr().unwrap();
        log::info!("SERVER: Made a connection with {}", peer_addr);
        let red = match stream.chug() {
            Ok(msg) => msg,
            Err(e) => {
                log::error!("{}", e);
                continue;
            }
        };
        log::info!("SERVER: {}", red)
    }
    Ok(EXIT_OKAY)
}
