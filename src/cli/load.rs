//! Rulesets and strategies by preset name or JSON file path.

use std::path::Path;

use anyhow::Context;
use mjers_core::advisor::search::SearchConfig;
use mjers_core::bot::neural::NeuralNetwork;
use mjers_core::bot::spec::BotSpec;
use mjers_core::rules::Ruleset;
use mjers_sim::runner::Seeds;
use serde::Deserialize;

use super::checkpoint;

pub fn ruleset(name_or_path: &str) -> anyhow::Result<Ruleset> {
    if let Some(rules) = Ruleset::preset(name_or_path) {
        return Ok(rules);
    }
    json_file(name_or_path).with_context(|| format!("not a preset ({}) or ruleset file", Ruleset::PRESETS.join(", ")))
}

pub fn strategy(name_or_path: &str) -> anyhow::Result<BotSpec> {
    if let Some(spec) = BotSpec::preset(name_or_path) {
        return Ok(spec);
    }
    let value: serde_json::Value = json_file(name_or_path).with_context(|| format!("not a preset ({}) or strategy file", BotSpec::PRESETS.join(", ")))?;
    if value.get("kind").and_then(serde_json::Value::as_str) == Some("neural") {
        let NeuralFile::Neural { name, network } = NeuralFile::deserialize(&value).with_context(|| format!("parsing {name_or_path}"))?;
        let net = checkpoint::read(&network)?;
        return Ok(BotSpec::Neural {
            name,
            network: NeuralNetwork::new(network, net),
        });
    }
    if value.get("kind").and_then(serde_json::Value::as_str) == Some("search") {
        let SearchFile::Search { name, network, search } = SearchFile::deserialize(&value).with_context(|| format!("parsing {name_or_path}"))?;
        let net = checkpoint::read(&network)?;
        return Ok(BotSpec::Search {
            name,
            network: NeuralNetwork::new(network, net),
            search: Box::new(SearchConfig {
                samples: search.samples,
                candidates: search.candidates,
                confident: search.confident,
                margin: search.margin,
                seed: search.seed,
                field: strategy(&search.field)?,
            }),
        });
    }
    let names_parts = value.get("turns").is_some_and(serde_json::Value::is_string) || value.get("claims").is_some_and(serde_json::Value::is_string);
    if value.get("kind").and_then(serde_json::Value::as_str) == Some("hybrid") && names_parts {
        let HybridFile::Hybrid { name, turns, claims } = HybridFile::deserialize(&value).with_context(|| format!("parsing {name_or_path}"))?;
        return Ok(BotSpec::Hybrid {
            name,
            turns: Box::new(strategy(&turns)?),
            claims: Box::new(strategy(&claims)?),
        });
    }
    serde_json::from_value(value).with_context(|| format!("parsing {name_or_path}"))
}

/// `{"kind": "neural", "name": ..., "network": "<checkpoint path>"}`.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum NeuralFile {
    Neural { name: String, network: String },
}

/// `{"kind": "search", "name": ..., "network": "<checkpoint path>", "search": {...}}`,
/// where the search's playout `field` names a strategy by preset or file.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SearchFile {
    Search { name: String, network: String, search: SearchFileConfig },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchFileConfig {
    samples: u32,
    candidates: usize,
    confident: f32,
    margin: f64,
    seed: u64,
    field: String,
}

/// `{"kind": "hybrid", "name": ..., "turns": "<strategy>", "claims": "<strategy>"}`,
/// naming each part by preset or file so neural parts can load their weights.
/// Hybrids with inline strategy objects parse as ordinary `BotSpec` JSON.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum HybridFile {
    Hybrid { name: String, turns: String, claims: String },
}

/// Neural strategies may not play under other rules or on seeds they trained on.
pub fn check_unseen(
    spec: &BotSpec,
    rules: &Ruleset,
    seeds: Seeds,
) -> anyhow::Result<()> {
    match spec {
        BotSpec::Neural { name, network } => {
            checkpoint::read_metadata(network.checkpoint())?
                .check_unseen(rules, seeds)
                .with_context(|| format!("strategy {name}"))?;
        },
        BotSpec::Search { name, network, search } => {
            checkpoint::read_metadata(network.checkpoint())?
                .check_unseen(rules, seeds)
                .with_context(|| format!("strategy {name}"))?;
            check_unseen(&search.field, rules, seeds)?;
        },
        BotSpec::Hybrid { turns, claims, .. } => {
            check_unseen(turns, rules, seeds)?;
            check_unseen(claims, rules, seeds)?;
        },
        _ => {},
    }
    Ok(())
}

fn json_file<T: serde::de::DeserializeOwned>(path: &str) -> anyhow::Result<T> {
    let text = std::fs::read_to_string(Path::new(path)).with_context(|| format!("reading {path}"))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {path}"))
}
