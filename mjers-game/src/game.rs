//! One hand of mahjong, from the deal to settlement.

mod claims;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use mjers_hand::hand::MAX_MELDS;
use mjers_hand::{
    Hand,
    Meld,
};
use mjers_rules::{
    HandResult,
    Ruleset,
    SEATS,
    Seat,
};
use mjers_tile::counts::COPIES;
use mjers_tile::{
    Tile,
    TileCounts,
};

use crate::action::{
    Action,
    GameError,
    Offer,
    Phase,
};
use crate::rng::Rng;
use crate::wall::{
    Wall,
    WallTile,
};

pub const DEALT_TILES: usize = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    hands: [Hand; SEATS],
    dealer: Seat,
    phase: Phase,
    discards: Vec<Discard>,
    /// Tiles every player can see: discards and exposed melds.
    public: TileCounts,
    /// The tile drawn this turn, if the turn began with a draw.
    last_draw: Option<Tile>,
    /// In a claim window: seats with an option, and what each has chosen.
    asked: [bool; SEATS],
    decisions: [Option<Action>; SEATS],
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
            phase: Phase::Turn { seat: dealer, after_claim: false },
            discards: Vec::new(),
            public: TileCounts::new(),
            last_draw: None,
            asked: [false; SEATS],
            decisions: [None; SEATS],
        };
        for &seat in &order {
            for tile in std::mem::take(&mut dealt[seat.index()]) {
                if game.receive(seat, tile).is_none() {
                    return game;
                }
            }
        }
        game.start_turn(dealer, false);
        game
    }

    /// Seats that must decide before play continues.
    pub fn to_act(&self) -> Vec<Seat> {
        match &self.phase {
            Phase::Turn { seat, .. } => vec![*seat],
            Phase::Claim { .. } => self.undecided_claimants().collect(),
            Phase::Ended(_) => Vec::new(),
        }
    }

    pub fn legal_actions(
        &self,
        seat: Seat,
    ) -> Vec<Action> {
        match &self.phase {
            Phase::Turn { seat: current, after_claim } if *current == seat => {
                let hand = self.hand(seat);
                let mut actions = Vec::new();
                if !after_claim {
                    let can_replace = self.wall.remaining() > 0;
                    let concealed = hand.concealed();
                    if can_replace && hand.meld_count() < MAX_MELDS {
                        actions.extend(concealed.kinds().filter(|&t| concealed.get(t) == COPIES).map(Action::ConcealedKong));
                    }
                    if can_replace {
                        actions.extend(hand.melds().filter_map(|m| match m {
                            Meld::Pong(t) if concealed.get(t) > 0 => Some(Action::UpgradeKong(t)),
                            _ => None,
                        }));
                    }
                }
                actions.extend(hand.concealed().kinds().map(Action::Discard));
                actions
            },
            Phase::Claim { .. } => self.claim_actions(seat),
            _ => Vec::new(),
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
        if !self.to_act().contains(&seat) {
            return Err(GameError::NotAsked(seat));
        }
        if !self.legal_actions(seat).contains(&action) {
            return Err(GameError::Illegal { seat, action });
        }
        match action {
            Action::Discard(tile) => self.discard(seat, tile),
            Action::ConcealedKong(tile) => {
                self.hands[seat.index()].declare_concealed_kong(tile).expect("kong was legal");
                for _ in 0..COPIES {
                    self.public.add(tile);
                }
                self.start_turn(seat, true);
            },
            Action::UpgradeKong(tile) => {
                self.hands[seat.index()].upgrade_kong(tile).expect("upgrade was legal");
                self.public.add(tile);
                self.start_turn(seat, true);
            },
            Action::Pass | Action::Chow(_) | Action::Pong | Action::Kong | Action::Win => self.decide(seat, action),
            _ => unreachable!("not offered: {action}"),
        }
        Ok(())
    }

    fn discard(
        &mut self,
        seat: Seat,
        tile: Tile,
    ) {
        self.hands[seat.index()].discard(tile).expect("legal discards are held");
        self.discards.push(Discard { seat, tile, claimed: false });
        self.public.add(tile);
        self.open_claims(tile, seat, Offer::Discard);
    }

    /// Begins `seat`'s turn with a live draw, or a replacement draw after a kong.
    /// Ends the hand if the wall cannot supply the tile.
    fn start_turn(
        &mut self,
        seat: Seat,
        replacement: bool,
    ) {
        self.last_draw = None;
        let next = if replacement { self.wall.draw_replacement() } else { self.wall.draw() };
        let Some(next) = next else {
            self.end(HandResult::default());
            return;
        };
        if let Some(drawn) = self.receive(seat, next) {
            self.last_draw = Some(drawn);
            self.phase = Phase::Turn { seat, after_claim: false };
        }
    }

    /// Adds a tile to `seat`'s hand, setting flowers aside and drawing
    /// replacements until a playable tile arrives. Ends the hand if the wall
    /// runs out first.
    fn receive(
        &mut self,
        seat: Seat,
        mut tile: WallTile,
    ) -> Option<Tile> {
        loop {
            match tile {
                WallTile::Tile(t) => {
                    self.hands[seat.index()].add_tile(t);
                    return Some(t);
                },
                WallTile::Bonus(b) => {
                    self.hands[seat.index()].add_bonus(b);
                    match self.wall.draw_replacement() {
                        Some(next) => tile = next,
                        None => {
                            self.end(HandResult::default());
                            return None;
                        },
                    }
                },
            }
        }
    }

    fn end(
        &mut self,
        result: HandResult,
    ) {
        self.phase = Phase::Ended(result);
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

    pub fn hand(
        &self,
        seat: Seat,
    ) -> &Hand {
        &self.hands[seat.index()]
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
}
