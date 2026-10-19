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
    Settlement,
    WinSource,
};
use mjers_tile::counts::COPIES;
use mjers_tile::{
    Tile,
    TileCounts,
};

use serde::{
    Deserialize,
    Serialize,
};

use crate::action::{
    Action,
    GameError,
    Offer,
    Phase,
};
use crate::rng::Rng;
use crate::trace::{
    Event,
    Step,
};
use crate::wall::{
    Wall,
    WallTile,
};

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
    ) -> Game {
        let wall = Wall::shuffled(&mut Rng::seed(seed), rules.reserved_tiles());
        let mut game = Game::from_wall(rules, wall, dealer);
        game.seed = Some(seed);
        game
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
        let initial_wall = Arc::new(wall.clone());
        let order: Vec<Seat> = std::iter::once(dealer).chain(dealer.others()).collect();
        let mut dealt: [Vec<WallTile>; SEATS] = Default::default();
        for take in [4, 4, 4, 1] {
            for &seat in &order {
                for _ in 0..take {
                    dealt[seat.index()].push(wall.draw().expect("a full wall covers the deal"));
                }
            }
        }
        let mut game = Game::from_parts(rules, wall, [Hand::new(TileCounts::new()); SEATS], dealer, Vec::new(), TileCounts::new());
        game.initial_wall = initial_wall;
        for &seat in &order {
            for tile in std::mem::take(&mut dealt[seat.index()]) {
                if game.receive(seat, tile).is_none() {
                    return game;
                }
            }
        }
        for &seat in &order {
            let tiles = *game.hand(seat).concealed();
            game.events.push(Event::Deal { seat, tiles });
        }
        game.start_turn(dealer, false);
        game
    }

    /// A game around existing state, waiting at the dealer's turn until told otherwise.
    pub(crate) fn from_parts(
        rules: Arc<Ruleset>,
        wall: Wall,
        hands: [Hand; SEATS],
        dealer: Seat,
        discards: Vec<Discard>,
        public: TileCounts,
    ) -> Game {
        Game {
            rules,
            initial_wall: Arc::new(wall.clone()),
            wall,
            hands,
            dealer,
            phase: Phase::Turn { seat: dealer, after_claim: false },
            discards,
            public,
            last_draw: None,
            asked: [false; SEATS],
            decisions: [None; SEATS],
            seed: None,
            steps: Vec::new(),
            events: Vec::new(),
        }
    }

    pub(crate) fn set_turn(
        &mut self,
        seat: Seat,
        after_claim: bool,
        last_draw: Option<Tile>,
    ) {
        self.phase = Phase::Turn { seat, after_claim };
        self.last_draw = last_draw;
    }

    pub(crate) fn reopen_claims(
        &mut self,
        tile: Tile,
        from: Seat,
        offer: Offer,
    ) {
        self.open_claims(tile, from, offer);
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
                    if hand.is_winning() {
                        actions.push(Action::SelfDrawWin);
                    }
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
        self.steps.push(Step { seat, action });
        match action {
            Action::Discard(tile) => self.discard(seat, tile),
            Action::SelfDrawWin => {
                let tile = self.last_draw.expect("self-draw wins follow a draw");
                self.end_with_win(seat, WinSource::SelfDraw, tile);
            },
            Action::ConcealedKong(tile) => {
                let meld = self.hands[seat.index()].declare_concealed_kong(tile).expect("kong was legal");
                self.events.push(Event::Kong { seat, meld });
                for _ in 0..COPIES {
                    self.public.add(tile);
                }
                self.start_turn(seat, true);
            },
            Action::UpgradeKong(tile) if self.rules.allow_rob_kong => self.open_claims(tile, seat, Offer::KongUpgrade),
            Action::UpgradeKong(tile) => self.complete_upgrade(seat, tile),
            Action::Pass | Action::Chow(_) | Action::Pong | Action::Kong | Action::Win => self.decide(seat, action),
        }
        Ok(())
    }

    fn complete_upgrade(
        &mut self,
        seat: Seat,
        tile: Tile,
    ) {
        let meld = self.hands[seat.index()].upgrade_kong(tile).expect("upgrade was legal");
        self.events.push(Event::Kong { seat, meld });
        self.public.add(tile);
        self.start_turn(seat, true);
    }

    fn discard(
        &mut self,
        seat: Seat,
        tile: Tile,
    ) {
        self.hands[seat.index()].discard(tile).expect("legal discards are held");
        self.discards.push(Discard { seat, tile, claimed: false });
        self.events.push(Event::Discard { seat, tile });
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
            self.end_exhausted();
            return;
        };
        if let Some(drawn) = self.receive(seat, next) {
            self.events.push(Event::Draw { seat, tile: drawn });
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
                    self.events.push(Event::Bonus { seat, bonus: b });
                    match self.wall.draw_replacement() {
                        Some(next) => tile = next,
                        None => {
                            self.end_exhausted();
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

    /// The wall ran out: no winner, but kongs still count.
    fn end_exhausted(&mut self) {
        let result = HandResult { win: None, kongs: self.kongs() };
        self.events.push(Event::Exhausted);
        self.end(result);
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
