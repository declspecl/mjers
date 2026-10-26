//! `suit-resnet-v2` safetensors checkpoints written by `python/mjers/model.py`.

use std::path::Path;

use anyhow::{
    Context,
    bail,
    ensure,
};
use mjers_core::bot::encoding;
use mjers_core::bot::neural::{
    ResidualNet,
    Weights,
};
use mjers_core::rules::Ruleset;
use mjers_sim::runner::Seeds;
use serde::Deserialize;
use serde_json::Value;

const ARCHITECTURE: &str = "suit-resnet-v2";

/// The training record stored under the checkpoint's `mjers` metadata key.
#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub architecture: String,
    pub dataset: Dataset,
    pub training_seeds: Vec<u64>,
    pub validation_seeds: Vec<u64>,
    #[serde(default)]
    pub reinforcement: Vec<Stage>,
}

#[derive(Debug, Deserialize)]
pub struct Dataset {
    pub encoding_version: u32,
    pub rules_json: String,
}

#[derive(Debug, Deserialize)]
pub struct Stage {
    pub start_seed: u64,
    pub hands: u64,
}

impl Metadata {
    /// Refuses play under other rules or on hand seeds any training stage used.
    pub fn check_unseen(
        &self,
        rules: &Ruleset,
        seeds: Seeds,
    ) -> anyhow::Result<()> {
        ensure!(serde_json::to_string(rules)? == self.dataset.rules_json, "the network was trained under different rules");
        let (start, end) = (seeds.start, seeds.start.saturating_add(seeds.count));
        let seen = self.training_seeds.iter().chain(&self.validation_seeds).any(|seed| (start..end).contains(seed));
        let reinforced = self.reinforcement.iter().any(|stage| stage.start_seed < end && start < stage.start_seed.saturating_add(stage.hands));
        ensure!(!seen && !reinforced, "hand seeds {start}..{end} overlap the network's training or validation seeds");
        Ok(())
    }
}

/// Only the training record, for checks before play.
pub fn read_metadata(path: &str) -> anyhow::Result<Metadata> {
    let bytes = std::fs::read(Path::new(path)).with_context(|| format!("reading {path}"))?;
    let (header, _) = split(&bytes).with_context(|| format!("{path} is not a safetensors file"))?;
    metadata(&header).with_context(|| format!("reading {path} metadata"))
}

/// The network, after checking its architecture and observation encoding.
pub fn read(path: &str) -> anyhow::Result<ResidualNet> {
    let bytes = std::fs::read(Path::new(path)).with_context(|| format!("reading {path}"))?;
    let (header, data) = split(&bytes).with_context(|| format!("{path} is not a safetensors file"))?;
    let metadata = metadata(&header).with_context(|| format!("reading {path} metadata"))?;
    ensure!(metadata.architecture == ARCHITECTURE, "{path} is a {} network; only {ARCHITECTURE} runs in Rust", metadata.architecture);
    let version = metadata.dataset.encoding_version;
    let planes = encoding::planes(version).with_context(|| format!("{path} uses observation encoding {version}"))?;
    let mut weights = Weights::new();
    for (name, info) in &header {
        if name == "__metadata__" {
            continue;
        }
        ensure!(info["dtype"] == "F32", "{name} is not float32");
        let shape = info["shape"]
            .as_array()
            .context("tensor shape")?
            .iter()
            .map(|d| d.as_u64().map(|d| d as usize))
            .collect::<Option<Vec<_>>>()
            .context("tensor shape")?;
        let offsets = info["data_offsets"].as_array().context("tensor offsets")?;
        let (Some(begin), Some(end)) = (offsets.first().and_then(Value::as_u64), offsets.get(1).and_then(Value::as_u64)) else {
            bail!("{name} offsets")
        };
        let raw = data.get(begin as usize..end as usize).with_context(|| format!("{name} lies outside the file"))?;
        let (values, rest) = raw.as_chunks::<4>();
        ensure!(rest.is_empty(), "{name} is not a whole number of float32 values");
        let values = values.iter().map(|&b| f32::from_le_bytes(b)).collect();
        weights.insert(name.clone(), shape, values)?;
    }
    let net = ResidualNet::new(&weights).with_context(|| format!("loading {path}"))?;
    ensure!(net.planes() == planes, "{path}'s stem does not read encoding {version} observations");
    Ok(net)
}

/// The JSON header and the tensor bytes after it.
fn split(bytes: &[u8]) -> anyhow::Result<(serde_json::Map<String, Value>, &[u8])> {
    let length = bytes.get(..8).context("too short")?;
    let length = u64::from_le_bytes(length.try_into()?) as usize;
    let header = bytes.get(8..8 + length).context("truncated header")?;
    let Value::Object(header) = serde_json::from_slice(header)? else {
        bail!("header is not an object")
    };
    Ok((header, &bytes[8 + length..]))
}

fn metadata(header: &serde_json::Map<String, Value>) -> anyhow::Result<Metadata> {
    let text = header.get("__metadata__").and_then(|m| m.get("mjers")).and_then(Value::as_str).context("no mjers metadata")?;
    Ok(serde_json::from_str(text)?)
}
