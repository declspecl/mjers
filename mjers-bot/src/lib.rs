//! Policies that play from observations.

pub mod defense;
pub mod heuristic;
pub mod policy;

pub use heuristic::{
    DefensiveBot,
    EfficiencyBot,
};
pub use policy::{
    Policy,
    RandomBot,
    decide,
    play_out,
};
