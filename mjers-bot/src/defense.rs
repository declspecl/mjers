//! Estimating how likely a discard is to hand someone the win.
//!
//! Every estimate uses only what the deciding seat can see: its own hand,
//! public tiles, and each opponent's melds and discards.

use mjers_game::{
    Observation,
    Seat,
};
use mjers_tile::counts::COPIES;
use mjers_tile::tile::SUITED_KINDS;
use mjers_tile::{
    Tile,
    TileCounts,
};

/// Rough chance an opponent is ready: grows with exposed melds and with how
/// long they have been playing.
pub fn readiness(
    obs: &Observation<'_>,
    opponent: Seat,
) -> f64 {
    let melds = obs.melds(opponent).count() as f64;
    let discards = obs.discards().iter().filter(|d| d.seat == opponent).count() as f64;
    (0.035 * discards + 0.17 * melds).min(0.95)
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
    let live = |t: Tile| unseen.get(t) > 0;
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
    let at = |offset: isize| Tile::from_index((i as isize + offset) as usize).expect("checked in-suit");
    if rank >= 2 && live(at(-2)) && live(at(-1)) {
        weight += 1.0;
    }
    if rank <= 6 && live(at(1)) && live(at(2)) {
        weight += 1.0;
    }
    if (1..=7).contains(&rank) && live(at(-1)) && live(at(1)) {
        weight += 0.5;
    }
    weight
}

/// Expected number of opponents `tile` deals into, roughly.
pub fn danger(
    obs: &Observation<'_>,
    tile: Tile,
    unseen: &TileCounts,
) -> f64 {
    let weight = wait_weight(tile, unseen) / MAX_WEIGHT;
    obs.seat().others().map(|o| readiness(obs, o)).sum::<f64>() * weight
}

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
