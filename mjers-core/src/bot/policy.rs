//! The interface every player implements, and a driver that plays a hand.

use crate::game::Game;
use crate::game::action::{
    Action,
    GameError,
};
use crate::game::observation::{
    Decision,
    Observation,
};
use crate::game::rng::Rng;
use crate::rules::seat::Seat;

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

/// Turn decisions (discards and kongs) from one policy, claim responses from another.
/// Comparing hybrids attributes a strategy's edge to one kind of decision.
pub struct HybridBot {
    name: String,
    turns: Box<dyn Policy>,
    claims: Box<dyn Policy>,
}

impl HybridBot {
    pub fn new(
        name: impl Into<String>,
        turns: Box<dyn Policy>,
        claims: Box<dyn Policy>,
    ) -> HybridBot {
        HybridBot { name: name.into(), turns, claims }
    }
}

impl Policy for HybridBot {
    fn name(&self) -> &str {
        &self.name
    }

    fn act(
        &mut self,
        observation: &Observation<'_>,
        legal: &[Action],
    ) -> Action {
        match observation.decision() {
            Decision::Claim { .. } => self.claims.act(observation, legal),
            Decision::Turn { .. } | Decision::None => self.turns.act(observation, legal),
        }
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

    use super::*;
    use crate::rules::Ruleset;

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

    #[test]
    fn hybrids_route_turns_and_claims_to_their_policies() {
        use crate::bot::heuristic::HeuristicBot;
        use crate::bot::params::{
            ClaimRule,
            Params,
        };

        let rules = Arc::new(Ruleset::family());
        let never_claims = || {
            HeuristicBot::new(
                "closed",
                Params {
                    chow: ClaimRule::Never,
                    pong: ClaimRule::Never,
                    ..Params::efficiency()
                },
            )
        };
        let mut changed = false;
        for seed in 0..20 {
            let play = |seat_zero: Box<dyn Policy>| {
                let mut game = Game::new(rules.clone(), seed, Seat::ALL[0]).unwrap();
                let mut table: [Box<dyn Policy>; 4] = std::array::from_fn(|_| Box::new(HeuristicBot::efficiency()) as Box<dyn Policy>);
                table[0] = seat_zero;
                play_out(&mut game, &mut table).unwrap();
                game.steps().to_vec()
            };
            let plain = play(Box::new(HeuristicBot::efficiency()));
            assert_eq!(
                play(Box::new(HybridBot::new("same", Box::new(HeuristicBot::efficiency()), Box::new(HeuristicBot::efficiency())))),
                plain
            );
            // Turns follow the first policy: the closed bot's discards equal efficiency's.
            assert_eq!(play(Box::new(HybridBot::new("turns", Box::new(never_claims()), Box::new(HeuristicBot::efficiency())))), plain);
            changed |= play(Box::new(HybridBot::new("claims", Box::new(HeuristicBot::efficiency()), Box::new(never_claims())))) != plain;
        }
        assert!(changed, "claim responses come from the second policy");
    }
}
