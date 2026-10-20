//! Duplicate comparison: two candidates play the same walls from the same
//! seats against the same field, so luck of the deal cancels out.

use std::sync::Arc;

use mjers_bot::spec::BotSpec;
use mjers_rules::ruleset::Ruleset;
use mjers_rules::seat::Seat;
use serde::{
    Deserialize,
    Serialize,
};

use crate::runner::{
    HandFailure,
    Seeds,
    Table,
    par_try_fold,
    play_hand,
};
use crate::stats::Stats;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub a: String,
    pub b: String,
    pub field: String,
    /// (seed, seat) pairs played by both candidates.
    pub pairs: u64,
    /// Mean of A's units minus B's units per hand, on identical deals.
    pub mean_difference: f64,
    pub stderr: f64,
    pub a_stats: Stats,
    pub b_stats: Stats,
}

impl Comparison {
    /// Standard errors between the candidates; beyond about 2 is unlikely to be luck.
    pub fn z(&self) -> f64 {
        if self.stderr == 0.0 { 0.0 } else { self.mean_difference / self.stderr }
    }
}

#[derive(Default)]
struct Paired {
    n: u64,
    sum: i64,
    sum_squared: u64,
    a: Stats,
    b: Stats,
}

impl Paired {
    fn merge(
        mut self,
        other: Paired,
    ) -> Paired {
        self.n += other.n;
        self.sum += other.sum;
        self.sum_squared += other.sum_squared;
        self.a.merge(&other.a);
        self.b.merge(&other.b);
        self
    }
}

/// Each seed is played eight times: A in each of the four seats, then B in
/// each, with `field` in the other seats.
pub fn compare(
    rules: &Ruleset,
    a: &BotSpec,
    b: &BotSpec,
    field: &BotSpec,
    seeds: Seeds,
) -> Result<Comparison, HandFailure> {
    let rules = Arc::new(rules.clone());
    let with = |candidate: &BotSpec, seat: Seat| -> Table { std::array::from_fn(|i| if i == seat.index() { candidate.clone() } else { field.clone() }) };
    let paired = par_try_fold(
        seeds.count * 4,
        |acc: &mut Paired, i| {
            let seed = seeds.start.wrapping_add(i / 4);
            let seat = Seat::ALL[(i % 4) as usize];
            let game_a = play_hand(&rules, &with(a, seat), seed)?;
            let game_b = play_hand(&rules, &with(b, seat), seed)?;
            if let (Some(units_a), Some(units_b)) = (game_a.settlement(), game_b.settlement()) {
                let diff = (units_a.total()[seat.index()] - units_b.total()[seat.index()]) as i64;
                acc.n += 1;
                acc.sum += diff;
                acc.sum_squared += (diff * diff) as u64;
                acc.a.record(&game_a, seat);
                acc.b.record(&game_b, seat);
            }
            Ok(())
        },
        Paired::merge,
    )?;

    let n = paired.n.max(1) as f64;
    let mean = paired.sum as f64 / n;
    let variance = (paired.sum_squared as f64 / n - mean * mean).max(0.0) * n / (n - 1.0).max(1.0);
    Ok(Comparison {
        a: a.name().to_string(),
        b: b.name().to_string(),
        field: field.name().to_string(),
        pairs: paired.n,
        mean_difference: mean,
        stderr: (variance / n).sqrt(),
        a_stats: paired.a,
        b_stats: paired.b,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(name: &str) -> BotSpec {
        BotSpec::preset(name).unwrap()
    }

    #[test]
    fn identical_candidates_tie_exactly() {
        let c = compare(&Ruleset::family(), &preset("efficiency"), &preset("efficiency"), &preset("efficiency"), Seeds { start: 0, count: 50 }).unwrap();
        assert_eq!(c.pairs, 200);
        assert_eq!((c.mean_difference, c.stderr), (0.0, 0.0));
        assert_eq!(c.a_stats, c.b_stats);
    }

    #[test]
    fn detects_a_better_candidate() {
        let c = compare(&Ruleset::family(), &preset("efficiency"), &preset("random"), &preset("efficiency"), Seeds { start: 0, count: 100 }).unwrap();
        assert!(c.z() > 5.0, "{c:?}");
    }
}
