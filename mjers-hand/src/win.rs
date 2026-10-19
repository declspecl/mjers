//! Standard-form win detection: four sets plus an eye.
//!
//! Special hands (seven pairs, thirteen orphans) are not wins in any supported
//! ruleset, so only the standard form is recognised.

#[cfg(test)]
mod tests;

use mjers_tile::tile::{
    KINDS,
    SUITED_KINDS,
};
use mjers_tile::{
    Tile,
    TileCounts,
    counts::COPIES,
};

use crate::hand::{
    Hand,
    MAX_MELDS,
};

/// Whether `concealed` forms the remaining sets plus an eye beside `melds`
/// declared melds.
pub fn is_complete(
    concealed: &TileCounts,
    melds: usize,
) -> bool {
    if melds > MAX_MELDS || concealed.len() != 3 * (MAX_MELDS - melds) + 2 {
        return false;
    }
    let counts = *concealed.as_array();
    (0..KINDS).any(|eye| {
        if counts[eye] < 2 {
            return false;
        }
        let mut rest = counts;
        rest[eye] -= 2;
        decomposes_into_sets(&mut rest)
    })
}

/// Tiles that would complete `concealed`, which must be one short of a win.
pub fn waits(
    concealed: &TileCounts,
    melds: usize,
) -> Vec<Tile> {
    Tile::all().filter(|&t| concealed.get(t) < COPIES && is_complete(&concealed.with(t), melds)).collect()
}

/// Greedily strips sets from the lowest tile upward.
///
/// Taking a pong first is safe: three identical chows from the lowest tile
/// can always be regrouped as three pongs.
fn decomposes_into_sets(counts: &mut [u8; KINDS]) -> bool {
    for i in 0..KINDS {
        if counts[i] >= 3 {
            counts[i] -= 3;
        }
        let n = counts[i];
        if n == 0 {
            continue;
        }
        let chow_fits = i < SUITED_KINDS && i % 9 <= 6 && counts[i + 1] >= n && counts[i + 2] >= n;
        if !chow_fits {
            return false;
        }
        counts[i] = 0;
        counts[i + 1] -= n;
        counts[i + 2] -= n;
    }
    true
}

impl Hand {
    pub fn is_winning(&self) -> bool {
        is_complete(self.concealed(), self.meld_count())
    }

    /// Whether adding `tile` (drawn or claimed) completes the hand.
    pub fn wins_with(
        &self,
        tile: Tile,
    ) -> bool {
        self.concealed().get(tile) < COPIES && is_complete(&self.concealed().with(tile), self.meld_count())
    }

    pub fn waits(&self) -> Vec<Tile> {
        waits(self.concealed(), self.meld_count())
    }
}
