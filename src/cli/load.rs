//! Rulesets and strategies by preset name or JSON file path.

use std::path::Path;

use anyhow::Context;
use mjers_bot::spec::BotSpec;
use mjers_rules::ruleset::Ruleset;

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
    json_file(name_or_path).with_context(|| format!("not a preset ({}) or strategy file", BotSpec::PRESETS.join(", ")))
}

fn json_file<T: serde::de::DeserializeOwned>(path: &str) -> anyhow::Result<T> {
    let text = std::fs::read_to_string(Path::new(path)).with_context(|| format!("reading {path}"))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {path}"))
}
