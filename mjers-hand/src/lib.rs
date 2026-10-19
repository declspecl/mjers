//! Melds, win detection, shanten, and ukeire.

pub mod hand;
pub mod meld;

pub use hand::{
    BonusSet,
    Hand,
    HandError,
};
pub use meld::{
    KongKind,
    Meld,
};
