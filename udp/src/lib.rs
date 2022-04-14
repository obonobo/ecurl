pub mod logging;
pub mod packet;
mod transport;

mod traits;
pub use traits::*;

pub const LOCALHOST: &str = "127.0.0.1";
pub const NINE_THOUSAND: u16 = 9000;
pub const ANY_PORT: u16 = 0;

pub fn random_udp_socket_addr() -> String {
    format!("{}:{}", LOCALHOST, ANY_PORT)
}
