//! The wall: live draws from the front, replacement draws from the back.
//!
//! The ruleset's flower wall is held back from live draws. Replacements come
//! from the back and the flower wall is topped up from the live wall, so live
//! draws simply stop once only the reserved tiles remain.

#[cfg(test)]
mod tests;

use mjers_tile::{
    Bonus,
    Tile,
    counts::COPIES,
};

use crate::rng::Rng;

pub const WALL_SIZE: usize = 144;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WallTile {
    Tile(Tile),
    Bonus(Bonus),
}

/// Every tile in a set: four of each playable kind, then the eight bonus tiles.
pub fn full_set() -> [WallTile; WALL_SIZE] {
    let mut tiles = Tile::all()
        .flat_map(|t| std::iter::repeat_n(WallTile::Tile(t), COPIES as usize))
        .chain(Bonus::all().map(WallTile::Bonus));
    std::array::from_fn(|_| tiles.next().expect("144 tiles"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wall {
    tiles: [WallTile; WALL_SIZE],
    front: u8,
    back: u8,
    reserved: u8,
}

impl Wall {
    pub fn shuffled(
        rng: &mut Rng,
        reserved: usize,
    ) -> Wall {
        let mut tiles = full_set();
        rng.shuffle(&mut tiles);
        Wall::from_order(tiles, reserved)
    }

    /// A wall in exactly this order; the first tile is drawn first.
    pub fn from_order(
        tiles: [WallTile; WALL_SIZE],
        reserved: usize,
    ) -> Wall {
        assert!(reserved <= WALL_SIZE);
        Wall {
            tiles,
            front: 0,
            back: WALL_SIZE as u8,
            reserved: reserved as u8,
        }
    }

    pub const fn remaining(&self) -> usize {
        (self.back - self.front) as usize
    }

    /// Normal draws left before the hand ends with no winner.
    pub const fn live_remaining(&self) -> usize {
        self.remaining().saturating_sub(self.reserved as usize)
    }

    pub fn draw(&mut self) -> Option<WallTile> {
        if self.live_remaining() == 0 {
            return None;
        }
        let tile = self.tiles[self.front as usize];
        self.front += 1;
        Some(tile)
    }

    pub fn draw_replacement(&mut self) -> Option<WallTile> {
        if self.remaining() == 0 {
            return None;
        }
        self.back -= 1;
        Some(self.tiles[self.back as usize])
    }

    /// Tiles not yet drawn, in draw order then replacement order. Hidden
    /// information: only for tests and analysis tools, never for players.
    pub fn undrawn(&self) -> &[WallTile] {
        &self.tiles[self.front as usize..self.back as usize]
    }
}
