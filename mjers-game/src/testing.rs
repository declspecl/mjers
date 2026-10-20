//! Helpers for arranging walls in tests.

use std::sync::Arc;

use mjers_rules::ruleset::Ruleset;
use mjers_rules::seat::Seat;
use mjers_tile::tile::{
    Bonus,
    Tile,
};

use crate::game::Game;
use crate::wall::{
    WALL_SIZE,
    Wall,
    WallTile,
    full_set,
};

/// Tiles in the order written. Besides `m`/`p`/`s`/`z`, `F` and `S` mark
/// flowers and seasons, so `"5m1F2z"` is 5m, flower 1, 2z.
pub fn ordered(notation: &str) -> Vec<WallTile> {
    let mut tiles = Vec::new();
    let mut digits = Vec::new();
    for c in notation.chars() {
        if let Some(d) = c.to_digit(10) {
            digits.push(d as u8);
            continue;
        }
        for &d in &digits {
            tiles.push(match c {
                'F' => WallTile::Bonus(Bonus::Flower(d)),
                'S' => WallTile::Bonus(Bonus::Season(d)),
                _ => WallTile::Tile(format!("{d}{c}").parse::<Tile>().unwrap()),
            });
        }
        digits.clear();
    }
    tiles
}

/// A wall that deals `hands` (indexed by seat, 13 tiles each), then yields
/// `draws` as live draws and `replacements` as replacement draws, in order.
/// Every other tile sits in between.
pub fn stacked(
    dealer: Seat,
    hands: [&str; 4],
    draws: &str,
    replacements: &str,
) -> Wall {
    let mut hands = hands.map(|h| ordered(h).into_iter());
    let mut order = Vec::with_capacity(WALL_SIZE);
    let seats: Vec<Seat> = std::iter::once(dealer).chain(dealer.others()).collect();
    for take in [4, 4, 4, 1] {
        for seat in &seats {
            for _ in 0..take {
                order.push(hands[seat.index()].next().expect("13 tiles per hand"));
            }
        }
    }
    assert!(hands.iter_mut().all(|h| h.next().is_none()), "13 tiles per hand");
    order.extend(ordered(draws));
    let back: Vec<WallTile> = ordered(replacements).into_iter().rev().collect();

    let mut rest = full_set().to_vec();
    for tile in order.iter().chain(&back) {
        let i = rest.iter().position(|t| t == tile).unwrap_or_else(|| panic!("more than four of {tile:?}"));
        rest.remove(i);
    }
    // Bonus tiles last among the filler, so short tests never meet them.
    rest.sort_by_key(|t| matches!(t, WallTile::Bonus(_)));
    order.extend(rest);
    order.extend(back);
    Wall::from_order(order.try_into().expect("144 tiles"), Ruleset::family().reserved_tiles())
}

pub fn family() -> Arc<Ruleset> {
    Arc::new(Ruleset::family())
}

pub fn game(
    hands: [&str; 4],
    draws: &str,
    replacements: &str,
) -> Game {
    Game::from_wall(family(), stacked(Seat::ALL[0], hands, draws, replacements), Seat::ALL[0]).unwrap()
}

pub fn tile(notation: &str) -> Tile {
    notation.parse().unwrap()
}

pub const S: [Seat; 4] = Seat::ALL;

pub fn seats(seats: &[Seat]) -> mjers_rules::seat::SeatSet {
    seats.iter().copied().collect()
}
