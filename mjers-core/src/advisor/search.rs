//! A network's move, checked by playouts where the network is unsure.
//!
//! When the favourite has at least `confident` probability it is played at
//! once. Otherwise the network's `candidates` most likely actions are played
//! out on the same sampled deals, with the network in this seat and `field`
//! in the others, and another candidate replaces the favourite only when it
//! leads by more than `margin` standard errors of the paired difference.
//! With `bootstrap`, playouts stop at the seat's next choice and the network's
//! value head scores it (`montecarlo::estimate_values`).

use serde::{
    Deserialize,
    Serialize,
};

use crate::advisor::montecarlo::{
    Estimate,
    estimate,
    estimate_values,
};
use crate::bot::encoding;
use crate::bot::neural::NeuralNetwork;
use crate::bot::policy::Policy;
use crate::bot::spec::BotSpec;
use crate::game::action::Action;
use crate::game::observation::Observation;
use crate::game::rng::derive_seed;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchConfig {
    /// Playouts per candidate.
    pub samples: u32,
    /// How many of the network's most likely actions to play out.
    pub candidates: usize,
    /// Favourites at least this probable are played without playouts.
    pub confident: f32,
    /// Standard errors by which a candidate must beat the favourite.
    pub margin: f64,
    pub seed: u64,
    /// Policy of the other seats during playouts.
    pub field: BotSpec,
    /// Score playouts by the network's value at the seat's next choice
    /// instead of playing to the end of the hand.
    #[serde(default)]
    pub bootstrap: bool,
}

pub struct SearchBot {
    name: String,
    network: NeuralNetwork,
    config: SearchConfig,
    seed: u64,
    decisions: u64,
}

impl SearchBot {
    pub fn new(
        name: impl Into<String>,
        network: NeuralNetwork,
        config: SearchConfig,
        seed: u64,
    ) -> SearchBot {
        SearchBot {
            name: name.into(),
            network,
            seed: derive_seed(config.seed, seed),
            config,
            decisions: 0,
        }
    }
}

/// The network's legal choices, most likely first, and playout estimates for its
/// leading candidates (favourite first) when the decision was searched.
#[derive(Debug, Clone, PartialEq)]
pub struct Search {
    pub choices: Vec<(Action, f32)>,
    pub estimates: Vec<Estimate>,
}

impl Search {
    /// The favourite, unless a candidate leads it by more than `margin`
    /// standard errors of the paired difference.
    pub fn best(
        &self,
        margin: f64,
    ) -> Option<Action> {
        let favourite = self.choices.first()?.0;
        let lead = self
            .estimates
            .iter()
            .filter(|e| e.difference > 0.0 && e.difference > margin * e.difference_stderr)
            .max_by(|a, b| a.difference.total_cmp(&b.difference));
        Some(lead.map_or(favourite, |e| e.action))
    }
}

/// Evaluates `network` at `observation` and plays out its candidates unless
/// the favourite is a win or at least `config.confident` likely.
pub fn search(
    network: &NeuralNetwork,
    observation: &Observation<'_>,
    config: &SearchConfig,
    seed: u64,
) -> Search {
    let choices = network.net().evaluate(&encoding::encode(observation)).choices(observation);
    let wins = observation.legal_actions().iter().any(|action| matches!(action, Action::Win | Action::SelfDrawWin));
    let sure = choices.first().is_none_or(|&(_, probability)| probability >= config.confident);
    if wins || sure || choices.len() < 2 || config.candidates < 2 {
        return Search { choices, estimates: Vec::new() };
    }
    let candidates: Vec<Action> = choices.iter().take(config.candidates).map(|&(action, _)| action).collect();
    let estimates = if config.bootstrap {
        estimate_values(observation, &candidates, config.samples, seed, network.net(), &config.field)
    } else {
        let own = BotSpec::Neural {
            name: network.checkpoint().into(),
            network: network.clone(),
        };
        estimate(observation, &candidates, config.samples, seed, &own, &config.field)
    };
    Search { choices, estimates }
}

impl Policy for SearchBot {
    fn name(&self) -> &str {
        &self.name
    }

    fn act(
        &mut self,
        observation: &Observation<'_>,
        legal: &[Action],
    ) -> Action {
        let result = search(&self.network, observation, &self.config, derive_seed(self.seed, self.decisions));
        // Only searched decisions use up a seed.
        self.decisions += u64::from(!result.estimates.is_empty());
        result.best(self.config.margin).or_else(|| legal.first().copied()).unwrap_or(Action::Pass)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::bot::neural::tests::random_weights;
    use crate::bot::neural::{
        NeuralBot,
        ResidualNet,
    };
    use crate::bot::policy::play_out;
    use crate::game::Game;
    use crate::rules::Ruleset;
    use crate::rules::seat::Seat;

    fn network() -> NeuralNetwork {
        NeuralNetwork::new("random", ResidualNet::new(&random_weights(4, 1, 11)).unwrap())
    }

    fn config(
        margin: f64,
        bootstrap: bool,
    ) -> SearchConfig {
        SearchConfig {
            samples: 4,
            candidates: 2,
            confident: 0.99,
            margin,
            seed: 3,
            field: BotSpec::preset("efficiency").unwrap(),
            bootstrap,
        }
    }

    fn play(
        seed: u64,
        first: Box<dyn Policy>,
    ) -> [i32; 4] {
        let mut game = Game::new(Arc::new(Ruleset::family()), seed, Seat::ALL[0]).unwrap();
        let mut first = Some(first);
        let mut table: [Box<dyn Policy>; 4] = std::array::from_fn(|i| first.take().unwrap_or_else(|| BotSpec::preset("efficiency").unwrap().build(i as u64)));
        play_out(&mut game, &mut table).unwrap();
        game.settlement().unwrap().total()
    }

    #[test]
    fn playouts_that_never_convince_leave_the_network_s_play() {
        for seed in 0..3 {
            let searched = play(seed, Box::new(SearchBot::new("search", network(), config(f64::INFINITY, false), seed)));
            assert_eq!(searched, play(seed, Box::new(NeuralBot::new("net", network().net().clone()))));
        }
    }

    #[test]
    fn searching_plays_deterministic_legal_hands() {
        for bootstrap in [false, true] {
            let searched = |seed| play(seed, Box::new(SearchBot::new("search", network(), config(0.0, bootstrap), seed)));
            let mut changed = false;
            for seed in 0..4 {
                assert_eq!(searched(seed), searched(seed));
                changed |= searched(seed) != play(seed, Box::new(NeuralBot::new("net", network().net().clone())));
            }
            assert!(changed, "a zero margin follows any playout lead (bootstrap {bootstrap})");
        }
    }
}
