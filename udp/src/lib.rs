pub mod bullshit_scanner;
pub mod errors;
pub mod html;
pub mod packet;
pub mod parse;
pub mod server;
pub mod transport;
pub mod util;

mod traits;
pub use traits::*;

pub const LOCALHOST: &str = "127.0.0.1";
pub const NINE_THOUSAND: u16 = 9000;
pub const ANY_PORT: u16 = 0;

pub fn random_udp_socket_addr() -> String {
    format!("{}:{}", LOCALHOST, ANY_PORT)
}
