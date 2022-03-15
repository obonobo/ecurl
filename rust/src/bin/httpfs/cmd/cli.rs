use clap::Parser;
use httpfs::server::Server;

use crate::cmd::{
    config::{Config, LOCALHOST},
    exit::{EXIT_NOT_OKAY, EXIT_OKAY},
    utils,
};

/// Runs the CLI and exits with an error code.
pub fn run_and_exit() -> ! {
    std::process::exit(run(std::env::args()))
}

/// Runs the CLI on the iterable args provided. Returns program exit code.
pub fn run(args: impl Iterator<Item = String>) -> i32 {
    let cfg = match Config::try_parse_from(args) {
        Ok(cfg) => match cfg.verify() {
            Ok(cfg) => cfg,
            Err(_) => return EXIT_NOT_OKAY,
        },
        Err(e) => {
            eprintln!("{}", e);
            return EXIT_NOT_OKAY;
        }
    };

    utils::logging::init_logging(cfg.verbose);
    log::info!("Configuration: {}", cfg);

    let srv = Server {
        addr: LOCALHOST,
        dir: cfg.dir,
        port: cfg.port,
        n_workers: num_cpus::get(),
    };

    std::process::exit(match srv.serve() {
        Ok(_) => EXIT_OKAY,
        Err(e) => {
            log::info!("{}", e);
            EXIT_NOT_OKAY
        }
    })
}
