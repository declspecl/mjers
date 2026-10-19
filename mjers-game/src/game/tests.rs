use mjers_tile::Bonus;

use mjers_hand::{
    KongKind,
    Meld,
};
use mjers_rules::WinSource;

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
        if game.legal_actions(seat).contains(&Action::Pass) {
            game.apply(seat, Action::Pass).unwrap();
            continue;
        }
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

/// Seat 0 discards 9s. Seat 1 and seat 3 can win on it; seat 2 can pong it.
const CONTESTED: [&str; 4] = ["123456789m1234z", "123456789p1119s", "1234567s99s5566z", "78s222333444z66z"];

fn contested() -> Game {
    let mut game = game(CONTESTED, "9s5m", "");
    game.apply(S[0], Action::Discard(tile("9s"))).unwrap();
    game
}

#[test]
fn claim_window_asks_only_seats_with_options() {
    let game = contested();
    assert_eq!(game.to_act(), [S[1], S[2], S[3]]);
    assert_eq!(game.legal_actions(S[1]), [Action::Win, Action::Pass]);
    assert_eq!(game.legal_actions(S[2]), [Action::Pong, Action::Pass]);
    // seat 3 is not after the discarder, so 78s cannot chow
    assert_eq!(game.legal_actions(S[3]), [Action::Win, Action::Pass]);
}

#[test]
fn closest_winner_takes_the_discard() {
    let mut game = contested();
    game.apply(S[3], Action::Win).unwrap();
    game.apply(S[2], Action::Pong).unwrap();
    assert_eq!(game.to_act(), [S[1]], "waits for every asked seat");
    game.apply(S[1], Action::Win).unwrap();
    let win = game.result().unwrap().win.unwrap();
    assert_eq!((win.winner, win.source), (S[1], WinSource::Discard(S[0])));
    assert!(game.hand(S[1]).is_winning());
    assert!(game.discards()[0].claimed);
}

#[test]
fn farther_win_beats_closer_pong() {
    let mut game = contested();
    game.apply(S[1], Action::Pass).unwrap();
    game.apply(S[2], Action::Pong).unwrap();
    game.apply(S[3], Action::Win).unwrap();
    assert_eq!(game.result().unwrap().win.unwrap().winner, S[3]);
}

#[test]
fn pong_takes_the_turn_without_a_draw() {
    let mut game = contested();
    game.apply(S[1], Action::Pass).unwrap();
    game.apply(S[3], Action::Pass).unwrap();
    game.apply(S[2], Action::Pong).unwrap();
    assert_eq!(game.phase(), &Phase::Turn { seat: S[2], after_claim: true });
    assert_eq!(game.hand(S[2]).melds().collect::<Vec<_>>(), [Meld::Pong(tile("9s"))]);
    assert_eq!(game.last_draw(), None);
    assert!(game.legal_actions(S[2]).iter().all(|a| matches!(a, Action::Discard(_))));
    assert_eq!(game.public_tiles().to_string(), "999s");
    game.apply(S[2], Action::Discard(tile("5z"))).unwrap();
    assert_eq!(game.to_act(), [S[3]], "play continues after the claimer");
}

#[test]
fn everyone_passing_moves_to_the_next_draw() {
    let mut game = contested();
    for seat in [S[1], S[2], S[3]] {
        game.apply(seat, Action::Pass).unwrap();
    }
    assert_eq!(game.phase(), &Phase::Turn { seat: S[1], after_claim: false });
    assert_eq!(game.last_draw(), Some(tile("5m")));
}

#[test]
fn only_the_next_seat_may_chow_in_any_position() {
    let hands = ["123456789m1234z", "24p3456789s1234z", "13p3456789s123z5z", "123456789p1235z"];
    let mut game = game(hands, "3p", "");
    game.apply(S[0], Action::Discard(tile("3p"))).unwrap();
    assert_eq!(game.legal_actions(S[1]), [Action::Chow(tile("2p")), Action::Pass]);
    assert_eq!(game.legal_actions(S[2]), [], "seat 2 holds 1p/3p but is not next");
    assert_eq!(game.to_act(), [S[1]]);
    game.apply(S[1], Action::Chow(tile("2p"))).unwrap();
    assert_eq!(game.hand(S[1]).melds().collect::<Vec<_>>(), [Meld::Chow(tile("2p"))]);
    assert_eq!(game.public_tiles().to_string(), "234p");
}

/// Passes every open claim window.
fn pass_claims(game: &mut Game) {
    while let Phase::Claim { .. } = game.phase() {
        let seat = game.to_act()[0];
        game.apply(seat, Action::Pass).unwrap();
    }
}

#[test]
fn concealed_kong_draws_a_replacement() {
    let hands = ["1111m56789m1234z", "123456789p1234z", "123456789s1234z", "23456789m55667z"];
    let mut game = game(hands, "7z", "7p");
    assert!(game.legal_actions(S[0]).contains(&Action::ConcealedKong(tile("1m"))));
    game.apply(S[0], Action::ConcealedKong(tile("1m"))).unwrap();
    assert_eq!(game.hand(S[0]).melds().collect::<Vec<_>>(), [Meld::Kong(tile("1m"), KongKind::Concealed)]);
    assert_eq!(game.phase(), &Phase::Turn { seat: S[0], after_claim: false });
    assert_eq!(game.last_draw(), Some(tile("7p")));
    assert_eq!(game.hand(S[0]).size(), DEALT_TILES + 1);
    assert_eq!(game.public_tiles().to_string(), "1111m");
}

#[test]
fn exposed_kong_from_a_discard() {
    let hands = ["123456789m1234z", "123456789p1234z", "123478999s5566z", "123456789m5677z"];
    let mut game = game(hands, "9s", "1p");
    game.apply(S[0], Action::Discard(tile("9s"))).unwrap();
    assert_eq!(game.legal_actions(S[2]), [Action::Pong, Action::Kong, Action::Pass]);
    game.apply(S[2], Action::Kong).unwrap();
    assert_eq!(game.hand(S[2]).melds().collect::<Vec<_>>(), [Meld::Kong(tile("9s"), KongKind::Exposed)]);
    assert_eq!(game.phase(), &Phase::Turn { seat: S[2], after_claim: false });
    assert_eq!(game.last_draw(), Some(tile("1p")));
    assert_eq!(game.public_tiles().to_string(), "9999s");
}

#[test]
fn pong_upgrades_when_the_fourth_tile_is_drawn() {
    let hands = ["123456789m1234z", "123456789p1255z", "123456789s1234z", "123456789m3467z"];
    let mut game = game(hands, "5z1s2s3s5z", "6p");
    game.apply(S[0], Action::Discard(tile("5z"))).unwrap();
    game.apply(S[1], Action::Pong).unwrap();
    for (seat, discard) in [(S[1], "1z"), (S[2], "1s"), (S[3], "2s"), (S[0], "3s")] {
        game.apply(seat, Action::Discard(tile(discard))).unwrap();
        pass_claims(&mut game);
    }
    assert_eq!(game.last_draw(), Some(tile("5z")));
    assert!(game.legal_actions(S[1]).contains(&Action::UpgradeKong(tile("5z"))));
    game.apply(S[1], Action::UpgradeKong(tile("5z"))).unwrap();
    assert_eq!(game.hand(S[1]).melds().collect::<Vec<_>>(), [Meld::Kong(tile("5z"), KongKind::Exposed)]);
    assert_eq!(game.last_draw(), Some(tile("6p")));
}

#[test]
fn no_kong_declarations_right_after_a_claim() {
    let hands = ["123456789m1234z", "123456789p1255z", "123456789s1234z", "123456789m3467z"];
    let mut game = game(hands, "5z", "");
    game.apply(S[0], Action::Discard(tile("5z"))).unwrap();
    game.apply(S[1], Action::Pong).unwrap();
    assert!(game.legal_actions(S[1]).iter().all(|a| matches!(a, Action::Discard(_))));
}

#[test]
fn self_drawn_win_settles_three_each_plus_flowers() {
    let hands = ["123456789m1112z", "123456789p5566z", "123456789s5567z", "123456789m3467z"];
    let mut game = game(hands, "1F", "2z");
    assert_eq!(game.legal_actions(S[0])[0], Action::SelfDrawWin);
    game.apply(S[0], Action::SelfDrawWin).unwrap();
    let win = game.result().unwrap().win.unwrap();
    assert_eq!((win.winner, win.source, win.bonus_tiles), (S[0], WinSource::SelfDraw, 1));
    assert_eq!(game.settlement().unwrap().total(), [12, -4, -4, -4]);
    assert!(game.to_act().is_empty());
}

#[test]
fn discard_win_settles_with_the_discarder_paying_double() {
    let mut game = contested();
    game.apply(S[1], Action::Win).unwrap();
    game.apply(S[2], Action::Pass).unwrap();
    game.apply(S[3], Action::Pass).unwrap();
    assert_eq!(game.settlement().unwrap().total(), [-2, 4, -1, -1]);
}

#[test]
fn kongs_are_paid_when_the_wall_runs_out() {
    let hands = ["1111m56789m1234z", "123456789p1234z", "123456789s1234z", "23456789m55667z"];
    let mut game = game(hands, "7z", "7p");
    game.apply(S[0], Action::ConcealedKong(tile("1m"))).unwrap();
    while let Some(&seat) = game.to_act().first() {
        let legal = game.legal_actions(seat);
        let action = if legal.contains(&Action::Pass) {
            Action::Pass
        } else {
            *legal.iter().rev().find(|a| matches!(a, Action::Discard(_))).unwrap()
        };
        game.apply(seat, action).unwrap();
    }
    let result = game.result().unwrap();
    assert_eq!(result.win, None);
    assert!(result.kongs.contains(&mjers_rules::Kong {
        owner: S[0],
        kind: KongKind::Concealed
    }));
    assert_eq!(game.settlement().unwrap().kongs[0], 6 * result.kongs.iter().filter(|k| k.owner == S[0]).count() as i32);
}
