//! Batch simulation, comparison, and reports.

pub mod compare;
pub mod features;
pub mod report;
pub mod runner;
pub mod stats;
pub mod study;
pub mod tune;

pub use compare::{
    Comparison,
    compare,
};
pub use features::{
    HandFeatures,
    Outcome,
    write_features,
};
pub use report::{
    ComparisonResult,
    Provenance,
    Report,
    SimulationResult,
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
