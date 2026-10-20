//! Cross-checks the fast hand algorithms against slow, obviously-correct references.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]

use std::collections::HashSet;

use mjers_core::hand::shanten::shanten;
use mjers_core::hand::ukeire::{
    discard_options,
    ukeire,
};
use mjers_core::hand::win::{
    is_complete,
    waits,
};
use mjers_core::tile::counts::TileCounts;
use mjers_core::tile::{
    KINDS,
    SUITED_KINDS,
    Tile,
};
use proptest::prelude::*;

type Counts = [u8; KINDS];

/// Exhaustive search over every pong/chow/eye choice at the lowest tile.
fn reference_complete(
    counts: &mut Counts,
    eye_used: bool,
) -> bool {
    let Some(i) = (0..KINDS).find(|&i| counts[i] > 0) else {
        return eye_used;
    };
    let try_take = |counts: &mut Counts, taken: &[usize], eye: bool| {
        if taken.iter().any(|&t| counts[t] == 0) || taken.iter().filter(|&&t| t == i).count() > counts[i] as usize {
            return false;
        }
        for &t in taken {
            counts[t] -= 1;
        }
        let ok = reference_complete(counts, eye);
        for &t in taken {
            counts[t] += 1;
        }
        ok
    };
    if try_take(counts, &[i, i, i], eye_used) {
        return true;
    }
    if i < SUITED_KINDS && i % 9 <= 6 && try_take(counts, &[i, i + 1, i + 2], eye_used) {
        return true;
    }
    !eye_used && try_take(counts, &[i, i], true)
}

fn to_counts(counts: &Counts) -> TileCounts {
    Tile::all().flat_map(|t| std::iter::repeat_n(t, counts[t.index()] as usize)).collect()
}

/// Kinds a small reference search draws from: Wan 1-9 and two honors.
const ALPHABET: [usize; 11] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 27, 28];

fn reference_ready(
    counts: &Counts,
    melds: usize,
) -> bool {
    ALPHABET.iter().any(|&t| {
        let mut with = *counts;
        with[t] += 1;
        with[t] <= 4 && 3 * (4 - melds) + 2 == with.iter().map(|&n| n as usize).sum::<usize>() && reference_complete(&mut with, false)
    })
}

/// Fewest single-tile swaps that make a waiting hand ready: the definition of shanten.
fn reference_shanten(
    start: Counts,
    melds: usize,
) -> i8 {
    let mut frontier = vec![start];
    let mut seen = HashSet::from([start]);
    for distance in 0.. {
        if frontier.iter().any(|c| reference_ready(c, melds)) {
            return distance;
        }
        let mut next = Vec::new();
        for counts in &frontier {
            for &out in ALPHABET.iter().filter(|&&t| counts[t] > 0) {
                for &inn in ALPHABET.iter().filter(|&&t| t != out && counts[t] < 4) {
                    let mut swapped = *counts;
                    swapped[out] -= 1;
                    swapped[inn] += 1;
                    if seen.insert(swapped) {
                        next.push(swapped);
                    }
                }
            }
        }
        frontier = next;
    }
    unreachable!()
}

fn counts_from(indices: &[usize]) -> Option<Counts> {
    let mut counts = [0u8; KINDS];
    for &i in indices {
        counts[i] += 1;
        if counts[i] > 4 {
            return None;
        }
    }
    Some(counts)
}

fn any_tiles(len: usize) -> impl Strategy<Value = Counts> {
    prop::collection::vec(0..KINDS, len).prop_filter_map("at most four copies", |v| counts_from(&v))
}

/// Hands assembled from four random sets and an eye, so they are always complete.
fn complete_hands() -> impl Strategy<Value = Counts> {
    let set = prop_oneof![
        (0..KINDS).prop_map(|t| vec![t; 3]),
        (0..3usize, 0..7usize).prop_map(|(suit, rank)| (0..3).map(|k| suit * 9 + rank + k).collect()),
    ];
    (prop::collection::vec(set, 4), 0..KINDS).prop_filter_map("at most four copies", |(sets, eye)| {
        let mut tiles: Vec<usize> = sets.concat();
        tiles.extend([eye, eye]);
        counts_from(&tiles)
    })
}

fn small_hands(len: usize) -> impl Strategy<Value = Counts> {
    prop::collection::vec(prop::sample::select(ALPHABET.to_vec()), len).prop_filter_map("at most four copies", |v| counts_from(&v))
}

proptest! {
    #[test]
    fn assembled_hands_are_complete(counts in complete_hands()) {
        let hand = to_counts(&counts);
        prop_assert!(is_complete(&hand, 0), "{hand}");
        prop_assert_eq!(shanten(&hand, 0), -1, "{}", hand);
    }

    #[test]
    fn greedy_win_detection_matches_exhaustive(counts in any_tiles(14)) {
        let hand = to_counts(&counts);
        prop_assert_eq!(is_complete(&hand, 0), reference_complete(&mut counts.clone(), false), "{}", hand);
    }

    #[test]
    fn shanten_minus_one_iff_complete(counts in any_tiles(14)) {
        let hand = to_counts(&counts);
        prop_assert_eq!(shanten(&hand, 0) == -1, is_complete(&hand, 0), "{}", hand);
    }

    #[test]
    fn waits_imply_ready(counts in any_tiles(13)) {
        let hand = to_counts(&counts);
        if !waits(&hand, 0).is_empty() {
            prop_assert_eq!(shanten(&hand, 0), 0, "{}", hand);
        }
    }

    #[test]
    fn one_draw_lowers_shanten_by_at_most_one(counts in any_tiles(13), draw in 0..KINDS) {
        prop_assume!(counts[draw] < 4);
        let hand = to_counts(&counts);
        let mut drawn = counts;
        drawn[draw] += 1;
        let before = shanten(&hand, 0);
        let after = shanten(&to_counts(&drawn), 0);
        prop_assert!(after >= before - 1 && after <= before, "{} + {}: {} -> {}", hand, Tile::from_index(draw).unwrap(), before, after);
    }

    /// Hands holding all four copies of a tile are excluded: the formula
    /// ignores the copy limit there (see the shanten module docs).
    #[test]
    fn shanten_matches_swap_distance(counts in small_hands(7).prop_filter("at most three copies", |c| c.iter().all(|&n| n < 4))) {
        let hand = to_counts(&counts);
        prop_assert_eq!(shanten(&hand, 2), reference_shanten(counts, 2), "{}", hand);
    }

    #[test]
    fn ukeire_matches_its_definition(counts in any_tiles(13), seen in any_tiles(10)) {
        let hand = to_counts(&counts);
        let visible: TileCounts = Tile::all()
            .flat_map(|t| std::iter::repeat_n(t, seen[t.index()].min(4 - counts[t.index()].min(4)) as usize))
            .collect();
        let fast = ukeire(&hand, 0, &visible);
        let base = shanten(&hand, 0);
        prop_assert_eq!(fast.shanten, base);
        for tile in Tile::all() {
            let live = 4u8.saturating_sub(hand.get(tile) + visible.get(tile));
            let improves = live > 0 && shanten(&hand.with(tile), 0) < base;
            prop_assert_eq!(fast.live_by_tile[tile.index()], if improves { live } else { 0 }, "{} + {}", hand, tile);
        }
    }

    #[test]
    fn discard_options_match_per_discard_ukeire(counts in any_tiles(14)) {
        let hand = to_counts(&counts);
        for option in discard_options(&hand, 0, &TileCounts::new()) {
            let mut after = hand;
            after.remove(option.tile);
            prop_assert_eq!(option.ukeire, ukeire(&after, 0, &TileCounts::new().with(option.tile)), "{} - {}", hand, option.tile);
        }
    }
}
