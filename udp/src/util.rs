//! A package for configuring logging, used in both the client and the server

pub use misc::*;

/// Misc. utilities
mod misc {
    use crate::{ANY_PORT, LOCALHOST};
    use std::{
        fmt::Display,
        io::{self, Read},
        time::Duration,
    };

    /// A clone of [Into]. I made this clone so that I could implement `into`
    /// for types outside this crate. For example, the blanket implementation
    /// below that let's you convert any [Display] object into a
    /// [std::io::Error] by wrapping it in a [std::io::ErrorKind::Other] error.
    pub trait InTwo<T> {
        /// Let's change the name slightly so this method is a bit easier to
        /// find...
        fn intwo(self) -> T;
    }

    impl<S: Display> InTwo<std::io::Error> for S {
        fn intwo(self) -> std::io::Error {
            use std::io::{Error, ErrorKind};
            Error::new(ErrorKind::Other, self.to_string())
        }
    }

    /// A trait for truncating a collection from the left side
    pub trait TruncateLeft {
        /// Cut elements from the left side of a collection
        fn truncate_left(&mut self, n: usize);
    }

    impl<T> TruncateLeft for Vec<T> {
        fn truncate_left(&mut self, n: usize) {
            self.drain(0..n);
        }
    }

    /// An [Reader](Read) extension that allows you to quickly chug UTF-8 readers
    pub trait Chug: Read + Sized {
        fn borrow_chug(&mut self) -> io::Result<String>;
        fn chug(mut self) -> io::Result<String> {
            self.borrow_chug()
        }
        fn must_chug(self) -> String {
            self.chug().unwrap()
        }
    }

    impl<R: Read + Sized> Chug for R {
        fn borrow_chug(&mut self) -> io::Result<String> {
            let mut buf = Vec::with_capacity(1024);
            self.read_to_end(&mut buf)?;
            std::str::from_utf8(&buf)
                .map(ToOwned::to_owned)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        }
        // fn chug(mut self) -> io::Result<String> {
        //     let mut buf = Vec::with_capacity(1024);
        //     self.read_to_end(&mut buf)?;
        //     std::str::from_utf8(&buf)
        //         .map(ToOwned::to_owned)
        //         .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        // }
    }

    pub fn random_udp_socket_addr() -> String {
        format!("{}:{}", LOCALHOST, ANY_PORT)
    }

    /// For setting read/write timeouts (those functions take
    /// [Optionals](Option) containing the duration)
    pub fn millis(how_many: u64) -> Option<Duration> {
        Some(Duration::from_millis(how_many))
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

                /// Proxy configuration (for use with router.go)
                #[clap(long)]
                pub proxy: Option<std::net::SocketAddrV4>,

                #[clap(short, long)]
                pub file: Option<String>,

                #[clap(short, long)]
                pub inline_data: Option<String>,

                #[clap(long)]
                pub get: bool,

                #[clap(long)]
                pub post: bool,

                pub args: Vec<String>,
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
                        "{}: verbose={}, dir={}, port={}, proxy={:?}{}",
                        std::any::type_name::<Self>(),
                        self.verbose,
                        self.dir,
                        self.port,
                        self.proxy,
                        if self.args.len() > 0 {
                            format!(", files={:?}", self.args)
                        } else {
                            String::from("")
                        }
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
    use std::sync::atomic::{AtomicBool, Ordering};

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

    static INITIALIZED: AtomicBool = AtomicBool::new(false);
    pub fn init_logging_with_level(level: &str) {
        if !INITIALIZED.swap(true, Ordering::SeqCst) {
            env_logger::init_from_env(
                env_logger::Env::default().filter_or(LOGGING_ENV_VARIABLE, level),
            );
        } else {
            log::error!(
                "logging::init_logging_with_level(): env_logger cannot be initialized twice"
            );
        }
    }
}

/// A macro that creates a trait alias
///
/// # Examples
///
/// ```ignore
/// trait_alias! { pub trait Threadable = Send + Sync + 'static; }
///
/// // Creates the following code:
/// pub trait Threadable: Send + Sync + 'static {}
/// impl<T: Send + Sync + 'static> Threadable for T {}
///
/// // Can be used like so:
/// fn do_something<T: Threadable>(param: T) {}
/// do_something("Must implement Send + Sync + 'static!!!")
/// ```
#[macro_export]
macro_rules! trait_alias {
    () => {};

    (
        $(#[$($attrss:tt)*])*
        $visibility:vis trait $alias:ident = $bounds:tt $(+ $another:tt)*
    ) => {
        $(#[$($attrss)*])*
        $visibility trait $alias: $bounds $(+ $another)* {}
        impl<T: $bounds $(+ $another)*> $alias for T {}
    };

    (
        $(#[$($attrss:tt)*])*
        $visibility:vis trait $alias:ident = $bounds:tt $(+ $another:tt)*;
        $($tail:tt)*
    ) => {
        $(#[$($attrss)*])*
        $visibility trait $alias: $bounds $(+ $another)* {}
        impl<T: $bounds $(+ $another)*> $alias for T {}
        trait_alias! { $($tail)* }
    };

    (
        $(#[$($attrss:tt)*])*
        $visibility:vis trait $alias:ident = $bounds:tt<$generic_params:tt>;
        $($tail:tt)*
    ) => {
        $(#[$($attrss)*])*
        $visibility trait $alias<B: $generic_params>: $bounds<B> {}
        impl<B: $generic_params, T: $bounds<B>> $alias<B> for T {}
        trait_alias! { $($tail)* }
    };

    (
        $(#[$($attrss:tt)*])*
        $visibility:vis trait $alias:ident = $bounds:tt<$generic_params:tt> $(+ $another:tt)*;
        $($tail:tt)*
    ) => {
        $(#[$($attrss)*])*
        $visibility trait $alias<B: $generic_params>: $bounds<B> $(+ $another)* {}
        impl<B: $generic_params, T: $bounds<B> $(+ $another)*> $alias<B> for T {}
        trait_alias! { $($tail)* }
    };
}
