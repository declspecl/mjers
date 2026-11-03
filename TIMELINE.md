# mjers Roadmap

## Vision

A fast, deterministic mahjong engine with two uses:

1. **Advisor/simulator:** play hands, see the best move and why, and compare the
   hand-written engine with a Python-trained neural net. Ship it to the web via WASM.
2. **Laboratory:** run millions of hands under configurable rulesets (the family
   rules in `RULES.md` first) and measure which patterns and policies maximise EV.

Primary metric: **EV per hand** (units won minus units paid), alongside win rate,
deal-in rate (discarding the winning tile), self-draw rate, and kong income.
Observational associations are hypotheses; policy questions are answered by
head-to-head runs on identical seeds.

## Repository layout

The root is the `mjers-cli` package and workspace. The engine is one library
with domain modules; simulation is a separate native package. The six original
engine crates were consolidated after M7. Historical commit descriptions below
refer to the original layout.

```text
mjers/                  repo root
├── Cargo.toml          workspace + mjers-cli package (binary mj)
├── src/cli/            commands, file loading, terminal output
├── mjers-core/         engine library (Serde only at runtime)
│   ├── src/
│   │   ├── tile/       encoding, counts, sets, notation
│   │   ├── hand/       melds, wins, waits, shanten, ukeire
│   │   ├── rules/      rulesets, seats, payouts, settlement
│   │   ├── game/       state machine, wall, actions, observations, traces
│   │   ├── bot/        policy contract and heuristic bots
│   │   └── advisor/    action ranking and Monte Carlo rollouts
│   ├── tests/          hand properties and game fuzz tests
│   └── benches/        hand, game, bots, advisor
├── mjers-sim/          Rayon batch runs, comparisons, studies, tuning, reports
├── mjers-wasm/         wasm-bindgen bindings for games, bots, advice and analysis
├── npm/                @mjers/mjers ES module package and TypeScript types
├── mjers-py/           pyo3/maturin training environments
├── python/             Python package, decision collection and tests
├── web/                planned browser app
├── rulesets/           JSON rulesets
└── strategies/         JSON bot presets
```

`mjers-sim`, `mjers-py`, and `mjers-wasm` depend on `mjers-core`; the CLI depends on core and sim.
The entire `mjers-core` library must build for
`wasm32-unknown-unknown`: no threads, OS randomness, or file I/O. CI checks this
with `cargo build --locked -p mjers-core --target wasm32-unknown-unknown`.

## Status (2026-10-20)

M0–M7 are done: every Rust commit through 43 is in git, and the M6 study
results are in `experiments.md`. M8 is underway. On 2026-10-20 a heuristic
discard bug (bots broke ready hands) was found through the CNN beating its own
teacher; it was fixed (`158ce93`), tuned-family re-tuned and every study re-run.
Against the fixed teacher, imitation with symmetry augmentation and the
`suit-resnet-v2` network reaches teacher level; PPO fine-tuning then beats
tuned-family by +0.061 ± 0.017 units/hand on fresh walls, entirely through turn
decisions. The network runs from Rust (`mj compare`, dependency-free inference)
and in the browser. Vectorized stepping in Rust and self-play remain. Learning
results and their limits are in `NN.md`.

M9's binding/package task (44) is implemented ahead of the browser app:
`mjers-wasm` exposes seeded games, seat-limited observations, bots, heuristic and
Monte Carlo advice, and hand analysis. `npm/` follows Shark's package layout with
an async factory, `./types` exports, and generated artifacts staged by
`npm run build --prefix npm`. Rust checks, real WASM JavaScript tests, TypeScript
checks, and a clean installed-tarball check pass. CI includes the npm checks.
Browser app tasks 45–47 remain planned. Nothing has been published to npm.

Deviations from the plan below:
- Commit 2 (docs) is not committed; docs stay local by choice.
- Commit 12: the second preset is `discarder-pays`, not "Hong Kong". Real HK
  scoring needs faan patterns the engine doesn't model, so labelling a preset
  HK would mislead.
- Commits 26–28 became one `HeuristicBot` with `Params`; "efficiency" and
  "defensive" are presets.
- After M7: consolidated the engine into `mjers-core`, removed automatic build
  provenance and `build.rs`, and kept experiment reports as schema 2. Simulation
  and CLI remain separate; a direct WASM build replaces the Cargo alias.
- Extra commits: passed-tile safety for the defence model (found while
  preparing study 39), and the strategy/ruleset files the studies use.
- Commits 37–40 are documentation (`experiments.md`), not commits.

## Commits

Conventional commit format, lowercase. Web is deferred until after the neural net. Each commit builds, passes `cargo test`,
`cargo clippy`, and `cargo fmt --check`, and adds tests for what it introduces.

### M0: Scaffold

1. `chore: gix-style workspace skeleton`
   Root `Cargo.toml` as workspace + `mjers-cli` package, empty plumbing crates
   with lib stubs, shared `[workspace.package]` and `[workspace.dependencies]`,
   rustfmt/clippy config, `.gitignore`.
2. `docs: add RULES.md, TIMELINE.md, and agent instructions`
   Family ruleset, this roadmap, `CLAUDE.md`/`AGENTS.md` with build commands
   and the layout and dependency rules above.
3. `ci: check fmt, clippy, tests, and wasm32 build`
   Workflow (or `just check`) that also builds the plumbing crates for
   `wasm32-unknown-unknown`, so WASM compatibility never regresses.

### M1: Tiles and hand analysis (`mjers-tile`, `mjers-hand`)

4. `feat(tile): compact tile encoding`
   `Tile(u8)` over 34 kinds, `Suit`, `Honor`, `Bonus`, `chow_successor`,
   `m/p/s/z` display and parsing.
5. `feat(tile): TileCounts multiset`
   `[u8; 34]` counts, add/remove/iterate, grouped notation (`123m55p7z`),
   copy-limit validation.
6. `feat(hand): melds and hand representation`
   `Meld` (chow/pong/kong), `KongKind` (exposed/concealed), `Hand` = concealed
   counts + melds + bonus tiles, with hand-size invariants.
7. `feat(hand): win detection and waits`
   4 sets + eye over concealed tiles plus melds; `waits()` lists the tiles that
   complete a 13-tile hand. Special hands are not wins.
8. `feat(hand): shanten`
   Tiles-from-ready for the standard form, accounting for exposed melds.
9. `feat(hand): ukeire and discard evaluation`
   Live improving tiles given visible tiles; for a 14-tile hand, rank each
   discard by resulting shanten and ukeire.
10. `test(hand): property tests`
    proptest cross-checks: shanten −1 ⇔ win, waits ⇔ shanten 0 + win check,
    shanten against a brute-force reference on small hands.
11. `perf(hand): benchmarks and fast shanten`
    Criterion benches, then per-suit lookup tables; results recorded in the
    commit message.

### M2: Rules and settlement (`mjers-rules`)

12. `feat(rules): Ruleset and presets`
    `Ruleset` (flower wall stacks, rob-kong flag, payouts), family and Hong Kong
    presets, serde JSON round trip, `rulesets/*.json`.
13. `feat(rules): hand settlement`
    `HandResult` (win source, flowers, kongs) → zero-sum per-seat deltas per
    `RULES.md`; table tests for every payment row.

### M3: Game engine (`mjers-game`)

14. `feat(game): seats, seeded RNG, and wall`
    `Seat` with counter-clockwise turn order, ChaCha RNG from a `u64` seed,
    144-tile wall, flower wall sized from the ruleset.
15. `feat(game): deal and initial flower replacement`
    13/14-tile deal, flowers set aside and replaced from the flower wall,
    which is topped up from the live wall.
16. `feat(game): action model and turn loop`
    `Action` enum, phases, `legal_actions(seat)`, `apply(action)`; draw and
    discard; typed `GameError`.
17. `feat(game): claims and priority`
    Claim window after each discard: chow from the left only; pong from anyone;
    win from anyone (including taking the eye). Win > pong/kong > chow, ties to
    the closest seat in turn order.
18. `feat(game): kongs`
    Exposed (from a discard), concealed, and pong-upgrade kongs; replacement
    draws from the flower wall.
19. `feat(game): hand end and settlement`
    Self-draw and discard wins, exhausted wall with no winner, `HandResult`
    settled by `mjers-rules`.
20. `feat(game): per-seat observations`
    Everything a seat may legally see (own hand, melds, discards, flowers, wall
    counts) and nothing else. This is the only input bots receive.
21. `feat(game): event log, traces, and replay`
    Every applied action recorded; serializable trace; replay reproduces the
    final state exactly.
22. `test(game): random-play fuzzing`
    Thousands of seeded random games asserting tile conservation, hand sizes,
    zero-sum settlement, and replay equality.
23. `feat(game): matches and dealer rotation`
    Multi-hand sessions with counter-clockwise dealer rotation every hand and
    cumulative scores.
24. `feat(game): rob-the-kong variant`
    Behind the ruleset flag (off for the family preset).

### M4: Bots and terminal play (`mjers-bot`, CLI)

25. `feat(bot): Policy trait and RandomBot`
    `Policy::act(&Observation, &[Action]) -> Action`; baseline random legal play.
26. `feat(bot): EfficiencyBot`
    Discards by shanten then ukeire; claims when a claim reduces shanten.
27. `feat(bot): DefensiveBot`
    Estimates the deal-in risk of each tile from visible information and folds
    when an opponent looks ready and the bot's own hand is far from ready.
28. `feat(bot): parameterized claim policies`
    Chow/pong/kong toggles and thresholds, loadable from `strategies/*.json`.
29. `feat(cli): mj analyze and mj play`
    Hand analysis from notation; a terminal game against bots.

### M5: Simulation lab (`mjers-sim`, CLI)

30. `feat(sim): parallel batch runner`
    Rayon over seed ranges; results depend only on seed, rules, and policies.
31. `feat(sim): metrics with uncertainty`
    EV/hand, win, deal-in, self-draw, exhausted-wall, and kong-income rates,
    with standard errors.
32. `feat(sim): duplicate head-to-head`
    Policies compared on identical walls with seats rotated, which cuts variance.
33. `feat(sim): versioned JSON reports with provenance`
    Resolved rules, strategies, seeds, and build revision via `build.rs`, as in
    monopolez.
34. `feat(sim): per-hand feature log`
    JSON Lines rows (meld mix, concealment, wait shape, kongs, flowers, turn of
    win, outcome) for observational analysis in Python.
35. `feat(cli): mj sim, mj compare, mj trace, mj replay`
36. `feat(sim): policy tuning`
    Grid and evolutionary search over bot parameters, validated on held-out seeds.

### M6: Strategy studies (`experiments.md`, `strategies/`)

37. `docs(experiments): claim policies`
    Always/never/conditional chow and pong, by EV.
38. `docs(experiments): kong timing`
    Concealed kong now vs. holding; exposed vs. concealed value under family
    payouts.
39. `docs(experiments): defense vs. speed`
    The cost of the 2-unit discarder penalty and when folding pays.
40. `docs(experiments): ruleset sensitivity`
    Flower wall 8 vs. 9 stacks, rob-kong on/off, family vs. Hong Kong.

### M7: Advisor (`mjers-advisor`, CLI)

41. `feat(advisor): heuristic ranking with explanations`
    Every legal action scored with shanten, ukeire, deal-in risk, and a
    human-readable reason.
42. `feat(advisor): determinized Monte Carlo`
    Sample hidden tiles consistent with the observation, roll out with bots,
    and estimate each action's win chance and EV.
43. `feat(cli): mj advise and advisor overlay in mj play`

### M8: Python and neural net (`mjers-py`, `python/`)

48. `feat(py): environment bindings` — implemented, 2026-10-19
    pyo3/maturin `reset`/`step`, legal-action masks, observation tensor encoding.
    Configurable engine rulesets, shared version-1 encoder, teacher actions and NPZ decision shards.
49. `feat(py): vectorized environments`
    Many games stepped per call in Rust for training throughput. Not done;
    `mjers.reinforce` batches inference across 256 Python-held hands instead.
50. `feat(train): imitation learning` — implemented, 2026-10-20
    MLP, suit CNN and `suit-resnet-v2` (value head); symmetry augmentation;
    DAgger collection (`mjers.dagger`); decision-scope evaluation.
51. `feat(train): self-play reinforcement learning` — PPO against a fixed field
    implemented, 2026-10-20 (`mjers.reinforce`); self-play fields remain.
52. `feat(bot): neural-net policy in Rust` — implemented, 2026-10-20
    Hand-written `suit-resnet-v2` inference in `mjers-core` instead of
    ONNX/tract, keeping the engine dependency-free; `BotSpec::Neural` in the CLI,
    `Network`/`loadNetwork` in WASM and npm.
53. `docs(experiments): neural net vs. engine bots`
    Duplicate head-to-head on held-out seeds: results in `NN.md`.

### M9: Web (`mjers-wasm`, `web/`)

44. `feat(wasm): bindings and npm package`
    Game, observation, advisor, and analysis exposed via wasm-bindgen with
    TypeScript types.
45. `feat(web): app scaffold and play vs. bots`
    Same stack as gills-pro; the engine runs entirely in the browser.
46. `feat(web): advisor overlay and replay viewer`
47. `feat(web): experiment report explorer`
    Load `mjers-sim` JSON reports and chart them.
