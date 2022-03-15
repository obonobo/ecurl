use std::{
    error::Error,
    fmt::Display,
    net::{IpAddr, Ipv4Addr},
    path::Path,
};

use clap::Parser;

pub const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

#[derive(Debug)]
pub struct ConfigError(pub String);

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.as_str() {
            "" => write!(f, "ConfigError"),
            s => write!(f, "ConfigError: {}", s),
        }
    }
}

impl Error for ConfigError {}

/// httpfs is a simple file server
#[derive(Parser, Debug, Hash, Clone, Default)]
#[clap(author, version, about, long_about = None)]
pub struct Config {
    /// Prints debugging messages.
    #[clap(short, long)]
    pub verbose: bool,

    /// Specifies the directory that the server will use to read/write requested
    /// files. Default is the current directory when launching the application.
    #[clap(short, long, default_value = "./")]
    pub dir: String,

    /// Specifies the port number that the server will listen and serve at.
    #[clap(short, long, default_value_t = 8080)]
    pub port: i32,
}

impl Config {
    pub fn verify(self) -> Result<Self, ConfigError> {
        if !Self::dir_exists(self.dir.as_str()) {
            Err(ConfigError(String::from("ConfigError")))
        } else {
            Ok(self)
        }
    }

    fn dir_exists(dir: &str) -> bool {
        Path::new(dir).exists()
    }
}

impl Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "port: {}, dir: {}, verbose: {}",
            self.port, self.dir, self.verbose,
        )
    }
}
