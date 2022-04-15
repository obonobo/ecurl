use udpx::transport::UdpxListener;
use udpx::util::config::err_to_exit_code;
use udpx::util::constants::EXIT_OKAY;
use udpx::Incoming;
use udpx::Stream;

udpx::cli_binary!(ServerConfig, server_main);
fn server_main(_: ServerConfig) -> Result<i32, i32> {
    let err = err_to_exit_code;
    let mut listener = UdpxListener::bind("localhost:8080").map_err(err())?;
    for stream in listener.incoming() {
        let stream = stream.unwrap();
        log::info!("Made a connection with {}", stream.peer_addr().unwrap());
    }
    Ok(EXIT_OKAY)
}
