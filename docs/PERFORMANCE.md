# Performance measurements

## Baseline before crate consolidation

Measured on 2026-10-19 at `cac74b06aa29e96c2b86e897b83b425d238b82da`,
with a clean tracked source tree. Apple Silicon (`aarch64-apple-darwin`),
`rustc 1.100.0-nightly (fd7ed57df 2026-08-29)`, release/bench optimization,
release debug info level 1. These are local measurements, not portable targets.
No algorithm or data-model changes were made during this measurement pass.

## Criterion results

Each benchmark used a 1-second warm-up, 2-second measurement window, and
30 samples. The table gives Criterion's point estimate, rounded. Bot and
advisor measurements were repeated after profiling finished to avoid competing
with the profiler. Criterion's comparisons to previous local runs are not
controlled before/after comparisons and are not used to claim speedups here.

| Workload | Time | Unit and scope |
|---|---:|---|
| Win detection (`is_complete`) | 93.1 ns | Batch of four fixed hands |
| Shanten | 235.9 ns | Batch of four fixed hands; 59.0 ns/hand average |
| Discard options | 33.34 µs | Batch of four fixed hands; 8.33 µs/hand average |
| Deal | 581 ns | One game construction and deal |
| Random hand | 32.63 µs | One complete hand, random legal actions |
| Efficiency decisions | 491.3 µs | Batch of positions reached from up to 64 seeds after 12 actions |
| Efficiency hand | 474.6 µs | One complete hand, four efficiency bots |
| Defensive decisions | 500.2 µs | Same position batch, defensive policy |
| Defensive hand | 590.1 µs | One complete hand, four defensive bots |
| Advisor rank | 12.36 µs | Seed 5, after 12 actions |
| Advisor estimates | 18.77 ms | Same position, 3 candidates × 20 samples, efficiency rollouts |

The fixed-hand benchmarks repeatedly reuse four shapes and warm the suit cache.
Their per-hand averages are not a prediction of discard evaluation latency in
real games. The bot position batch can include claim decisions: despite the
benchmark comment, its setup does not filter for discard turns. Do not divide
by 64 and describe that as latency of a discard decision.

At the baseline revision, the benchmark commands were (run sequentially).
For the current layout, use `-p mjers-core` for all four benchmarks:

```sh
cargo bench -p mjers-hand --bench hand -- --warm-up-time 1 --measurement-time 2 --sample-size 30
cargo bench -p mjers-game --bench game -- --warm-up-time 1 --measurement-time 2 --sample-size 30
cargo bench -p mjers-bot --bench bots -- --warm-up-time 1 --measurement-time 2 --sample-size 30
cargo bench -p mjers-advisor --bench advisor -- --warm-up-time 1 --measurement-time 2 --sample-size 30
```

Criterion estimates and reports are under `target/criterion/`.

## Simulation throughput

Family rules, four efficiency bots, seeds `0..10000`. Three fresh process runs
per worker count, timed around the CLI process including startup and JSON output.
Numbers use median elapsed time. No profiling or other benchmark ran alongside
these measurements. Ten workers is an explicit setting, not an inferred CPU count.

| Rayon workers | Elapsed runs (seconds) | Median hands/second |
|---|---|---:|
| 1 | 4.933, 4.955, 4.929 | 2,027 |
| 10 | 0.880, 0.933, 0.911 | 10,976 |

```sh
cargo build --release
RAYON_NUM_THREADS=1 target/release/mj sim --hands 10000 --strategy efficiency --json
RAYON_NUM_THREADS=10 target/release/mj sim --hands 10000 --strategy efficiency --json
```

The raw elapsed times and binary provenance are saved locally in
`target/performance/throughput.json`.

## CPU profile and next target

Samply sampled a separate 20,000-hand run, family rules, four efficiency bots,
one Rayon worker, default 1 kHz sampling. The rebuilt binary reported revision
`cac74b0`, `dirty: false`. Percentages below use the worker's 10,021 weighted
samples; they exclude the main thread waiting for Rayon. This is a sampling
estimate, not exact per-function timing.

| Function/path | Worker samples | Interpretation |
|---|---:|---|
| `discard_options` | 90.7% inclusive | Most simulation work still comes from choosing discards |
| `accepted` | 86.8% inclusive | Trying improving draws dominates discard evaluation |
| `shanten_with` | 60.1% inclusive; 39.5% leaf | Recompute one group and score against the other groups |
| `Summary::combine` | 15.4% leaf | Building combined group summaries |
| `GroupSearch::visit` | 11.0% leaf | Recursive decomposition on suit cache misses |

Inclusive percentages overlap; do not add them. Symbols are resolved using
Samply's sidecar and counted once per sampled stack, including recursive search.
Leaf attribution follows the native symbol and can include inlined work.

The next optimization experiment should target repeated summary combination
and scoring inside improving-draw evaluation. For example, investigate reusing
more of the summaries for groups unchanged by a discard/draw, then measure
whole-hand simulation and verify against the existing reference/property tests.
This profile does not justify focusing on game dispatch, action allocation, or
rewriting the suit cache first. It also does not prove any proposed optimization
will help. Keep this baseline for comparison after consolidation.

```sh
RAYON_NUM_THREADS=1 samply record --save-only --unstable-presymbolicate \
  -o /private/tmp/mjers-efficiency-profile.json.gz -- \
  target/release/mj sim --hands 20000 --strategy efficiency --json \
  > /private/tmp/mjers-profile-result.json
```

On this Mac, sampling requires permission outside the sandbox. Put the environment
assignment before `samply`; profiling through `/usr/bin/env` fails because macOS
blocks the profiler's injection into that system executable.

The profile, symbols, simulation report, summary, and local summarizer are copied
to `target/performance/`. These artifacts are ignored by Git and are disposable.

## Correctness caveat retained

Shanten uses the usual shape-based formula and ignores the four-copy limit.
For example, with two declared melds, `2347777m` can be called ready on an
impossible fifth `7m`. Wait enumeration and live improving-tile counts exclude
fifth copies. Policies also compare shanten, so this does not establish that
claim/discard rankings are unaffected. No copy-limited shanten change is part
of this pass; the existing reference/property tests retain their documented
restriction for this case.

## Verification after documentation refresh

All passed: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, `cargo wasm-check`, and `git diff --check`.
The only tracked source edit is the shanten documentation caveat; behavior is
unchanged. Project notes remain uncommitted, following the repository convention.

## After consolidation

Measured on 2026-10-19 in the consolidation working tree (parent revision
`c376831`, reported dirty). The six engine packages are now one `mjers` library;
`mjers-sim` and the root CLI remain separate. Algorithms and benchmark workloads
are unchanged. Release code generation keeps Cargo's defaults; the dev/test
optimization override now applies to the whole engine rather than only hands.

Using the same Criterion settings, representative point estimates were:

| Workload | Before | Consolidated |
|---|---:|---:|
| Shanten, four fixed hands | 235.9 ns | 237.3 ns |
| Discard ranking, four fixed hands | 33.34 µs | 32.95 µs |
| Complete efficiency-bot hand | 474.6 µs | 482.9 µs |
| Complete defensive-bot hand | 590.1 µs | 599.0 µs |
| Advisor ranking | 12.36 µs | 12.42 µs |
| Advisor estimates, 3 × 20 | 18.77 ms | 18.84 ms |
| Random-play engine hand | 32.6 µs | about 35.7 µs |

The main analysis and rollout workloads stay close to the baseline. The
random-play engine benchmark has a repeatable approximately 3 µs cost increase:
alternating the old and new binaries gave old means of 32.40/32.50 µs and new
means of 35.60/35.78 µs. This is a real observed tradeoff, not a claimed speedup.
An experiment with one release codegen unit reduced random-play overhead but
slowed analysis and rollout workloads, so that setting is not retained.

Three fresh CLI process runs per worker count, the same 10,000 efficiency hands:

| Rayon workers | Elapsed runs (seconds) | Median hands/second |
|---|---|---:|
| 1 | 5.142, 4.804, 4.846 | 2,063 |
| 10 | 0.891, 0.986, 1.064 | 10,143 |

The ten-worker runs are more variable than the baseline and have a roughly 8%
lower median in this measurement. They establish throughput around 10,000–11,000
hands/s on this machine, not a controlled claim of unchanged parallel speed.

Current benchmark commands:

```sh
cargo bench --locked -p mjers-core --bench hand --bench game --bench bots --bench advisor -- --warm-up-time 1 --measurement-time 2 --sample-size 30
cargo build --locked -p mjers-core --target wasm32-unknown-unknown
```

Verification: formatting, strict Clippy across all workspace targets, all
workspace tests, the WASM build, and a release build pass. Saved before/after
outputs match exactly for hand analysis, an efficiency simulation, a mixed-bot
simulation, a full trace, and advisor playouts, excluding build provenance.
All 149 engine test functions and the saved property-test regression seed were
retained. A normalized source comparison also confirms unchanged implementation
bodies across all 52 moved implementation/benchmark/test files (excluding module
entry files containing only declarations), apart from imports, visibility,
module declarations, and comments.

Local comparison data and the temporary verification scripts are in
`target/consolidation/`. The finalized benchmark estimates are saved separately
there; later codegen experiments overwrote some `target/criterion/*/new` files.

The engine was subsequently renamed to `mjers-core` without algorithm changes.
Automatic build provenance and `build.rs` were removed; schema-2 reports contain
experiment inputs and results. Build stamps above describe historical measurements.

## Neural inference (2026-10-20)

`cargo bench -p mjers-core --bench bots -- neural_decision`: a width-96,
six-block `suit-resnet-v2` network with random weights deciding the same 64
benchmark positions as the heuristic decision benchmarks. Median **35.8 ms per
batch** (about 0.56 ms per decision), single-threaded, measured on battery
power on this Apple Silicon Mac, so possibly throttled. PyTorch CPU inference
on single observations measured 0.47 ms per decision in comparison.

After `1a6fe8d` (four tiles share each weight row's loads in the rank
convolutions): **28.9 ms per batch** (−19.5%, about 0.45 ms per decision).
Blocking the pointwise layers (stem, honors, tile head) the same way was 2.5%
slower, because grouping allocations outweighed the small layers' savings; it
was not kept. Per-call buffer allocations are negligible next to about ten
million multiply-adds per decision.
