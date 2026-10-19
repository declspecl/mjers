//! Batch simulation, comparison, and reports.

pub mod runner;
pub mod stats;

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
