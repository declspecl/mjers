//! Bot descriptions that load from JSON and build fresh policies.

use serde::{
    Deserialize,
    Serialize,
};

use crate::advisor::AdviceConfig;
use crate::advisor::policy::AdvisorBot;
use crate::bot::heuristic::HeuristicBot;
use crate::bot::neural::{
    NeuralBot,
    NeuralNetwork,
};
use crate::bot::params::Params;
use crate::bot::policy::{
    HybridBot,
    Policy,
    RandomBot,
};
use crate::game::rng::derive_seed;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BotSpec {
    Random {
        name: String,
    },
    Heuristic {
        name: String,
        #[serde(default)]
        params: Params,
    },
    /// Monte Carlo advice at every decision; far slower than the others.
    Advisor {
        name: String,
        advice: Box<AdviceConfig>,
    },
    /// A trained `suit-resnet-v2` network playing its best legal action. Its
    /// JSON names the checkpoint; only callers that read files can load it.
    Neural {
        name: String,
        network: NeuralNetwork,
    },
    /// Turn decisions from `turns`, claim responses from `claims`.
    Hybrid {
        name: String,
        turns: Box<BotSpec>,
        claims: Box<BotSpec>,
    },
}

impl BotSpec {
    pub const PRESETS: [&str; 3] = ["random", "efficiency", "defensive"];

    pub fn preset(name: &str) -> Option<BotSpec> {
        match name {
            "random" => Some(BotSpec::Random { name: name.into() }),
            "efficiency" => Some(BotSpec::Heuristic {
                name: name.into(),
                params: Params::efficiency(),
            }),
            "defensive" => Some(BotSpec::Heuristic {
                name: name.into(),
                params: Params::defensive(),
            }),
            _ => None,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            BotSpec::Random { name } | BotSpec::Heuristic { name, .. } | BotSpec::Advisor { name, .. } | BotSpec::Neural { name, .. } | BotSpec::Hybrid { name, .. } => name,
        }
    }

    /// A new policy; `seed` feeds any randomness it uses.
    pub fn build(
        &self,
        seed: u64,
    ) -> Box<dyn Policy> {
        match self {
            BotSpec::Random { .. } => Box::new(RandomBot::new(seed)),
            BotSpec::Heuristic { name, params } => Box::new(HeuristicBot::new(name.clone(), params.clone())),
            BotSpec::Advisor { name, advice } => Box::new(AdvisorBot::new(name.clone(), (**advice).clone(), seed)),
            BotSpec::Neural { name, network } => Box::new(NeuralBot::new(name.clone(), network.net().clone())),
            BotSpec::Hybrid { name, turns, claims } => Box::new(HybridBot::new(name.clone(), turns.build(derive_seed(seed, 0)), claims.build(derive_seed(seed, 1)))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_round_trip_through_json() {
        for name in BotSpec::PRESETS {
            let spec = BotSpec::preset(name).unwrap();
            let json = serde_json::to_string(&spec).unwrap();
            assert_eq!(serde_json::from_str::<BotSpec>(&json).unwrap(), spec);
            assert_eq!(spec.build(0).name(), name);
        }
    }

    #[test]
    fn hybrids_round_trip_and_name_themselves() {
        let json = r#"{"kind": "hybrid", "name": "mixed", "turns": {"kind": "heuristic", "name": "efficiency"}, "claims": {"kind": "random", "name": "random"}}"#;
        let spec: BotSpec = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::from_str::<BotSpec>(&serde_json::to_string(&spec).unwrap()).unwrap(), spec);
        assert_eq!(spec.build(3).name(), "mixed");
    }

    #[test]
    fn params_fill_from_defaults() {
        let spec: BotSpec = serde_json::from_str(r#"{"kind": "heuristic", "name": "no-chow", "params": {"chow": "never"}}"#).unwrap();
        let BotSpec::Heuristic { params, .. } = spec else { panic!() };
        assert_eq!(
            params,
            Params {
                chow: crate::bot::params::ClaimRule::Never,
                ..Params::efficiency()
            }
        );
    }

    #[test]
    fn checked_in_strategies_parse() {
        let dir = format!("{}/../strategies", env!("CARGO_MANIFEST_DIR"));
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            let spec: BotSpec = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(Some(spec.name()), path.file_stem().and_then(|s| s.to_str()), "name matches file");
            count += 1;
        }
        assert!(count >= 5);
    }
}
