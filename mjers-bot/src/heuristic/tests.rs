use std::sync::Arc;

use mjers_game::{
    Game,
    Ruleset,
    Seat,
};

use super::*;
use crate::policy::{
    RandomBot,
    play_out,
};

fn hand(notation: &str) -> Hand {
    Hand::new(notation.parse().unwrap())
}

fn tile(notation: &str) -> Tile {
    notation.parse().unwrap()
}

#[test]
fn discards_the_isolated_honor() {
    assert_eq!(best_discard(&hand("123m456p78s1122z9m5z"), &TileCounts::new()), tile("5z"));
}

#[test]
fn breaks_ties_by_connectivity() {
    // 1z and 9s are both useless; 9s touches 7s/8s.
    assert_eq!(best_discard(&hand("123m456p789s78s11z1z"), &TileCounts::new()), tile("1z"));
}

#[test]
fn visible_tiles_steer_the_discard() {
    // Keeping 45m (waits 3m/6m) vs 78m (6m/9m): if 3m is dead, keep 78m.
    let both = hand("45m78m456p789s111z");
    let dead_three: TileCounts = "3333m".parse().unwrap();
    let discard = best_discard(&both, &dead_three);
    assert!(discard == tile("4m") || discard == tile("5m"), "{discard}");
}

#[test]
fn claims_improving_pongs_only() {
    let h = hand("55z123m456p78s19m");
    assert_eq!(best_claim(&h, tile("5z"), &[Action::Pong, Action::Pass]), Action::Pong);
    let complete_sets = hand("111m234p567s789s5z");
    assert_eq!(best_claim(&complete_sets, tile("1m"), &[Action::Pong, Action::Kong, Action::Pass]), Action::Kong);
}

#[test]
fn declines_chows_that_do_not_help() {
    let h = hand("123m456p789s1122z");
    assert_eq!(best_claim(&h, tile("4m"), &[Action::Chow(tile("2m")), Action::Pass]), Action::Pass);
}

#[test]
fn connectivity_counts_neighbours_in_suit_only() {
    let counts: TileCounts = "79m1p11z".parse().unwrap();
    assert_eq!(connectivity(&counts, tile("9m")), 1);
    assert_eq!(connectivity(&counts, tile("1p")), 0);
    assert_eq!(connectivity(&counts, tile("1z")), 1);
}

#[test]
fn efficiency_beats_random_play() {
    let rules = Arc::new(Ruleset::family());
    let mut efficiency_total = 0i64;
    for seed in 0..300 {
        let mut game = Game::new(rules.clone(), seed, Seat::ALL[(seed % 4) as usize]);
        let mut table: [Box<dyn Policy>; 4] = std::array::from_fn(|i| {
            if i == 0 {
                Box::new(EfficiencyBot) as Box<dyn Policy>
            } else {
                Box::new(RandomBot::new(seed * 4 + i as u64))
            }
        });
        play_out(&mut game, &mut table);
        efficiency_total += game.settlement().unwrap().total()[0] as i64;
    }
    assert!(efficiency_total > 300, "efficiency bot won {efficiency_total} units over 300 hands");
}
