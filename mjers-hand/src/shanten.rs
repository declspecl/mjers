//! Shanten: how many tiles a hand is from ready (−1 means complete).
//!
//! Standard form only, with the usual formula
//! `8 − 2·sets − partials − eye`, where partials (pairs, two-sided, edge, and
//! gap shapes) are capped so sets plus partials never exceed four.

#[cfg(test)]
mod tests;

use mjers_tile::TileCounts;
use mjers_tile::tile::{
    KINDS,
    SUITED_KINDS,
};

use crate::hand::{
    Hand,
    MAX_MELDS,
};

pub const MAX_SHANTEN: i8 = 8;

/// Shanten of `concealed` beside `melds` declared melds.
pub fn shanten(
    concealed: &TileCounts,
    melds: usize,
) -> i8 {
    let mut search = Search {
        counts: *concealed.as_array(),
        melds: melds as i8,
        best: MAX_SHANTEN,
    };
    search.visit(0, 0, 0, false);
    search.best
}

struct Search {
    counts: [u8; KINDS],
    melds: i8,
    best: i8,
}

impl Search {
    fn visit(
        &mut self,
        start: usize,
        sets: i8,
        partials: i8,
        eye: bool,
    ) {
        let Some(i) = (start..KINDS).find(|&i| self.counts[i] > 0) else {
            self.record(sets, partials, eye);
            return;
        };
        let suited = i < SUITED_KINDS;
        let rank = i % 9;
        let next = suited && rank <= 7 && self.counts[i + 1] > 0;
        let gap = suited && rank <= 6 && self.counts[i + 2] > 0;

        if self.counts[i] >= 3 {
            self.with(&[(i, 3)], |s| s.visit(i, sets + 1, partials, eye));
        }
        if next && gap {
            self.with(&[(i, 1), (i + 1, 1), (i + 2, 1)], |s| s.visit(i, sets + 1, partials, eye));
        }
        if self.counts[i] >= 2 {
            if !eye {
                self.with(&[(i, 2)], |s| s.visit(i, sets, partials, true));
            }
            self.with(&[(i, 2)], |s| s.visit(i, sets, partials + 1, eye));
        }
        if next {
            self.with(&[(i, 1), (i + 1, 1)], |s| s.visit(i, sets, partials + 1, eye));
        }
        if gap {
            self.with(&[(i, 1), (i + 2, 1)], |s| s.visit(i, sets, partials + 1, eye));
        }
        self.with(&[(i, 1)], |s| s.visit(i, sets, partials, eye));
    }

    /// Runs `f` with `taken` tiles removed, restoring them afterwards.
    fn with(
        &mut self,
        taken: &[(usize, u8)],
        f: impl FnOnce(&mut Self),
    ) {
        for &(i, n) in taken {
            self.counts[i] -= n;
        }
        f(self);
        for &(i, n) in taken {
            self.counts[i] += n;
        }
    }

    fn record(
        &mut self,
        sets: i8,
        partials: i8,
        eye: bool,
    ) {
        let sets = sets + self.melds;
        let partials = partials.min((MAX_MELDS as i8 - sets).max(0));
        let value = MAX_SHANTEN - 2 * sets - partials - eye as i8;
        self.best = self.best.min(value);
    }
}

impl Hand {
    pub fn shanten(&self) -> i8 {
        shanten(self.concealed(), self.meld_count())
    }
}
