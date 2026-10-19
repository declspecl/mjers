//! A session of consecutive hands with a rotating dealer.

use std::sync::Arc;

use mjers_rules::{
    HandResult,
    Ruleset,
    SEATS,
    Seat,
    Settlement,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::action::{
    Action,
    GameError,
};
use crate::game::Game;
use crate::rng::derive_seed;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandRecord {
    pub seed: u64,
    pub dealer: Seat,
    pub result: HandResult,
    pub settlement: Settlement,
}

/// Plays `hands` hands. The deal passes counter-clockwise after every hand,
/// whatever its outcome, and each hand's wall comes from the match seed.
#[derive(Debug, Clone)]
pub struct Match {
    rules: Arc<Ruleset>,
    seed: u64,
    hands: u32,
    game: Game,
    scores: [i64; SEATS],
    history: Vec<HandRecord>,
}

impl Match {
    pub fn new(
        rules: Arc<Ruleset>,
        seed: u64,
        hands: u32,
    ) -> Match {
        assert!(hands > 0, "a match has at least one hand");
        let game = Game::new(rules.clone(), derive_seed(seed, 0), Seat::ALL[0]);
        Match {
            rules,
            seed,
            hands,
            game,
            scores: [0; SEATS],
            history: Vec::new(),
        }
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Applies an action to the current hand, settling it and dealing the next
    /// when it ends.
    pub fn apply(
        &mut self,
        seat: Seat,
        action: Action,
    ) -> Result<(), GameError> {
        self.game.apply(seat, action)?;
        if self.game.is_over() {
            self.settle_hand();
        }
        Ok(())
    }

    fn settle_hand(&mut self) {
        let settlement = self.game.settlement().expect("hand is over");
        for (score, delta) in self.scores.iter_mut().zip(settlement.total()) {
            *score += delta as i64;
        }
        self.history.push(HandRecord {
            seed: self.game.seed().expect("match hands are seeded"),
            dealer: self.game.dealer(),
            result: self.game.result().expect("hand is over").clone(),
            settlement,
        });
        let played = self.history.len() as u32;
        if played < self.hands {
            let dealer = self.game.dealer().next();
            self.game = Game::new(self.rules.clone(), derive_seed(self.seed, played as u64), dealer);
        }
    }

    pub fn is_over(&self) -> bool {
        self.history.len() as u32 == self.hands
    }

    pub fn scores(&self) -> [i64; SEATS] {
        self.scores
    }

    pub fn history(&self) -> &[HandRecord] {
        &self.history
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(
        seed: u64,
        hands: u32,
    ) -> Match {
        let mut m = Match::new(Arc::new(Ruleset::family()), seed, hands);
        while !m.is_over() {
            let seat = m.game().to_act()[0];
            let legal = m.game().legal_actions(seat);
            let action = [Action::Win, Action::SelfDrawWin, Action::Pong]
                .into_iter()
                .find(|a| legal.contains(a))
                .unwrap_or(*legal.last().unwrap());
            m.apply(seat, action).unwrap();
        }
        m
    }

    #[test]
    fn dealer_rotates_every_hand() {
        let m = play(1, 9);
        let dealers: Vec<usize> = m.history().iter().map(|h| h.dealer.index()).collect();
        assert_eq!(dealers, [0, 1, 2, 3, 0, 1, 2, 3, 0]);
    }

    #[test]
    fn scores_accumulate_settlements_and_sum_to_zero() {
        let m = play(2, 8);
        let summed: [i64; SEATS] = std::array::from_fn(|i| m.history().iter().map(|h| h.settlement.total()[i] as i64).sum());
        assert_eq!(m.scores(), summed);
        assert_eq!(m.scores().iter().sum::<i64>(), 0);
    }

    #[test]
    fn matches_are_reproducible() {
        assert_eq!(play(3, 4).history(), play(3, 4).history());
        assert_ne!(play(3, 4).history(), play(4, 4).history());
    }

    #[test]
    fn finished_match_rejects_actions() {
        let mut m = play(5, 1);
        assert_eq!(m.apply(Seat::ALL[0], Action::Pass), Err(GameError::HandOver));
    }
}
