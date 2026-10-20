use mjers_bot::spec::BotSpec;
use mjers_hand::hand::Hand;
use mjers_tile::counts::TileCounts;

use super::*;
use crate::runner::play_hand;
use crate::study::ready::ready_shape;

fn table() -> Table {
    std::array::from_fn(|_| BotSpec::preset("efficiency").unwrap())
}

#[test]
fn every_entered_tile_has_one_fate() {
    let s = study(&Ruleset::family(), &table(), Seeds { start: 0, count: 300 }).unwrap();
    assert_eq!(s.hands, 300);
    for (i, f) in s.fates_by_tile.iter().enumerate() {
        assert_eq!(f.entered, f.discarded + f.melded + f.in_win + f.held_at_end, "tile {i}: {f:?}");
        assert!(f.discards_claimed + f.discards_won_on <= f.discarded);
        assert!(f.isolated_at_end <= f.held_at_end);
    }
    let wins: u64 = s.wins_by_split.values().map(|t| t.count).sum();
    assert_eq!(wins, s.wins_by_eye_class.values().map(|t| t.count).sum::<u64>());
    assert_eq!(s.seats_by_exposed_melds.values().map(|t| t.count).sum::<u64>(), 1200);
    assert_eq!(s.seats_by_exposed_melds.values().map(|t| t.wins).sum::<u64>(), wins);
    assert!(s.ready_by_shape.values().map(|t| t.count).sum::<u64>() > 300, "most seats get ready at some point");
}

#[test]
fn splits_have_four_sets() {
    let s = study(&Ruleset::family(), &table(), Seeds { start: 0, count: 100 }).unwrap();
    for key in s.wins_by_split.keys() {
        let total: usize = key.split(' ').map(|part| part[..part.len() - 1].parse::<usize>().unwrap()).sum();
        assert_eq!(total, 4, "{key}");
    }
}

#[test]
fn watched_play_matches_plain_play() {
    let rules = Arc::new(Ruleset::family());
    for seed in 0..20 {
        let plain = play_hand(&rules, &table(), seed).unwrap();
        let watched = play_hand_watched(&rules, &table(), seed, |_| {}).unwrap();
        assert_eq!(plain.steps(), watched.steps());
    }
}

#[test]
fn classifies_ready_shapes() {
    let shape = |notation: &str| ready_shape(&Hand::new(notation.parse().unwrap()), &TileCounts::new()).map(|s| (s.name, s.waits, s.live));
    assert_eq!(shape("11m78m123p456p789s"), Some(("two-sided chow", 2, 8)));
    assert_eq!(shape("789m1m123p456p789s"), Some(("eye (tanki)", 1, 3)));
    assert_eq!(shape("11m55z123p456p789s"), Some(("pair to pong", 2, 4)));
    assert_eq!(shape("11m79m123p456p789s"), Some(("gap chow", 1, 4)));
    assert_eq!(shape("11z12m123p456p789s"), Some(("edge chow", 1, 4)));
    assert_eq!(shape("1112345678999m"), Some(("multi-sided (3+ tiles)", 9, 23)));
    assert_eq!(shape("12m45m123p456p789s"), None);
}

#[test]
fn classes_group_suits_and_honors() {
    let class = |s: &str| CLASSES[class_of(s.parse().unwrap())];
    assert_eq!((class("1m"), class("1p"), class("9s")), ("1", "1", "9"));
    assert_eq!((class("4z"), class("5z"), class("7z")), ("wind", "dragon", "dragon"));
}

#[test]
fn composition_and_turns_are_consistent() {
    let s = study(&Ruleset::family(), &table(), Seeds { start: 0, count: 300 }).unwrap();
    let c = &s.composition;
    let wins: u64 = s.wins_by_split.values().map(|t| t.count).sum();
    assert_eq!(c.hands, wins);
    let sets = c.exposed_chows + c.concealed_chows + c.exposed_pongs + c.concealed_pongs + c.exposed_kongs + c.concealed_kongs;
    assert_eq!(sets, 4 * wins);
    assert_eq!(c.terminal_chows + c.inner_chows, c.exposed_chows + c.concealed_chows);
    assert_eq!(c.claimed_two_sided + c.claimed_gap + c.claimed_edge, c.exposed_chows);
    assert_eq!(c.chow_suits.iter().sum::<u64>(), c.multi_chow_hands);
    assert!(c.run_of_four_plus <= c.overlapping + c.back_to_back);
    let ready: u64 = s.ready_by_wait_count.values().map(|t| t.count).sum();
    assert_eq!(ready, s.ready_by_shape.values().map(|t| t.count).sum::<u64>());
    for t in s.ready_by_shape.values() {
        assert!(t.self_draws <= t.wins);
        assert!(t.own_draws <= t.table_turns + t.wins);
    }
}
