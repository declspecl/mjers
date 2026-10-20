//! The interface every player implements, and a driver that plays a hand.

use mjers_game::action::{
    Action,
    GameError,
};
use mjers_game::game::Game;
use mjers_game::observation::Observation;
use mjers_game::rng::Rng;
use mjers_rules::seat::Seat;

/// Chooses an action from what one seat can see.
///
/// The returned action should be one of `legal`; anything else is rejected
/// by the game as illegal.
pub trait Policy: Send {
    fn name(&self) -> &str;

    fn act(
        &mut self,
        observation: &Observation<'_>,
        legal: &[Action],
    ) -> Action;
}

/// Plays uniformly random legal actions.
pub struct RandomBot {
    rng: Rng,
}

impl RandomBot {
    pub fn new(seed: u64) -> RandomBot {
        RandomBot { rng: Rng::seed(seed) }
    }
}

impl Policy for RandomBot {
    fn name(&self) -> &str {
        "random"
    }

    fn act(
        &mut self,
        _: &Observation<'_>,
        legal: &[Action],
    ) -> Action {
        let pick = self.rng.below(legal.len() as u64) as usize;
        legal.get(pick).copied().unwrap_or(Action::Pass)
    }
}

/// Plays `game` to the end, asking `policies[seat]` for every decision.
/// Stops with the game's error if a policy chooses an illegal action.
pub fn play_out(
    game: &mut Game,
    policies: &mut [Box<dyn Policy>; 4],
) -> Result<(), GameError> {
    while let Some(seat) = game.to_act().first() {
        let action = decide(game, seat, &mut *policies[seat.index()]);
        game.apply(seat, action)?;
    }
    Ok(())
}

/// Asks one policy for its decision.
pub fn decide(
    game: &Game,
    seat: Seat,
    policy: &mut dyn Policy,
) -> Action {
    let legal = game.legal_actions(seat);
    policy.act(&game.observe(seat), &legal)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mjers_rules::ruleset::Ruleset;

    use super::*;

    fn random_table(seed: u64) -> [Box<dyn Policy>; 4] {
        std::array::from_fn(|i| Box::new(RandomBot::new(seed * 4 + i as u64)) as Box<dyn Policy>)
    }

    #[test]
    fn random_bots_finish_hands() {
        let rules = Arc::new(Ruleset::family());
        for seed in 0..200 {
            let mut game = Game::new(rules.clone(), seed, Seat::ALL[0]).unwrap();
            play_out(&mut game, &mut random_table(seed)).unwrap();
            assert!(game.is_over());
        }
    }

    #[test]
    fn same_seeds_same_hand() {
        let rules = Arc::new(Ruleset::family());
        let mut a = Game::new(rules.clone(), 9, Seat::ALL[0]).unwrap();
        let mut b = Game::new(rules, 9, Seat::ALL[0]).unwrap();
        play_out(&mut a, &mut random_table(9)).unwrap();
        play_out(&mut b, &mut random_table(9)).unwrap();
        assert_eq!(a.steps(), b.steps());
    }
}
