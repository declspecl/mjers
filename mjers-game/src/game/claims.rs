//! The claim window after a discard (or a kong upgrade that may be robbed).

use mjers_hand::hand::MAX_MELDS;
use mjers_hand::meld::Meld;
use mjers_rules::seat::{
    SEATS,
    Seat,
    SeatSet,
};
use mjers_rules::settlement::WinSource;
use mjers_tile::tile::Tile;

use super::Game;
use crate::action::{
    Action,
    Actions,
    GameError,
    Offer,
    Phase,
};
use crate::trace::Event;

/// The claim that takes the offered tile, by priority.
enum Winning {
    Win(Seat),
    Kong(Seat),
    Pong(Seat),
    Chow(Seat, Tile),
    Nobody,
}

impl Game {
    /// Offers `tile` to every seat that could use it, or moves on if none can.
    pub(super) fn open_claims(
        &mut self,
        tile: Tile,
        from: Seat,
        offer: Offer,
    ) -> Result<(), GameError> {
        self.decision_by_seat = [None; SEATS];
        self.asked = from.others().filter(|&seat| !self.claim_options(seat, tile, from, offer).is_empty()).collect();
        self.phase = Phase::Claim { tile, from, offer };
        if self.asked.is_empty() {
            return self.resolve_claims();
        }
        Ok(())
    }

    pub(super) fn undecided_claimants(&self) -> SeatSet {
        self.asked.iter().filter(|s| self.decision_by_seat[s.index()].is_none()).collect()
    }

    pub(super) fn claim_actions(
        &self,
        seat: Seat,
    ) -> Actions {
        let Phase::Claim { tile, from, offer } = self.phase else {
            return Actions::new();
        };
        if !self.undecided_claimants().contains(seat) {
            return Actions::new();
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
    ) -> Actions {
        let hand = self.hand(seat);
        let mut options = Actions::new();
        if hand.wins_with(tile) {
            options.push(Action::Win);
        }
        if offer == Offer::KongUpgrade || hand.meld_count() == MAX_MELDS {
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
            let chows = chow_starts(tile).filter(|&lowest| {
                let mut needed = Meld::Chow(lowest).tiles();
                needed.remove(tile);
                needed.kinds().all(|t| hand.concealed().get(t) >= needed.get(t))
            });
            options.extend(chows.map(Action::Chow));
        }
        options
    }

    pub(super) fn decide(
        &mut self,
        seat: Seat,
        action: Action,
    ) -> Result<(), GameError> {
        self.decision_by_seat[seat.index()] = Some(action);
        if self.undecided_claimants().is_empty() {
            return self.resolve_claims();
        }
        Ok(())
    }

    /// Win beats pong and kong, which beat chow. Among winners, the seat
    /// soonest after the discarder takes the tile.
    fn winning_claim(
        &self,
        from: Seat,
    ) -> Winning {
        let decided = |wanted: fn(Action) -> bool| from.others().find(|s| self.decision_by_seat[s.index()].is_some_and(wanted));
        if let Some(seat) = decided(|a| a == Action::Win) {
            return Winning::Win(seat);
        }
        if let Some(seat) = decided(|a| a == Action::Kong) {
            return Winning::Kong(seat);
        }
        if let Some(seat) = decided(|a| a == Action::Pong) {
            return Winning::Pong(seat);
        }
        for seat in from.others() {
            if let Some(Action::Chow(lowest)) = self.decision_by_seat[seat.index()] {
                return Winning::Chow(seat, lowest);
            }
        }
        Winning::Nobody
    }

    fn resolve_claims(&mut self) -> Result<(), GameError> {
        let Phase::Claim { tile, from, offer } = self.phase else {
            return Ok(());
        };
        match (self.winning_claim(from), offer) {
            (Winning::Win(winner), _) => {
                self.hand_by_seat[winner.index()].add_tile(tile);
                match offer {
                    Offer::Discard => self.mark_last_discard_claimed(),
                    Offer::KongUpgrade => {
                        // Robbed: the tile leaves the kong owner's hand face up.
                        self.hand_by_seat[from.index()].discard(tile)?;
                        self.public.add(tile);
                    },
                }
                self.end_with_win(winner, WinSource::Discard(from), tile);
            },
            (_, Offer::KongUpgrade) => self.complete_upgrade(from, tile)?,
            (Winning::Kong(seat), Offer::Discard) => {
                let meld = self.hand_by_seat[seat.index()].claim_kong(tile)?;
                self.after_meld_claim(seat, from, meld, tile);
                self.start_turn(seat, true);
            },
            (Winning::Pong(seat), Offer::Discard) => {
                let meld = self.hand_by_seat[seat.index()].claim_pong(tile)?;
                self.after_meld_claim(seat, from, meld, tile);
            },
            (Winning::Chow(seat, lowest), Offer::Discard) => {
                let meld = self.hand_by_seat[seat.index()].claim_chow(lowest, tile)?;
                self.after_meld_claim(seat, from, meld, tile);
            },
            (Winning::Nobody, Offer::Discard) => self.start_turn(from.next(), false),
        }
        Ok(())
    }

    fn after_meld_claim(
        &mut self,
        seat: Seat,
        from: Seat,
        meld: Meld,
        claimed: Tile,
    ) {
        self.mark_last_discard_claimed();
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
        if let Some(discard) = self.discards.last_mut() {
            discard.claimed = true;
        }
    }
}

/// Lowest tiles of the chows that could include `tile`.
fn chow_starts(tile: Tile) -> impl Iterator<Item = Tile> {
    let index = tile.index();
    (index.saturating_sub(2)..=index)
        .filter_map(Tile::from_index)
        .filter(move |&lowest| Meld::chow(lowest).is_some() && lowest.suit() == tile.suit())
}
