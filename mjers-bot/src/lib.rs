//! Policies that play from observations.

pub mod defense;
pub mod heuristic;
pub mod policy;
pub mod spec;

pub use heuristic::{
    ClaimRule,
    HeuristicBot,
    KongRule,
    Params,
};
pub use policy::{
    Policy,
    RandomBot,
    decide,
    play_out,
};
pub use spec::BotSpec;
