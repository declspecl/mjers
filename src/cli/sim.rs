use std::fs::File;
use std::io::BufWriter;

use anyhow::{
    Context,
    bail,
};
use mjers_bot::spec::BotSpec;
use mjers_sim::features::write_features;
use mjers_sim::report::{
    Report,
    SimulationResult,
};
use mjers_sim::runner::{
    Seeds,
    Table,
};
use mjers_sim::stats::{
    Metrics,
    simulate,
};

use super::{
    load,
    provenance,
};

#[derive(clap::Args)]
pub struct Args {
    /// Ruleset preset or JSON file.
    #[arg(long, default_value = "family")]
    rules: String,
    /// Strategy for every seat (preset or JSON file).
    #[arg(long, default_value = "efficiency", conflicts_with = "seats")]
    strategy: String,
    /// Four comma-separated strategies, one per seat.
    #[arg(long, value_delimiter = ',')]
    seats: Vec<String>,
    #[arg(long, default_value_t = 10_000)]
    hands: u64,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    /// Print a JSON report instead of a table.
    #[arg(long)]
    json: bool,
    /// Also write one JSON Lines row per seat per hand to this file.
    #[arg(long)]
    features: Option<String>,
}

pub fn run(args: Args) -> anyhow::Result<()> {
    let rules = load::ruleset(&args.rules)?;
    let names = if args.seats.is_empty() { vec![args.strategy.clone(); 4] } else { args.seats.clone() };
    if names.len() != 4 {
        bail!("--seats needs exactly four strategies, got {}", names.len());
    }
    let specs = names.iter().map(|n| load::strategy(n)).collect::<anyhow::Result<Vec<BotSpec>>>()?;
    let Ok(table) = Table::try_from(specs.clone()) else {
        bail!("--seats needs exactly four strategies");
    };
    let seeds = Seeds { start: args.seed, count: args.hands };

    if let Some(path) = &args.features {
        let file = File::create(path).with_context(|| format!("creating {path}"))?;
        let rows = write_features(&rules, &table, seeds, BufWriter::new(file))?;
        eprintln!("wrote {rows} rows to {path}");
    }
    let result = SimulationResult::from(simulate(&rules, &table, seeds)?);
    if args.json {
        let report = Report::new("simulation", provenance::current(), rules, seeds, specs, result);
        say!("{}", serde_json::to_string_pretty(&report)?)?;
        return Ok(());
    }
    say!("{} hands, rules {}, seeds {}..{}", args.hands, rules.name, seeds.start, seeds.start.wrapping_add(seeds.count))?;
    say!()?;
    print_header("strategy")?;
    for (name, metrics) in &result.metrics {
        print_row(name, metrics)?;
    }
    say!()?;
    print_header("position")?;
    for (i, metrics) in result.position_metrics.iter().enumerate() {
        let label = if i == 0 { "dealer".to_string() } else { format!("dealer+{i}") };
        print_row(&label, metrics)?;
    }
    Ok(())
}

pub fn print_header(label: &str) -> std::io::Result<()> {
    say!(
        "{label:<18} {:>9} {:>15} {:>6} {:>6} {:>8} {:>6} {:>7} {:>6} {:>6}",
        "hands",
        "ev ± se",
        "win%",
        "self%",
        "dealin%",
        "draw%",
        "kong/h",
        "chow/h",
        "pong/h"
    )
}

pub fn print_row(
    label: &str,
    m: &Metrics,
) -> std::io::Result<()> {
    say!(
        "{label:<18} {:>9} {:>15} {:>6.1} {:>6.1} {:>8.1} {:>6.1} {:>7.3} {:>6.2} {:>6.2}",
        m.hands,
        format!("{:+.3} ± {:.3}", m.ev, m.ev_stderr),
        100.0 * m.win_rate,
        100.0 * m.self_draw_rate,
        100.0 * m.deal_in_rate,
        100.0 * m.exhausted_rate,
        m.kong_units_per_hand,
        m.chows_per_hand,
        m.pongs_per_hand,
    )
}
