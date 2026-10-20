//! The advisor: instant ranking, refined by playouts when asked.

use mjers_bot::params::Params;
use mjers_bot::spec::BotSpec;
use mjers_game::action::Action;
use mjers_game::observation::Observation;
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
            rollout: BotSpec::Heuristic {
                name: "efficiency".into(),
                params: Params::efficiency(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Advice {
    /// Every legal action, best first by the instant heuristic.
    pub ranked: Vec<Assessment>,
    /// Playout estimates for the top candidates, highest EV first. Each
    /// carries its paired difference from the top-ranked action.
    pub estimates: Vec<Estimate>,
}

impl Advice {
    /// The top-ranked action, unless playouts show another beating it by more
    /// than one standard error of the paired difference.
    pub fn best(&self) -> Action {
        self.estimates
            .iter()
            .filter(|e| e.difference - e.difference_stderr > 0.0)
            .max_by(|a, b| a.difference.total_cmp(&b.difference))
            .map_or(self.ranked[0].action, |e| e.action)
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

    use mjers_game::game::Game;
    use mjers_rules::ruleset::Ruleset;
    use mjers_rules::seat::Seat;

    use super::*;

    #[test]
    fn advice_covers_every_action_and_estimates_candidates() {
        let game = Game::new(Arc::new(Ruleset::family()), 3, Seat::ALL[0]).unwrap();
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
        let game = Game::new(Arc::new(Ruleset::family()), 3, Seat::ALL[0]).unwrap();
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

#[cfg(test)]
mod best_tests {
    use super::*;
    use crate::montecarlo::Estimate;

    fn estimate(
        action: Action,
        difference: f64,
        difference_stderr: f64,
    ) -> Estimate {
        Estimate {
            action,
            samples: 100,
            ev: difference,
            ev_stderr: 1.0,
            win_rate: 0.0,
            deal_in_rate: 0.0,
            difference,
            difference_stderr,
        }
    }

    fn advice(estimates: Vec<Estimate>) -> Advice {
        let tile = |s: &str| s.parse().unwrap();
        let ranked = ["1m", "2m"]
            .iter()
            .map(|t| Assessment {
                action: Action::Discard(tile(t)),
                shanten: 1,
                live_improving: 0,
                danger: 0.0,
                immediate_units: 0,
                reasons: vec![],
            })
            .collect();
        Advice { ranked, estimates }
    }

    #[test]
    fn noise_does_not_override_the_ranking() {
        let second = Action::Discard("2m".parse().unwrap());
        let a = advice(vec![estimate(second, 0.3, 0.4), estimate(Action::Discard("1m".parse().unwrap()), 0.0, 0.0)]);
        assert_eq!(a.best(), Action::Discard("1m".parse().unwrap()));
        let b = advice(vec![estimate(second, 0.9, 0.4)]);
        assert_eq!(b.best(), second);
    }
}
