//! Declared sets that sit beside a hand.

use mjers_tile::counts::TileCounts;
use mjers_tile::tile::Tile;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KongKind {
    /// Claimed from a discard, or an exposed pong upgraded with a drawn tile.
    Exposed,
    /// All four tiles drawn by the owner (am kong).
    Concealed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Meld {
    /// Identified by its lowest tile.
    Chow(Tile),
    Pong(Tile),
    Kong(Tile, KongKind),
}

impl Meld {
    /// A chow starting at `lowest`, if three consecutive tiles exist in its suit.
    pub fn chow(lowest: Tile) -> Option<Meld> {
        lowest.chow_successor()?.chow_successor()?;
        Some(Meld::Chow(lowest))
    }

    pub const fn tile(self) -> Tile {
        match self {
            Meld::Chow(t) | Meld::Pong(t) | Meld::Kong(t, _) => t,
        }
    }

    pub const fn is_concealed(self) -> bool {
        matches!(self, Meld::Kong(_, KongKind::Concealed))
    }

    /// The physical tiles in the meld.
    pub fn tiles(self) -> TileCounts {
        match self {
            Meld::Chow(t) => {
                let second = t.chow_successor().expect("chow is validated on construction");
                let third = second.chow_successor().expect("chow is validated on construction");
                [t, second, third].into_iter().collect()
            },
            Meld::Pong(t) => std::iter::repeat_n(t, 3).collect(),
            Meld::Kong(t, _) => std::iter::repeat_n(t, 4).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use mjers_tile::tile::{
        Honor,
        Suit,
    };

    use super::*;

    #[test]
    fn chow_requires_three_in_suit() {
        assert!(Meld::chow(Tile::suited(Suit::Tiao, 7)).is_some());
        assert!(Meld::chow(Tile::suited(Suit::Tiao, 8)).is_none());
        assert!(Meld::chow(Tile::honor(Honor::East)).is_none());
    }

    #[test]
    fn tiles_lists_physical_tiles() {
        let chow = Meld::chow(Tile::suited(Suit::Wan, 3)).unwrap();
        assert_eq!(chow.tiles().to_string(), "345m");
        assert_eq!(Meld::Kong(Tile::honor(Honor::Red), KongKind::Concealed).tiles().to_string(), "7777z");
    }
}
