//! One row per seat per hand, for pattern analysis outside Rust.
//!
//! Rows describe what each player ended up with and how the hand went for
//! them. They are observational: a feature common in winning hands may only
//! accompany good starting tiles. Test policies head to head before
//! concluding anything causal.

use std::io::Write;
use std::sync::Arc;

use mjers_game::{
    Event,
    Game,
    Ruleset,
    Seat,
};
use mjers_hand::wait::wait_shapes;
use mjers_hand::{
    KongKind,
    Meld,
    WaitShape,
};
use mjers_rules::WinSource;
use mjers_tile::{
    Suit,
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
    play_hand,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    WonSelfDraw,
    WonDiscard,
    /// Discarded the winning tile.
    DealtIn,
    /// Someone else won without this seat's discard.
    Lost,
    Exhausted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandFeatures {
    pub seed: u64,
    pub seat: u8,
    /// Turns after the dealer (0 is the dealer).
    pub position: u8,
    pub strategy: String,
    pub outcome: Outcome,
    pub units: i32,
    pub win_units: i32,
    pub kong_units: i32,
    pub chows: u8,
    pub pongs: u8,
    pub exposed_kongs: u8,
    pub concealed_kongs: u8,
    /// No chow, pong, or exposed kong declared.
    pub concealed_hand: bool,
    pub bonus_tiles: u8,
    /// Pongs and kongs of honors, declared or concealed.
    pub honor_sets: u8,
    /// Suits present among concealed and melded tiles.
    pub suits: u8,
    pub honor_tiles: u8,
    /// -1 for the winner.
    pub final_shanten: i8,
    pub own_discards: u16,
    /// Discards by everyone when the hand ended.
    pub hand_length: u16,
    /// Every role the winning tile could have played (winner only).
    pub wait: Vec<WaitShape>,
    pub winning_tile: Option<Tile>,
}

pub fn features(
    game: &Game,
    table: &Table,
) -> Vec<HandFeatures> {
    let result = game.result().expect("hand is over");
    let settlement = game.settlement().expect("hand is over");
    let winning_tile = game.events().iter().rev().find_map(|e| match e {
        Event::Win { tile, .. } => Some(*tile),
        _ => None,
    });
    Seat::ALL
        .iter()
        .map(|&seat| {
            let hand = game.hand(seat);
            let melds: Vec<Meld> = hand.melds().collect();
            let count = |f: fn(&Meld) -> bool| melds.iter().filter(|m| f(m)).count() as u8;
            let outcome = match result.win {
                Some(w) if w.winner == seat && w.source == WinSource::SelfDraw => Outcome::WonSelfDraw,
                Some(w) if w.winner == seat => Outcome::WonDiscard,
                Some(w) if w.source == WinSource::Discard(seat) => Outcome::DealtIn,
                Some(_) => Outcome::Lost,
                None => Outcome::Exhausted,
            };
            let won = matches!(outcome, Outcome::WonSelfDraw | Outcome::WonDiscard);
            let mut all_tiles = *hand.concealed();
            for meld in &melds {
                for tile in meld.tiles().tiles().take(3) {
                    all_tiles.add(tile);
                }
            }
            let concealed_honor_sets = hand.concealed().kinds().filter(|t| t.is_honor() && hand.concealed().get(*t) >= 3).count() as u8;
            HandFeatures {
                seed: game.seed().unwrap_or_default(),
                seat: seat.index() as u8,
                position: seat.turns_after(game.dealer()),
                strategy: table[seat.index()].name().to_string(),
                outcome,
                units: settlement.total()[seat.index()],
                win_units: settlement.win[seat.index()],
                kong_units: settlement.kongs[seat.index()],
                chows: count(|m| matches!(m, Meld::Chow(_))),
                pongs: count(|m| matches!(m, Meld::Pong(_))),
                exposed_kongs: count(|m| matches!(m, Meld::Kong(_, KongKind::Exposed))),
                concealed_kongs: count(|m| matches!(m, Meld::Kong(_, KongKind::Concealed))),
                concealed_hand: melds.iter().all(|m| m.is_concealed()),
                bonus_tiles: hand.bonus().len(),
                honor_sets: count(|m| !matches!(m, Meld::Chow(_)) && m.tile().is_honor()) + concealed_honor_sets,
                suits: Suit::ALL.iter().filter(|&&s| all_tiles.kinds().any(|t| t.suit() == Some(s))).count() as u8,
                honor_tiles: all_tiles.tiles().filter(|t| t.is_honor()).count() as u8,
                final_shanten: hand.shanten(),
                own_discards: game.discards().iter().filter(|d| d.seat == seat).count() as u16,
                hand_length: game.discards().len() as u16,
                wait: if won {
                    winning_tile.map(|t| wait_shapes(hand.concealed(), melds.len(), t)).unwrap_or_default()
                } else {
                    Vec::new()
                },
                winning_tile: winning_tile.filter(|_| won),
            }
        })
        .collect()
}

const CHUNK: u64 = 4096;

/// Writes JSON Lines rows for every hand, in seed then seat order. Hands are
/// played in parallel chunks so memory stays bounded.
pub fn write_features(
    rules: &Ruleset,
    table: &Table,
    seeds: Seeds,
    mut out: impl Write,
) -> std::io::Result<u64> {
    let rules = Arc::new(rules.clone());
    let mut rows = 0;
    let mut done = 0;
    while done < seeds.count {
        let chunk = CHUNK.min(seeds.count - done);
        let lines: Vec<String> = (done..done + chunk)
            .into_par_iter()
            .map(|i| {
                let game = play_hand(&rules, table, seeds.start.wrapping_add(i));
                features(&game, table).iter().map(|row| serde_json::to_string(row).expect("rows serialize") + "\n").collect()
            })
            .collect();
        for line in &lines {
            out.write_all(line.as_bytes())?;
        }
        rows += chunk * 4;
        done += chunk;
    }
    out.flush()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use mjers_bot::BotSpec;

    use super::*;

    fn table() -> Table {
        std::array::from_fn(|_| BotSpec::preset("efficiency").unwrap())
    }

    #[test]
    fn rows_agree_with_the_game() {
        let rules = Arc::new(Ruleset::family());
        for seed in 0..100 {
            let game = play_hand(&rules, &table(), seed);
            let rows = features(&game, &table());
            assert_eq!(rows.iter().map(|r| r.units).sum::<i32>(), 0);
            let winners: Vec<_> = rows.iter().filter(|r| matches!(r.outcome, Outcome::WonSelfDraw | Outcome::WonDiscard)).collect();
            match game.result().unwrap().win {
                Some(_) => {
                    assert_eq!(winners.len(), 1);
                    assert_eq!(winners[0].final_shanten, -1);
                    assert!(!winners[0].wait.is_empty(), "seed {seed}: winner has a wait shape");
                },
                None => assert!(rows.iter().all(|r| r.outcome == Outcome::Exhausted)),
            }
        }
    }

    #[test]
    fn jsonl_output_is_ordered_and_complete() {
        let mut buffer = Vec::new();
        let rows = write_features(&Ruleset::family(), &table(), Seeds { start: 5, count: 10 }, &mut buffer).unwrap();
        assert_eq!(rows, 40);
        let parsed: Vec<HandFeatures> = String::from_utf8(buffer).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(parsed.len(), 40);
        assert_eq!((parsed[0].seed, parsed[0].seat), (5, 0));
        assert_eq!((parsed[39].seed, parsed[39].seat), (14, 3));
    }
}
