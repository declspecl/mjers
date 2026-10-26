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

    /// Leading observation planes the network reads; see `encodingPlanes`.
    #[wasm_bindgen(getter)]
    pub fn planes(&self) -> u32 {
        self.net.planes() as u32
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
        let actions = evaluation.choices(&observation).into_iter().map(|(action, probability)| Choice { action, probability }).collect();
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

/// The current observation encoding.
#[wasm_bindgen(js_name = encodingVersion)]
pub fn encoding_version() -> u32 {
    encoding::VERSION
}

/// How many planes a network trained under `version` reads, if supported.
#[wasm_bindgen(js_name = encodingPlanes)]
pub fn encoding_planes(version: u32) -> Option<u32> {
    encoding::planes(version).map(|planes| planes as u32)
}
