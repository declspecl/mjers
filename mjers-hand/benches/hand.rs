use std::hint::black_box;

use criterion::{
    Criterion,
    criterion_group,
    criterion_main,
};
use mjers_hand::shanten::shanten;
use mjers_hand::ukeire::discard_options;
use mjers_hand::win::is_complete;
use mjers_tile::counts::TileCounts;

/// A spread of shapes: complete, ready, mid-game, and scattered starting hands.
const HANDS: [&str; 4] = ["123m456p789s11122z", "1112345678999m5p", "13579m2468p13s15z", "19m19p19s12345677z"];

fn bench(c: &mut Criterion) {
    let hands: Vec<TileCounts> = HANDS.iter().map(|h| h.parse().unwrap()).collect();
    let visible = TileCounts::new();
    c.bench_function("is_complete", |b| b.iter(|| hands.iter().filter(|h| is_complete(black_box(h), 0)).count()));
    c.bench_function("shanten", |b| b.iter(|| hands.iter().map(|h| shanten(black_box(h), 0) as i32).sum::<i32>()));
    c.bench_function("discard_options", |b| b.iter(|| hands.iter().map(|h| discard_options(black_box(h), 0, &visible).len()).sum::<usize>()));
}

criterion_group!(benches, bench);
criterion_main!(benches);
