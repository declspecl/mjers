//! One hand of mahjong, from the deal to settlement.

#[cfg(test)]
mod tests;

use std::sync::Arc;

use mjers_hand::Hand;
use mjers_rules::{
    Ruleset,
    SEATS,
    Seat,
};
use mjers_tile::{
    Tile,
    TileCounts,
};

use crate::rng::Rng;
use crate::wall::{
    Wall,
    WallTile,
};

pub const DEALT_TILES: usize = 13;

#[derive(Debug, Clone)]
pub struct Game {
    rules: Arc<Ruleset>,
    wall: Wall,
    hands: [Hand; SEATS],
    dealer: Seat,
    /// Seat whose turn it is.
    turn: Seat,
    /// The tile drawn this turn, if the turn began with a draw.
    last_draw: Option<Tile>,
    exhausted: bool,
}

impl Game {
    /// Shuffles a wall from `seed` and deals.
    pub fn new(
        rules: Arc<Ruleset>,
        seed: u64,
        dealer: Seat,
    ) -> Game {
        let wall = Wall::shuffled(&mut Rng::seed(seed), rules.reserved_tiles());
        Game::from_wall(rules, wall, dealer)
    }

    /// Deals from a wall in a known order.
    ///
    /// Each seat from the dealer takes four tiles three times, then one more;
    /// flowers are then replaced seat by seat, and the dealer draws to start.
    pub fn from_wall(
        rules: Arc<Ruleset>,
        mut wall: Wall,
        dealer: Seat,
    ) -> Game {
        let order: Vec<Seat> = std::iter::once(dealer).chain(dealer.others()).collect();
        let mut dealt: [Vec<WallTile>; SEATS] = Default::default();
        for take in [4, 4, 4, 1] {
            for &seat in &order {
                for _ in 0..take {
                    dealt[seat.index()].push(wall.draw().expect("a full wall covers the deal"));
                }
            }
        }
        let mut game = Game {
            rules,
            wall,
            hands: [Hand::new(TileCounts::new()); SEATS],
            dealer,
            turn: dealer,
            last_draw: None,
            exhausted: false,
        };
        for &seat in &order {
            for tile in std::mem::take(&mut dealt[seat.index()]) {
                game.receive(seat, tile, true);
            }
        }
        game.draw(dealer, false);
        game
    }

    /// Adds a drawn tile to `seat`'s hand, setting flowers aside and drawing
    /// replacements until a playable tile arrives.
    fn receive(
        &mut self,
        seat: Seat,
        mut tile: WallTile,
        from_deal: bool,
    ) -> Option<Tile> {
        loop {
            match tile {
                WallTile::Tile(t) => {
                    self.hands[seat.index()].add_tile(t);
                    if !from_deal {
                        self.last_draw = Some(t);
                    }
                    return Some(t);
                },
                WallTile::Bonus(b) => {
                    self.hands[seat.index()].add_bonus(b);
                    match self.wall.draw_replacement() {
                        Some(next) => tile = next,
                        None => {
                            self.exhausted = true;
                            return None;
                        },
                    }
                },
            }
        }
    }

    /// Starts `seat`'s turn with a live draw, or a replacement draw after a kong.
    pub(crate) fn draw(
        &mut self,
        seat: Seat,
        replacement: bool,
    ) -> Option<Tile> {
        self.turn = seat;
        self.last_draw = None;
        let next = if replacement { self.wall.draw_replacement() } else { self.wall.draw() };
        match next {
            Some(tile) => self.receive(seat, tile, false),
            None => {
                self.exhausted = true;
                None
            },
        }
    }

    pub fn rules(&self) -> &Ruleset {
        &self.rules
    }

    pub fn dealer(&self) -> Seat {
        self.dealer
    }

    pub fn turn(&self) -> Seat {
        self.turn
    }

    pub fn hand(
        &self,
        seat: Seat,
    ) -> &Hand {
        &self.hands[seat.index()]
    }

    pub fn last_draw(&self) -> Option<Tile> {
        self.last_draw
    }

    pub fn wall(&self) -> &Wall {
        &self.wall
    }

    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}
