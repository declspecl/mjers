use mjers_sim::compare::compare;
use mjers_sim::report::{
    ComparisonResult,
    Report,
};
use mjers_sim::runner::Seeds;

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
    /// Candidate strategy (preset or JSON file).
    a: String,
    /// Strategy to compare against.
    b: String,
    /// Strategy in the other three seats.
    #[arg(long, default_value = "efficiency")]
    field: String,
    #[arg(long, default_value = "family")]
    rules: String,
    #[arg(long, default_value_t = 10_000)]
    hands: u64,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    #[arg(long)]
    json: bool,
}

pub fn run(args: Args) -> anyhow::Result<()> {
    let rules = load::ruleset(&args.rules)?;
    let (a, b, field) = (load::strategy(&args.a)?, load::strategy(&args.b)?, load::strategy(&args.field)?);
    let seeds = Seeds { start: args.seed, count: args.hands };
    let result = ComparisonResult::from(compare(&rules, &a, &b, &field, seeds));
    if args.json {
        let report = Report::new("comparison", provenance::current(), rules, seeds, vec![a, b, field], result);
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    let c = &result.comparison;
    println!("{} vs {} against a {} field, {} hands × 4 seats, rules {}", c.a, c.b, c.field, args.hands, rules.name);
    println!();
    print_header("candidate");
    print_row(&c.a, &result.a_metrics);
    print_row(&c.b, &result.b_metrics);
    println!();
    println!("difference {:+.3} ± {:.3} units per hand (z = {:.2})", c.mean_difference, c.stderr, result.z);
    Ok(())
}
