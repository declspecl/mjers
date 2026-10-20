use anyhow::{
    Context,
    bail,
};
use mjers_bot::spec::BotSpec;
use mjers_sim::report::Report;
use mjers_sim::runner::Seeds;
use mjers_sim::tune::{
    TuneConfig,
    tune,
};

use super::sim::{
    print_header,
    print_row,
};
use super::{
    load,
    provenance,
};

#[derive(clap::Args)]
pub struct Args {
    /// Heuristic strategy to start from.
    #[arg(long, default_value = "efficiency")]
    start: String,
    /// Strategy in the other three seats.
    #[arg(long, default_value = "efficiency")]
    field: String,
    #[arg(long, default_value = "family")]
    rules: String,
    /// Training hands per trial; validation uses the next block of this size.
    #[arg(long, default_value_t = 2_000)]
    hands: u64,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    #[arg(long, default_value_t = 2)]
    rounds: u32,
    /// Standard errors a change must win by.
    #[arg(long, default_value_t = 2.0)]
    min_z: f64,
    /// Name for the tuned strategy.
    #[arg(long, default_value = "tuned")]
    name: String,
    /// Write the tuned strategy to this JSON file.
    #[arg(long)]
    output: Option<String>,
    #[arg(long)]
    json: bool,
}

pub fn run(args: Args) -> anyhow::Result<()> {
    let rules = load::ruleset(&args.rules)?;
    let BotSpec::Heuristic { params: start, .. } = load::strategy(&args.start)? else {
        bail!("--start must be a heuristic strategy");
    };
    let field = load::strategy(&args.field)?;
    let config = TuneConfig {
        start,
        field: field.clone(),
        training: Seeds { start: args.seed, count: args.hands },
        rounds: args.rounds,
        min_z: args.min_z,
    };
    let result = tune(&rules, &config)?;
    let tuned = BotSpec::Heuristic {
        name: args.name.clone(),
        params: result.champion.clone(),
    };
    if let Some(path) = &args.output {
        std::fs::write(path, serde_json::to_string_pretty(&tuned)? + "\n").with_context(|| format!("writing {path}"))?;
    }
    if args.json {
        let report = Report::new("tuning", provenance::current(), rules, config.training, vec![tuned, field], result);
        say!("{}", serde_json::to_string_pretty(&report)?)?;
        return Ok(());
    }
    for trial in result.trials.iter().filter(|t| t.adopted) {
        say!("round {}: adopted {:?} change, {:+.3} ± {:.3}", trial.round, trial.dimension, trial.mean_difference, trial.stderr)?;
    }
    say!("{} trials; champion: {}", result.trials.len(), serde_json::to_string(&result.champion)?)?;
    say!()?;
    say!(
        "held-out seeds {}..{}:",
        result.validation_seeds.start,
        result.validation_seeds.start.wrapping_add(result.validation_seeds.count)
    )?;
    print_header("candidate")?;
    print_row("champion", &result.validation.a_metrics)?;
    print_row("start", &result.validation.b_metrics)?;
    let c = &result.validation.comparison;
    say!("difference {:+.3} ± {:.3} units per hand (z = {:.2})", c.mean_difference, c.stderr, result.validation.z)?;
    Ok(())
}
