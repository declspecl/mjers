//! Classifying how the winning tile completed a hand.

use mjers_tile::counts::TileCounts;
use mjers_tile::tile::Tile;
use serde::{
    Deserialize,
    Serialize,
};

use crate::hand::MAX_MELDS;
use crate::meld::Meld;
use crate::win::{
    forms_sets,
    is_complete,
};

/// The role a winning tile can play. Ordered from most to least flexible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitShape {
    /// Completed a two-sided sequence, like 45 waiting on 3 or 6.
    Ryanmen,
    /// Turned one of two pairs into a pong.
    Shanpon,
    /// Filled the middle of a sequence, like 46 waiting on 5.
    Kanchan,
    /// Completed an edge sequence, like 12 waiting on 3.
    Penchan,
    /// Paired a lone tile as the eye.
    Tanki,
}

/// Every role `tile` could have played in `complete` (concealed tiles
/// including `tile`, beside `melds` declared melds). Empty if not complete.
pub fn wait_shapes(
    complete: &TileCounts,
    melds: usize,
    tile: Tile,
) -> Vec<WaitShape> {
    let mut shapes = Vec::new();
    let without = |n: u8| {
        let mut rest = *complete;
        (0..n).all(|_| rest.remove(tile)).then_some(rest)
    };
    if let Some(rest) = without(2)
        && forms_sets(&rest, MAX_MELDS.saturating_sub(melds))
    {
        shapes.push(WaitShape::Tanki);
    }
    if let Some(rest) = without(3)
        && is_complete(&rest, melds + 1)
    {
        shapes.push(WaitShape::Shanpon);
    }
    let index = tile.index();
    for lowest in (index.saturating_sub(2)..=index).filter_map(Tile::from_index) {
        let Some(chow) = Meld::chow(lowest).filter(|_| lowest.suit() == tile.suit()) else {
            continue;
        };
        let needed = chow.tiles();
        if needed.get(tile) == 0 {
            continue;
        }
        let mut rest = *complete;
        if !needed.tiles().all(|t| rest.remove(t)) || !is_complete(&rest, melds + 1) {
            continue;
        }
        let Some(low) = lowest.rank() else { continue };
        let position = index - lowest.index();
        shapes.push(match position {
            1 => WaitShape::Kanchan,
            2 if low == 1 => WaitShape::Penchan,
            0 if low == 7 => WaitShape::Penchan,
            _ => WaitShape::Ryanmen,
        });
    }
    shapes.sort();
    shapes.dedup();
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shapes(
        hand: &str,
        tile: &str,
    ) -> Vec<WaitShape> {
        wait_shapes(&hand.parse().unwrap(), 0, tile.parse().unwrap())
    }

    #[test]
    fn classifies_each_shape() {
        assert_eq!(shapes("123456m789p11z345s", "3s"), [WaitShape::Ryanmen]);
        assert_eq!(shapes("123456m789p11z345s", "4s"), [WaitShape::Kanchan]);
        assert_eq!(shapes("123456m789p11z123s", "3s"), [WaitShape::Penchan]);
        assert_eq!(shapes("123456m789p11z789s", "7s"), [WaitShape::Penchan]);
        assert_eq!(shapes("123456m789p345s11z", "1z"), [WaitShape::Tanki]);
        assert_eq!(shapes("123456m789p11z222z", "2z"), [WaitShape::Shanpon]);
    }

    #[test]
    fn ambiguous_tiles_report_every_role() {
        // 3455m waited on 2m/5m as 34 + 55, or on 5m as 345 + 5.
        assert_eq!(shapes("34555m456p789s111z", "5m"), [WaitShape::Ryanmen, WaitShape::Tanki]);
    }

    #[test]
    fn incomplete_hands_have_no_shape() {
        assert!(shapes("123456m789p11z346s", "3s").is_empty());
    }

    #[test]
    fn melds_count_toward_sets() {
        let complete: TileCounts = "345s11z".parse().unwrap();
        assert_eq!(wait_shapes(&complete, 3, "3s".parse().unwrap()), [WaitShape::Ryanmen]);
        assert_eq!(wait_shapes(&complete, 3, "1z".parse().unwrap()), [WaitShape::Tanki]);
    }
}
