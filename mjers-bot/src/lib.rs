//! Policies that play from observations.

pub mod policy;

pub use policy::{
    Policy,
    RandomBot,
    decide,
    play_out,
};
