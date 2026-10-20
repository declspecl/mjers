//! Shanten: how many tiles a hand is from ready (−1 means complete).
//!
//! Standard form only, with the usual formula
//! `8 − 2·sets − partials − eye`, where partials (pairs, two-sided, edge, and
//! gap shapes) are capped so sets plus partials never exceed four.
//!
//! Like the standard formula, this ignores the four-copy limit: a hand holding
//! all four copies of a tile can count a wait on a fifth (2347777m reads as
//! ready on 7m). `waits` and ukeire exclude such tiles, so decisions are unaffected.
//!
//! Suits and honors never share a set, so each of the four groups (three
//! suits, then honors) is summarised on its own and the summaries combined.
//! `Groups` keeps those summaries so callers that change one tile at a time
//! (ukeire, discard ranking) recompute one group and one combination.

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::sync::OnceLock;

use mjers_tile::counts::TileCounts;
use mjers_tile::tile::{
    KINDS,
    SUITED_KINDS,
};

use crate::hand::{
    Hand,
    MAX_MELDS,
};

pub const MAX_SHANTEN: i8 = 8;

/// Three suits then the honors.
pub(crate) const GROUPS: usize = 4;
const RANKS: usize = 9;
const SETS: usize = MAX_MELDS + 1;
const IMPOSSIBLE: i8 = -1;
/// Partials beyond this never change the capped formula.
const PARTIAL_CAP: i8 = MAX_MELDS as i8;

/// Most partials for each (sets, eye used) combination, or `IMPOSSIBLE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Summary([[i8; 2]; SETS]);

impl Summary {
    const EMPTY: Summary = {
        let mut partials_by_sets_and_eye = [[IMPOSSIBLE; 2]; SETS];
        partials_by_sets_and_eye[0][0] = 0;
        Summary(partials_by_sets_and_eye)
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

    /// Shanten of the hand made of `self` and `other`, without building their
    /// combined summary.
    fn shanten_with(
        self,
        other: Summary,
        melds: usize,
    ) -> i8 {
        let mut best = MAX_SHANTEN;
        for (s1, row1) in self.0.iter().enumerate() {
            for (e1, &p1) in row1.iter().enumerate() {
                if p1 == IMPOSSIBLE {
                    continue;
                }
                for (s2, row2) in other.0.iter().enumerate() {
                    let sets = (s1 + s2 + melds) as i8;
                    if sets > MAX_MELDS as i8 {
                        break;
                    }
                    for (e2, &p2) in row2.iter().enumerate().take(2 - e1) {
                        if p2 != IMPOSSIBLE {
                            let partials = (p1 + p2).min(MAX_MELDS as i8 - sets);
                            best = best.min(MAX_SHANTEN - 2 * sets - partials - (e1 + e2) as i8);
                        }
                    }
                }
            }
        }
        best
    }

    /// Shanten of a whole hand summarised by `self`, beside `melds` declared melds.
    fn shanten(
        self,
        melds: usize,
    ) -> i8 {
        let mut best = MAX_SHANTEN;
        for (sets, row) in self.0.iter().enumerate() {
            let sets = (sets + melds) as i8;
            if sets > MAX_MELDS as i8 {
                break;
            }
            for (eye, &partials) in row.iter().enumerate() {
                if partials != IMPOSSIBLE {
                    let partials = partials.min(MAX_MELDS as i8 - sets);
                    best = best.min(MAX_SHANTEN - 2 * sets - partials - eye as i8);
                }
            }
        }
        best
    }
}

/// The group a tile index belongs to: 0-2 for the suits, 3 for honors.
pub(crate) const fn group_of(index: usize) -> usize {
    if index < SUITED_KINDS { index / RANKS } else { GROUPS - 1 }
}

/// Per-group summaries of one hand.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Groups {
    summary_by_group: [Summary; GROUPS],
}

impl Groups {
    pub(crate) fn of(counts: &[u8; KINDS]) -> Groups {
        Groups {
            summary_by_group: std::array::from_fn(|group| group_summary(counts, group)),
        }
    }

    /// The same hand with `group` re-summarised from `counts`.
    pub(crate) fn with_group(
        mut self,
        counts: &[u8; KINDS],
        group: usize,
    ) -> Groups {
        self.summary_by_group[group] = group_summary(counts, group);
        self
    }

    /// Every group except `group`, combined.
    pub(crate) fn others(
        &self,
        group: usize,
    ) -> Summary {
        self.summary_by_group
            .iter()
            .enumerate()
            .filter(|&(g, _)| g != group)
            .fold(Summary::EMPTY, |acc, (_, &summary)| acc.combine(summary))
    }

    pub(crate) fn shanten(
        &self,
        melds: usize,
    ) -> i8 {
        self.summary_by_group.iter().fold(Summary::EMPTY, |acc, &summary| acc.combine(summary)).shanten(melds)
    }
}

/// Shanten with `group` re-summarised from `counts` and combined with `others`.
pub(crate) fn shanten_with(
    others: Summary,
    counts: &[u8; KINDS],
    group: usize,
    melds: usize,
) -> i8 {
    group_summary(counts, group).shanten_with(others, melds)
}

/// Shanten of `concealed` beside `melds` declared melds.
pub fn shanten(
    concealed: &TileCounts,
    melds: usize,
) -> i8 {
    Groups::of(concealed.as_array()).shanten(melds)
}

fn group_summary(
    counts: &[u8; KINDS],
    group: usize,
) -> Summary {
    if group < GROUPS - 1 {
        let start = group * RANKS;
        let mut suit = [0u8; RANKS];
        suit.copy_from_slice(&counts[start..start + RANKS]);
        suit_summary(suit)
    } else {
        honor_summary(&counts[SUITED_KINDS..])
    }
}

/// Honors never chow, so only how many honors appear two, three, and four
/// times matters. `HONOR_SUMMARY_BY_SHAPE` covers every such shape.
fn honor_summary(honors: &[u8]) -> Summary {
    let mut copies_by_count = [0usize; 5];
    for &count in honors {
        copies_by_count[(count as usize).min(4)] += 1;
    }
    let table = HONOR_SUMMARY_BY_SHAPE.get_or_init(|| {
        std::array::from_fn(|key| {
            let (pairs, triples, quads) = (key / 64, key / 8 % 8, key % 8);
            let single = |count: u8| GroupSearch::summarise(&[count], false);
            std::iter::repeat_n(single(2), pairs)
                .chain(std::iter::repeat_n(single(3), triples))
                .chain(std::iter::repeat_n(single(4), quads))
                .fold(Summary::EMPTY, Summary::combine)
        })
    });
    table[copies_by_count[2] * 64 + copies_by_count[3] * 8 + copies_by_count[4]]
}

static HONOR_SUMMARY_BY_SHAPE: OnceLock<[Summary; 512]> = OnceLock::new();

/// Direct-mapped cache of suit summaries keyed by the suit's base-5 counts.
/// A collision simply recomputes.
const SUIT_CACHE_SLOTS: usize = 1 << 17;
const EMPTY_SLOT: u32 = u32::MAX;

struct SuitCache {
    key_by_slot: Box<[u32]>,
    summary_by_slot: Box<[Summary]>,
}

thread_local! {
    static SUIT_CACHE: RefCell<SuitCache> = RefCell::new(SuitCache {
        key_by_slot: vec![EMPTY_SLOT; SUIT_CACHE_SLOTS].into_boxed_slice(),
        summary_by_slot: vec![Summary::EMPTY; SUIT_CACHE_SLOTS].into_boxed_slice(),
    });
}

fn suit_summary(suit: [u8; RANKS]) -> Summary {
    if suit.iter().all(|&n| n == 0) {
        return Summary::EMPTY;
    }
    let key = suit.iter().fold(0u32, |key, &n| key * 5 + n as u32);
    let slot = (key.wrapping_mul(0x9E37_79B1) >> 15) as usize % SUIT_CACHE_SLOTS;
    SUIT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.key_by_slot[slot] == key {
            return cache.summary_by_slot[slot];
        }
        let summary = GroupSearch::summarise(&suit, true);
        cache.key_by_slot[slot] = key;
        cache.summary_by_slot[slot] = summary;
        summary
    })
}

/// Exhaustive decomposition of one suit (or a single honor).
struct GroupSearch<'a> {
    counts: &'a mut [u8],
    chows: bool,
    partials_by_sets_and_eye: [[i8; 2]; SETS],
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
            partials_by_sets_and_eye: [[IMPOSSIBLE; 2]; SETS],
        };
        search.visit(0, 0, 0, false);
        Summary(search.partials_by_sets_and_eye)
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
            if let Some(row) = self.partials_by_sets_and_eye.get_mut(sets) {
                let slot = &mut row[eye as usize];
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
