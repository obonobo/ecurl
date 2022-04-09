pub mod packet;
mod transport;

mod traits;
pub use traits::*;

pub const LOCALHOST: &str = "127.0.0.1";
pub const NINE_THOUSAND: u16 = 9000;
