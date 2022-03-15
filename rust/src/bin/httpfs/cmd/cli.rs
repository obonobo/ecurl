use clap::Parser;
use std::{ffi::OsString, path::PathBuf};

use super::utils;

pub const EXIT_NOT_OKAY: i32 = 1;
pub const EXIT_OKAY: i32 = 0;

/// Runs the CLI and exits with an error code.
pub fn run_and_exit() -> ! {
    std::process::exit(run(std::env::args()))
}

/// Runs the CLI on the iterable args provided. Returns program exit code.
pub fn run<T: Into<OsString> + Clone>(args: impl IntoIterator<Item = T>) -> i32 {
    let cfg = match Cli::try_parse_from(args) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{}", e);
            return EXIT_NOT_OKAY;
        }
    };

    utils::logging::init_logging(cfg.verbose);
    log::info!("CONFIG: {:?}", cfg);

    std::process::exit(EXIT_OKAY)
}

/// httpfs is a simple file server
#[derive(Parser, Debug, Hash, Clone, Default)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Prints debugging messages.
    #[clap(short, long)]
    verbose: bool,

    /// Specifies the directory that the server will use to read/write requested
    /// files. Default is the current directory when launching the application.
    #[clap(short, long, default_value = "./")]
    dir: PathBuf,

    /// Specifies the port number that the server will listen and serve at.
    #[clap(short, long, default_value_t = 8080)]
    port: i32,
}
