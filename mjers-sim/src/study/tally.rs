//! Outcome counters for a group of seats or hands.

use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    pub count: u64,
    pub wins: u64,
    pub units: i64,
    /// Sum of live winning tiles at the moment of becoming ready.
    pub live: u64,
    /// Of the wins: self-drawn.
    pub self_draws: u64,
    /// Summed over wins: discards at the table from becoming ready to winning.
    pub table_turns: u64,
    /// Summed over wins: the winner's own draws from becoming ready to winning.
    pub own_draws: u64,
}

impl Tally {
    pub(super) fn add(
        &mut self,
        won: bool,
        units: i32,
        live: u32,
    ) {
        self.count += 1;
        self.wins += won as u64;
        self.units += units as i64;
        self.live += live as u64;
    }

    pub(super) fn merge(
        &mut self,
        o: &Tally,
    ) {
        self.count += o.count;
        self.wins += o.wins;
        self.units += o.units;
        self.live += o.live;
        self.self_draws += o.self_draws;
        self.table_turns += o.table_turns;
        self.own_draws += o.own_draws;
    }

    pub fn win_rate(&self) -> f64 {
        self.wins as f64 / self.count.max(1) as f64
    }

    pub fn mean_units(&self) -> f64 {
        self.units as f64 / self.count.max(1) as f64
    }

    pub fn mean_live(&self) -> f64 {
        self.live as f64 / self.count.max(1) as f64
    }

    pub fn self_draw_share(&self) -> f64 {
        self.self_draws as f64 / self.wins.max(1) as f64
    }

    pub fn mean_table_turns(&self) -> f64 {
        self.table_turns as f64 / self.wins.max(1) as f64
    }

    pub fn mean_own_draws(&self) -> f64 {
        self.own_draws as f64 / self.wins.max(1) as f64
    }
}
