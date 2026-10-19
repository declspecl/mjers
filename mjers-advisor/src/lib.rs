//! Action ranking with explanations, refined by Monte Carlo playouts.

pub mod advice;
pub mod heuristic;
pub mod montecarlo;

pub use advice::{
    Advice,
    AdviceConfig,
    advise,
};
pub use heuristic::{
    Assessment,
    rank,
};
pub use montecarlo::{
    Estimate,
    estimate,
    sample_position,
};
