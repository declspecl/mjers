use std::sync::Arc;

use anyhow::{
    Context,
    bail,
};
use mjers_game::trace::Trace;
use mjers_rules::ruleset::Ruleset;
use mjers_rules::seat::Seat;
use mjers_sim::runner::play_hand;

use super::load;
use super::play::print_events;

#[derive(clap::Args)]
pub struct RecordArgs {
    #[arg(long, default_value_t = 0)]
    seed: u64,
    #[arg(long, default_value = "family")]
    rules: String,
    /// Strategy for every seat.
    #[arg(long, default_value = "efficiency")]
    strategy: String,
}

#[derive(clap::Args)]
pub struct ReplayArgs {
    /// Trace JSON written by `mj trace`.
    file: String,
}

/// Plays one hand with bots and prints its trace as JSON.
pub fn record(args: RecordArgs) -> anyhow::Result<()> {
    let rules: Arc<Ruleset> = Arc::new(load::ruleset(&args.rules)?);
    let spec = load::strategy(&args.strategy)?;
    let game = play_hand(&rules, &std::array::from_fn(|_| spec.clone()), args.seed)?;
    say!("{}", serde_json::to_string_pretty(&game.trace())?)?;
    Ok(())
}

/// Replays a trace, checking it reproduces, and narrates it.
pub fn replay(args: ReplayArgs) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(&args.file).with_context(|| format!("reading {}", args.file))?;
    let trace: Trace = serde_json::from_str(&text).with_context(|| format!("parsing {}", args.file))?;
    let game = trace.replay()?;
    say!("replayed {} steps; dealer {}, rules {}", trace.steps.len(), game.dealer(), trace.rules.name)?;
    print_events(&game, 0, None)?;
    let Some(settlement) = game.settlement() else {
        bail!("the trace ends before the hand is over");
    };
    let totals = settlement.total();
    for seat in Seat::ALL {
        say!("{seat}: {:+}", totals[seat.index()])?;
    }
    Ok(())
}
