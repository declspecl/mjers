//! Self-describing experiment output.

use std::collections::BTreeMap;

use mjers_bot::spec::BotSpec;
use mjers_rules::ruleset::Ruleset;
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

pub const REPORT_SCHEMA: u32 = 1;

/// Which build produced a report. Filled in by the binary from build-time
/// metadata; a dirty revision does not identify uncommitted changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub revision: String,
    pub dirty: Option<bool>,
    pub compiler: String,
    pub target: String,
    pub profile: String,
    pub opt_level: String,
}

impl Provenance {
    pub fn unknown() -> Provenance {
        let unknown = || "unknown".to_string();
        Provenance {
            revision: unknown(),
            dirty: None,
            compiler: unknown(),
            target: unknown(),
            profile: unknown(),
            opt_level: unknown(),
        }
    }
}

/// Everything needed to reproduce and interpret a result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report<T> {
    pub schema: u32,
    pub kind: String,
    pub provenance: Provenance,
    pub rules: Ruleset,
    pub seeds: Seeds,
    /// Resolved strategies, in seat order for simulations.
    pub strategies: Vec<BotSpec>,
    pub result: T,
}

impl<T> Report<T> {
    pub fn new(
        kind: &str,
        provenance: Provenance,
        rules: Ruleset,
        seeds: Seeds,
        strategies: Vec<BotSpec>,
        result: T,
    ) -> Report<T> {
        Report {
            schema: REPORT_SCHEMA,
            kind: kind.to_string(),
            provenance,
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
        let report = Report::new("simulation", Provenance::unknown(), Ruleset::family(), seeds, table.to_vec(), SimulationResult::from(summary));
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("\"schema\": 1"));
        assert!(json.contains("\"ev\""));
        let back: Report<SimulationResult> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.result.summary, report.result.summary);
        assert_eq!(back.strategies, report.strategies);
    }
}
