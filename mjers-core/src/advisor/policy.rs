//! The advisor as a player: instant ranking refined by playouts at every decision.

use crate::advisor::{
    AdviceConfig,
    advise,
};
use crate::bot::policy::Policy;
use crate::game::action::Action;
use crate::game::observation::Observation;
use crate::game::rng::derive_seed;

/// Plays `advise(..).best()`, with fresh playout samples for every decision.
pub struct AdvisorBot {
    name: String,
    config: AdviceConfig,
    seed: u64,
    decisions: u64,
}

impl AdvisorBot {
    pub fn new(
        name: impl Into<String>,
        config: AdviceConfig,
        seed: u64,
    ) -> AdvisorBot {
        AdvisorBot {
            name: name.into(),
            seed: derive_seed(config.seed, seed),
            config,
            decisions: 0,
        }
    }
}

impl Policy for AdvisorBot {
    fn name(&self) -> &str {
        &self.name
    }

    fn act(
        &mut self,
        observation: &Observation<'_>,
        legal: &[Action],
    ) -> Action {
        // Wins end the hand in the seat's favour under every ruleset; no playouts needed.
        for win in [Action::Win, Action::SelfDrawWin] {
            if legal.contains(&win) {
                return win;
            }
        }
        if let [only] = legal {
            return *only;
        }
        let config = AdviceConfig {
            seed: derive_seed(self.seed, self.decisions),
            ..self.config.clone()
        };
        self.decisions += 1;
        let best = advise(observation, &config).best();
        if legal.contains(&best) { best } else { legal.first().copied().unwrap_or(Action::Pass) }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::bot::policy::play_out;
    use crate::bot::spec::BotSpec;
    use crate::game::Game;
    use crate::rules::Ruleset;
    use crate::rules::seat::Seat;

    fn config() -> AdviceConfig {
        AdviceConfig {
            samples: 4,
            candidates: 2,
            seed: 9,
            rollout: BotSpec::preset("efficiency").unwrap(),
        }
    }

    fn play(seed: u64) -> [i32; 4] {
        let mut game = Game::new(Arc::new(Ruleset::family()), seed, Seat::ALL[0]).unwrap();
        let mut table: [Box<dyn Policy>; 4] = std::array::from_fn(|i| {
            if i == 0 {
                Box::new(AdvisorBot::new("advisor", config(), seed)) as Box<dyn Policy>
            } else {
                BotSpec::preset("efficiency").unwrap().build(i as u64)
            }
        });
        play_out(&mut game, &mut table).unwrap();
        game.settlement().unwrap().total()
    }

    #[test]
    fn finishes_hands_deterministically() {
        for seed in 0..3 {
            assert_eq!(play(seed), play(seed));
        }
    }
}
