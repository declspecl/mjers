//! One hand of mahjong, from the deal to settlement.
//!
//! `Game` holds the state; its operations live in submodules by stage of play:
//! dealing, turns, the claim window after a discard, and ending the hand.

pub mod action;
pub mod matches;
pub mod observation;
pub mod position;
pub mod rng;
#[cfg(test)]
mod testing;
pub mod trace;
pub mod wall;

mod claims;
mod deal;
mod ending;
#[cfg(test)]
mod tests;
mod turn;

use std::sync::Arc;

use serde::{
    Deserialize,
    Serialize,
};

use crate::game::action::{
    Action,
    Actions,
    GameError,
    Phase,
};
use crate::game::rng::Rng;
use crate::game::trace::{
    Event,
    Step,
};
use crate::game::wall::Wall;
use crate::hand::Hand;
use crate::rules::Ruleset;
use crate::rules::seat::{
    SEATS,
    Seat,
    SeatSet,
};
use crate::rules::settlement::{
    HandResult,
    Settlement,
};
use crate::tile::Tile;
use crate::tile::counts::TileCounts;

pub const DEALT_TILES: usize = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Discard {
    pub seat: Seat,
    pub tile: Tile,
    /// Taken by another player for a meld or a win.
    pub claimed: bool,
}

#[derive(Debug, Clone)]
pub struct Game {
    rules: Arc<Ruleset>,
    wall: Wall,
    hand_by_seat: [Hand; SEATS],
    dealer: Seat,
    phase: Phase,
    discards: Vec<Discard>,
    /// Tiles every player can see: discards and exposed melds.
    public: TileCounts,
    /// The tile drawn this turn, if the turn began with a draw.
    last_draw: Option<Tile>,
    /// In a claim window: seats with an option, and what each has chosen.
    asked: SeatSet,
    decision_by_seat: [Option<Action>; SEATS],
    seed: Option<u64>,
    initial_wall: Arc<Wall>,
    steps: Vec<Step>,
    events: Vec<Event>,
}

impl Game {
    /// Shuffles a wall from `seed` and deals.
    pub fn new(
        rules: Arc<Ruleset>,
        seed: u64,
        dealer: Seat,
    ) -> Result<Game, GameError> {
        let wall = Wall::shuffled(&mut Rng::seed(seed), rules.reserved_tiles());
        let mut game = Game::from_wall(rules, wall, dealer)?;
        game.seed = Some(seed);
        Ok(game)
    }

    /// Seats that must decide before play continues.
    pub fn to_act(&self) -> SeatSet {
        match self.phase {
            Phase::Turn { seat, .. } => SeatSet::only(seat),
            Phase::Claim { .. } => self.undecided_claimants(),
            Phase::Ended(_) => SeatSet::EMPTY,
        }
    }

    pub fn legal_actions(
        &self,
        seat: Seat,
    ) -> Actions {
        match self.phase {
            Phase::Turn { seat: current, after_claim } if current == seat => self.turn_actions(seat, after_claim),
            Phase::Claim { .. } => self.claim_actions(seat),
            _ => Actions::new(),
        }
    }

    pub fn apply(
        &mut self,
        seat: Seat,
        action: Action,
    ) -> Result<(), GameError> {
        if self.is_over() {
            return Err(GameError::HandOver);
        }
        if !self.to_act().contains(seat) {
            return Err(GameError::NotAsked(seat));
        }
        if !self.legal_actions(seat).contains(&action) {
            return Err(GameError::Illegal { seat, action });
        }

        self.steps.push(Step { seat, action });
        match action {
            Action::Discard(_) | Action::SelfDrawWin | Action::ConcealedKong(_) | Action::UpgradeKong(_) => self.take_turn(seat, action),
            Action::Pass | Action::Chow(_) | Action::Pong | Action::Kong | Action::Win => self.decide(seat, action),
        }
    }

    pub fn rules(&self) -> &Ruleset {
        &self.rules
    }

    pub fn dealer(&self) -> Seat {
        self.dealer
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn is_over(&self) -> bool {
        matches!(self.phase, Phase::Ended(_))
    }

    pub fn result(&self) -> Option<&HandResult> {
        match &self.phase {
            Phase::Ended(result) => Some(result),
            _ => None,
        }
    }

    /// Payments once the hand is over.
    pub fn settlement(&self) -> Option<Settlement> {
        self.result().map(|result| self.rules.payouts.settle(result))
    }

    pub fn hand(
        &self,
        seat: Seat,
    ) -> &Hand {
        &self.hand_by_seat[seat.index()]
    }

    pub fn discards(&self) -> &[Discard] {
        &self.discards
    }

    /// Tiles every player can see: discards and exposed melds.
    pub fn public_tiles(&self) -> &TileCounts {
        &self.public
    }

    pub fn last_draw(&self) -> Option<Tile> {
        self.last_draw
    }

    pub fn wall(&self) -> &Wall {
        &self.wall
    }

    pub fn seed(&self) -> Option<u64> {
        self.seed
    }

    /// The wall as it was before the deal.
    pub fn initial_wall(&self) -> &Wall {
        &self.initial_wall
    }

    /// Every applied action, in order.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }
}
