//! Estimating how likely a discard is to hand someone the win.
//!
//! Every estimate uses only what the deciding seat can see: its own hand,
//! public tiles, and each opponent's melds and discards.

use crate::game::observation::Observation;
use crate::rules::seat::Seat;
use crate::tile::counts::{
    COPIES,
    TileCounts,
};
use crate::tile::set::TileSet;
use crate::tile::{
    SUITED_KINDS,
    Tile,
};

/// What one seat can infer about every opponent, gathered once per decision.
pub struct Threats {
    unseen: TileCounts,
    threat_by_opponent: [Threat; 3],
}

/// One opponent: how ready they look, and the tiles they have let pass.
#[derive(Debug, Clone, Copy)]
pub struct Threat {
    pub seat: Seat,
    pub readiness: f64,
    pub passed: TileSet,
}

impl Threats {
    pub fn assess(obs: &Observation<'_>) -> Threats {
        let mut opponents = obs.seat().others();
        let threat_by_opponent = std::array::from_fn(|_| {
            let seat = opponents.next().unwrap_or(obs.seat());
            Threat {
                seat,
                readiness: readiness(obs, seat),
                passed: passed(obs, seat),
            }
        });
        Threats {
            unseen: unseen(obs),
            threat_by_opponent,
        }
    }

    /// Expected number of opponents `tile` deals into, roughly.
    pub fn danger(
        &self,
        tile: Tile,
    ) -> f64 {
        let weight = wait_weight(tile, &self.unseen) / MAX_WEIGHT;
        self.threat_by_opponent.iter().filter(|t| !t.passed.contains(tile)).map(|t| t.readiness).sum::<f64>() * weight
    }

    /// The readiness of the most dangerous-looking opponent.
    pub fn max_readiness(&self) -> f64 {
        self.threat_by_opponent.iter().map(|t| t.readiness).fold(0.0, f64::max)
    }

    pub fn opponents(&self) -> &[Threat; 3] {
        &self.threat_by_opponent
    }
}

/// Rough chance an opponent is ready: grows with exposed melds and with how
/// long they have been playing.
pub fn readiness(
    obs: &Observation<'_>,
    opponent: Seat,
) -> f64 {
    let melds = obs.melds(opponent).count() as f64;
    let discards = obs.discards().iter().filter(|d| d.seat == opponent).count() as f64;
    (0.02 * discards + 0.22 * melds).min(0.95)
}

/// Copies of each tile this seat cannot account for.
pub fn unseen(obs: &Observation<'_>) -> TileCounts {
    let own = obs.hand().concealed();
    let visible = obs.public_tiles();
    Tile::all().flat_map(|t| std::iter::repeat_n(t, COPIES.saturating_sub(own.get(t) + visible.get(t)) as usize)).collect()
}

/// How many plausible waits `tile` completes, given which tiles are unseen.
/// Two-sided waits count 1, gaps 0.5, pairs 0.5, and lone eyes 0.3.
pub fn wait_weight(
    tile: Tile,
    unseen: &TileCounts,
) -> f64 {
    let live = |offset: isize| tile.index().checked_add_signed(offset).and_then(Tile::from_index).is_some_and(|t| unseen.get(t) > 0);
    let mut weight = 0.0;
    if unseen.get(tile) >= 1 {
        weight += 0.3;
    }
    if unseen.get(tile) >= 2 {
        weight += 0.5;
    }
    let i = tile.index();
    if i >= SUITED_KINDS {
        return weight;
    }
    let rank = i % 9;
    if rank >= 2 && live(-2) && live(-1) {
        weight += 1.0;
    }
    if rank <= 6 && live(1) && live(2) {
        weight += 1.0;
    }
    if (1..=7).contains(&rank) && live(-1) && live(1) {
        weight += 0.5;
    }
    weight
}

/// Tiles `opponent` has visibly declined to win on: discarded by others since
/// the opponent's own latest discard. A player who always takes a win is not
/// currently waiting on any of them.
pub fn passed(
    obs: &Observation<'_>,
    opponent: Seat,
) -> TileSet {
    let discards = obs.discards();
    let since = discards.iter().rposition(|d| d.seat == opponent).map_or(0, |i| i + 1);
    discards.get(since..).unwrap_or_default().iter().filter(|d| d.seat != opponent).map(|d| d.tile).collect()
}

/// The largest `wait_weight`, used to scale danger into roughly 0..=1 per opponent.
const MAX_WEIGHT: f64 = 3.3;

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(notation: &str) -> Tile {
        notation.parse().unwrap()
    }

    fn all_unseen() -> TileCounts {
        Tile::all().flat_map(|t| std::iter::repeat_n(t, 4)).collect()
    }

    #[test]
    fn middle_tiles_are_most_dangerous() {
        let unseen = all_unseen();
        let five = wait_weight(tile("5m"), &unseen);
        let one = wait_weight(tile("1m"), &unseen);
        let honor = wait_weight(tile("1z"), &unseen);
        assert!(five > one && one > honor, "{five} {one} {honor}");
    }

    #[test]
    fn dead_neighbours_remove_sequence_waits() {
        let mut unseen = all_unseen();
        let before = wait_weight(tile("3p"), &unseen);
        for t in ["1p", "2p", "4p", "5p"] {
            for _ in 0..4 {
                unseen.remove(tile(t));
            }
        }
        assert!(wait_weight(tile("3p"), &unseen) < before);
        assert_eq!(wait_weight(tile("3p"), &unseen), 0.8);
    }

    #[test]
    fn fully_seen_honor_is_safe() {
        let mut unseen = all_unseen();
        for _ in 0..4 {
            unseen.remove(tile("7z"));
        }
        assert_eq!(wait_weight(tile("7z"), &unseen), 0.0);
    }
}

#[cfg(test)]
mod passed_tests {
    use std::sync::Arc;

    use super::*;
    use crate::game::Game;
    use crate::game::action::Action;
    use crate::rules::Ruleset;

    #[test]
    fn passed_tiles_are_those_discarded_after_the_opponents_last_discard() {
        let mut game = Game::new(Arc::new(Ruleset::family()), 1, Seat::ALL[0]).unwrap();
        let mut discarded = Vec::new();
        for _ in 0..6 {
            while let Some(seat) = game.to_act().first() {
                let legal = game.legal_actions(seat);
                if legal.contains(&Action::Pass) {
                    game.apply(seat, Action::Pass).unwrap();
                    continue;
                }
                let tile = game.last_draw().unwrap();
                discarded.push((seat, tile));
                game.apply(seat, Action::Discard(tile)).unwrap();
                break;
            }
        }
        // Six discards in turn order 0, 1, 2, 3, 0, 1 with every claim passed;
        // seat 2's latest was the third, so it has passed everything after.
        let obs = game.observe(Seat::ALL[0]);
        let expected: TileSet = discarded[3..].iter().filter(|(seat, _)| *seat != Seat::ALL[2]).map(|(_, tile)| *tile).collect();
        assert_eq!(passed(&obs, Seat::ALL[2]), expected);
    }
}
