//! Shanten: how many tiles a hand is from ready (−1 means complete).
//!
//! Standard form only, with the usual formula
//! `8 − 2·sets − partials − eye`, where partials (pairs, two-sided, edge, and
//! gap shapes) are capped so sets plus partials never exceed four.
//!
//! Suits and honors never share a set, so each group is summarised on its own
//! (the most partials achievable for every set count, with and without the
//! eye) and the summaries are combined. Suit summaries are cached per thread.

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{
    BuildHasherDefault,
    Hasher,
};

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

const SETS: usize = MAX_MELDS + 1;
const IMPOSSIBLE: i8 = -1;
/// Partials beyond this never change the capped formula.
const PARTIAL_CAP: i8 = MAX_MELDS as i8;

/// Most partials for each (sets, eye used) combination, or `IMPOSSIBLE`.
#[derive(Clone, Copy)]
struct Summary([[i8; 2]; SETS]);

impl Summary {
    const EMPTY: Summary = {
        let mut table = [[IMPOSSIBLE; 2]; SETS];
        table[0][0] = 0;
        Summary(table)
    };

    fn combine(
        self,
        other: Summary,
    ) -> Summary {
        let mut out = [[IMPOSSIBLE; 2]; SETS];
        for (s1, row1) in self.0.iter().enumerate() {
            for (e1, &p1) in row1.iter().enumerate() {
                if p1 == IMPOSSIBLE {
                    continue;
                }
                for (s2, row2) in other.0.iter().enumerate().take(SETS - s1) {
                    for (e2, &p2) in row2.iter().enumerate().take(2 - e1) {
                        if p2 != IMPOSSIBLE {
                            let slot = &mut out[s1 + s2][e1 + e2];
                            *slot = (*slot).max((p1 + p2).min(PARTIAL_CAP));
                        }
                    }
                }
            }
        }
        Summary(out)
    }
}

/// Shanten of `concealed` beside `melds` declared melds.
pub fn shanten(
    concealed: &TileCounts,
    melds: usize,
) -> i8 {
    let counts = concealed.as_array();
    let mut total = Summary::EMPTY;
    for start in (0..SUITED_KINDS).step_by(9) {
        let suit: [u8; 9] = counts[start..start + 9].try_into().expect("suits have nine ranks");
        total = total.combine(suit_summary(suit));
    }
    for &count in &counts[SUITED_KINDS..KINDS] {
        if count > 0 {
            total = total.combine(HONOR_SUMMARIES.with(|h| h[count as usize]));
        }
    }

    let mut best = MAX_SHANTEN;
    for (sets, row) in total.0.iter().enumerate() {
        for (eye, &partials) in row.iter().enumerate() {
            let sets = (sets + melds) as i8;
            if partials == IMPOSSIBLE || sets > MAX_MELDS as i8 {
                continue;
            }
            let partials = partials.min(MAX_MELDS as i8 - sets);
            best = best.min(MAX_SHANTEN - 2 * sets - partials - eye as i8);
        }
    }
    best
}

#[derive(Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(
        &mut self,
        _: &[u8],
    ) {
        unreachable!("only u32 keys are hashed")
    }

    fn write_u32(
        &mut self,
        key: u32,
    ) {
        self.0 = (key as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

thread_local! {
    static SUIT_CACHE: RefCell<HashMap<u32, Summary, BuildHasherDefault<KeyHasher>>> = RefCell::default();
    static HONOR_SUMMARIES: [Summary; 5] = std::array::from_fn(|count| GroupSearch::summarise(&[count as u8], false));
}

fn suit_summary(suit: [u8; 9]) -> Summary {
    if suit.iter().all(|&n| n == 0) {
        return Summary::EMPTY;
    }
    let key = suit.iter().fold(0u32, |key, &n| key * 5 + n as u32);
    if let Some(summary) = SUIT_CACHE.with(|c| c.borrow().get(&key).copied()) {
        return summary;
    }
    let summary = GroupSearch::summarise(&suit, true);
    SUIT_CACHE.with(|c| c.borrow_mut().insert(key, summary));
    summary
}

/// Exhaustive decomposition of one suit (or a single honor).
struct GroupSearch<'a> {
    counts: &'a mut [u8],
    chows: bool,
    summary: [[i8; 2]; SETS],
}

impl GroupSearch<'_> {
    fn summarise(
        counts: &[u8],
        chows: bool,
    ) -> Summary {
        let mut counts = counts.to_vec();
        let mut search = GroupSearch {
            counts: &mut counts,
            chows,
            summary: [[IMPOSSIBLE; 2]; SETS],
        };
        search.visit(0, 0, 0, false);
        Summary(search.summary)
    }

    fn visit(
        &mut self,
        start: usize,
        sets: usize,
        partials: i8,
        eye: bool,
    ) {
        let len = self.counts.len();
        let Some(i) = (start..len).find(|&i| self.counts[i] > 0) else {
            if sets < SETS {
                let slot = &mut self.summary[sets][eye as usize];
                *slot = (*slot).max(partials.min(PARTIAL_CAP));
            }
            return;
        };
        let next = self.chows && i + 1 < len && self.counts[i + 1] > 0;
        let gap = self.chows && i + 2 < len && self.counts[i + 2] > 0;

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
}

impl Hand {
    pub fn shanten(&self) -> i8 {
        shanten(self.concealed(), self.meld_count())
    }
}
