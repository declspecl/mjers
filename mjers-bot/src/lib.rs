//! Policies that play from observations.

pub mod heuristic;
pub mod policy;

pub use heuristic::EfficiencyBot;
pub use policy::{
    Policy,
    RandomBot,
    decide,
    play_out,
};
