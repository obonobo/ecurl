use udpx::{
    transport::UdpxListener,
    util::{config::err_to_exit_code, constants::EXIT_OKAY, Chug},
    Bindable, Incoming, Stream,
};

udpx::cli_binary!(ServerConfig, server_main);
fn server_main(_: ServerConfig) -> Result<i32, i32> {
    let err = err_to_exit_code;
    let listener = UdpxListener::bind("localhost:8080").map_err(err())?;
    for stream in listener.incoming() {
        let stream = stream.unwrap();
        let peer_addr = stream.peer_addr().unwrap();
        log::info!("SERVER: Made a connection with {}", peer_addr);
        let red = stream.must_chug();
        log::info!("SERVER: {}", red)
    }
    Ok(EXIT_OKAY)
}
