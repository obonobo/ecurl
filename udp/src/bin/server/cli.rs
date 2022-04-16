use udpx::{
    transport::UdpxListener,
    util::{config::err_to_exit_code, constants::EXIT_OKAY},
    Bindable, Incoming, Stream,
};

udpx::cli_binary!(ServerConfig, server_main);
fn server_main(_: ServerConfig) -> Result<i32, i32> {
    let err = err_to_exit_code;
    let listener = UdpxListener::bind("localhost:8080").map_err(err())?;
    for stream in listener.incoming() {
        let stream = stream.unwrap();
        log::info!("Made a connection with {}", stream.peer_addr().unwrap());
    }
    Ok(EXIT_OKAY)
}
