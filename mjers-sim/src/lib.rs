//! Batch simulation, comparison, and reports.

pub mod compare;
pub mod runner;
pub mod stats;

pub use compare::{
    Comparison,
    compare,
};
pub use runner::{
    Seeds,
    Table,
    play_hand,
    run,
};
pub use stats::{
    Metrics,
    Stats,
    Summary,
    simulate,
};
