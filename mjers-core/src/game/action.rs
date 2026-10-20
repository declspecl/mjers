//! Player decisions and the phases that ask for them.

use std::fmt;

use serde::{
    Deserialize,
    Serialize,
};

use crate::hand::HandError;
use crate::rules::RulesError;
use crate::rules::seat::Seat;
use crate::rules::settlement::HandResult;
use crate::tile::Tile;

/// Everything a player can choose. Draws and flower replacements are automatic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

/// Most legal actions a decision can offer: a turn has at most 14 discards,
/// a win, 3 concealed kongs, and 4 kong upgrades.
pub const MAX_ACTIONS: usize = 24;

/// The legal actions at one decision, stored inline.
#[derive(Clone, Copy)]
pub struct Actions {
    items: [Action; MAX_ACTIONS],
    len: u8,
}

impl Actions {
    pub const fn new() -> Actions {
        Actions {
            items: [Action::Pass; MAX_ACTIONS],
            len: 0,
        }
    }

    /// Adds `action`; the bound above means a legal decision never fills the list.
    pub fn push(
        &mut self,
        action: Action,
    ) {
        if let Some(slot) = self.items.get_mut(self.len as usize) {
            *slot = action;
            self.len += 1;
        }
    }
}

impl Default for Actions {
    fn default() -> Self {
        Actions::new()
    }
}

impl std::ops::Deref for Actions {
    type Target = [Action];

    fn deref(&self) -> &[Action] {
        &self.items[..self.len as usize]
    }
}

impl Extend<Action> for Actions {
    fn extend<I: IntoIterator<Item = Action>>(
        &mut self,
        actions: I,
    ) {
        for action in actions {
            self.push(action);
        }
    }
}

impl fmt::Debug for Actions {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl PartialEq for Actions {
    fn eq(
        &self,
        other: &Actions,
    ) -> bool {
        **self == **other
    }
}

impl<const N: usize> PartialEq<[Action; N]> for Actions {
    fn eq(
        &self,
        other: &[Action; N],
    ) -> bool {
        **self == *other
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameError {
    HandOver,
    NotAsked(Seat),
    Illegal {
        seat: Seat,
        action: Action,
    },
    Rules(RulesError),
    /// A position or wall that cannot be played from.
    InvalidPosition(&'static str),
    /// The engine's own bookkeeping disagreed with a hand; a bug, reported
    /// rather than crashing.
    Inconsistent(HandError),
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
            GameError::Rules(error) => write!(f, "{error}"),
            GameError::InvalidPosition(reason) => write!(f, "invalid position: {reason}"),
            GameError::Inconsistent(error) => write!(f, "engine bookkeeping error: {error}"),
        }
    }
}

impl From<RulesError> for GameError {
    fn from(error: RulesError) -> Self {
        GameError::Rules(error)
    }
}

impl From<HandError> for GameError {
    fn from(error: HandError) -> Self {
        GameError::Inconsistent(error)
    }
}

impl std::error::Error for GameError {}
