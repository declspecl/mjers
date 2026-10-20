//! Ukeire (tile acceptance) and discard evaluation.
//!
//! `visible` holds every tile the player can see outside their own concealed
//! hand: all discards and all exposed melds, including their own.

#[cfg(test)]
mod tests;

use crate::hand::Hand;
use crate::hand::shanten::{
    GROUPS,
    Groups,
    group_of,
    shanten_with,
};
use crate::tile::counts::{
    COPIES,
    TileCounts,
};
use crate::tile::{
    KINDS,
    SUITED_KINDS,
    Tile,
};

/// A tile that lowers shanten, and how many copies of it are still live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Improvement {
    pub tile: Tile,
    pub live: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ukeire {
    pub shanten: i8,
    /// Live copies of each improving tile; zero for tiles that do not improve.
    pub live_by_tile: [u8; KINDS],
}

impl Ukeire {
    /// Live copies of all improving tiles.
    pub fn total(&self) -> u32 {
        self.live_by_tile.iter().map(|&n| n as u32).sum()
    }

    pub fn improvements(&self) -> impl Iterator<Item = Improvement> + '_ {
        Tile::all().filter_map(|tile| {
            let live = self.live_by_tile[tile.index()];
            (live > 0).then_some(Improvement { tile, live })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscardOption {
    pub tile: Tile,
    pub ukeire: Ukeire,
}

/// Improving draws for a hand waiting to draw.
pub fn ukeire(
    concealed: &TileCounts,
    melds: usize,
    visible: &TileCounts,
) -> Ukeire {
    let mut counts = *concealed.as_array();
    let groups = Groups::of(&counts);
    accepted(&mut counts, &groups, melds, visible)
}

/// Every distinct discard from a hand awaiting discard, best first: lowest
/// resulting shanten, then most live improving tiles.
pub fn discard_options(
    concealed: &TileCounts,
    melds: usize,
    visible: &TileCounts,
) -> Vec<DiscardOption> {
    let mut counts = *concealed.as_array();
    let groups = Groups::of(&counts);
    let mut options: Vec<DiscardOption> = concealed
        .kinds()
        .map(|tile| {
            let i = tile.index();
            counts[i] -= 1;
            let after = groups.with_group(&counts, group_of(i));
            // The discard itself becomes visible.
            let ukeire = accepted(&mut counts, &after, melds, &visible.with(tile));
            counts[i] += 1;
            DiscardOption { tile, ukeire }
        })
        .collect();
    options.sort_by(|a, b| a.ukeire.shanten.cmp(&b.ukeire.shanten).then_with(|| b.ukeire.total().cmp(&a.ukeire.total())));
    options
}

/// Ukeire of the hand in `counts` (summarised by `groups`), trying each
/// candidate draw in place and restoring `counts` afterwards.
fn accepted(
    counts: &mut [u8; KINDS],
    groups: &Groups,
    melds: usize,
    visible: &TileCounts,
) -> Ukeire {
    let base = groups.shanten(melds);
    let others_by_group: [_; GROUPS] = std::array::from_fn(|group| groups.others(group));
    let mut live_by_tile = [0u8; KINDS];
    for i in 0..KINDS {
        let live = COPIES.saturating_sub(counts[i] + visible.as_array()[i]);
        if live == 0 || !could_connect(counts, i) {
            continue;
        }
        let group = group_of(i);
        counts[i] += 1;
        let improves = shanten_with(others_by_group[group], counts, group, melds) < base;
        counts[i] -= 1;
        if improves {
            live_by_tile[i] = live;
        }
    }
    Ukeire { shanten: base, live_by_tile }
}

/// Whether drawing tile `i` could join anything held: a copy, or a suit
/// neighbour within two ranks. A tile that joins nothing never lowers shanten.
fn could_connect(
    counts: &[u8; KINDS],
    i: usize,
) -> bool {
    if counts[i] > 0 {
        return true;
    }
    if i >= SUITED_KINDS {
        return false;
    }
    let rank = i % 9;
    let suit_start = i - rank;
    (rank.saturating_sub(2)..=(rank + 2).min(8)).any(|r| counts[suit_start + r] > 0)
}

impl Hand {
    pub fn ukeire(
        &self,
        visible: &TileCounts,
    ) -> Ukeire {
        ukeire(self.concealed(), self.meld_count(), visible)
    }

    pub fn discard_options(
        &self,
        visible: &TileCounts,
    ) -> Vec<DiscardOption> {
        discard_options(self.concealed(), self.meld_count(), visible)
    }
}
