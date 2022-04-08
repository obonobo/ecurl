use std::{
    fmt::Display,
    io::{Error, ErrorKind},
};

use clap::Parser;

const EXIT_NOT_OKAY: i32 = 1;
const EXIT_OKAY: i32 = 0;

/// File server over UDP, with custom packets and reliable transport
/// implementation
#[derive(Parser, Debug, Hash, Clone, Default)]
#[clap(author, version, about, long_about = None)]
pub struct ServerConfig {
    /// Logs debugging messages
    #[clap(short, long)]
    pub verbose: bool,

    /// Specifies the directory that the server will use to read/write requested
    /// files. Default is the current directory when launching the application.
    #[clap(short, long, default_value = "./")]
    pub dir: String,

    /// Specifies the port number that the server will listen and serve at.
    #[clap(short, long, default_value_t = 8080)]
    pub port: u16,
}

impl Display for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ServerConfig {
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, (i32, Error)> {
        Self::try_parse_from(args)
            .map_err(|e| Error::new(ErrorKind::Other, e))
            .and_then(Self::verify)
            .map_err(|e| (EXIT_NOT_OKAY, e))
    }

    pub fn verify(self) -> Result<Self, Error> {
        Ok(self)
    }
}

pub fn run_and_exit() -> ! {
    std::process::exit(run(std::env::args()))
}

pub fn run(args: impl IntoIterator<Item = String>) -> i32 {
    let cfg = match ServerConfig::from_args(args) {
        Ok(cfg) => cfg,
        Err((exit, err)) => {
            eprintln!("{}", err);
            return exit;
        }
    };
    println!("{}", cfg);
    EXIT_OKAY
}
