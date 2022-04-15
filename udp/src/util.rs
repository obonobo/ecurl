//! A package for configuring logging, used in both the client and the server

pub use funcs::*;
mod funcs {
    use crate::{ANY_PORT, LOCALHOST};

    pub fn random_udp_socket_addr() -> String {
        format!("{}:{}", LOCALHOST, ANY_PORT)
    }
}

/// Code for Client and Server CLIs
pub mod config {
    use super::constants::*;
    use clap::{self, Parser};
    use std::io;

    pub fn err_to_exit_code() -> Box<dyn Fn(io::Error) -> i32> {
        Box::new(|err| {
            log::error!("{}", err);
            EXIT_NOT_OKAY
        })
    }

    /// The client and server config objects share the methods in this trait
    pub trait Config: Parser {
        fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, (i32, io::Error)> {
            Self::try_parse_from(args)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
                .and_then(Self::verify)
                .map_err(|e| (EXIT_NOT_OKAY, e))
        }

        fn verify(self) -> Result<Self, io::Error>;
    }

    /// Used for generating CLI binaries - Client and Server
    #[macro_export]
    macro_rules! cli_binary {
        ($name:ident, $body:expr) => {
            udpx::cli_config!($name);
            udpx::cli_run!($name, $body);
        };
    }

    #[macro_export]
    macro_rules! cli_run {
        ($name:ident, $body:expr) => {
            pub fn run_and_exit() -> ! {
                std::process::exit(run(std::env::args()))
            }

            pub fn run(args: impl IntoIterator<Item = String>) -> i32 {
                let cfg = match $name::from_args(args) {
                    Ok(cfg) => cfg,
                    Err((exit, err)) => {
                        eprint!("{}", err);
                        return exit;
                    }
                };

                // crate::util::logging::init_logging(cfg.verbose);
                udpx::util::logging::init_logging(cfg.verbose);
                log::info!("{}", cfg);
                std::process::exit(match $body(cfg) {
                    Ok(code) | Err(code) => code,
                })
            }
        };
    }

    #[macro_export]
    macro_rules! cli_config {
        ($name:ident) => {
            #[derive(clap::Parser, PartialEq, Eq, PartialOrd, Ord, Hash)]
            pub struct $name {
                /// Logs debugging messages
                #[clap(short, long)]
                pub verbose: bool,

                /// Specifies the directory that the server will use to
                /// read/write requested files. Default is the current directory
                /// when launching the application.
                #[clap(short, long, default_value = "./")]
                pub dir: String,

                /// Specifies the port number that the server will listen and
                /// serve at.
                #[clap(short, long, default_value_t = 8080)]
                pub port: u16,
            }

            impl $name {
                pub fn from_args(
                    args: impl IntoIterator<Item = String>,
                ) -> Result<Self, (i32, std::io::Error)> {
                    clap::Parser::try_parse_from(args)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                        .and_then(udpx::util::config::Config::verify)
                        .map_err(|e| (udpx::util::constants::EXIT_NOT_OKAY, e))
                }
            }

            impl udpx::util::config::Config for $name {
                fn verify(self) -> Result<Self, std::io::Error> {
                    std::fs::metadata(&self.dir).map(|_| self)
                }
            }

            impl std::fmt::Display for $name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(
                        f,
                        "{}: verbose={}, dir={}, port={}",
                        std::any::type_name::<Self>(),
                        self.verbose,
                        self.dir,
                        self.port
                    )
                }
            }
        };
    }
}

/// Constants used throughout the app
pub mod constants {
    pub const EXIT_NOT_OKAY: i32 = 1;
    pub const EXIT_OKAY: i32 = 0;
}

/// Logging utilities
pub mod logging {
    pub const LOGGING_ENV_VARIABLE: &str = "UDPX_LOG_LEVEL";
    pub const DEFAULT_LOG_LEVEL: &str = "info";
    pub const VERBOSE_LOG_LEVEL: &str = "debug";

    pub fn init_logging(verbose: bool) {
        init_logging_with_level(if verbose {
            VERBOSE_LOG_LEVEL
        } else {
            DEFAULT_LOG_LEVEL
        });
    }

    pub fn init_logging_with_level(level: &str) {
        env_logger::init_from_env(
            env_logger::Env::default().filter_or(LOGGING_ENV_VARIABLE, level),
        );
    }
}
