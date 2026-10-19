mod analyze;
mod load;
mod play;

use clap::{
    Parser,
    Subcommand,
};

#[derive(Parser)]
#[command(name = "mj", version, about = "mahjong engine, advisor, and simulator")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Shanten, improving tiles, and discard ranking for a hand.
    Analyze(analyze::Args),
    /// Play a hand in the terminal against bots.
    Play(play::Args),
}

impl Cli {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            Command::Analyze(args) => analyze::run(args),
            Command::Play(args) => play::run(args),
        }
    }
}
