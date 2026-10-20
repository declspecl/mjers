use std::sync::Arc;

use mjers_game::game::Game;
use mjers_rules::ruleset::Ruleset;
use mjers_rules::seat::Seat;
use mjers_tile::tile::Tile;

use super::*;
use crate::params::{
    ClaimRule,
    KongRule,
    Params,
};
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
fn claims_improving_pongs_only() {
    let h = hand("55z123m456p78s19m");
    assert_eq!(HeuristicBot::efficiency().claim(&h, tile("5z"), &[Action::Pong, Action::Pass]), Action::Pong);
    let complete_sets = hand("111m234p567s789s5z");
    assert_eq!(HeuristicBot::efficiency().claim(&complete_sets, tile("1m"), &[Action::Pong, Action::Kong, Action::Pass]), Action::Kong);
}

#[test]
fn declines_chows_that_do_not_help() {
    let h = hand("123m456p789s1122z");
    assert_eq!(HeuristicBot::efficiency().claim(&h, tile("4m"), &[Action::Chow(tile("2m")), Action::Pass]), Action::Pass);
}

#[test]
fn efficiency_beats_random_play() {
    let rules = Arc::new(Ruleset::family());
    let mut efficiency_total = 0i64;
    for seed in 0..300 {
        let mut game = Game::new(rules.clone(), seed, Seat::ALL[(seed % 4) as usize]).unwrap();
        let mut table: [Box<dyn Policy>; 4] = std::array::from_fn(|i| {
            if i == 0 {
                Box::new(HeuristicBot::efficiency()) as Box<dyn Policy>
            } else {
                Box::new(RandomBot::new(seed * 4 + i as u64))
            }
        });
        play_out(&mut game, &mut table).unwrap();
        efficiency_total += game.settlement().unwrap().total()[0] as i64;
    }
    assert!(efficiency_total > 300, "efficiency bot won {efficiency_total} units over 300 hands");
}

/// Deal-ins by seat 0 over `hands` hands, with seat 0 using `bot`.
fn deal_ins(
    hands: u64,
    bot: impl Fn() -> Box<dyn Policy>,
) -> usize {
    let rules = Arc::new(Ruleset::family());
    (0..hands)
        .filter(|&seed| {
            let mut game = Game::new(rules.clone(), seed, Seat::ALL[(seed % 4) as usize]).unwrap();
            let mut table: [Box<dyn Policy>; 4] = std::array::from_fn(|i| if i == 0 { bot() } else { Box::new(HeuristicBot::efficiency()) as Box<dyn Policy> });
            play_out(&mut game, &mut table).unwrap();
            game.result().unwrap().win.is_some_and(|w| w.source == mjers_rules::settlement::WinSource::Discard(Seat::ALL[0]))
        })
        .count()
}

#[test]
fn defensive_bot_deals_in_less() {
    let efficient = deal_ins(300, || Box::new(HeuristicBot::efficiency()));
    let defensive = deal_ins(300, || Box::new(HeuristicBot::defensive()));
    assert!(defensive < efficient, "defensive {defensive} vs efficient {efficient} deal-ins");
}

#[test]
fn claim_rules_gate_claims() {
    let h = hand("55z123m456p78s19m");
    let never = HeuristicBot::new(
        "closed",
        Params {
            pong: ClaimRule::Never,
            ..Params::efficiency()
        },
    );
    assert_eq!(never.claim(&h, tile("5z"), &[Action::Pong, Action::Pass]), Action::Pass);
    let ready = hand("123m456p789s1122z");
    let always = HeuristicBot::new(
        "eager",
        Params {
            chow: ClaimRule::Always,
            ..Params::efficiency()
        },
    );
    assert_eq!(always.claim(&ready, tile("4m"), &[Action::Chow(tile("2m")), Action::Pass]), Action::Chow(tile("2m")));
    let late = HeuristicBot::new(
        "late",
        Params {
            claim_max_shanten: 0,
            ..Params::efficiency()
        },
    );
    assert_eq!(late.claim(&h, tile("5z"), &[Action::Pong, Action::Pass]), Action::Pass);
}

#[test]
fn kong_rules_gate_kongs() {
    let h = hand("111m234p567s789s5z");
    let never = HeuristicBot::new(
        "no-kong",
        Params {
            exposed_kong: KongRule::Never,
            ..Params::efficiency()
        },
    );
    assert_eq!(never.claim(&h, tile("1m"), &[Action::Pong, Action::Kong, Action::Pass]), Action::Pass);
}
