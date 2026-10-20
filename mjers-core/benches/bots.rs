//! Bot decision latency and whole hands played by bots, including a
//! `suit-resnet-v2` network at the trained width (random weights).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]

use std::hint::black_box;
use std::sync::Arc;

use criterion::{
    Criterion,
    criterion_group,
    criterion_main,
};
use mjers_core::bot::heuristic::HeuristicBot;
use mjers_core::bot::neural::{
    NeuralBot,
    ResidualNet,
    Weights,
};
use mjers_core::bot::policy::{
    Policy,
    decide,
    play_out,
};
use mjers_core::game::Game;
use mjers_core::game::rng::Rng;
use mjers_core::rules::Ruleset;
use mjers_core::rules::seat::Seat;

fn table(make: fn() -> HeuristicBot) -> [Box<dyn Policy>; 4] {
    std::array::from_fn(|_| Box::new(make()) as Box<dyn Policy>)
}

/// Positions after twelve actions, including turn and claim decisions.
fn turn_positions(rules: &Arc<Ruleset>) -> Vec<Game> {
    (0..64)
        .filter_map(|seed| {
            let mut game = Game::new(rules.clone(), seed, Seat::ALL[0]).unwrap();
            let mut bot = HeuristicBot::efficiency();
            for _ in 0..12 {
                let seat = game.to_act().first()?;
                let action = decide(&game, seat, &mut bot);
                game.apply(seat, action).ok()?;
            }
            Some(game)
        })
        .collect()
}

/// A width-96, six-block network like `target/learning/v2/resnet-10k`.
fn network() -> ResidualNet {
    let (width, inputs) = (96, 50 + 22);
    let mut rng = Rng::seed(1);
    let mut weights = Weights::new();
    let mut layer = |name: &str, weight: Vec<usize>| {
        let outputs = weight[0];
        for (suffix, shape) in [("weight", weight), ("bias", vec![outputs])] {
            let data = (0..shape.iter().product::<usize>()).map(|_| rng.below(1001) as f32 / 10_000.0 - 0.05).collect();
            weights.insert(format!("{name}.{suffix}"), shape, data).unwrap();
        }
    };
    layer("stem", vec![width, inputs, 1]);
    for block in 0..6 {
        for convolution in ["suit_first", "suit_second"] {
            layer(&format!("blocks.{block}.{convolution}"), vec![width, width, 1, 3]);
        }
        for convolution in ["honor_first", "honor_second"] {
            layer(&format!("blocks.{block}.{convolution}"), vec![width, width, 1]);
        }
        layer(&format!("blocks.{block}.pooled"), vec![width, 2 * width]);
    }
    layer("tile_actions", vec![3, width, 1]);
    layer("summary", vec![width, 3 * width + 22]);
    layer("other_actions", vec![8, width]);
    layer("value_layer", vec![1, width]);
    ResidualNet::new(&weights).unwrap()
}

fn bench(c: &mut Criterion) {
    let rules = Arc::new(Ruleset::family());
    let positions = turn_positions(&rules);
    for (name, make) in [("efficiency", HeuristicBot::efficiency as fn() -> HeuristicBot), ("defensive", HeuristicBot::defensive)] {
        let mut bot = make();
        c.bench_function(&format!("{name}_decision"), |b| {
            b.iter(|| {
                for game in &positions {
                    if let Some(seat) = game.to_act().first() {
                        black_box(decide(game, seat, &mut bot));
                    }
                }
            })
        });
        let mut seed = 0;
        c.bench_function(&format!("{name}_hand"), |b| {
            b.iter(|| {
                seed += 1;
                let mut game = Game::new(rules.clone(), seed, Seat::ALL[0]).unwrap();
                play_out(&mut game, &mut table(make)).unwrap();
                black_box(game)
            })
        });
    }
}

fn bench_network(c: &mut Criterion) {
    let rules = Arc::new(Ruleset::family());
    let positions = turn_positions(&rules);
    let mut bot = NeuralBot::new("network", Arc::new(network()));
    c.bench_function("neural_decision", |b| {
        b.iter(|| {
            for game in &positions {
                if let Some(seat) = game.to_act().first() {
                    black_box(decide(game, seat, &mut bot));
                }
            }
        })
    });
}

criterion_group!(benches, bench, bench_network);
criterion_main!(benches);
