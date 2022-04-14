//! A package for configuring logging, used in both the client and the server

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
    env_logger::init_from_env(env_logger::Env::default().filter_or(LOGGING_ENV_VARIABLE, level));
}
