//! A multiset of tiles stored as per-kind counts.
//!
//! Hands, discards, and visible tiles all use this representation, so hand
//! analysis is array arithmetic rather than searching a `Vec<Tile>`.

#[cfg(test)]
mod tests;

use std::fmt;
use std::str::FromStr;

use crate::tile::{
    Honor,
    KINDS,
    Suit,
    Tile,
    tile_from_parts,
};

pub const COPIES: u8 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCounts([u8; KINDS]);

impl Default for TileCounts {
    fn default() -> Self {
        TileCounts::new()
    }
}

impl TileCounts {
    pub const fn new() -> Self {
        TileCounts([0; KINDS])
    }

    pub const fn get(
        &self,
        tile: Tile,
    ) -> u8 {
        self.0[tile.index()]
    }

    pub fn add(
        &mut self,
        tile: Tile,
    ) {
        self.0[tile.index()] = self.0[tile.index()].saturating_add(1);
    }

    /// Removes one copy, returning false if none was present.
    pub fn remove(
        &mut self,
        tile: Tile,
    ) -> bool {
        let count = &mut self.0[tile.index()];
        if *count == 0 {
            return false;
        }
        *count -= 1;
        true
    }

    pub fn with(
        mut self,
        tile: Tile,
    ) -> Self {
        self.add(tile);
        self
    }

    pub fn len(&self) -> usize {
        self.0.iter().map(|&n| n as usize).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&n| n == 0)
    }

    pub const fn as_array(&self) -> &[u8; KINDS] {
        &self.0
    }

    /// Distinct tiles present, in index order.
    pub fn kinds(&self) -> impl Iterator<Item = Tile> + '_ {
        Tile::all().filter(|&t| self.get(t) > 0)
    }

    /// Every tile with multiplicity, in index order.
    pub fn tiles(&self) -> impl Iterator<Item = Tile> + '_ {
        self.kinds().flat_map(|t| std::iter::repeat_n(t, self.get(t) as usize))
    }
}

impl FromIterator<Tile> for TileCounts {
    fn from_iter<I: IntoIterator<Item = Tile>>(iter: I) -> Self {
        let mut counts = TileCounts::new();
        for tile in iter {
            counts.add(tile);
        }
        counts
    }
}

impl fmt::Display for TileCounts {
    /// Grouped notation, e.g. `123m55p7z`.
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        let groups = Suit::ALL.map(|s| (s.letter(), s as usize * 9));
        let honors = ('z', Tile::honor(Honor::East).index());
        for (letter, start) in groups.into_iter().chain([honors]) {
            let len = if letter == 'z' { Honor::ALL.len() } else { 9 };
            let mut any = false;
            for offset in 0..len {
                for _ in 0..self.0[start + offset] {
                    write!(f, "{}", offset + 1)?;
                    any = true;
                }
            }
            if any {
                write!(f, "{letter}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for TileCounts {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "TileCounts({self})")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    InvalidTile(String),
    MissingSuit,
    TooManyCopies(Tile),
}

impl fmt::Display for ParseError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            ParseError::InvalidTile(t) => write!(f, "invalid tile {t:?}"),
            ParseError::MissingSuit => write!(f, "digits without a trailing suit letter"),
            ParseError::TooManyCopies(t) => write!(f, "more than {COPIES} copies of {t}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl FromStr for TileCounts {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut counts = TileCounts::new();
        let mut digits = Vec::new();
        for c in s.chars() {
            if let Some(d) = c.to_digit(10) {
                digits.push(d as u8);
                continue;
            }
            for &d in &digits {
                let tile = tile_from_parts(d, c).ok_or_else(|| ParseError::InvalidTile(format!("{d}{c}")))?;
                if counts.get(tile) == COPIES {
                    return Err(ParseError::TooManyCopies(tile));
                }
                counts.add(tile);
            }
            digits.clear();
        }
        if digits.is_empty() { Ok(counts) } else { Err(ParseError::MissingSuit) }
    }
}
