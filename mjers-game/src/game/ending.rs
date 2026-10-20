//! Ending a hand: a win, or an exhausted wall.

use mjers_hand::meld::Meld;
use mjers_rules::seat::Seat;
use mjers_rules::settlement::{
    HandResult,
    Kong,
    Win,
    WinSource,
};
use mjers_tile::tile::Tile;

use super::Game;
use crate::action::Phase;
use crate::trace::Event;

impl Game {
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
        self.phase = Phase::Ended(HandResult { win: Some(win), kongs: self.kongs() });
    }

    /// The wall ran out: no winner, but kongs still count.
    pub(super) fn end_exhausted(&mut self) {
        self.events.push(Event::Exhausted);
        self.phase = Phase::Ended(HandResult { win: None, kongs: self.kongs() });
    }

    /// Every declared kong at the table.
    fn kongs(&self) -> Vec<Kong> {
        Seat::ALL
            .iter()
            .flat_map(|&owner| {
                self.hand(owner).melds().filter_map(move |meld| match meld {
                    Meld::Kong(_, kind) => Some(Kong { owner, kind }),
                    _ => None,
                })
            })
            .collect()
    }
}
