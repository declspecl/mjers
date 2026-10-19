//! Standard-form win detection: four sets plus an eye.
//!
//! Special hands (seven pairs, thirteen orphans) are not wins in any supported
//! ruleset, so only the standard form is recognised.

#[cfg(test)]
mod tests;

use mjers_tile::counts::{
    COPIES,
    TileCounts,
};
use mjers_tile::tile::{
    KINDS,
    SUITED_KINDS,
    Tile,
};

use crate::hand::{
    Hand,
    MAX_MELDS,
};
use crate::meld::Meld;

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

/// One way to split a complete hand's concealed tiles: the eye and the sets.
///
/// Where several splits exist, the lowest workable eye is chosen, then
/// pongs are preferred (111222333 splits as three pongs).
pub fn decompose(
    concealed: &TileCounts,
    melds: usize,
) -> Option<(Tile, Vec<Meld>)> {
    if melds > MAX_MELDS || concealed.len() != 3 * (MAX_MELDS - melds) + 2 {
        return None;
    }
    concealed.kinds().filter(|&t| concealed.get(t) >= 2).find_map(|eye| {
        let mut rest = *concealed.as_array();
        rest[eye.index()] -= 2;
        collect_sets(&mut rest).map(|sets| (eye, sets))
    })
}

/// The pong-first greedy split of `counts` into sets, if one exists.
fn collect_sets(counts: &mut [u8; KINDS]) -> Option<Vec<Meld>> {
    let mut sets = Vec::new();
    for i in 0..KINDS {
        let tile = Tile::from_index(i).expect("in range");
        if counts[i] >= 3 {
            counts[i] -= 3;
            sets.push(Meld::Pong(tile));
        }
        let n = counts[i];
        if n == 0 {
            continue;
        }
        if !(i < SUITED_KINDS && i % 9 <= 6 && counts[i + 1] >= n && counts[i + 2] >= n) {
            return None;
        }
        counts[i] = 0;
        counts[i + 1] -= n;
        counts[i + 2] -= n;
        sets.extend(std::iter::repeat_n(Meld::Chow(tile), n as usize));
    }
    Some(sets)
}

/// Whether `tiles` form exactly `sets` sets with no eye.
pub(crate) fn forms_sets(
    tiles: &TileCounts,
    sets: usize,
) -> bool {
    tiles.len() == 3 * sets && decomposes_into_sets(&mut tiles.as_array().clone())
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
