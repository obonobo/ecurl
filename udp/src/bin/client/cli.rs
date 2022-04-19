use std::io::Write;

use udpx::transport::UdpxStream;
use udpx::util::Chug;
use udpx::util::{config::err_to_exit_code, constants::EXIT_OKAY};

udpx::cli_binary!(ClientConfig, client_main);
fn client_main(_: ClientConfig) -> Result<i32, i32> {
    let _err = err_to_exit_code();
    let mut conn = UdpxStream::connect("localhost:8080").unwrap();
    conn.write_all("Hello world!".as_bytes()).unwrap();
    let response = conn.must_chug();
    log::info!("CLIENT: {}", response);
    Ok(EXIT_OKAY)
}
