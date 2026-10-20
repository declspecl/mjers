//! Rulesets and strategies by preset name or JSON file path.

use std::path::Path;

use anyhow::Context;
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
    serde_json::from_value(value).with_context(|| format!("parsing {name_or_path}"))
}

/// `{"kind": "neural", "name": ..., "network": "<checkpoint path>"}`.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum NeuralFile {
    Neural { name: String, network: String },
}

/// Neural strategies may not play under other rules or on seeds they trained on.
pub fn check_unseen(
    spec: &BotSpec,
    rules: &Ruleset,
    seeds: Seeds,
) -> anyhow::Result<()> {
    if let BotSpec::Neural { name, network } = spec {
        checkpoint::read_metadata(network.checkpoint())?
            .check_unseen(rules, seeds)
            .with_context(|| format!("strategy {name}"))?;
    }
    Ok(())
}

fn json_file<T: serde::de::DeserializeOwned>(path: &str) -> anyhow::Result<T> {
    let text = std::fs::read_to_string(Path::new(path)).with_context(|| format!("reading {path}"))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {path}"))
}
