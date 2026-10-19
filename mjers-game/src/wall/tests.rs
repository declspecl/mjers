use std::collections::HashMap;

use super::*;

#[test]
fn full_set_has_every_tile() {
    let mut counts: HashMap<WallTile, usize> = HashMap::new();
    for tile in full_set() {
        *counts.entry(tile).or_default() += 1;
    }
    assert_eq!(counts.len(), 34 + 8);
    assert!(Tile::all().all(|t| counts[&WallTile::Tile(t)] == 4));
    assert!(Bonus::all().all(|b| counts[&WallTile::Bonus(b)] == 1));
}

#[test]
fn shuffle_is_deterministic_per_seed() {
    let a = Wall::shuffled(&mut Rng::seed(5), 16);
    let b = Wall::shuffled(&mut Rng::seed(5), 16);
    let c = Wall::shuffled(&mut Rng::seed(6), 16);
    assert_eq!(a, b);
    assert_ne!(a, c);
    let mut sorted = a.undrawn().to_vec();
    sorted.sort_by_key(|t| format!("{t:?}"));
    let mut expected = full_set().to_vec();
    expected.sort_by_key(|t| format!("{t:?}"));
    assert_eq!(sorted, expected);
}

#[test]
fn live_draws_stop_at_the_flower_wall() {
    let mut wall = Wall::shuffled(&mut Rng::seed(1), 16);
    let mut drawn = 0;
    while wall.draw().is_some() {
        drawn += 1;
    }
    assert_eq!(drawn, WALL_SIZE - 16);
    assert_eq!(wall.remaining(), 16);
    assert!(wall.draw_replacement().is_some());
}

#[test]
fn replacements_come_from_the_back_and_shorten_live_draws() {
    let order = full_set();
    let mut wall = Wall::from_order(order, 16);
    assert_eq!(wall.draw(), Some(order[0]));
    assert_eq!(wall.draw_replacement(), Some(order[WALL_SIZE - 1]));
    assert_eq!(wall.draw_replacement(), Some(order[WALL_SIZE - 2]));
    assert_eq!(wall.live_remaining(), WALL_SIZE - 3 - 16);
}

#[test]
fn replacements_run_out_only_when_the_wall_is_empty() {
    let mut wall = Wall::from_order(full_set(), 16);
    for _ in 0..WALL_SIZE {
        assert!(wall.draw_replacement().is_some());
    }
    assert_eq!(wall.draw_replacement(), None);
    assert_eq!(wall.draw(), None);
}
