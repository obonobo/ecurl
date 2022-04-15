use udpx::transport::UdpxStream;
use udpx::util::{config::err_to_exit_code, constants::EXIT_OKAY};

udpx::cli_binary!(ClientConfig, |_: ClientConfig| -> Result<i32, i32> {
    let err = err_to_exit_code();
    let conn = UdpxStream::connect("localhost:8080").unwrap();
    Ok(EXIT_OKAY)
});
