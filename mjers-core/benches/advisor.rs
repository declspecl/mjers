//! Advisor latency: the instant ranking and Monte Carlo playouts.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]

use std::hint::black_box;
use std::sync::Arc;

use criterion::{
    Criterion,
    criterion_group,
    criterion_main,
};
use mjers_core::advisor::heuristic::rank;
use mjers_core::advisor::montecarlo::estimate;
use mjers_core::bot::heuristic::HeuristicBot;
use mjers_core::bot::policy::decide;
use mjers_core::bot::spec::BotSpec;
use mjers_core::game::Game;
use mjers_core::rules::Ruleset;
use mjers_core::rules::seat::Seat;

fn bench(c: &mut Criterion) {
    let rules = Arc::new(Ruleset::family());
    let mut game = Game::new(rules, 5, Seat::ALL[0]).unwrap();
    let mut bot = HeuristicBot::efficiency();
    for _ in 0..12 {
        let seat = game.to_act().first().unwrap();
        let action = decide(&game, seat, &mut bot);
        game.apply(seat, action).unwrap();
    }
    let seat = game.to_act().first().unwrap();
    let obs = game.observe(seat);
    let candidates: Vec<_> = rank(&obs).into_iter().take(3).map(|a| a.action).collect();
    let rollout = BotSpec::preset("efficiency").unwrap();
    c.bench_function("rank", |b| b.iter(|| black_box(rank(&obs))));
    c.bench_function("estimate_3x20", |b| b.iter(|| black_box(estimate(&obs, &candidates, 20, 0, &rollout))));
}

criterion_group!(benches, bench);
criterion_main!(benches);
