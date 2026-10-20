use mjers_tile::tile::{
    Honor,
    Suit,
};

use super::*;

fn hand(notation: &str) -> Hand {
    Hand::new(notation.parse().unwrap())
}

fn m(rank: u8) -> Tile {
    Tile::suited(Suit::Wan, rank).unwrap()
}

#[test]
fn size_counts_melds_as_three() {
    let mut h = hand("11123m456p789s12z");
    assert_eq!(h.size(), 13);
    h.claim_pong(m(1)).unwrap();
    assert_eq!(h.concealed().len(), 11);
    assert_eq!(h.size(), 14);
    assert!(h.is_awaiting_discard());
}

#[test]
fn claim_pong_needs_two_copies() {
    let mut h = hand("1m23p");
    assert_eq!(h.claim_pong(m(1)), Err(HandError::MissingTiles(m(1))));
    assert_eq!(h, hand("1m23p"), "failed claim leaves hand unchanged");
}

#[test]
fn claim_chow_uses_the_claimed_tile_in_any_position() {
    for (notation, claimed) in [("23m", 1), ("13m", 2), ("12m", 3)] {
        let mut h = hand(notation);
        assert_eq!(h.claim_chow(m(1), m(claimed)), Ok(Meld::Chow(m(1))), "{notation}");
        assert!(h.concealed().is_empty());
    }
}

#[test]
fn claim_chow_rejects_invalid_shapes() {
    let mut h = hand("89m");
    assert_eq!(h.claim_chow(m(8), m(7)), Err(HandError::NotAChow(m(8))));
    assert_eq!(h.claim_chow(m(7), m(4)), Err(HandError::NotAChow(m(7))));
    assert_eq!(h.claim_chow(m(7), m(9)), Err(HandError::MissingTiles(m(7))));
    assert_eq!(h, hand("89m"), "failed claim leaves hand unchanged");
}

#[test]
fn concealed_kong_and_upgrade() {
    let mut h = hand("1111m55z");
    assert_eq!(h.declare_concealed_kong(m(1)), Ok(Meld::Kong(m(1), KongKind::Concealed)));
    let white = Tile::honor(Honor::White);
    h.claim_pong(white).unwrap();
    assert_eq!(h.upgrade_kong(white), Err(HandError::MissingTiles(white)));
    h.add_tile(white);
    assert_eq!(h.upgrade_kong(white), Ok(Meld::Kong(white, KongKind::Exposed)));
    assert_eq!(h.meld_count(), 2);
    assert_eq!(h.to_string(), " [1111m*] [5555z]");
}

#[test]
fn upgrade_requires_existing_pong() {
    let mut h = hand("1m");
    assert_eq!(h.upgrade_kong(m(1)), Err(HandError::NoPongToUpgrade(m(1))));
}

#[test]
fn at_most_four_melds() {
    let mut h = hand("1111222233334444m55z");
    for rank in 1..=4 {
        h.declare_concealed_kong(m(rank)).unwrap();
    }
    assert_eq!(h.claim_pong(Tile::honor(Honor::White)), Err(HandError::TooManyMelds));
}

#[test]
fn bonus_set_tracks_flowers_and_seasons() {
    let mut h = hand("");
    h.add_bonus(Bonus::Flower(1));
    h.add_bonus(Bonus::Season(4));
    h.add_bonus(Bonus::Season(4));
    assert_eq!(h.bonus().len(), 2);
    assert_eq!(h.bonus().iter().collect::<Vec<_>>(), [Bonus::Flower(1), Bonus::Season(4)]);
}
