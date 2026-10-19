//! Player decisions and the phases that ask for them.

use std::fmt;

use mjers_rules::{
    HandResult,
    Seat,
};
use mjers_tile::Tile;

/// Everything a player can choose. Draws and flower replacements are automatic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// On your turn: discard a tile.
    Discard(Tile),
    /// On your turn: declare a win with the tile you drew.
    SelfDrawWin,
    /// On your turn: declare four concealed copies as a concealed kong.
    ConcealedKong(Tile),
    /// On your turn: add the fourth tile to your exposed pong.
    UpgradeKong(Tile),
    /// In a claim window: decline.
    Pass,
    /// In a claim window: chow starting at this tile.
    Chow(Tile),
    /// In a claim window: pong the offered tile.
    Pong,
    /// In a claim window: exposed kong with the offered tile.
    Kong,
    /// In a claim window: win with the offered tile.
    Win,
}

impl fmt::Display for Action {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            Action::Discard(t) => write!(f, "discard {t}"),
            Action::SelfDrawWin => write!(f, "self-draw win"),
            Action::ConcealedKong(t) => write!(f, "concealed kong {t}"),
            Action::UpgradeKong(t) => write!(f, "upgrade kong {t}"),
            Action::Pass => write!(f, "pass"),
            Action::Chow(t) => write!(f, "chow from {t}"),
            Action::Pong => write!(f, "pong"),
            Action::Kong => write!(f, "kong"),
            Action::Win => write!(f, "win"),
        }
    }
}

/// Where a claimable tile came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Offer {
    Discard,
    /// The tile being added to an exposed pong (robbing the kong).
    KongUpgrade,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// `seat` holds a full hand. After a chow or pong the only choice is a discard.
    Turn {
        seat: Seat,
        after_claim: bool,
    },
    /// Seats with a possible claim on `tile` decide; others are not asked.
    Claim {
        tile: Tile,
        from: Seat,
        offer: Offer,
    },
    Ended(HandResult),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameError {
    HandOver,
    NotAsked(Seat),
    Illegal { seat: Seat, action: Action },
}

impl fmt::Display for GameError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            GameError::HandOver => write!(f, "the hand is over"),
            GameError::NotAsked(seat) => write!(f, "{seat} has no decision to make"),
            GameError::Illegal { seat, action } => write!(f, "{seat} cannot {action} now"),
        }
    }
}

impl std::error::Error for GameError {}
