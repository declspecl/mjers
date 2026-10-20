//! Trained `suit-resnet-v2` networks in the browser. JavaScript reads the
//! safetensors file (`loadNetwork` in the npm wrapper) and adds each tensor;
//! inference runs in the core.

use std::sync::Arc;

use mjers_core::bot::encoding;
use mjers_core::bot::neural::{
    NeuralBot,
    ResidualNet,
    Weights,
};
use mjers_core::game::action::Action;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::types::NetworkEvaluationValue;
use crate::{
    Bot,
    Game,
    encode,
    error,
};

/// Named float32 tensors in PyTorch `state_dict` layout. Call `free()` after building a network.
#[wasm_bindgen]
#[derive(Default)]
pub struct NetworkWeights {
    weights: Weights,
}

#[wasm_bindgen]
impl NetworkWeights {
    #[wasm_bindgen(constructor)]
    pub fn new() -> NetworkWeights {
        NetworkWeights::default()
    }

    pub fn add(
        &mut self,
        name: String,
        shape: Vec<u32>,
        data: Vec<f32>,
    ) -> Result<(), JsError> {
        self.weights.insert(name, shape.into_iter().map(|d| d as usize).collect(), data).map_err(error)
    }
}

/// A loaded network. Synchronous inference takes about a millisecond per decision.
#[wasm_bindgen]
pub struct Network {
    net: Arc<ResidualNet>,
}

#[wasm_bindgen]
impl Network {
    /// Every tensor of a complete state dict must be present and used.
    #[wasm_bindgen(constructor)]
    pub fn new(weights: &NetworkWeights) -> Result<Network, JsError> {
        Ok(Network {
            net: Arc::new(ResidualNet::new(&weights.weights).map_err(error)?),
        })
    }

    /// The network's probability for each legal action and its predicted settlement.
    pub fn evaluate(
        &self,
        game: &Game,
        player: usize,
    ) -> Result<NetworkEvaluationValue, JsError> {
        let player = game.acting_seat(player)?;
        let observation = game.inner.observe(player);
        let evaluation = self.net.evaluate(&encoding::encode(&observation));
        let legal: Vec<(Action, f32)> = observation
            .legal_actions()
            .iter()
            .filter_map(|&action| encoding::action_index(action, observation.decision()).map(|index| (action, evaluation.logits[index])))
            .collect();
        let top = legal.iter().map(|&(_, logit)| logit).fold(f32::NEG_INFINITY, f32::max);
        let total: f32 = legal.iter().map(|&(_, logit)| (logit - top).exp()).sum();
        let mut actions: Vec<Choice> = legal
            .into_iter()
            .map(|(action, logit)| Choice {
                action,
                probability: (logit - top).exp() / total,
            })
            .collect();
        actions.sort_by(|a, b| b.probability.total_cmp(&a.probability));
        #[derive(Serialize)]
        struct Evaluation {
            settlement: f32,
            actions: Vec<Choice>,
        }
        encode(&Evaluation {
            settlement: evaluation.settlement,
            actions,
        })
    }
}

#[derive(Serialize)]
struct Choice {
    action: Action,
    probability: f32,
}

#[wasm_bindgen]
impl Bot {
    /// A bot that plays the network's most likely legal action.
    #[wasm_bindgen(js_name = fromNetwork)]
    pub fn from_network(network: &Network) -> Bot {
        Bot {
            policy: Box::new(NeuralBot::new("network", network.net.clone())),
        }
    }
}

/// The observation encoding networks must have been trained with.
#[wasm_bindgen(js_name = encodingVersion)]
pub fn encoding_version() -> u32 {
    encoding::VERSION
}
