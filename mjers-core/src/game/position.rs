//! Starting a game from the middle of a hand, for what-if analysis.

use std::sync::Arc;

use crate::game::action::{
    GameError,
    Offer,
    Phase,
};
use crate::game::wall::{
    Wall,
    WallTile,
};
use crate::game::{
    Discard,
    Game,
};
use crate::hand::Hand;
use crate::rules::Ruleset;
use crate::rules::seat::{
    SEATS,
    Seat,
};
use crate::tile::Tile;
use crate::tile::counts::TileCounts;

/// Where play resumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumePoint {
    Turn { seat: Seat, after_claim: bool, last_draw: Option<Tile> },
    Claim { tile: Tile, from: Seat, offer: Offer },
}

/// A complete mid-hand state, hidden information included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub hands: [Hand; SEATS],
    /// Undrawn tiles in draw order (replacements come from the end).
    pub wall: Vec<WallTile>,
    pub dealer: Seat,
    pub discards: Vec<Discard>,
    pub public: TileCounts,
    pub point: ResumePoint,
}

impl Game {
    /// The full state at the current decision, or `None` once the hand is over.
    pub fn position(&self) -> Option<Position> {
        let point = match *self.phase() {
            Phase::Turn { seat, after_claim } => ResumePoint::Turn {
                seat,
                after_claim,
                last_draw: self.last_draw(),
            },
            Phase::Claim { tile, from, offer } => ResumePoint::Claim { tile, from, offer },
            Phase::Ended(_) => return None,
        };
        Some(Position {
            hands: Seat::ALL.map(|s| *self.hand(s)),
            wall: self.wall().undrawn().to_vec(),
            dealer: self.dealer(),
            discards: self.discards().to_vec(),
            public: *self.public_tiles(),
            point,
        })
    }

    /// Continues play from `position`. A resumed game has no deal, so its
    /// trace does not replay; it is for simulation and analysis.
    pub fn resume(
        rules: Arc<Ruleset>,
        position: Position,
    ) -> Result<Game, GameError> {
        rules.validate()?;
        let wall = Wall::from_remaining(&position.wall, rules.reserved_tiles()).ok_or(GameError::InvalidPosition("more than a full wall of tiles"))?;
        let mut game = Game::from_parts(rules, wall, position.hands, position.dealer, position.discards, position.public);
        match position.point {
            ResumePoint::Turn { seat, after_claim, last_draw } => game.set_turn(seat, after_claim, last_draw),
            ResumePoint::Claim { tile, from, offer } => game.reopen_claims(tile, from, offer)?,
        }
        Ok(game)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::action::Action;
    use crate::game::testing::family;

    /// Discards the drawn tile (or first tile) and passes every claim.
    fn step(game: &mut Game) {
        let seat = game.to_act().first().unwrap();
        let legal = game.legal_actions(seat);
        let action = if legal.contains(&Action::Pong) {
            Action::Pong
        } else if legal.contains(&Action::Pass) {
            Action::Pass
        } else {
            Action::Discard(game.last_draw().unwrap_or_else(|| game.hand(seat).concealed().kinds().next().unwrap()))
        };
        game.apply(seat, action).unwrap();
    }

    #[test]
    fn resuming_a_position_continues_identically() {
        for seed in 0..40 {
            let mut original = Game::new(family(), seed, Seat::ALL[0]).unwrap();
            for _ in 0..(seed % 30) {
                if original.is_over() {
                    break;
                }
                step(&mut original);
            }
            if original.is_over() {
                continue;
            }
            let mut resumed = Game::resume(family(), original.position().unwrap()).unwrap();
            assert_eq!(resumed.to_act(), original.to_act(), "seed {seed}");
            for seat in original.to_act() {
                assert_eq!(resumed.legal_actions(seat), original.legal_actions(seat));
            }
            while !original.is_over() {
                step(&mut original);
                step(&mut resumed);
            }
            assert_eq!(resumed.settlement(), original.settlement(), "seed {seed}");
        }
    }
}
