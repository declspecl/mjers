mod advise;
mod analyze;
mod compare;
mod load;
mod play;
mod provenance;
mod sim;
mod trace;
mod tune;

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
    /// Recommend and explain the best action at a decision.
    Advise(advise::Args),
    /// Play a hand in the terminal against bots.
    Play(play::Args),
    /// Simulate many hands and report outcome metrics.
    Sim(sim::Args),
    /// Compare two strategies on identical deals.
    Compare(compare::Args),
    /// Play one bot hand and print its replayable trace.
    Trace(trace::RecordArgs),
    /// Verify and narrate a recorded trace.
    Replay(trace::ReplayArgs),
    /// Search heuristic bot parameters, validated on held-out seeds.
    Tune(tune::Args),
}

impl Cli {
    pub fn run(self) -> anyhow::Result<()> {
        match self.command {
            Command::Analyze(args) => analyze::run(args),
            Command::Advise(args) => advise::run(args),
            Command::Play(args) => play::run(args),
            Command::Sim(args) => sim::run(args),
            Command::Compare(args) => compare::run(args),
            Command::Trace(args) => trace::record(args),
            Command::Replay(args) => trace::replay(args),
            Command::Tune(args) => tune::run(args),
        }
    }
}
