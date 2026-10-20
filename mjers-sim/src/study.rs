//! Tile-level and hand-shape statistics across many hands.
//!
//! Three questions: which set splits win, which ready shapes go on to win,
//! and what becomes of each tile (discarded, melded, won with, or stuck).
//! Everything is observational: it describes how these bots' hands turn out,
//! not what would happen if a player changed course.

pub mod composition;
pub mod fates;
pub mod ready;
pub mod tally;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::Arc;

use mjers_game::game::Game;
use mjers_game::trace::Event;
use mjers_hand::meld::{
    KongKind,
    Meld,
};
use mjers_hand::win::decompose;
use mjers_rules::ruleset::Ruleset;
use mjers_rules::seat::{
    SEATS,
    Seat,
};
use mjers_rules::settlement::WinSource;
use mjers_tile::tile::{
    KINDS,
    SUITED_KINDS,
    Tile,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::runner::{
    HandFailure,
    Seeds,
    Table,
    par_try_fold,
    play_hand_watched,
};
use crate::study::composition::Composition;
use crate::study::fates::{
    TileFates,
    record_fates,
};
use crate::study::ready::{
    ReadyShape,
    ready_shape,
};
use crate::study::tally::Tally;

/// Tiles grouped by what matters under rules without hand patterns: the rank
/// of a suited tile, or the honor family.
pub const CLASSES: [&str; 11] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "wind", "dragon"];
const WIND_KINDS: usize = 4;

pub fn class_of(tile: Tile) -> usize {
    let i = tile.index();
    if i < SUITED_KINDS {
        i % 9
    } else if i < SUITED_KINDS + WIND_KINDS {
        9
    } else {
        10
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Study {
    pub hands: u64,
    /// One entry per tile kind, by tile index.
    pub fates_by_tile: Vec<TileFates>,
    /// Winning hands keyed `"<chows>c <pongs>p <kongs>k"`.
    pub wins_by_split: BTreeMap<String, Tally>,
    pub wins_by_eye_class: BTreeMap<String, Tally>,
    /// Every seat keyed by exposed melds at the end, `"<chows>c <pongs+kongs>p"`.
    pub seats_by_exposed_melds: BTreeMap<String, Tally>,
    /// Seats by their wait shape when they first became ready.
    pub ready_by_shape: BTreeMap<String, Tally>,
    /// Seats by live winning tiles when first ready (8 means 8+).
    pub ready_by_live_tiles: BTreeMap<u32, Tally>,
    /// Seats by how many different tiles they waited on when first ready.
    pub ready_by_wait_count: BTreeMap<String, Tally>,
    /// Single-tile eye waits (tanki) by the class of the waited tile.
    pub tanki_by_class: BTreeMap<String, Tally>,
    pub composition: Composition,
}

impl Default for Study {
    fn default() -> Self {
        Study {
            hands: 0,
            fates_by_tile: vec![TileFates::default(); KINDS],
            wins_by_split: BTreeMap::new(),
            wins_by_eye_class: BTreeMap::new(),
            seats_by_exposed_melds: BTreeMap::new(),
            ready_by_shape: BTreeMap::new(),
            ready_by_live_tiles: BTreeMap::new(),
            ready_by_wait_count: BTreeMap::new(),
            tanki_by_class: BTreeMap::new(),
            composition: Composition::default(),
        }
    }
}

impl Study {
    pub fn merge(
        mut self,
        o: Study,
    ) -> Study {
        self.hands += o.hands;
        for (mine, theirs) in self.fates_by_tile.iter_mut().zip(&o.fates_by_tile) {
            mine.merge(theirs);
        }
        for (mine, theirs) in [
            (&mut self.wins_by_split, &o.wins_by_split),
            (&mut self.wins_by_eye_class, &o.wins_by_eye_class),
            (&mut self.seats_by_exposed_melds, &o.seats_by_exposed_melds),
            (&mut self.ready_by_shape, &o.ready_by_shape),
            (&mut self.ready_by_wait_count, &o.ready_by_wait_count),
            (&mut self.tanki_by_class, &o.tanki_by_class),
        ] {
            for (key, tally) in theirs {
                mine.entry(key.clone()).or_default().merge(tally);
            }
        }
        for (key, tally) in &o.ready_by_live_tiles {
            self.ready_by_live_tiles.entry(*key).or_default().merge(tally);
        }
        self.composition.merge(&o.composition);
        self
    }

    /// Fates summed over each class in `CLASSES`.
    pub fn fates_by_class(&self) -> Vec<TileFates> {
        let mut by_class = vec![TileFates::default(); CLASSES.len()];
        for (tile, fates) in Tile::all().zip(&self.fates_by_tile) {
            if let Some(class) = by_class.get_mut(class_of(tile)) {
                class.merge(fates);
            }
        }
        by_class
    }

    /// Plays one hand, watching for each seat's first ready hand, and records it.
    fn record_hand(
        &mut self,
        rules: &Arc<Ruleset>,
        table: &Table,
        seed: u64,
    ) -> Result<(), HandFailure> {
        let mut first_ready: [Option<ReadyShape>; SEATS] = [None; SEATS];
        let game = play_hand_watched(rules, table, seed, |game| {
            if game.is_over() {
                return;
            }
            for seat in Seat::ALL {
                let hand = game.hand(seat);
                if first_ready[seat.index()].is_none() && hand.size() == 13 && hand.shanten() == 0 {
                    first_ready[seat.index()] = ready_shape(hand, game.public_tiles()).map(|shape| ReadyShape {
                        discards_at: game.discards().len(),
                        events_at: game.events().len(),
                        ..shape
                    });
                }
            }
        })?;
        self.record(&game, &first_ready);
        Ok(())
    }

    fn record(
        &mut self,
        game: &Game,
        first_ready: &[Option<ReadyShape>; SEATS],
    ) {
        let (Some(result), Some(settlement)) = (game.result(), game.settlement()) else {
            return;
        };
        self.hands += 1;
        let totals = settlement.total();
        let winner = result.win.map(|w| w.winner);
        record_fates(&mut self.fates_by_tile, game);

        for seat in Seat::ALL {
            let won = winner == Some(seat);
            let units = totals[seat.index()];
            let chows = game.hand(seat).melds().filter(|m| matches!(m, Meld::Chow(_))).count();
            let pongs = game.hand(seat).melds().filter(|m| matches!(m, Meld::Pong(_) | Meld::Kong(_, KongKind::Exposed))).count();
            self.seats_by_exposed_melds.entry(format!("{chows}c {pongs}p")).or_default().add(won, units, 0);
            if let Some(shape) = first_ready[seat.index()] {
                let mut tally = Tally::default();
                tally.add(won, units, shape.live);
                if won {
                    tally.self_draws = (result.win.map(|w| w.source) == Some(WinSource::SelfDraw)) as u64;
                    tally.table_turns = game.discards().len().saturating_sub(shape.discards_at) as u64;
                    let since_ready = game.events().get(shape.events_at..).unwrap_or_default();
                    tally.own_draws = since_ready.iter().filter(|e| matches!(e, Event::Draw { seat: s, .. } if *s == seat)).count() as u64;
                }
                let waits = if shape.waits >= 5 { "5+".to_string() } else { shape.waits.to_string() };
                self.ready_by_shape.entry(shape.name.to_string()).or_default().merge(&tally);
                self.ready_by_live_tiles.entry(shape.live.min(8)).or_default().merge(&tally);
                self.ready_by_wait_count.entry(waits).or_default().merge(&tally);
                if let Some(tile) = shape.tanki_tile {
                    self.tanki_by_class.entry(CLASSES[class_of(tile)].to_string()).or_default().merge(&tally);
                }
            }
        }

        let Some(seat) = winner else { return };
        let hand = game.hand(seat);
        let Some((eye, concealed_sets)) = decompose(hand.concealed(), hand.meld_count()) else { return };
        let all: Vec<Meld> = hand.melds().chain(concealed_sets.iter().copied()).collect();
        let count = |f: fn(&Meld) -> bool| all.iter().filter(|m| f(m)).count();
        let chows = count(|m| matches!(m, Meld::Chow(_)));
        let pongs = count(|m| matches!(m, Meld::Pong(_)));
        let kongs = count(|m| matches!(m, Meld::Kong(..)));
        self.wins_by_split.entry(format!("{chows}c {pongs}p {kongs}k")).or_default().add(true, totals[seat.index()], 0);
        self.wins_by_eye_class.entry(CLASSES[class_of(eye)].to_string()).or_default().add(true, totals[seat.index()], 0);
        self.composition.record(game, seat, &concealed_sets);
    }
}

/// Plays `seeds` in parallel and gathers a study.
pub fn study(
    rules: &Ruleset,
    table: &Table,
    seeds: Seeds,
) -> Result<Study, HandFailure> {
    let rules = Arc::new(rules.clone());
    par_try_fold(seeds.count, |acc: &mut Study, i| acc.record_hand(&rules, table, seeds.start.wrapping_add(i)), Study::merge)
}
