//! Deterministic mahjong engine, hand analysis, policies, and advice.
//!
//! This library runs natively and in WebAssembly. Batch simulation and CLI
//! integration live in separate packages.

pub mod advisor;
pub mod bot;
pub mod game;
pub mod hand;
pub mod rules;
pub mod tile;
