use mjers_tile::tile::Suit;

use super::*;
use crate::meld::Meld;

fn complete(
    notation: &str,
    melds: usize,
) -> bool {
    is_complete(&notation.parse().unwrap(), melds)
}

fn wait_list(notation: &str) -> String {
    waits(&notation.parse().unwrap(), 0).into_iter().collect::<TileCounts>().to_string()
}

#[test]
fn recognises_standard_wins() {
    for notation in ["123m456p789s11122z", "11123456789999m", "111222333m44455p", "123123123m45666p", "11z222z333z444z555z"] {
        assert!(complete(notation, 0), "{notation}");
    }
}

#[test]
fn rejects_non_wins() {
    for notation in [
        "123m456p789s11123z",   // honor chow
        "9m12p111z222z333z44z", // chow across suits
        "19m19p19s12345677z",   // thirteen orphans is not a standard win
        "1122m3344p5566s77z",   // seven pairs is not a standard win
        "123m456p789s1112z",    // wrong size
    ] {
        assert!(!complete(notation, 0), "{notation}");
    }
}

#[test]
fn melds_reduce_required_concealed_sets() {
    assert!(complete("123m11z", 3));
    assert!(complete("55z", 4));
    assert!(!complete("123m11z", 2));
    assert!(!complete("55z", 5));
}

#[test]
fn nine_gates_waits_on_every_wan() {
    assert_eq!(wait_list("1112345678999m"), "123456789m");
}

#[test]
fn waits_cover_eye_and_set_completion() {
    assert_eq!(wait_list("123m456p789s1112z"), "2z");
    assert_eq!(wait_list("123m456p789s11z45m"), "36m");
    assert_eq!(wait_list("123m456p789s11z22z"), "12z");
    assert_eq!(wait_list("123m456p789s1z222z"), "1z");
}

#[test]
fn no_wait_on_a_fifth_copy() {
    assert_eq!(wait_list("1111m456p789s222z"), "");
}

#[test]
fn hand_level_queries_include_melds() {
    let mut hand = Hand::new("11123m55z".parse().unwrap());
    hand.claim_pong(Tile::suited(Suit::Wan, 1).unwrap()).unwrap();
    hand.discard(Tile::suited(Suit::Wan, 3).unwrap()).unwrap();
    assert_eq!(hand.melds().collect::<Vec<_>>(), [Meld::Pong(Tile::suited(Suit::Wan, 1).unwrap())]);
    // 1m 2m 5z 5z beside the pong: not yet a 13-tile-equivalent hand.
    assert!(hand.waits().is_empty());
    let ready = Hand::new("123m456p789s1112z".parse().unwrap());
    assert!(ready.wins_with("2z".parse().unwrap()));
    assert!(!ready.wins_with("3z".parse().unwrap()));
    let mut won = ready;
    won.add_tile("2z".parse().unwrap());
    assert!(won.is_winning());
}

#[test]
fn decompose_finds_eye_and_sets() {
    let (eye, sets) = decompose(&"123m456p789s11122z".parse().unwrap(), 0).unwrap();
    assert_eq!(eye.to_string(), "2z");
    assert_eq!(sets.len(), 4);
    assert_eq!(sets.iter().filter(|m| matches!(m, Meld::Chow(_))).count(), 3);
    let hand: TileCounts = "11122233344455m".parse().unwrap();
    let (eye, sets) = decompose(&hand, 0).unwrap();
    let mut rebuilt = TileCounts::new().with(eye).with(eye);
    for tile in sets.iter().flat_map(|m| m.tiles().tiles().collect::<Vec<_>>()) {
        rebuilt.add(tile);
    }
    assert_eq!(rebuilt, hand);
    let (_, with_melds) = decompose(&"345s11z".parse().unwrap(), 3).unwrap();
    assert_eq!(with_melds, [Meld::Chow("3s".parse().unwrap())]);
    assert!(decompose(&"123m456p789s11123z".parse().unwrap(), 0).is_none());
}
