# mjers

Mahjong engine, advisor, and simulator for the family ruleset in `RULES.md`.
Roadmap and commit plan: `TIMELINE.md`. Study results: `experiments.md`.

## Commands

```sh
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all                              # nightly rustfmt: vertical params/imports, width 200
cargo build --locked -p mjers-core --target wasm32-unknown-unknown
npm ci --prefix npm
npm run format --prefix npm                    # JS/TypeScript formatting and imports
npm test --prefix npm                         # release WASM, TypeScript, JS tests
npm run test:package --prefix npm             # clean install of the npm tarball
cargo bench -p mjers-core --bench hand            # shanten / discard ranking benchmarks
cargo bench -p mjers-core --bench bots            # decisions and whole hands
cargo bench -p mjers-core --bench game            # state machine overhead
cargo bench -p mjers-core --bench advisor          # ranking and rollouts

cargo run --release -- analyze 123m456p78s1122z9m5z
cargo run --release -- play --hint
cargo run --release -- sim --hands 100000 --seats efficiency,defensive,efficiency,random
cargo run --release -- compare strategies/no-chow.json efficiency --hands 20000
cargo run --release -- advise --seed 2 --step 9 --samples 300
cargo run --release -- tune --hands 5000 --output strategies/tuned.json
# Neural strategy files: {"kind": "neural", "name": ..., "network": "<policy.safetensors>"}
cargo run --release -- compare target/learning/v3/ppo-11/strategy.json strategies/tuned-family.json --field strategies/tuned-family.json --hands 20000 --seed 1600000
cargo run --release -- play --hint-network target/learning/v3/ppo-11/strategy.json
cargo run --release -- advise --seed 2 --step 9 --samples 0 --network target/learning/v3/ppo-11/strategy.json
# Search strategy files also work as --network / --hint-network (playouts where the net is unsure)
cargo run --release -- advise --seed 2 --step 9 --samples 0 --network target/learning/v3/ppo-11-search-s128m1/strategy.json
```

## Layout

Five packages: `mjers-core` is the engine library, `mjers-sim` runs experiments,
`mjers-wasm` exposes browser bindings, `mjers-py` exposes Python training environments,
and the root `mjers-cli` package
builds the `mj` command (`src/cli/`). Python package/build config is in `python/`
and `pyproject.toml`, with dependencies locked by uv. See `NN.md` for encoding
and environment semantics. `uv sync --locked --python 3.13` builds the extension;
`uv run --no-sync python -m unittest discover -s python/tests -v` checks it.
Python requires 3.13+: all parameters are explicit, data/configuration/results are
typed dataclasses, and strict Mypy plus Ruff checks run in CI. Run `uv run --no-sync
mypy`, `uv run --no-sync ruff check python`, and `uv run --no-sync ruff format --check
python`. MLP/CNN training, diagnostic and paired evaluation commands/results are
in `NN.md`. Training requires an explicit `--architecture` (`mlp-relu-v1`,
`suit-cnn-relu-v1`, or `suit-resnet-v2` with a value head), `--augment`
(`none` or `symmetry`), `--value-weight` and `--soft-teacher` (a checkpoint to
distill, or `none`). `mjers.dagger` collects teacher
labels on student-reached positions; `mjers.reinforce` runs PPO against a fixed
field and records its hand seeds in the checkpoint; `mjers.deepen` appends identity
residual blocks to a checkpoint without changing its play. `mjers.expert collect` labels a
policy's unsure moves by paired playouts (`Env.estimate_actions`) and `mjers.expert
refine` fine-tunes it toward the posterior-better move. The `advisor` BotSpec kind
(`strategies/advisor.json`) plays Monte Carlo advice and is slow. `Env.rank_actions()` exposes observation-only advisor scores
for diagnosis; these are not policy inputs. Encoding version 2 appends planes (first six
discards and just-drawn discards per seat) to version 1's unchanged 50, so v1 networks
read its first 50; `mjers.upgrade` gives a v1 checkpoint zero-weight inputs for the rest.

The npm package is `@mjers/mjers` in `npm/`, following Shark's ES module
factory and `./types` export layout. `scripts/build-wasm.mjs` builds and stages
generated artifacts with wasm-bindgen CLI 0.2.125 (matching the crate pin).
See `npm/README.md` for setup and API usage. Generated JS/WASM/declarations and
tarballs stay ignored; the source wrapper, types, tests, and lockfile are tracked.
Biome checks JS/TypeScript formatting and braced conditionals in `npm test`;
`biome.json` excludes generated bindings and the lockfile.

The library's `mjers-core/src/` has six domain modules: `tile`, `hand`, `rules`,
`game`, `bot`, and `advisor`. Import from the defining module, for example
`mjers_core::game::Game`, `mjers_core::hand::Hand`, and `mjers_core::rules::Ruleset`.
There are no re-export facades. The engine depends only on Serde at runtime;
Rayon and report I/O live in simulation, and Clap/file loading live in the CLI.
The entire `mjers-core` library must stay WASM-safe: no threads, OS randomness, or
file I/O. CI builds it directly for `wasm32-unknown-unknown`; no Cargo alias is needed.

- Tiles are `Tile(u8)` over 34 kinds (`m`/`p`/`s` suits, `1z`-`7z` honors:
  E S W N White Green Red); bonus tiles are separate (`1F`, `2S`). Hands are
  `[u8; 34]` counts (`TileCounts`); notation like `123m55p7z` everywhere.
- `Game` is the only state machine. Every interface goes through
  `to_act()`, `legal_actions(seat)`, `apply(seat, action)`, and
  `observe(seat)`. Policies only ever see an `Observation`.
- Randomness is xoshiro256** (`mjers-core/src/game/rng.rs`); its first output is
  pinned by a test because changing it changes every experiment.
- `Game::position()` / `Game::resume()` start play mid-hand (advisor playouts).
- `bot::neural` runs `suit-resnet-v2` checkpoints without dependencies; callers
  parse safetensors (`src/cli/checkpoint.rs`, npm `loadNetwork`, Python
  `native_network`). `BotSpec` kinds: random, heuristic, advisor, neural (CLI
  loads `{"kind": "neural", "network": path}`), search (`advisor::search`: the
  network's move, checked by paired playouts when its favourite is below
  `confident`; the CLI file names the checkpoint and the playout `field` by path),
  hybrid (turns from one strategy, claims from another). Neural strategy files stay out of `strategies/`.
- `mjers-core` is optimized in dev/test builds to keep hand analysis and rollouts fast.
  This replaces the former hand-only package override.
- Reports contain rules, seeds, resolved strategies, and results (schema 2).
  Build provenance was removed; there is no custom build script.

## Performance baseline (2026-10-19)

Before consolidation, release build on this Apple Silicon Mac, revision `cac74b0`:

- Shanten: 236 ns per batch of four fixed hands (about 59 ns/hand).
- Discard ranking: 33.3 µs per batch of four fixed hands (about 8.3 µs/hand).
- A complete efficiency-bot hand: 475 µs; random-play engine: 32.6 µs.
- CLI simulation: about 2,030 hands/s with one Rayon worker and 10,980
  hands/s with ten workers (10,000 hands, median of three runs).
- Advisor ranking: 12.4 µs; rollouts with 3 candidates × 20 samples: 18.8 ms.
- A single-worker simulation profile spends about 91% of samples under
  discard ranking. The next target is shanten summary combination.

After consolidation: efficiency hands are about 483 µs and advisor rollouts
18.8 ms; CLI throughput is about 2,060 hands/s with one worker and 10,140 with
ten. Random-play engine overhead rose by about 3 µs/hand.

See `docs/PERFORMANCE.md` for measurements before and after consolidation,
workloads, profiling commands, and measurement limits. Fixed-hand benchmarks
have warm caches; they are not representative of all simulation hands.

## Conventions

- Tests live in `module/tests.rs` (or inline for small modules); game tests
  build stacked walls with the private `game::testing` helpers.
- Rulesets and strategies are data (`rulesets/*.json`, `strategies/*.json`)
  and must parse; preset files must match `Ruleset::preset` / `BotSpec::preset`.
- Commit messages: short, plain lowercase messages, no trailers.
- Project notes (`RULES.md`, `TIMELINE.md`, `experiments.md`, `NN.md`,
  `docs/`, this file) stay uncommitted.
