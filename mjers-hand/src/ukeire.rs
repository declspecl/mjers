//! Ukeire (tile acceptance) and discard evaluation.
//!
//! `visible` holds every tile the player can see outside their own concealed
//! hand: all discards and all exposed melds, including their own.

#[cfg(test)]
mod tests;

use mjers_tile::counts::COPIES;
use mjers_tile::{
    Tile,
    TileCounts,
};

use crate::hand::Hand;
use crate::shanten::shanten;

/// A tile that lowers shanten, and how many copies of it are still live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Improvement {
    pub tile: Tile,
    pub live: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ukeire {
    pub shanten: i8,
    pub improvements: Vec<Improvement>,
}

impl Ukeire {
    /// Live copies of all improving tiles.
    pub fn total(&self) -> u32 {
        self.improvements.iter().map(|i| i.live as u32).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    let base = shanten(concealed, melds);
    let improvements = Tile::all()
        .filter_map(|tile| {
            let live = COPIES.saturating_sub(concealed.get(tile) + visible.get(tile));
            (live > 0 && shanten(&concealed.with(tile), melds) < base).then_some(Improvement { tile, live })
        })
        .collect();
    Ukeire { shanten: base, improvements }
}

/// Every distinct discard from a hand awaiting discard, best first: lowest
/// resulting shanten, then most live improving tiles.
pub fn discard_options(
    concealed: &TileCounts,
    melds: usize,
    visible: &TileCounts,
) -> Vec<DiscardOption> {
    let mut options: Vec<DiscardOption> = concealed
        .kinds()
        .map(|tile| {
            let mut after = *concealed;
            after.remove(tile);
            // The discard itself becomes visible.
            DiscardOption {
                tile,
                ukeire: ukeire(&after, melds, &visible.with(tile)),
            }
        })
        .collect();
    options.sort_by(|a, b| a.ukeire.shanten.cmp(&b.ukeire.shanten).then_with(|| b.ukeire.total().cmp(&a.ukeire.total())));
    options
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
