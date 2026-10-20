use std::sync::Arc;

use super::*;
use crate::bot::policy::{
    Policy,
    RandomBot,
};
use crate::game::Game;
use crate::game::position::ResumePoint;
use crate::rules::Ruleset;
use crate::rules::seat::Seat;
use crate::tile::Tile;

#[test]
fn action_indices_are_unique_and_decode_over_complete_hands() {
    let rules = Arc::new(Ruleset::family());
    let mut rng = RandomBot::new(19);
    let mut saw_claim = false;
    for seed in 0..100 {
        let mut game = Game::new(rules.clone(), seed, Seat::ALL[seed as usize % 4]).unwrap();
        while let Some(seat) = game.to_act().first() {
            let observation = game.observe(seat);
            let encoded = encode(&observation);
            let legal = observation.legal_actions();
            for tile in Tile::all() {
                let own: f32 = encoded.tiles[OWN..OWN + 4].iter().map(|plane| plane[tile.index()]).sum();
                let public: f32 = encoded.tiles[PUBLIC..PUBLIC + 4].iter().map(|plane| plane[tile.index()]).sum();
                assert_eq!(own, observation.hand().concealed().get(tile) as f32);
                assert_eq!(public, observation.public_tiles().get(tile) as f32);
            }
            assert_eq!(encoded.action_mask.iter().filter(|&&v| v).count(), legal.len());
            for &action in legal.iter() {
                let index = action_index(action, observation.decision()).unwrap();
                assert!(encoded.action_mask[index]);
                assert_eq!(legal_action(&observation, index), Some(action));
            }
            assert_eq!(legal_action(&observation, ACTIONS), None);
            assert!(encoded.tiles.iter().flatten().chain(encoded.context.iter()).all(|&v| (0.0..=1.0).contains(&v)));
            saw_claim |= matches!(observation.decision(), Decision::Claim { .. });
            let action = rng.act(&observation, &legal);
            game.apply(seat, action).unwrap();
        }
    }
    assert!(saw_claim);
}

#[test]
fn chow_indices_mean_offered_tile_position_and_reject_cross_suit_sequences() {
    let decision = Decision::Claim {
        tile: "5m".parse().unwrap(),
        from: Seat::ALL[3],
        offer: Offer::Discard,
    };
    for (lowest, index) in [("5m", 102), ("4m", 103), ("3m", 104)] {
        assert_eq!(action_index(Action::Chow(lowest.parse().unwrap()), decision), Some(index));
    }
    assert_eq!(action_index(Action::Chow("2m".parse().unwrap()), decision), None);
    let crossing = Decision::Claim {
        tile: "1p".parse().unwrap(),
        from: Seat::ALL[3],
        offer: Offer::Discard,
    };
    assert_eq!(action_index(Action::Chow("8m".parse().unwrap()), crossing), None);
}

#[test]
fn encoding_is_invariant_to_hidden_hands_and_wall_order() {
    let rules = Arc::new(Ruleset::family());
    let game = Game::new(rules.clone(), 42, Seat::ALL[0]).unwrap();
    let before = encode(&game.observe(Seat::ALL[0]));
    let mut position = game.position().unwrap();
    // Keep public bonus tiles in place while swapping concealed hands.
    let a = position.hands[1];
    let b = position.hands[2];
    position.hands[1] = crate::hand::Hand::from_parts(*b.concealed(), a.melds(), a.bonus()).unwrap();
    position.hands[2] = crate::hand::Hand::from_parts(*a.concealed(), b.melds(), b.bonus()).unwrap();
    position.wall.reverse();
    let other = Game::resume(rules, position).unwrap();
    assert_eq!(before, encode(&other.observe(Seat::ALL[0])));
}

#[test]
fn encoding_uses_relative_seats() {
    let rules = Arc::new(Ruleset::family());
    let mut game = Game::new(rules.clone(), 9, Seat::ALL[0]).unwrap();
    let mut policy = crate::bot::spec::BotSpec::preset("efficiency").unwrap().build(0);
    while let Some(acting) = game.to_act().first() {
        let original = game.position().unwrap();
        for shift in 1..4 {
            let rotate = |seat: Seat| Seat::ALL[(seat.index() + shift) % 4];
            let mut position = original.clone();
            for seat in Seat::ALL {
                position.hands[rotate(seat).index()] = original.hands[seat.index()];
            }
            position.dealer = rotate(original.dealer);
            for discard in &mut position.discards {
                discard.seat = rotate(discard.seat);
            }
            position.point = match original.point {
                ResumePoint::Turn { seat, after_claim, last_draw } => ResumePoint::Turn {
                    seat: rotate(seat),
                    after_claim,
                    last_draw,
                },
                ResumePoint::Claim { tile, from, offer } => ResumePoint::Claim { tile, from: rotate(from), offer },
            };
            let rotated = Game::resume(rules.clone(), position).unwrap();
            assert_eq!(encode(&game.observe(acting)), encode(&rotated.observe(rotate(acting))));
        }
        let action = policy.act(&game.observe(acting), &game.legal_actions(acting));
        game.apply(acting, action).unwrap();
    }
}

#[test]
fn pending_claim_responses_do_not_leak() {
    let rules = Arc::new(Ruleset::family());
    let mut policy = RandomBot::new(7);
    let mut checked = 0;
    for seed in 0..30 {
        let mut game = Game::new(rules.clone(), seed, Seat::ALL[0]).unwrap();
        while let Some(first) = game.to_act().first() {
            let legal = game.legal_actions(first);
            if game.to_act().len() > 1 {
                let next = game.to_act().iter().nth(1).unwrap();
                let before = encode(&game.observe(next));
                for &action in legal.iter() {
                    let mut branch = game.clone();
                    branch.apply(first, action).unwrap();
                    assert_eq!(before, encode(&branch.observe(next)));
                    assert!(!branch.is_over());
                }
                checked += 1;
            }
            let action = policy.act(&game.observe(first), &legal);
            game.apply(first, action).unwrap();
        }
    }
    assert!(checked > 0);
}
