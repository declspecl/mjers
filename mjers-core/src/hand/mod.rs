//! A player's tiles: concealed tiles, declared melds, and bonus tiles.
//!
//! `Hand` is `Copy` and allocation-free so rollouts can clone it freely.

pub mod meld;
pub mod shanten;
pub mod ukeire;
pub mod wait;
pub mod win;

#[cfg(test)]
mod tests;

use std::fmt;

use crate::hand::meld::{
    KongKind,
    Meld,
};
use crate::tile::counts::TileCounts;
use crate::tile::{
    Bonus,
    Tile,
};

pub const MAX_MELDS: usize = 4;
/// Concealed tiles plus three per meld when it is not the owner's turn.
pub const HAND_SIZE: usize = 13;

/// The flowers and seasons a player has set aside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BonusSet(u8);

impl BonusSet {
    const fn bit(bonus: Bonus) -> u8 {
        match bonus {
            Bonus::Flower(n) => 1 << (n - 1),
            Bonus::Season(n) => 1 << (n + 3),
        }
    }

    pub fn insert(
        &mut self,
        bonus: Bonus,
    ) {
        self.0 |= Self::bit(bonus);
    }

    pub const fn contains(
        self,
        bonus: Bonus,
    ) -> bool {
        self.0 & Self::bit(bonus) != 0
    }

    pub const fn len(self) -> u8 {
        self.0.count_ones() as u8
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Bonus> {
        Bonus::all().filter(move |&b| self.contains(b))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandError {
    MissingTiles(Tile),
    TooManyMelds,
    NotAChow(Tile),
    NoPongToUpgrade(Tile),
}

impl fmt::Display for HandError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            HandError::MissingTiles(t) => write!(f, "not enough concealed {t}"),
            HandError::TooManyMelds => write!(f, "a hand holds at most {MAX_MELDS} melds"),
            HandError::NotAChow(t) => write!(f, "no chow starts at {t}"),
            HandError::NoPongToUpgrade(t) => write!(f, "no exposed pong of {t}"),
        }
    }
}

impl std::error::Error for HandError {}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hand {
    concealed: TileCounts,
    melds: [Option<Meld>; MAX_MELDS],
    bonus: BonusSet,
}

impl Hand {
    pub fn new(concealed: TileCounts) -> Self {
        Hand {
            concealed,
            melds: [None; MAX_MELDS],
            bonus: BonusSet::default(),
        }
    }

    /// A hand mid-play; at most four melds.
    pub fn from_parts(
        concealed: TileCounts,
        melds: impl IntoIterator<Item = Meld>,
        bonus: BonusSet,
    ) -> Result<Self, HandError> {
        let mut hand = Hand {
            concealed,
            melds: [None; MAX_MELDS],
            bonus,
        };
        for meld in melds {
            hand.ensure_meld_slot()?;
            hand.push_meld(meld);
        }
        Ok(hand)
    }

    pub const fn concealed(&self) -> &TileCounts {
        &self.concealed
    }

    pub fn melds(&self) -> impl Iterator<Item = Meld> + '_ {
        self.melds.iter().flatten().copied()
    }

    pub fn meld_count(&self) -> usize {
        self.melds.iter().flatten().count()
    }

    pub const fn bonus(&self) -> BonusSet {
        self.bonus
    }

    /// Tiles counted toward hand size: concealed plus three per meld.
    pub fn size(&self) -> usize {
        self.concealed.len() + 3 * self.meld_count()
    }

    /// True after drawing or claiming, when the player must discard or win.
    pub fn is_awaiting_discard(&self) -> bool {
        self.size() == HAND_SIZE + 1
    }

    pub fn add_tile(
        &mut self,
        tile: Tile,
    ) {
        self.concealed.add(tile);
    }

    pub fn add_bonus(
        &mut self,
        bonus: Bonus,
    ) {
        self.bonus.insert(bonus);
    }

    pub fn discard(
        &mut self,
        tile: Tile,
    ) -> Result<(), HandError> {
        self.take(tile, 1)
    }

    /// Pong `claimed` with two concealed copies.
    pub fn claim_pong(
        &mut self,
        claimed: Tile,
    ) -> Result<Meld, HandError> {
        self.ensure_meld_slot()?;
        self.take(claimed, 2)?;
        Ok(self.push_meld(Meld::Pong(claimed)))
    }

    /// Exposed kong of `claimed` with three concealed copies.
    pub fn claim_kong(
        &mut self,
        claimed: Tile,
    ) -> Result<Meld, HandError> {
        self.ensure_meld_slot()?;
        self.take(claimed, 3)?;
        Ok(self.push_meld(Meld::Kong(claimed, KongKind::Exposed)))
    }

    /// Chow starting at `lowest` completed by `claimed` from the discard.
    pub fn claim_chow(
        &mut self,
        lowest: Tile,
        claimed: Tile,
    ) -> Result<Meld, HandError> {
        self.ensure_meld_slot()?;
        let meld = Meld::chow(lowest).ok_or(HandError::NotAChow(lowest))?;
        let mut needed = meld.tiles();
        if !needed.remove(claimed) {
            return Err(HandError::NotAChow(lowest));
        }
        if let Some(missing) = needed.kinds().find(|&t| self.concealed.get(t) < needed.get(t)) {
            return Err(HandError::MissingTiles(missing));
        }
        for tile in needed.tiles() {
            self.concealed.remove(tile);
        }
        Ok(self.push_meld(meld))
    }

    /// Concealed kong from four concealed copies.
    pub fn declare_concealed_kong(
        &mut self,
        tile: Tile,
    ) -> Result<Meld, HandError> {
        self.ensure_meld_slot()?;
        self.take(tile, 4)?;
        Ok(self.push_meld(Meld::Kong(tile, KongKind::Concealed)))
    }

    /// Adds a concealed fourth tile to an exposed pong.
    pub fn upgrade_kong(
        &mut self,
        tile: Tile,
    ) -> Result<Meld, HandError> {
        let slot = self.melds.iter().position(|m| *m == Some(Meld::Pong(tile))).ok_or(HandError::NoPongToUpgrade(tile))?;
        self.take(tile, 1)?;
        let meld = Meld::Kong(tile, KongKind::Exposed);
        self.melds[slot] = Some(meld);
        Ok(meld)
    }

    fn take(
        &mut self,
        tile: Tile,
        copies: u8,
    ) -> Result<(), HandError> {
        if self.concealed.get(tile) < copies {
            return Err(HandError::MissingTiles(tile));
        }
        for _ in 0..copies {
            self.concealed.remove(tile);
        }
        Ok(())
    }

    fn ensure_meld_slot(&self) -> Result<(), HandError> {
        if self.meld_count() < MAX_MELDS { Ok(()) } else { Err(HandError::TooManyMelds) }
    }

    fn push_meld(
        &mut self,
        meld: Meld,
    ) -> Meld {
        if let Some(slot) = self.melds.iter_mut().find(|m| m.is_none()) {
            *slot = Some(meld);
        }
        meld
    }
}

impl fmt::Debug for Hand {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "Hand({self})")
    }
}

impl fmt::Display for Hand {
    /// Concealed notation followed by each meld, e.g. `123m55p [777z] [3333s*]`.
    /// `*` marks a concealed kong.
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "{}", self.concealed)?;
        for meld in self.melds() {
            let mark = if meld.is_concealed() { "*" } else { "" };
            write!(f, " [{}{mark}]", meld.tiles())?;
        }
        Ok(())
    }
}
