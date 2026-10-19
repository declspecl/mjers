//! Tile-level and hand-shape statistics across many hands.
//!
//! Three questions: which set splits win, which ready shapes go on to win,
//! and what becomes of each tile (discarded, melded, won with, or stuck).
//! Everything is observational: it describes how these bots' hands turn out,
//! not what would happen if a player changed course.

pub mod composition;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::Arc;

use mjers_bot::heuristic::connectivity;
use mjers_game::game::Game;
use mjers_game::trace::Event;
use mjers_hand::hand::Hand;
use mjers_hand::meld::{
    KongKind,
    Meld,
};
use mjers_hand::wait::{
    WaitShape,
    wait_shapes,
};
use mjers_hand::win::decompose;
use mjers_rules::ruleset::Ruleset;
use mjers_rules::seat::Seat;
use mjers_rules::settlement::WinSource;
use mjers_tile::counts::{
    COPIES,
    TileCounts,
};
use mjers_tile::tile::{
    KINDS,
    SUITED_KINDS,
    Tile,
};
use rayon::prelude::*;
use serde::{
    Deserialize,
    Serialize,
};

use crate::runner::{
    Seeds,
    Table,
    play_hand_watched,
};

/// Tiles grouped by what matters under rules without hand patterns: the rank
/// of a suited tile, or the honor family.
pub const CLASSES: [&str; 11] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "wind", "dragon"];

pub fn class_of(tile: Tile) -> usize {
    let i = tile.index();
    if i < SUITED_KINDS {
        i % 9
    } else if i < SUITED_KINDS + 4 {
        9
    } else {
        10
    }
}

/// What happened to every copy of one tile kind that entered a hand.
/// `entered = discarded + melded + in_win + held_at_end`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileFates {
    /// Dealt or drawn into a hand.
    pub entered: u64,
    pub discarded: u64,
    /// Of the discards: claimed into another player's meld.
    pub discards_claimed: u64,
    /// Of the discards: won on.
    pub discards_won_on: u64,
    /// Moved from the holder's own hand into one of their melds.
    pub melded: u64,
    /// Still concealed in the winner's completed hand.
    pub in_win: u64,
    /// Still concealed in a non-winner's hand when the hand ended.
    pub held_at_end: u64,
    /// Of those held at the end: with no copy or same-suit neighbour within two ranks.
    pub isolated_at_end: u64,
}

impl TileFates {
    fn merge(
        &mut self,
        o: &TileFates,
    ) {
        self.entered += o.entered;
        self.discarded += o.discarded;
        self.discards_claimed += o.discards_claimed;
        self.discards_won_on += o.discards_won_on;
        self.melded += o.melded;
        self.in_win += o.in_win;
        self.held_at_end += o.held_at_end;
        self.isolated_at_end += o.isolated_at_end;
    }
}

/// Outcomes for a group of seats or hands.
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
    fn add(
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

    fn merge(
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

    pub fn self_draw_share(&self) -> f64 {
        self.self_draws as f64 / self.wins.max(1) as f64
    }

    pub fn mean_table_turns(&self) -> f64 {
        self.table_turns as f64 / self.wins.max(1) as f64
    }

    pub fn mean_own_draws(&self) -> f64 {
        self.own_draws as f64 / self.wins.max(1) as f64
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
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Study {
    pub hands: u64,
    /// Indexed by tile index (34 kinds).
    pub tiles: Vec<TileFates>,
    /// Winning hands by full split, keyed `"<chows>c <pongs>p <kongs>k"`.
    pub winning_splits: BTreeMap<String, Tally>,
    /// Winning hands by eye class.
    pub winning_eyes: BTreeMap<String, Tally>,
    /// Every seat by exposed melds at the end, keyed `"<chows>c <pongs+kongs>p"`.
    pub exposed: BTreeMap<String, Tally>,
    /// Seats by their wait shape when they first became ready.
    pub ready_shapes: BTreeMap<String, Tally>,
    /// Single-tile eye waits (tanki) by the class of the waited tile.
    pub tanki_tiles: BTreeMap<String, Tally>,
    /// Seats by live winning tiles when they first became ready (8 means 8+).
    pub ready_live: BTreeMap<u32, Tally>,
    /// Seats by how many different tiles they waited on when first ready.
    pub ready_waits: BTreeMap<String, Tally>,
    pub composition: composition::Composition,
}

impl Study {
    pub fn new() -> Study {
        Study {
            tiles: vec![TileFates::default(); KINDS],
            ..Study::default()
        }
    }

    pub fn merge(
        mut self,
        o: Study,
    ) -> Study {
        if self.tiles.is_empty() {
            return o;
        }
        self.hands += o.hands;
        for (mine, theirs) in self.tiles.iter_mut().zip(&o.tiles) {
            mine.merge(theirs);
        }
        for (mine, theirs) in [
            (&mut self.winning_splits, &o.winning_splits),
            (&mut self.winning_eyes, &o.winning_eyes),
            (&mut self.exposed, &o.exposed),
            (&mut self.ready_shapes, &o.ready_shapes),
            (&mut self.tanki_tiles, &o.tanki_tiles),
            (&mut self.ready_waits, &o.ready_waits),
        ] {
            for (key, tally) in theirs {
                mine.entry(key.clone()).or_default().merge(tally);
            }
        }
        self.composition.merge(&o.composition);
        for (key, tally) in &o.ready_live {
            self.ready_live.entry(*key).or_default().merge(tally);
        }
        self
    }

    /// Fates summed over each class in `CLASSES`.
    pub fn tiles_by_class(&self) -> Vec<TileFates> {
        let mut classes = vec![TileFates::default(); CLASSES.len()];
        for (i, fates) in self.tiles.iter().enumerate() {
            classes[class_of(Tile::from_index(i).expect("in range"))].merge(fates);
        }
        classes
    }
}

/// How a ready hand waits, by the most flexible role any winning tile plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyShape {
    pub name: &'static str,
    pub waits: usize,
    pub live: u32,
    /// The waited tile, for single-tile eye waits.
    pub tanki_tile: Option<Tile>,
    /// Discards at the table, and events, when the seat became ready.
    pub discards_at: usize,
    pub events_at: usize,
}

pub fn ready_shape(
    hand: &Hand,
    public: &TileCounts,
) -> Option<ReadyShape> {
    let waits = hand.waits();
    if waits.is_empty() {
        return None;
    }
    let concealed = hand.concealed();
    let live = waits.iter().map(|&t| COPIES.saturating_sub(concealed.get(t) + public.get(t)) as u32).sum();
    let mut shapes: Vec<WaitShape> = waits.iter().flat_map(|&t| wait_shapes(&concealed.with(t), hand.meld_count(), t)).collect();
    shapes.sort();
    let name = if waits.len() >= 3 {
        "multi-sided (3+ tiles)"
    } else {
        match shapes.first() {
            Some(WaitShape::Ryanmen) => "two-sided chow",
            Some(WaitShape::Shanpon) => "pair to pong",
            Some(WaitShape::Kanchan) => "gap chow",
            Some(WaitShape::Penchan) => "edge chow",
            _ => "eye (tanki)",
        }
    };
    let tanki_tile = (name == "eye (tanki)" && waits.len() == 1).then(|| waits[0]);
    Some(ReadyShape {
        name,
        waits: waits.len(),
        live,
        tanki_tile,
        discards_at: 0,
        events_at: 0,
    })
}

/// Plays one hand and records it.
fn study_hand(
    study: &mut Study,
    rules: &Arc<Ruleset>,
    table: &Table,
    seed: u64,
) {
    let mut first_ready: [Option<ReadyShape>; 4] = [None; 4];
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
    });
    record(study, &game, &first_ready);
}

fn record(
    study: &mut Study,
    game: &Game,
    first_ready: &[Option<ReadyShape>; 4],
) {
    study.hands += 1;
    let result = game.result().expect("hand is over");
    let totals = game.settlement().expect("hand is over").total();
    let winner = result.win.map(|w| w.winner);
    record_fates(study, game);

    for seat in Seat::ALL {
        let won = winner == Some(seat);
        let units = totals[seat.index()];
        let melds: Vec<Meld> = game.hand(seat).melds().collect();
        let chows = melds.iter().filter(|m| matches!(m, Meld::Chow(_))).count();
        let pongs = melds.iter().filter(|m| matches!(m, Meld::Pong(_)) || matches!(m, Meld::Kong(_, KongKind::Exposed))).count();
        study.exposed.entry(format!("{chows}c {pongs}p")).or_default().add(won, units, 0);
        if let Some(shape) = first_ready[seat.index()] {
            let mut tally = Tally::default();
            tally.add(won, units, shape.live);
            if won {
                tally.self_draws = (result.win.map(|w| w.source) == Some(WinSource::SelfDraw)) as u64;
                tally.table_turns = (game.discards().len() - shape.discards_at) as u64;
                tally.own_draws = game.events()[shape.events_at..].iter().filter(|e| matches!(e, Event::Draw { seat: s, .. } if *s == seat)).count() as u64;
            }
            let waits = if shape.waits >= 5 { "5+".to_string() } else { shape.waits.to_string() };
            study.ready_shapes.entry(shape.name.to_string()).or_default().merge(&tally);
            study.ready_live.entry(shape.live.min(8)).or_default().merge(&tally);
            study.ready_waits.entry(waits).or_default().merge(&tally);
            if let Some(tile) = shape.tanki_tile {
                study.tanki_tiles.entry(CLASSES[class_of(tile)].to_string()).or_default().merge(&tally);
            }
        }
    }

    if let Some(seat) = winner {
        let hand = game.hand(seat);
        let (eye, sets) = decompose(hand.concealed(), hand.meld_count()).expect("winners hold complete hands");
        let all: Vec<Meld> = hand.melds().chain(sets).collect();
        let count = |f: fn(&Meld) -> bool| all.iter().filter(|m| f(m)).count();
        let key = format!(
            "{}c {}p {}k",
            count(|m| matches!(m, Meld::Chow(_))),
            count(|m| matches!(m, Meld::Pong(_))),
            count(|m| matches!(m, Meld::Kong(..)))
        );
        study.winning_splits.entry(key).or_default().add(true, totals[seat.index()], 0);
        study.winning_eyes.entry(CLASSES[class_of(eye)].to_string()).or_default().add(true, totals[seat.index()], 0);
        let concealed_sets: Vec<Meld> = all[hand.meld_count()..].to_vec();
        study.composition.record(game, seat, &concealed_sets);
    }
}

fn record_fates(
    study: &mut Study,
    game: &Game,
) {
    let fates = &mut study.tiles;
    let mut last_discard = None;
    let mut won_on_discard = None;
    for event in game.events() {
        match *event {
            Event::Deal { tiles, .. } => tiles.tiles().for_each(|t| fates[t.index()].entered += 1),
            Event::Draw { tile, .. } => fates[tile.index()].entered += 1,
            Event::Discard { tile, .. } => {
                fates[tile.index()].discarded += 1;
                last_discard = Some(tile);
            },
            Event::Claim { meld, .. } => {
                let claimed = last_discard.expect("claims follow discards");
                fates[claimed.index()].discards_claimed += 1;
                let mut own = meld.tiles();
                own.remove(claimed);
                own.tiles().for_each(|t| fates[t.index()].melded += 1);
            },
            Event::Kong {
                meld: Meld::Kong(t, KongKind::Concealed),
                ..
            } => fates[t.index()].melded += 4,
            Event::Kong { meld, .. } => fates[meld.tile().index()].melded += 1,
            Event::Win {
                source: WinSource::Discard(_), tile, ..
            } => {
                fates[tile.index()].discards_won_on += 1;
                won_on_discard = Some(tile);
            },
            Event::Win { .. } | Event::Bonus { .. } | Event::Exhausted => {},
        }
    }
    let winner = game.result().and_then(|r| r.win).map(|w| w.winner);
    for seat in Seat::ALL {
        let mut concealed = *game.hand(seat).concealed();
        if Some(seat) == winner {
            if let Some(tile) = won_on_discard {
                concealed.remove(tile);
            }
            concealed.tiles().for_each(|t| fates[t.index()].in_win += 1);
            continue;
        }
        for t in concealed.tiles() {
            fates[t.index()].held_at_end += 1;
            if connectivity(&concealed, t) == 0 {
                fates[t.index()].isolated_at_end += 1;
            }
        }
    }
}

/// Plays `seeds` in parallel and gathers a study.
pub fn study(
    rules: &Ruleset,
    table: &Table,
    seeds: Seeds,
) -> Study {
    let rules = Arc::new(rules.clone());
    (0..seeds.count)
        .into_par_iter()
        .fold(Study::new, |mut acc, i| {
            study_hand(&mut acc, &rules, table, seeds.start.wrapping_add(i));
            acc
        })
        .reduce(Study::new, Study::merge)
}
