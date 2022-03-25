use std::{error::Error, fmt::Display, path::Path};

use clap::Parser;

use crate::cmd::exit::EXIT_NOT_OKAY;

#[derive(Debug)]
pub struct ConfigError(pub String);

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_str())
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
    pub port: u32,
}

impl Config {
    pub fn from_args(args: impl Iterator<Item = String>) -> Result<Config, i32> {
        Config::try_parse_from(args)
            .map_err(|e| ConfigError(format!("{}", e)))
            .and_then(Self::verify)
            .map_err(|e| {
                eprint!("{}{}", e, if e.0.ends_with("\n") { "" } else { "\n" });
                EXIT_NOT_OKAY
            })
    }

    pub fn verify(self) -> Result<Self, ConfigError> {
        if !Path::new(self.dir.as_str()).exists() {
            Err(ConfigError(String::from("ConfigError")))
        } else {
            Ok(self)
        }
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
