use std::sync::Arc;

use super::*;
use crate::bot::policy::play_out;
use crate::bot::spec::BotSpec;
use crate::game::Game;
use crate::game::rng::Rng;
use crate::rules::Ruleset;
use crate::rules::seat::Seat;

/// Every tensor of a `width`, `blocks` state dict, filled with small pseudo-random values.
fn random_weights(
    width: usize,
    blocks: usize,
    seed: u64,
) -> Weights {
    let mut rng = Rng::seed(seed);
    let mut weights = Weights::new();
    let mut add = |name: String, shape: Vec<usize>| {
        let data = (0..shape.iter().product::<usize>()).map(|_| rng.below(2001) as f32 / 4000.0 - 0.25).collect();
        weights.insert(name, shape, data).unwrap();
    };
    let mut layer = |name: &str, weight: Vec<usize>| {
        let outputs = weight[0];
        add(format!("{name}.weight"), weight);
        add(format!("{name}.bias"), vec![outputs]);
    };
    layer("stem", vec![width, PLANES + CONTEXT, 1]);
    for block in 0..blocks {
        layer(&format!("blocks.{block}.suit_first"), vec![width, width, 1, 3]);
        layer(&format!("blocks.{block}.suit_second"), vec![width, width, 1, 3]);
        layer(&format!("blocks.{block}.honor_first"), vec![width, width, 1]);
        layer(&format!("blocks.{block}.honor_second"), vec![width, width, 1]);
        layer(&format!("blocks.{block}.pooled"), vec![width, 2 * width]);
    }
    layer("tile_actions", vec![3, width, 1]);
    layer("summary", vec![width, 3 * width + CONTEXT]);
    layer("other_actions", vec![ACTIONS - TILE_ACTIONS, width]);
    layer("value_layer", vec![1, width]);
    weights
}

fn observation(seed: u64) -> EncodedObservation {
    let game = Game::new(Arc::new(Ruleset::family()), seed, Seat::ALL[0]).unwrap();
    encoding::encode(&game.observe(Seat::ALL[0]))
}

#[test]
fn rejects_missing_misshapen_and_unexpected_tensors() {
    let complete = random_weights(4, 1, 1);
    assert!(ResidualNet::new(&complete).is_ok());

    let mut missing = complete.clone();
    missing.tensors.remove("summary.bias");
    assert_eq!(ResidualNet::new(&missing).unwrap_err(), WeightError::Missing("summary.bias".into()));

    let mut misshapen = complete.clone();
    misshapen.insert("value_layer.weight", vec![2, 4], vec![0.0; 8]).unwrap();
    assert!(matches!(ResidualNet::new(&misshapen), Err(WeightError::Shape { .. })));

    let mut extra = complete.clone();
    extra.insert("head.weight", vec![1], vec![0.0]).unwrap();
    assert_eq!(ResidualNet::new(&extra).unwrap_err(), WeightError::Unexpected("head.weight".into()));

    assert!(matches!(Weights::new().insert("x", vec![2, 2], vec![0.0; 3]), Err(WeightError::Length { .. })));
}

#[test]
fn swapping_two_suits_swaps_their_tile_logits() {
    let net = ResidualNet::new(&random_weights(6, 2, 2)).unwrap();
    let original = observation(5);
    let swap = |tile: usize| match tile {
        0..9 => tile + 9,
        9..18 => tile - 9,
        _ => tile,
    };
    let mut swapped = original.clone();
    for (plane, source) in swapped.tiles.iter_mut().zip(&original.tiles) {
        for tile in 0..KINDS {
            plane[swap(tile)] = source[tile];
        }
    }
    for block in 0..3 {
        for tile in 0..KINDS {
            swapped.action_mask[block * KINDS + swap(tile)] = original.action_mask[block * KINDS + tile];
        }
    }
    let a = net.evaluate(&original);
    let b = net.evaluate(&swapped);
    for block in 0..3 {
        for tile in 0..KINDS {
            let (x, y) = (a.logits[block * KINDS + tile], b.logits[block * KINDS + swap(tile)]);
            assert!(x == y || (x - y).abs() < 1e-5, "action {}: {x} vs {y}", block * KINDS + tile);
        }
    }
    assert!((a.settlement - b.settlement).abs() < 1e-4);
    let legal = original.action_mask.iter().filter(|&&m| m).count();
    assert_eq!(a.logits.iter().filter(|l| l.is_finite()).count(), legal);
    assert!(a.best().is_some_and(|i| original.action_mask[i]));
}

#[test]
fn plays_complete_hands_with_legal_actions() {
    let net = Arc::new(ResidualNet::new(&random_weights(4, 1, 3)).unwrap());
    for seed in 0..3 {
        let mut game = Game::new(Arc::new(Ruleset::family()), seed, Seat::ALL[(seed % 4) as usize]).unwrap();
        let mut table: [Box<dyn Policy>; 4] = std::array::from_fn(|i| {
            if i == 0 {
                Box::new(NeuralBot::new("net", net.clone())) as Box<dyn Policy>
            } else {
                BotSpec::preset("efficiency").unwrap().build(i as u64)
            }
        });
        play_out(&mut game, &mut table).unwrap();
        assert!(game.is_over());
    }
}

#[test]
fn choices_are_a_distribution_over_legal_actions() {
    let net = ResidualNet::new(&random_weights(4, 1, 4)).unwrap();
    let game = Game::new(Arc::new(Ruleset::family()), 8, Seat::ALL[0]).unwrap();
    let observation = game.observe(Seat::ALL[0]);
    let choices = net.evaluate(&encoding::encode(&observation)).choices(&observation);
    assert_eq!(choices.len(), observation.legal_actions().len());
    assert!((choices.iter().map(|&(_, p)| p).sum::<f32>() - 1.0).abs() < 1e-5);
    assert!(choices.windows(2).all(|w| w[0].1 >= w[1].1));
    assert_eq!(encoding::action_index(choices[0].0, observation.decision()), net.evaluate(&encoding::encode(&observation)).best());
}
