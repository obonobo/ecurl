pub const EXIT_OKAY: i32 = 0;

/// Runs the CLI and exits with an error code.
pub fn run_and_exit() -> ! {
    std::process::exit(EXIT_OKAY)
}
