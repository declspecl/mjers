//! Compact tile encoding.
//!
//! The 34 playable kinds are indexed 0..34: Wan 1-9 (`m`), Tong 1-9 (`p`),
//! Tiao 1-9 (`s`), then honors East, South, West, North, White, Green, Red
//! (`1z`..`7z`). Flowers and seasons never enter a hand and are kept separate.

#[cfg(test)]
mod tests;

use std::fmt;
use std::str::FromStr;

pub const KINDS: usize = 34;
pub const SUITED_KINDS: usize = 27;
pub const BONUS_KINDS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Suit {
    Wan,
    Tong,
    Tiao,
}

impl Suit {
    pub const ALL: [Suit; 3] = [Suit::Wan, Suit::Tong, Suit::Tiao];

    pub const fn letter(self) -> char {
        match self {
            Suit::Wan => 'm',
            Suit::Tong => 'p',
            Suit::Tiao => 's',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Honor {
    East,
    South,
    West,
    North,
    White,
    Green,
    Red,
}

impl Honor {
    pub const ALL: [Honor; 7] = [Honor::East, Honor::South, Honor::West, Honor::North, Honor::White, Honor::Green, Honor::Red];
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tile(u8);

impl Tile {
    pub const fn from_index(index: usize) -> Option<Tile> {
        if index < KINDS { Some(Tile(index as u8)) } else { None }
    }

    /// Panics if `rank` is outside 1..=9.
    pub const fn suited(
        suit: Suit,
        rank: u8,
    ) -> Tile {
        assert!(rank >= 1 && rank <= 9);
        Tile(suit as u8 * 9 + rank - 1)
    }

    pub const fn honor(honor: Honor) -> Tile {
        Tile(SUITED_KINDS as u8 + honor as u8)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub const fn suit(self) -> Option<Suit> {
        match self.0 / 9 {
            0 => Some(Suit::Wan),
            1 => Some(Suit::Tong),
            2 => Some(Suit::Tiao),
            _ => None,
        }
    }

    pub const fn rank(self) -> Option<u8> {
        if self.is_honor() { None } else { Some(self.0 % 9 + 1) }
    }

    pub const fn honor_kind(self) -> Option<Honor> {
        if self.is_honor() { Some(Honor::ALL[self.0 as usize - SUITED_KINDS]) } else { None }
    }

    pub const fn is_honor(self) -> bool {
        self.0 as usize >= SUITED_KINDS
    }

    /// The next tile in a chow sequence, if any.
    pub const fn chow_successor(self) -> Option<Tile> {
        match self.rank() {
            Some(rank) if rank < 9 => Some(Tile(self.0 + 1)),
            _ => None,
        }
    }

    pub fn all() -> impl Iterator<Item = Tile> {
        (0..KINDS as u8).map(Tile)
    }
}

impl fmt::Display for Tile {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self.suit() {
            Some(suit) => write!(f, "{}{}", self.0 % 9 + 1, suit.letter()),
            None => write!(f, "{}z", self.0 as usize - SUITED_KINDS + 1),
        }
    }
}

impl fmt::Debug for Tile {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTileError(pub String);

impl fmt::Display for ParseTileError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "invalid tile: {:?}", self.0)
    }
}

impl std::error::Error for ParseTileError {}

/// Builds a tile from a digit and a suit letter, as in `5m` or `7z`.
pub(crate) fn tile_from_parts(
    digit: u8,
    letter: char,
) -> Option<Tile> {
    let suit = match letter {
        'm' => Suit::Wan,
        'p' => Suit::Tong,
        's' => Suit::Tiao,
        'z' if (1..=7).contains(&digit) => return Some(Tile::honor(Honor::ALL[digit as usize - 1])),
        _ => return None,
    };
    (1..=9).contains(&digit).then(|| Tile::suited(suit, digit))
}

impl FromStr for Tile {
    type Err = ParseTileError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseTileError(s.to_string());
        let mut chars = s.chars();
        let (Some(digit), Some(letter), None) = (chars.next(), chars.next(), chars.next()) else {
            return Err(err());
        };
        let digit = digit.to_digit(10).ok_or_else(err)? as u8;
        tile_from_parts(digit, letter).ok_or_else(err)
    }
}

/// Flowers and seasons, numbered 1..=4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bonus {
    Flower(u8),
    Season(u8),
}

impl Bonus {
    pub fn all() -> impl Iterator<Item = Bonus> {
        (1..=4).map(Bonus::Flower).chain((1..=4).map(Bonus::Season))
    }
}
