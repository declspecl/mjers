//! The claim window after a discard.

use mjers_hand::Meld;
use mjers_rules::{
    HandResult,
    Kong,
    Seat,
    Win,
    WinSource,
};
use mjers_tile::Tile;

use super::Game;
use crate::action::{
    Action,
    Offer,
    Phase,
};
use crate::trace::Event;

impl Game {
    /// Offers `tile` to every seat that could use it, or moves on if none can.
    pub(super) fn open_claims(
        &mut self,
        tile: Tile,
        from: Seat,
        offer: Offer,
    ) {
        self.decisions = [None; mjers_rules::SEATS];
        self.asked = [false; mjers_rules::SEATS];
        for seat in from.others() {
            self.asked[seat.index()] = !self.claim_options(seat, tile, from, offer).is_empty();
        }
        self.phase = Phase::Claim { tile, from, offer };
        if !self.asked.contains(&true) {
            self.resolve_claims();
        }
    }

    pub(super) fn undecided_claimants(&self) -> impl Iterator<Item = Seat> + '_ {
        let Phase::Claim { from, .. } = self.phase else { unreachable!("only called in a claim window") };
        from.others().filter(|s| self.asked[s.index()] && self.decisions[s.index()].is_none())
    }

    pub(super) fn claim_actions(
        &self,
        seat: Seat,
    ) -> Vec<Action> {
        let Phase::Claim { tile, from, offer } = self.phase else {
            return Vec::new();
        };
        if !self.asked[seat.index()] || self.decisions[seat.index()].is_some() {
            return Vec::new();
        }
        let mut actions = self.claim_options(seat, tile, from, offer);
        actions.push(Action::Pass);
        actions
    }

    /// Claims `seat` could make on `tile`, excluding passing.
    fn claim_options(
        &self,
        seat: Seat,
        tile: Tile,
        from: Seat,
        offer: Offer,
    ) -> Vec<Action> {
        let hand = self.hand(seat);
        let mut options = Vec::new();
        if hand.wins_with(tile) {
            options.push(Action::Win);
        }
        if offer == Offer::KongUpgrade || hand.meld_count() == mjers_hand::hand::MAX_MELDS {
            return options;
        }
        let held = hand.concealed().get(tile);
        if held >= 2 {
            options.push(Action::Pong);
        }
        if held == 3 && self.wall.remaining() > 0 {
            options.push(Action::Kong);
        }
        if seat.previous() == from {
            options.extend(
                chow_starts(tile)
                    .filter(|&lowest| {
                        let mut needed = Meld::Chow(lowest).tiles();
                        needed.remove(tile);
                        needed.kinds().all(|t| hand.concealed().get(t) >= needed.get(t))
                    })
                    .map(Action::Chow),
            );
        }
        options
    }

    pub(super) fn decide(
        &mut self,
        seat: Seat,
        action: Action,
    ) {
        self.decisions[seat.index()] = Some(action);
        if self.undecided_claimants().next().is_none() {
            self.resolve_claims();
        }
    }

    /// Win beats pong and kong, which beat chow. Among winners, the seat
    /// soonest after the discarder takes the tile.
    fn resolve_claims(&mut self) {
        let Phase::Claim { tile, from, offer } = self.phase else {
            unreachable!("only called in a claim window")
        };
        let chosen = |wanted: fn(Action) -> bool| from.others().find(|s| self.decisions[s.index()].is_some_and(wanted));
        if let Some(winner) = chosen(|a| a == Action::Win) {
            self.hands[winner.index()].add_tile(tile);
            match offer {
                Offer::Discard => self.mark_last_discard_claimed(),
                Offer::KongUpgrade => {
                    // Robbed: the tile leaves the kong owner's hand face up.
                    self.hands[from.index()].discard(tile).expect("upgrade tile is held");
                    self.public.add(tile);
                },
            }
            self.end_with_win(winner, WinSource::Discard(from), tile);
        } else if offer == Offer::KongUpgrade {
            self.complete_upgrade(from, tile);
        } else if let Some(seat) = chosen(|a| a == Action::Kong) {
            let meld = self.hands[seat.index()].claim_kong(tile).expect("kong was legal");
            self.after_meld_claim(seat, meld, tile);
            self.start_turn(seat, true);
        } else if let Some(seat) = chosen(|a| a == Action::Pong) {
            let meld = self.hands[seat.index()].claim_pong(tile).expect("pong was legal");
            self.after_meld_claim(seat, meld, tile);
        } else if let Some(seat) = chosen(|a| matches!(a, Action::Chow(_))) {
            let Some(Action::Chow(lowest)) = self.decisions[seat.index()] else { unreachable!() };
            let meld = self.hands[seat.index()].claim_chow(lowest, tile).expect("chow was legal");
            self.after_meld_claim(seat, meld, tile);
        } else {
            self.start_turn(from.next(), false);
        }
    }

    fn after_meld_claim(
        &mut self,
        seat: Seat,
        meld: Meld,
        claimed: Tile,
    ) {
        self.mark_last_discard_claimed();
        let from = self.discards.last().expect("a discard was claimed").seat;
        self.events.push(Event::Claim { seat, from, meld });
        let mut revealed = meld.tiles();
        revealed.remove(claimed);
        for tile in revealed.tiles() {
            self.public.add(tile);
        }
        self.last_draw = None;
        self.phase = Phase::Turn { seat, after_claim: true };
    }

    fn mark_last_discard_claimed(&mut self) {
        self.discards.last_mut().expect("a discard was offered").claimed = true;
    }

    pub(super) fn end_with_win(
        &mut self,
        winner: Seat,
        source: WinSource,
        tile: Tile,
    ) {
        let win = Win {
            winner,
            source,
            bonus_tiles: self.hand(winner).bonus().len(),
        };
        self.events.push(Event::Win { seat: winner, source, tile });
        let result = HandResult { win: Some(win), kongs: self.kongs() };
        self.end(result);
    }

    /// Every declared kong at the table.
    pub(super) fn kongs(&self) -> Vec<Kong> {
        mjers_rules::Seat::ALL
            .iter()
            .flat_map(|&owner| {
                self.hand(owner).melds().filter_map(move |m| match m {
                    Meld::Kong(_, kind) => Some(Kong { owner, kind }),
                    _ => None,
                })
            })
            .collect()
    }
}

/// Lowest tiles of the chows that could include `tile`.
fn chow_starts(tile: Tile) -> impl Iterator<Item = Tile> {
    let index = tile.index();
    (index.saturating_sub(2)..=index)
        .filter_map(Tile::from_index)
        .filter(move |&lowest| Meld::chow(lowest).is_some() && lowest.suit() == tile.suit() && lowest.index() + 2 >= index)
}
