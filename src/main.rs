//! `mj`: analyse hands, play against bots, and run experiments.

mod cli;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    match cli::Cli::parse().run() {
        Ok(()) => ExitCode::SUCCESS,
        // Output piped into a reader that stopped early (such as `head`) is not a failure.
        Err(error) if is_closed_pipe(&error) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        },
    }
}

fn is_closed_pipe(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind() == std::io::ErrorKind::BrokenPipe))
}
