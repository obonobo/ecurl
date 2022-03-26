use std::time::Instant;

use httpfs::server::{Handle, Server};

use crate::cmd::{
    config::Config,
    exit::{EXIT_NOT_OKAY, EXIT_OKAY},
    utils,
};

/// Runs the CLI and exits with an error code.
pub fn run_and_exit() -> ! {
    std::process::exit(run(std::env::args()))
}

/// Runs the CLI on the iterable args provided. Returns program exit code.
pub fn run(args: impl Iterator<Item = String>) -> i32 {
    let cfg = match Config::from_args(args) {
        Ok(cfg) => cfg,
        Err(exit) => return exit,
    };

    utils::logging::init_logging(cfg.verbose);
    log::info!("Configuration: {}", cfg);

    let srv = server(cfg);
    std::process::exit(match srv.serve() {
        Ok(handle) => {
            log::debug!("Got a server handle: {:?}", handle);
            set_at_exit_handler(handle.clone());
            handle.join();
            EXIT_OKAY
        }
        Err(e) => {
            log::info!("{}", e);
            EXIT_NOT_OKAY
        }
    })
}

fn server(cfg: Config) -> Server {
    Server {
        dir: cfg.dir,
        port: cfg.port,
        n_workers: num_cpus::get(),
        ..Default::default()
    }
}

fn set_at_exit_handler(mut handle: Handle) {
    let now = Instant::now();
    let set_handler = ctrlc::set_handler(move || {
        log::info!("Server shutting down...");
        handle.shutdown();
        log::debug!("Server ran for {} seconds...", now.elapsed().as_secs());
    });
    if set_handler.is_err() {
        log::debug!(concat!(
            "Failed to set ctrl-c handler, ",
            "no program exit handler has been registered..."
        ))
    }
}
