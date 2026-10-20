//! Browser bindings. The core owns all game rules and decision logic.

mod analysis;
mod observation;
mod types;

use std::sync::Arc;

use mjers_core::advisor::{
    AdviceConfig,
    advise,
};
use mjers_core::bot::policy::Policy;
use mjers_core::bot::spec::BotSpec;
use mjers_core::game::Game as EngineGame;
use mjers_core::game::action::Action;
use mjers_core::rules::Ruleset;
use mjers_core::rules::seat::Seat;
use serde::de::DeserializeOwned;
use serde::{
    Deserialize,
    Serialize,
};
use wasm_bindgen::prelude::*;

use crate::types::{
    ActionValue,
    ActionsValue,
    AdviceOptionsValue,
    AdviceValue,
    ObservationValue,
    ResultValue,
    RulesetValue,
    SeatsValue,
    SettlementValue,
};

/// One seeded hand, from the deal through settlement. Call `free()` when done.
#[wasm_bindgen]
pub struct Game {
    inner: EngineGame,
}

#[wasm_bindgen]
impl Game {
    /// `seed` is an unsigned 64-bit bigint. Omitted rules use the family preset.
    #[wasm_bindgen(constructor)]
    pub fn new(
        seed: u64,
        dealer: usize,
        rules: Option<RulesetValue>,
    ) -> Result<Game, JsError> {
        let rules: Ruleset = rules.map(decode).transpose()?.unwrap_or_default();
        let inner = EngineGame::new(Arc::new(rules), seed, seat(dealer)?).map_err(error)?;
        Ok(Game { inner })
    }

    /// All seats whose decisions are needed before play continues.
    #[wasm_bindgen(js_name = toAct)]
    pub fn to_act(&self) -> Result<SeatsValue, JsError> {
        encode(&self.inner.to_act().iter().collect::<Vec<_>>())
    }

    #[wasm_bindgen(js_name = legalActions)]
    pub fn legal_actions(
        &self,
        player: usize,
    ) -> Result<ActionsValue, JsError> {
        encode(&self.inner.legal_actions(seat(player)?).to_vec())
    }

    /// Public information and only the requested player's concealed tiles.
    pub fn observe(
        &self,
        player: usize,
    ) -> Result<ObservationValue, JsError> {
        encode(&observation::Snapshot::new(&self.inner, seat(player)?))
    }

    /// Apply one legal action. Invalid input throws a JavaScript Error.
    pub fn apply(
        &mut self,
        player: usize,
        action: ActionValue,
    ) -> Result<(), JsError> {
        self.inner.apply(seat(player)?, decode(action)?).map_err(error)
    }

    #[wasm_bindgen(js_name = isOver)]
    pub fn is_over(&self) -> bool {
        self.inner.is_over()
    }

    pub fn result(&self) -> Result<ResultValue, JsError> {
        encode(&self.inner.result())
    }

    pub fn settlement(&self) -> Result<SettlementValue, JsError> {
        #[derive(Serialize)]
        struct Settlement {
            win: [i32; 4],
            kongs: [i32; 4],
            total: [i32; 4],
        }
        encode(&self.inner.settlement().map(|s| Settlement {
            win: s.win,
            kongs: s.kongs,
            total: s.total(),
        }))
    }

    /// Rank actions and optionally refine with seeded Monte Carlo playouts.
    /// Synchronous: use a Web Worker for large sample counts.
    pub fn advise(
        &self,
        player: usize,
        options: Option<AdviceOptionsValue>,
    ) -> Result<AdviceValue, JsError> {
        let player = self.acting_seat(player)?;
        let options: AdviceOptions = options.map(decode).transpose()?.unwrap_or_default();
        let config = AdviceConfig {
            samples: options.samples,
            candidates: options.candidates,
            seed: options.seed,
            rollout: strategy(&options.rollout)?,
        };
        let advice = advise(&self.inner.observe(player), &config);
        #[derive(Serialize)]
        struct Advice {
            recommended: Action,
            ranked: Vec<mjers_core::advisor::heuristic::Assessment>,
            estimates: Vec<mjers_core::advisor::montecarlo::Estimate>,
        }
        encode(&Advice {
            recommended: advice.best(),
            ranked: advice.ranked,
            estimates: advice.estimates,
        })
    }
}

impl Game {
    fn acting_seat(
        &self,
        player: usize,
    ) -> Result<Seat, JsError> {
        let player = seat(player)?;
        if !self.inner.to_act().contains(player) {
            return Err(JsError::new("this seat has no decision to make"));
        }
        Ok(player)
    }
}

/// A persistent policy. Give each seat its own bot to retain its random stream.
#[wasm_bindgen]
pub struct Bot {
    policy: Box<dyn Policy>,
}

#[wasm_bindgen]
impl Bot {
    #[wasm_bindgen(constructor)]
    pub fn new(
        preset: &str,
        seed: u64,
    ) -> Result<Bot, JsError> {
        Ok(Bot {
            policy: strategy(preset)?.build(seed),
        })
    }

    /// Choose without applying. The policy sees only this seat's observation.
    pub fn choose(
        &mut self,
        game: &Game,
        player: usize,
    ) -> Result<ActionValue, JsError> {
        let player = game.acting_seat(player)?;
        let obs = game.inner.observe(player);
        encode(&self.policy.act(&obs, &obs.legal_actions()))
    }
}

/// Resolve a built-in ruleset: "family" or "discarder-pays".
#[wasm_bindgen]
pub fn ruleset(preset: &str) -> Result<RulesetValue, JsError> {
    encode(&Ruleset::preset(preset).ok_or_else(|| JsError::new("unknown ruleset preset"))?)
}

#[derive(Deserialize)]
#[serde(default)]
struct AdviceOptions {
    samples: u32,
    candidates: usize,
    seed: u64,
    rollout: String,
}

impl Default for AdviceOptions {
    fn default() -> Self {
        let config = AdviceConfig::default();
        Self {
            samples: config.samples,
            candidates: config.candidates,
            seed: config.seed,
            rollout: config.rollout.name().into(),
        }
    }
}

fn strategy(preset: &str) -> Result<BotSpec, JsError> {
    BotSpec::preset(preset).ok_or_else(|| JsError::new("unknown bot preset: use efficiency, defensive, or random"))
}

fn seat(index: usize) -> Result<Seat, JsError> {
    Seat::new(index).ok_or_else(|| JsError::new("seat must be 0-3"))
}

fn decode<T: DeserializeOwned>(value: impl Into<JsValue>) -> Result<T, JsError> {
    serde_wasm_bindgen::from_value(value.into()).map_err(error)
}

fn encode<T: Serialize, J: JsCast>(value: &T) -> Result<J, JsError> {
    value.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).map(JsCast::unchecked_into).map_err(error)
}

fn error(error: impl std::fmt::Display) -> JsError {
    JsError::new(&error.to_string())
}
