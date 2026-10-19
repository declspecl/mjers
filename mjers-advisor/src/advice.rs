//! The advisor: instant ranking, refined by playouts when asked.

use mjers_bot::BotSpec;
use mjers_game::{
    Action,
    Observation,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::heuristic::{
    Assessment,
    rank,
};
use crate::montecarlo::{
    Estimate,
    estimate,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdviceConfig {
    /// Playouts per candidate; 0 skips estimation.
    pub samples: u32,
    /// How many of the best-ranked actions to estimate.
    pub candidates: usize,
    pub seed: u64,
    /// Policy every seat follows during playouts.
    pub rollout: BotSpec,
}

impl Default for AdviceConfig {
    fn default() -> Self {
        AdviceConfig {
            samples: 200,
            candidates: 6,
            seed: 0,
            rollout: BotSpec::preset("efficiency").expect("preset"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Advice {
    /// Every legal action, best first by the instant heuristic.
    pub ranked: Vec<Assessment>,
    /// Playout estimates for the top candidates, highest EV first.
    pub estimates: Vec<Estimate>,
}

impl Advice {
    /// The recommended action: best estimate if any, else best ranked.
    pub fn best(&self) -> Action {
        self.estimates.first().map_or(self.ranked[0].action, |e| e.action)
    }

    pub fn assessment(
        &self,
        action: Action,
    ) -> Option<&Assessment> {
        self.ranked.iter().find(|a| a.action == action)
    }
}

pub fn advise(
    obs: &Observation<'_>,
    config: &AdviceConfig,
) -> Advice {
    let ranked = rank(obs);
    let candidates: Vec<Action> = ranked.iter().take(config.candidates.max(1)).map(|a| a.action).collect();
    let mut estimates = if config.samples > 0 && ranked.len() > 1 {
        estimate(obs, &candidates, config.samples, config.seed, &config.rollout)
    } else {
        Vec::new()
    };
    estimates.sort_by(|a, b| b.ev.total_cmp(&a.ev));
    Advice { ranked, estimates }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mjers_game::{
        Game,
        Ruleset,
        Seat,
    };

    use super::*;

    #[test]
    fn advice_covers_every_action_and_estimates_candidates() {
        let game = Game::new(Arc::new(Ruleset::family()), 3, Seat::ALL[0]);
        let obs = game.observe(Seat::ALL[0]);
        let config = AdviceConfig {
            samples: 30,
            candidates: 3,
            ..AdviceConfig::default()
        };
        let advice = advise(&obs, &config);
        assert_eq!(advice.ranked.len(), obs.legal_actions().len());
        assert_eq!(advice.estimates.len(), 3);
        assert!(advice.estimates.windows(2).all(|w| w[0].ev >= w[1].ev));
        assert!(obs.legal_actions().contains(&advice.best()));
    }

    #[test]
    fn zero_samples_uses_the_ranking() {
        let game = Game::new(Arc::new(Ruleset::family()), 3, Seat::ALL[0]);
        let advice = advise(
            &game.observe(Seat::ALL[0]),
            &AdviceConfig {
                samples: 0,
                ..AdviceConfig::default()
            },
        );
        assert!(advice.estimates.is_empty());
        assert_eq!(advice.best(), advice.ranked[0].action);
    }
}
