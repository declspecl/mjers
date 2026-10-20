//! Bot decision latency and whole hands played by bots.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{
    Criterion,
    criterion_group,
    criterion_main,
};
use mjers_bot::heuristic::HeuristicBot;
use mjers_bot::policy::{
    Policy,
    decide,
    play_out,
};
use mjers_game::game::Game;
use mjers_rules::ruleset::Ruleset;
use mjers_rules::seat::Seat;

fn table(make: fn() -> HeuristicBot) -> [Box<dyn Policy>; 4] {
    std::array::from_fn(|_| Box::new(make()) as Box<dyn Policy>)
}

/// Positions a few turns into hands, each waiting on a discard decision.
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

criterion_group!(benches, bench);
criterion_main!(benches);
