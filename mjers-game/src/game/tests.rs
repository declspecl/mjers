use mjers_tile::Bonus;

use super::*;
use crate::action::{
    Action,
    GameError,
};
use crate::testing::*;
use crate::wall::WALL_SIZE;

fn tiles_accounted_for(game: &Game) -> usize {
    let held: usize = S.iter().map(|&s| game.hand(s).concealed().len() + game.hand(s).bonus().len() as usize).sum();
    held + game.wall().remaining()
}

#[test]
fn deal_gives_dealer_fourteen() {
    for seed in 0..50 {
        let game = Game::new(family(), seed, S[2]);
        assert_eq!(game.to_act(), [S[2]]);
        for seat in S {
            let expected = if seat == S[2] { DEALT_TILES + 1 } else { DEALT_TILES };
            assert_eq!(game.hand(seat).size(), expected, "seed {seed} {seat}");
        }
        assert_eq!(tiles_accounted_for(&game), WALL_SIZE);
    }
}

#[test]
fn seed_determines_the_deal() {
    let a = Game::new(family(), 11, S[0]);
    let b = Game::new(family(), 11, S[0]);
    let c = Game::new(family(), 12, S[0]);
    assert!(S.iter().all(|&s| a.hand(s) == b.hand(s)));
    assert!(S.iter().any(|&s| a.hand(s) != c.hand(s)));
}

#[test]
fn stacked_wall_deals_exactly() {
    let hands = ["123456789m1234z", "123456789p1234z", "123456789s1234z", "111222333444m5z"];
    let game = game(hands, "7z", "");
    assert_eq!(game.hand(S[0]).concealed().to_string(), "123456789m12347z");
    assert_eq!(game.last_draw(), Some(tile("7z")));
    assert_eq!(game.hand(S[3]).concealed().to_string(), "111222333444m5z");
}

#[test]
fn dealt_flowers_are_replaced_from_the_back() {
    let hands = ["123456789m1234z", "12345678p1234z1F", "123456789s1234z", "11122233344m5z2S"];
    let game = game(hands, "6z", "9p4m");
    assert_eq!(game.hand(S[1]).concealed().to_string(), "123456789p1234z");
    assert_eq!(game.hand(S[3]).concealed().to_string(), "111222333444m5z");
    assert!(game.hand(S[1]).bonus().contains(Bonus::Flower(1)));
    assert!(game.hand(S[3]).bonus().contains(Bonus::Season(2)));
}

#[test]
fn drawn_flowers_chain_replacements() {
    let hands = ["123456789m1234z", "123456789p1234z", "123456789s1234z", "111222333444m5z"];
    let game = game(hands, "3F", "4F7z");
    assert_eq!(game.hand(S[0]).bonus().len(), 2);
    assert_eq!(game.last_draw(), Some(tile("7z")));
    assert_eq!(game.hand(S[0]).size(), DEALT_TILES + 1);
}

const PLAIN: [&str; 4] = ["123456789m1234z", "123456789p1234z", "123456789s1234z", "111222333444m5z"];

#[test]
fn discard_passes_the_turn_with_a_draw() {
    let mut game = game(PLAIN, "7z6z", "");
    game.apply(S[0], Action::Discard(tile("7z"))).unwrap();
    assert_eq!(game.to_act(), [S[1]]);
    assert_eq!(game.last_draw(), Some(tile("6z")));
    assert_eq!(
        game.discards(),
        [Discard {
            seat: S[0],
            tile: tile("7z"),
            claimed: false
        }]
    );
    assert_eq!(game.public_tiles().to_string(), "7z");
}

#[test]
fn rejects_out_of_turn_and_unheld_discards() {
    let mut game = game(PLAIN, "7z", "");
    assert_eq!(game.apply(S[1], Action::Discard(tile("1p"))), Err(GameError::NotAsked(S[1])));
    let illegal = Action::Discard(tile("9s"));
    assert_eq!(game.apply(S[0], illegal), Err(GameError::Illegal { seat: S[0], action: illegal }));
    assert_eq!(game.to_act(), [S[0]], "rejected actions change nothing");
}

#[test]
fn hand_ends_without_winner_when_live_wall_runs_out() {
    let mut game = Game::new(family(), 3, S[0]);
    let mut turns = 0;
    while let Some(&seat) = game.to_act().first() {
        let tile = game.last_draw().unwrap_or_else(|| game.hand(seat).concealed().kinds().next().unwrap());
        game.apply(seat, Action::Discard(tile)).unwrap();
        turns += 1;
    }
    assert_eq!(game.result(), Some(&HandResult::default()));
    assert_eq!(game.wall().live_remaining(), 0);
    assert_eq!(turns, game.discards().len());
    assert_eq!(game.apply(S[0], Action::Pass), Err(GameError::HandOver));
    assert_eq!(tiles_accounted_for(&game) + game.discards().len(), WALL_SIZE);
}
