//! `mj`: analyse hands, play against bots, and run experiments.

mod cli;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    cli::Cli::parse().run()
}
