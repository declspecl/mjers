//! Melds, win detection, shanten, and ukeire.

pub mod hand;
pub mod meld;
pub mod shanten;
pub mod ukeire;
pub mod win;

pub use hand::{
    BonusSet,
    Hand,
    HandError,
};
pub use meld::{
    KongKind,
    Meld,
};
pub use shanten::shanten;
pub use ukeire::{
    DiscardOption,
    Improvement,
    Ukeire,
};
pub use win::{
    is_complete,
    waits,
};
