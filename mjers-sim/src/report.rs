//! Self-describing experiment output.

use std::collections::BTreeMap;

use mjers_core::bot::spec::BotSpec;
use mjers_core::rules::Ruleset;
use serde::{
    Deserialize,
    Serialize,
};

use crate::compare::Comparison;
use crate::runner::Seeds;
use crate::stats::{
    Metrics,
    Summary,
};

pub const REPORT_SCHEMA: u32 = 2;

/// Experiment inputs and results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report<T> {
    pub schema: u32,
    pub kind: String,
    pub rules: Ruleset,
    pub seeds: Seeds,
    /// Resolved strategies, in seat order for simulations.
    pub strategies: Vec<BotSpec>,
    pub result: T,
}

impl<T> Report<T> {
    pub fn new(
        kind: &str,
        rules: Ruleset,
        seeds: Seeds,
        strategies: Vec<BotSpec>,
        result: T,
    ) -> Report<T> {
        Report {
            schema: REPORT_SCHEMA,
            kind: kind.to_string(),
            rules,
            seeds,
            strategies,
            result,
        }
    }
}

/// A simulation summary with derived metrics alongside the raw counters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationResult {
    pub metrics: BTreeMap<String, Metrics>,
    /// Metrics by position after the dealer (0 is the dealer).
    pub position_metrics: Vec<Metrics>,
    pub summary: Summary,
}

impl From<Summary> for SimulationResult {
    fn from(summary: Summary) -> Self {
        SimulationResult {
            metrics: summary.by_strategy.iter().map(|(name, stats)| (name.clone(), stats.metrics())).collect(),
            position_metrics: summary.by_position.iter().map(|s| s.metrics()).collect(),
            summary,
        }
    }
}

/// A comparison with its z-score spelled out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonResult {
    pub z: f64,
    pub a_metrics: Metrics,
    pub b_metrics: Metrics,
    pub comparison: Comparison,
}

impl From<Comparison> for ComparisonResult {
    fn from(comparison: Comparison) -> Self {
        ComparisonResult {
            z: comparison.z(),
            a_metrics: comparison.a_stats.metrics(),
            b_metrics: comparison.b_stats.metrics(),
            comparison,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::simulate;

    #[test]
    fn simulation_report_round_trips() {
        let table: [BotSpec; 4] = std::array::from_fn(|_| BotSpec::preset("efficiency").unwrap());
        let seeds = Seeds { start: 0, count: 20 };
        let summary = simulate(&Ruleset::family(), &table, seeds).unwrap();
        let report = Report::new("simulation", Ruleset::family(), seeds, table.to_vec(), SimulationResult::from(summary));
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("\"ev\""));
        let back: Report<SimulationResult> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema, REPORT_SCHEMA);
        assert_eq!(back.result.summary, report.result.summary);
        assert_eq!(back.strategies, report.strategies);
    }
}
