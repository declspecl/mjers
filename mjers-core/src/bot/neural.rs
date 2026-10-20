//! Inference for `suit-resnet-v2` policies, mirroring
//! `python/mjers/model.py::ResidualPolicyNet` without dependencies.
//!
//! Callers parse checkpoint files and supply named tensors in PyTorch's
//! `state_dict` layout. Activations are stored tile-major (`[tile][channel]`)
//! so every convolution is a run of contiguous dot products.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};

use crate::bot::encoding::{
    self,
    ACTIONS,
    CONTEXT,
    EncodedObservation,
    PLANES,
};
use crate::bot::policy::Policy;
use crate::game::action::Action;
use crate::game::observation::Observation;
use crate::tile::{
    KINDS,
    SUITED_KINDS,
};

/// The value head predicts settlement units divided by this.
pub const VALUE_SCALE: f32 = 8.0;
const TILE_ACTIONS: usize = 102;
const OFFERED_PLANE: usize = 48;
const RANKS: usize = 9;

#[derive(Debug, Clone, PartialEq)]
pub struct Tensor {
    pub shape: Vec<usize>,
    pub data: Vec<f32>,
}

/// Named tensors, as in a PyTorch `state_dict`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Weights {
    tensors: BTreeMap<String, Tensor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeightError {
    Length { name: String, shape: Vec<usize>, values: usize },
    Missing(String),
    Shape { name: String, expected: Vec<usize>, found: Vec<usize> },
    Unexpected(String),
}

impl fmt::Display for WeightError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            WeightError::Length { name, shape, values } => write!(f, "{name} has shape {shape:?} but {values} values"),
            WeightError::Missing(name) => write!(f, "missing tensor {name}"),
            WeightError::Shape { name, expected, found } => write!(f, "{name} has shape {found:?}, expected {expected:?}"),
            WeightError::Unexpected(name) => write!(f, "unexpected tensor {name}"),
        }
    }
}

impl std::error::Error for WeightError {}

impl Weights {
    pub fn new() -> Weights {
        Weights::default()
    }

    pub fn insert(
        &mut self,
        name: impl Into<String>,
        shape: Vec<usize>,
        data: Vec<f32>,
    ) -> Result<(), WeightError> {
        let name = name.into();
        if shape.iter().product::<usize>() != data.len() {
            return Err(WeightError::Length { name, shape, values: data.len() });
        }
        self.tensors.insert(name, Tensor { shape, data });
        Ok(())
    }

    fn get(
        &self,
        name: &str,
        expected: &[usize],
    ) -> Result<&[f32], WeightError> {
        let tensor = self.tensors.get(name).ok_or_else(|| WeightError::Missing(name.into()))?;
        if tensor.shape != expected {
            return Err(WeightError::Shape {
                name: name.into(),
                expected: expected.to_vec(),
                found: tensor.shape.clone(),
            });
        }
        Ok(&tensor.data)
    }
}

/// A fully connected map, also used for pointwise convolutions.
#[derive(Debug, Clone)]
struct Dense {
    inputs: usize,
    outputs: usize,
    /// `[output][input]`.
    weight: Vec<f32>,
    bias: Vec<f32>,
}

impl Dense {
    fn load(
        weights: &Weights,
        used: &mut Vec<String>,
        prefix: &str,
        inputs: usize,
        outputs: usize,
        convolution: bool,
    ) -> Result<Dense, WeightError> {
        let shape = if convolution { vec![outputs, inputs, 1] } else { vec![outputs, inputs] };
        let weight = weights.get(&format!("{prefix}.weight"), &shape)?.to_vec();
        let bias = weights.get(&format!("{prefix}.bias"), &[outputs])?.to_vec();
        used.extend([format!("{prefix}.weight"), format!("{prefix}.bias")]);
        Ok(Dense { inputs, outputs, weight, bias })
    }

    fn apply(
        &self,
        input: &[f32],
        output: &mut [f32],
    ) {
        for ((out, row), bias) in output.iter_mut().zip(self.weight.chunks_exact(self.inputs)).zip(&self.bias) {
            *out = bias + dot(row, input);
        }
    }
}

/// A `(1, 3)` convolution along the ranks of each suit, zero-padded.
#[derive(Debug, Clone)]
struct RankConvolution {
    channels: usize,
    /// `[offset][output][input]` for rank offsets −1, 0, +1.
    weight: Vec<f32>,
    bias: Vec<f32>,
}

impl RankConvolution {
    fn load(
        weights: &Weights,
        used: &mut Vec<String>,
        prefix: &str,
        channels: usize,
    ) -> Result<RankConvolution, WeightError> {
        let torch = weights.get(&format!("{prefix}.weight"), &[channels, channels, 1, 3])?;
        let bias = weights.get(&format!("{prefix}.bias"), &[channels])?.to_vec();
        used.extend([format!("{prefix}.weight"), format!("{prefix}.bias")]);
        let mut weight = vec![0.0; 3 * channels * channels];
        for output in 0..channels {
            for input in 0..channels {
                for offset in 0..3 {
                    weight[(offset * channels + output) * channels + input] = torch[(output * channels + input) * 3 + offset];
                }
            }
        }
        Ok(RankConvolution { channels, weight, bias })
    }

    /// `input` and `output` hold the 27 suited tiles, tile-major.
    fn apply(
        &self,
        input: &[f32],
        output: &mut [f32],
    ) {
        let c = self.channels;
        for tile in 0..SUITED_KINDS {
            let rank = tile % RANKS;
            let out = &mut output[tile * c..(tile + 1) * c];
            out.copy_from_slice(&self.bias);
            for offset in 0..3 {
                let Some(source) = (rank + offset).checked_sub(1).filter(|&r| r < RANKS) else { continue };
                let neighbour = &input[(tile - rank + source) * c..(tile - rank + source + 1) * c];
                for (value, row) in out.iter_mut().zip(self.weight[offset * c * c..(offset + 1) * c * c].chunks_exact(c)) {
                    *value += dot(row, neighbour);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Block {
    suit_first: RankConvolution,
    suit_second: RankConvolution,
    honor_first: Dense,
    honor_second: Dense,
    pooled: Dense,
}

impl Block {
    fn apply(
        &self,
        features: &mut [f32],
        scratch: &mut Scratch,
    ) {
        let c = self.pooled.outputs;
        let suited = SUITED_KINDS * c;
        relu_into(features, &mut scratch.active);
        pool(&scratch.active, c, &mut scratch.pooled);
        let mut bias = vec![0.0; c];
        self.pooled.apply(&scratch.pooled, &mut bias);
        self.suit_first.apply(&scratch.active[..suited], &mut scratch.hidden[..suited]);
        for tile in SUITED_KINDS..KINDS {
            self.honor_first.apply(&scratch.active[tile * c..(tile + 1) * c], &mut scratch.hidden[tile * c..(tile + 1) * c]);
        }
        for row in scratch.hidden.chunks_exact_mut(c) {
            for (value, b) in row.iter_mut().zip(&bias) {
                *value = (*value + b).max(0.0);
            }
        }
        self.suit_second.apply(&scratch.hidden[..suited], &mut scratch.active[..suited]);
        for tile in SUITED_KINDS..KINDS {
            self.honor_second.apply(&scratch.hidden[tile * c..(tile + 1) * c], &mut scratch.active[tile * c..(tile + 1) * c]);
        }
        for (value, residual) in features.iter_mut().zip(&scratch.active) {
            *value += residual;
        }
    }
}

struct Scratch {
    active: Vec<f32>,
    hidden: Vec<f32>,
    pooled: Vec<f32>,
}

/// Policy logits (illegal actions at −∞) and the predicted settlement in units.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub logits: [f32; ACTIONS],
    pub settlement: f32,
}

impl Evaluation {
    /// Each legal action of `observation` with its softmax probability, most likely first.
    pub fn choices(
        &self,
        observation: &Observation<'_>,
    ) -> Vec<(Action, f32)> {
        let legal: Vec<(Action, f32)> = observation
            .legal_actions()
            .iter()
            .filter_map(|&action| encoding::action_index(action, observation.decision()).map(|index| (action, self.logits[index])))
            .collect();
        let top = legal.iter().map(|&(_, logit)| logit).fold(f32::NEG_INFINITY, f32::max);
        let total: f32 = legal.iter().map(|&(_, logit)| (logit - top).exp()).sum();
        let mut choices: Vec<(Action, f32)> = legal.into_iter().map(|(action, logit)| (action, (logit - top).exp() / total)).collect();
        choices.sort_by(|a, b| b.1.total_cmp(&a.1));
        choices
    }

    /// The highest legal logit, first index on ties (as `torch.argmax`).
    pub fn best(&self) -> Option<usize> {
        let mut best: Option<usize> = None;
        for (index, &logit) in self.logits.iter().enumerate() {
            if logit > f32::NEG_INFINITY && best.is_none_or(|b| logit > self.logits[b]) {
                best = Some(index);
            }
        }
        best
    }
}

#[derive(Debug, Clone)]
pub struct ResidualNet {
    width: usize,
    stem: Dense,
    blocks: Vec<Block>,
    tile_actions: Dense,
    summary: Dense,
    other_actions: Dense,
    value: Dense,
}

impl ResidualNet {
    /// Builds from a complete `suit-resnet-v2` state dict; every tensor must be used.
    pub fn new(weights: &Weights) -> Result<ResidualNet, WeightError> {
        let stem_shape = &weights.tensors.get("stem.weight").ok_or_else(|| WeightError::Missing("stem.weight".into()))?.shape;
        let width = stem_shape.first().copied().unwrap_or(0);
        let mut used = Vec::new();
        let stem = Dense::load(weights, &mut used, "stem", PLANES + CONTEXT, width, true)?;
        let mut blocks = Vec::new();
        while weights.tensors.contains_key(&format!("blocks.{}.pooled.weight", blocks.len())) {
            let prefix = format!("blocks.{}", blocks.len());
            blocks.push(Block {
                suit_first: RankConvolution::load(weights, &mut used, &format!("{prefix}.suit_first"), width)?,
                suit_second: RankConvolution::load(weights, &mut used, &format!("{prefix}.suit_second"), width)?,
                honor_first: Dense::load(weights, &mut used, &format!("{prefix}.honor_first"), width, width, true)?,
                honor_second: Dense::load(weights, &mut used, &format!("{prefix}.honor_second"), width, width, true)?,
                pooled: Dense::load(weights, &mut used, &format!("{prefix}.pooled"), 2 * width, width, false)?,
            });
        }
        let net = ResidualNet {
            width,
            stem,
            blocks,
            tile_actions: Dense::load(weights, &mut used, "tile_actions", width, TILE_ACTIONS / KINDS, true)?,
            summary: Dense::load(weights, &mut used, "summary", 3 * width + CONTEXT, width, false)?,
            other_actions: Dense::load(weights, &mut used, "other_actions", width, ACTIONS - TILE_ACTIONS, false)?,
            value: Dense::load(weights, &mut used, "value_layer", width, 1, false)?,
        };
        if let Some(extra) = weights.tensors.keys().find(|name| !used.contains(name)) {
            return Err(WeightError::Unexpected(extra.clone()));
        }
        Ok(net)
    }

    pub fn evaluate(
        &self,
        encoded: &EncodedObservation,
    ) -> Evaluation {
        let c = self.width;
        let mut input = [0.0; PLANES + CONTEXT];
        let mut features = vec![0.0; KINDS * c];
        for (tile, row) in features.chunks_exact_mut(c).enumerate() {
            for (value, plane) in input.iter_mut().zip(&encoded.tiles) {
                *value = plane[tile];
            }
            input[PLANES..].copy_from_slice(&encoded.context);
            self.stem.apply(&input, row);
        }
        let mut scratch = Scratch {
            active: vec![0.0; KINDS * c],
            hidden: vec![0.0; KINDS * c],
            pooled: vec![0.0; 2 * c],
        };
        for block in &self.blocks {
            block.apply(&mut features, &mut scratch);
        }
        let active = &mut scratch.active;
        relu_into(&features, active);

        let mut logits = [f32::NEG_INFINITY; ACTIONS];
        let mut tile_logits = [0.0; TILE_ACTIONS / KINDS];
        for (tile, row) in active.chunks_exact(c).enumerate() {
            self.tile_actions.apply(row, &mut tile_logits);
            for (kind, &logit) in tile_logits.iter().enumerate() {
                logits[kind * KINDS + tile] = logit;
            }
        }
        let mut summary = vec![0.0; 3 * c + CONTEXT];
        pool(active, c, &mut summary[..2 * c]);
        for (tile, row) in active.chunks_exact(c).enumerate() {
            let offered = encoded.tiles[OFFERED_PLANE][tile];
            for (value, &feature) in summary[2 * c..3 * c].iter_mut().zip(row) {
                *value += feature * offered;
            }
        }
        summary[3 * c..].copy_from_slice(&encoded.context);
        let mut hidden = vec![0.0; c];
        self.summary.apply(&summary, &mut hidden);
        for value in &mut hidden {
            *value = value.max(0.0);
        }
        self.other_actions.apply(&hidden, &mut logits[TILE_ACTIONS..]);
        let mut value = [0.0];
        self.value.apply(&hidden, &mut value);
        for (logit, &legal) in logits.iter_mut().zip(&encoded.action_mask) {
            if !legal {
                *logit = f32::NEG_INFINITY;
            }
        }
        Evaluation {
            logits,
            settlement: value[0] * VALUE_SCALE,
        }
    }
}

/// A loaded network and the checkpoint it came from. It serializes as the
/// checkpoint path; deserializing would need the weights, so callers that read
/// files construct it with [`NeuralNetwork::new`] instead.
#[derive(Debug, Clone)]
pub struct NeuralNetwork {
    checkpoint: String,
    net: Arc<ResidualNet>,
}

impl NeuralNetwork {
    pub fn new(
        checkpoint: impl Into<String>,
        net: ResidualNet,
    ) -> NeuralNetwork {
        NeuralNetwork {
            checkpoint: checkpoint.into(),
            net: Arc::new(net),
        }
    }

    pub fn checkpoint(&self) -> &str {
        &self.checkpoint
    }

    pub fn net(&self) -> &Arc<ResidualNet> {
        &self.net
    }
}

/// Networks are identified by their checkpoint.
impl PartialEq for NeuralNetwork {
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.checkpoint == other.checkpoint
    }
}

impl Serialize for NeuralNetwork {
    fn serialize<S: Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.checkpoint)
    }
}

impl<'de> Deserialize<'de> for NeuralNetwork {
    fn deserialize<D: Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
        Err(serde::de::Error::custom("neural strategies must be loaded together with their checkpoint weights"))
    }
}

/// Plays the network's highest-scoring legal action.
pub struct NeuralBot {
    name: String,
    net: Arc<ResidualNet>,
}

impl NeuralBot {
    pub fn new(
        name: impl Into<String>,
        net: Arc<ResidualNet>,
    ) -> NeuralBot {
        NeuralBot { name: name.into(), net }
    }
}

impl Policy for NeuralBot {
    fn name(&self) -> &str {
        &self.name
    }

    fn act(
        &mut self,
        observation: &Observation<'_>,
        legal: &[Action],
    ) -> Action {
        let chosen = self.net.evaluate(&encoding::encode(observation)).best().and_then(|index| encoding::legal_action(observation, index));
        chosen.or_else(|| legal.first().copied()).unwrap_or(Action::Pass)
    }
}

/// Eight independent partial sums let the compiler vectorize the loop.
fn dot(
    a: &[f32],
    b: &[f32],
) -> f32 {
    const LANES: usize = 8;
    let mut sums = [0.0f32; LANES];
    let ((a_chunks, a_tail), (b_chunks, b_tail)) = (a.as_chunks::<LANES>(), b.as_chunks::<LANES>());
    for (x, y) in a_chunks.iter().zip(b_chunks) {
        for ((sum, x), y) in sums.iter_mut().zip(x).zip(y) {
            *sum += x * y;
        }
    }
    let tail: f32 = a_tail.iter().zip(b_tail).map(|(x, y)| x * y).sum();
    sums.iter().sum::<f32>() + tail
}

fn relu_into(
    input: &[f32],
    output: &mut [f32],
) {
    for (out, &value) in output.iter_mut().zip(input) {
        *out = value.max(0.0);
    }
}

/// Per-channel mean then maximum over all tiles of tile-major `features`.
fn pool(
    features: &[f32],
    channels: usize,
    output: &mut [f32],
) {
    let (mean, max) = output.split_at_mut(channels);
    mean.fill(0.0);
    max.fill(f32::NEG_INFINITY);
    for row in features.chunks_exact(channels) {
        for ((m, x), &value) in mean.iter_mut().zip(max.iter_mut()).zip(row) {
            *m += value;
            *x = x.max(value);
        }
    }
    for m in mean {
        *m /= KINDS as f32;
    }
}
