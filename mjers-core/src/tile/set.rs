//! A set of tile kinds as a bitset.

use crate::tile::{
    KINDS,
    Tile,
};

/// Which of the 34 kinds are present, one bit each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TileSet(u64);

impl TileSet {
    pub const EMPTY: TileSet = TileSet(0);

    pub fn insert(
        &mut self,
        tile: Tile,
    ) {
        self.0 |= 1 << tile.index();
    }

    pub const fn contains(
        self,
        tile: Tile,
    ) -> bool {
        self.0 & (1 << tile.index()) != 0
    }

    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Tile> {
        (0..KINDS).filter(move |&i| self.0 & (1 << i) != 0).filter_map(Tile::from_index)
    }
}

impl FromIterator<Tile> for TileSet {
    fn from_iter<I: IntoIterator<Item = Tile>>(tiles: I) -> Self {
        let mut set = TileSet::EMPTY;
        for tile in tiles {
            set.insert(tile);
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membership_and_order() {
        let tiles: Vec<Tile> = ["7z", "1m", "9s", "1m"].iter().map(|t| t.parse().unwrap()).collect();
        let set: TileSet = tiles.into_iter().collect();
        assert_eq!(set.len(), 3);
        assert!(set.contains("9s".parse().unwrap()) && !set.contains("2m".parse().unwrap()));
        assert_eq!(set.iter().map(|t| t.to_string()).collect::<Vec<_>>(), ["1m", "9s", "7z"]);
        assert!(TileSet::EMPTY.is_empty());
    }
}
