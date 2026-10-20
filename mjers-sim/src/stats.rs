//! Per-seat outcome counters and the metrics derived from them.

use std::collections::BTreeMap;

use mjers_core::game::Game;
use mjers_core::hand::meld::{
    KongKind,
    Meld,
};
use mjers_core::rules::Ruleset;
use mjers_core::rules::seat::Seat;
use mjers_core::rules::settlement::WinSource;
use serde::{
    Deserialize,
    Serialize,
};

use crate::runner::{
    HandFailure,
    Seeds,
    Table,
    run,
};

/// Raw counters for one seat across many hands. Mergeable, so batches
/// combine exactly.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub hands: u64,
    /// Net units won.
    pub units: i64,
    pub units_squared: u64,
    pub win_units: i64,
    pub kong_units: i64,
    pub wins: u64,
    pub self_draw_wins: u64,
    /// Hands lost by discarding the winning tile.
    pub deal_ins: u64,
    /// Hands that ended with the wall exhausted.
    pub exhausted: u64,
    pub chows: u64,
    pub pongs: u64,
    pub exposed_kongs: u64,
    pub concealed_kongs: u64,
}

/// Rates and averages per hand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub hands: u64,
    pub ev: f64,
    /// Standard error of `ev`, treating hands as independent.
    pub ev_stderr: f64,
    pub win_rate: f64,
    pub self_draw_rate: f64,
    pub deal_in_rate: f64,
    pub exhausted_rate: f64,
    pub win_units_per_hand: f64,
    pub kong_units_per_hand: f64,
    pub chows_per_hand: f64,
    pub pongs_per_hand: f64,
    pub exposed_kongs_per_hand: f64,
    pub concealed_kongs_per_hand: f64,
}

impl Stats {
    pub fn record(
        &mut self,
        game: &Game,
        seat: Seat,
    ) {
        let (Some(result), Some(settlement)) = (game.result(), game.settlement()) else {
            return;
        };
        let total = settlement.total()[seat.index()] as i64;
        self.hands += 1;
        self.units += total;
        self.units_squared += (total * total) as u64;
        self.win_units += settlement.win[seat.index()] as i64;
        self.kong_units += settlement.kongs[seat.index()] as i64;
        match result.win {
            Some(win) if win.winner == seat => {
                self.wins += 1;
                if win.source == WinSource::SelfDraw {
                    self.self_draw_wins += 1;
                }
            },
            Some(win) if win.source == WinSource::Discard(seat) => self.deal_ins += 1,
            Some(_) => {},
            None => self.exhausted += 1,
        }
        for meld in game.hand(seat).melds() {
            match meld {
                Meld::Chow(_) => self.chows += 1,
                Meld::Pong(_) => self.pongs += 1,
                Meld::Kong(_, KongKind::Exposed) => self.exposed_kongs += 1,
                Meld::Kong(_, KongKind::Concealed) => self.concealed_kongs += 1,
            }
        }
    }

    pub fn merge(
        &mut self,
        other: &Stats,
    ) {
        self.hands += other.hands;
        self.units += other.units;
        self.units_squared += other.units_squared;
        self.win_units += other.win_units;
        self.kong_units += other.kong_units;
        self.wins += other.wins;
        self.self_draw_wins += other.self_draw_wins;
        self.deal_ins += other.deal_ins;
        self.exhausted += other.exhausted;
        self.chows += other.chows;
        self.pongs += other.pongs;
        self.exposed_kongs += other.exposed_kongs;
        self.concealed_kongs += other.concealed_kongs;
    }

    pub fn metrics(&self) -> Metrics {
        let n = self.hands.max(1) as f64;
        let per = |x: f64| x / n;
        let ev = per(self.units as f64);
        let variance = (per(self.units_squared as f64) - ev * ev).max(0.0) * n / (n - 1.0).max(1.0);
        Metrics {
            hands: self.hands,
            ev,
            ev_stderr: (variance / n).sqrt(),
            win_rate: per(self.wins as f64),
            self_draw_rate: per(self.self_draw_wins as f64),
            deal_in_rate: per(self.deal_ins as f64),
            exhausted_rate: per(self.exhausted as f64),
            win_units_per_hand: per(self.win_units as f64),
            kong_units_per_hand: per(self.kong_units as f64),
            chows_per_hand: per(self.chows as f64),
            pongs_per_hand: per(self.pongs as f64),
            exposed_kongs_per_hand: per(self.exposed_kongs as f64),
            concealed_kongs_per_hand: per(self.concealed_kongs as f64),
        }
    }
}

/// Outcomes of a batch, by strategy name and by position after the dealer
/// (0 is the dealer).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub hands: u64,
    pub by_strategy: BTreeMap<String, Stats>,
    pub by_position: [Stats; 4],
}

impl Summary {
    pub fn record(
        &mut self,
        table: &Table,
        game: &Game,
    ) {
        self.hands += 1;
        for seat in Seat::ALL {
            self.by_strategy.entry(table[seat.index()].name().to_string()).or_default().record(game, seat);
            self.by_position[seat.turns_after(game.dealer()) as usize].record(game, seat);
        }
    }

    pub fn merge(
        mut self,
        other: Summary,
    ) -> Summary {
        self.hands += other.hands;
        for (name, stats) in other.by_strategy {
            self.by_strategy.entry(name).or_default().merge(&stats);
        }
        for (mine, theirs) in self.by_position.iter_mut().zip(&other.by_position) {
            mine.merge(theirs);
        }
        self
    }
}

/// Plays `seeds` with `table` and summarises every seat.
pub fn simulate(
    rules: &Ruleset,
    table: &Table,
    seeds: Seeds,
) -> Result<Summary, HandFailure> {
    run(rules, table, seeds, |summary: &mut Summary, game| summary.record(table, game), Summary::merge)
}

#[cfg(test)]
mod tests {
    use mjers_core::bot::spec::BotSpec;

    use super::*;

    fn efficiency_table() -> Table {
        std::array::from_fn(|_| BotSpec::preset("efficiency").unwrap())
    }

    #[test]
    fn summary_is_consistent() {
        let summary = simulate(&Ruleset::family(), &efficiency_table(), Seeds { start: 0, count: 400 }).unwrap();
        assert_eq!(summary.hands, 400);
        let all = &summary.by_strategy["efficiency"];
        assert_eq!(all.hands, 1600);
        assert_eq!(all.units, 0, "a closed table is zero-sum");
        assert_eq!(all.wins + all.exhausted / 4, 400, "every hand has a winner or is exhausted at all four seats");
        let total_deal_ins: u64 = summary.by_position.iter().map(|s| s.deal_ins).sum();
        assert!(total_deal_ins <= all.wins);
        let m = all.metrics();
        assert!(m.win_rate > 0.1 && m.win_rate < 0.25, "{m:?}");
        assert!(m.ev_stderr > 0.0);
    }

    #[test]
    fn efficiency_beats_random_seats() {
        let mut table = efficiency_table();
        table[1] = BotSpec::preset("random").unwrap();
        table[3] = BotSpec::preset("random").unwrap();
        let summary = simulate(&Ruleset::family(), &table, Seeds { start: 0, count: 300 }).unwrap();
        let efficient = summary.by_strategy["efficiency"].metrics();
        let random = summary.by_strategy["random"].metrics();
        assert!(efficient.ev > random.ev + 3.0 * (efficient.ev_stderr + random.ev_stderr), "{efficient:?} {random:?}");
    }

    #[test]
    fn merge_matches_single_pass() {
        let rules = Ruleset::family();
        let table = efficiency_table();
        let whole = simulate(&rules, &table, Seeds { start: 0, count: 60 }).unwrap();
        let halves = simulate(&rules, &table, Seeds { start: 0, count: 30 })
            .unwrap()
            .merge(simulate(&rules, &table, Seeds { start: 30, count: 30 }).unwrap());
        assert_eq!(whole, halves);
    }

    #[test]
    fn metrics_handle_empty_stats() {
        let m = Stats::default().metrics();
        assert_eq!((m.hands, m.ev, m.ev_stderr), (0, 0.0, 0.0));
    }
}
