//! Engine overhead alone: dealing and playing hands with uniformly random legal actions.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]

use std::hint::black_box;
use std::sync::Arc;

use criterion::{
    Criterion,
    criterion_group,
    criterion_main,
};
use mjers_game::game::Game;
use mjers_game::rng::Rng;
use mjers_rules::ruleset::Ruleset;
use mjers_rules::seat::Seat;

fn random_hand(
    rules: &Arc<Ruleset>,
    seed: u64,
) -> Game {
    let mut game = Game::new(rules.clone(), seed, Seat::ALL[0]).unwrap();
    let mut rng = Rng::seed(seed);
    while let Some(seat) = game.to_act().first() {
        let legal = game.legal_actions(seat);
        let action = legal[rng.below(legal.len() as u64) as usize];
        let _ = game.apply(seat, action);
    }
    game
}

fn bench(c: &mut Criterion) {
    let rules = Arc::new(Ruleset::family());
    let mut seed = 0;
    c.bench_function("deal", |b| {
        b.iter(|| {
            seed += 1;
            black_box(Game::new(rules.clone(), seed, Seat::ALL[0]).unwrap())
        })
    });
    c.bench_function("random_hand", |b| {
        b.iter(|| {
            seed += 1;
            black_box(random_hand(&rules, seed))
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
