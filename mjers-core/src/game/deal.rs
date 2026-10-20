//! Building a game: dealing from a wall, or resuming a mid-hand position.

use std::sync::Arc;

use super::{
    DEALT_TILES,
    Discard,
    Game,
};
use crate::game::action::{
    GameError,
    Offer,
    Phase,
};
use crate::game::trace::Event;
use crate::game::wall::{
    Wall,
    WallTile,
};
use crate::hand::Hand;
use crate::rules::Ruleset;
use crate::rules::seat::{
    SEATS,
    Seat,
    SeatSet,
};
use crate::tile::Tile;
use crate::tile::counts::TileCounts;

/// Tiles each seat takes per round of the deal: four, four, four, then one.
const DEAL_ROUNDS: [usize; 4] = [4, 4, 4, 1];
/// Room reserved up front so a typical hand never regrows its logs.
const EXPECTED_EVENTS: usize = 192;
const EXPECTED_DISCARDS: usize = 96;

impl Game {
    /// Deals from a wall in a known order.
    ///
    /// Each seat from the dealer takes four tiles three times, then one more;
    /// flowers are then replaced seat by seat, and the dealer draws to start.
    pub fn from_wall(
        rules: Arc<Ruleset>,
        mut wall: Wall,
        dealer: Seat,
    ) -> Result<Game, GameError> {
        rules.validate()?;
        let initial_wall = Arc::new(wall.clone());
        let order: [Seat; SEATS] = std::array::from_fn(|k| if k == 0 { dealer } else { Seat::ALL[(dealer.index() + k) % SEATS] });
        let mut dealt_by_seat = [[WallTile::Bonus(crate::tile::Bonus::Flower(1)); DEALT_TILES]; SEATS];
        let mut dealt_count = [0usize; SEATS];
        for take in DEAL_ROUNDS {
            for seat in order {
                for _ in 0..take {
                    let tile = wall.draw().ok_or(GameError::InvalidPosition("the wall is too short to deal"))?;
                    dealt_by_seat[seat.index()][dealt_count[seat.index()]] = tile;
                    dealt_count[seat.index()] += 1;
                }
            }
        }

        let mut game = Game::from_parts(rules, wall, [Hand::new(TileCounts::new()); SEATS], dealer, Vec::new(), TileCounts::new());
        game.initial_wall = initial_wall;
        for seat in order {
            for &tile in &dealt_by_seat[seat.index()] {
                if game.receive(seat, tile).is_none() {
                    return Ok(game);
                }
            }
        }
        for seat in order {
            let tiles = *game.hand(seat).concealed();
            game.events.push(Event::Deal { seat, tiles });
        }
        game.start_turn(dealer, false);
        Ok(game)
    }

    /// A game around existing state, waiting at the dealer's turn until told otherwise.
    pub(super) fn from_parts(
        rules: Arc<Ruleset>,
        wall: Wall,
        hand_by_seat: [Hand; SEATS],
        dealer: Seat,
        mut discards: Vec<Discard>,
        public: TileCounts,
    ) -> Game {
        discards.reserve(EXPECTED_DISCARDS);
        Game {
            rules,
            initial_wall: Arc::new(wall.clone()),
            wall,
            hand_by_seat,
            dealer,
            phase: Phase::Turn { seat: dealer, after_claim: false },
            discards,
            public,
            last_draw: None,
            asked: SeatSet::EMPTY,
            decision_by_seat: [None; SEATS],
            seed: None,
            steps: Vec::with_capacity(EXPECTED_EVENTS),
            events: Vec::with_capacity(EXPECTED_EVENTS),
        }
    }

    pub(super) fn set_turn(
        &mut self,
        seat: Seat,
        after_claim: bool,
        last_draw: Option<Tile>,
    ) {
        self.phase = Phase::Turn { seat, after_claim };
        self.last_draw = last_draw;
    }

    pub(super) fn reopen_claims(
        &mut self,
        tile: Tile,
        from: Seat,
        offer: Offer,
    ) -> Result<(), GameError> {
        self.open_claims(tile, from, offer)
    }
}
