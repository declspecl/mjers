# Neural Net Notes

Everything learned so far that should shape the neural-net work (TIMELINE
milestone M8). Interfaces and performance refreshed after the cleanup pass on
2026-10-19. The engine now lives in one `mjers-core` library; simulation and CLI
remain separate packages. Performance baseline below predates consolidation;
see `docs/PERFORMANCE.md` for the follow-up measurements.

## Current pipeline (2026-10-20)

1. Collect teacher shards: `python -m mjers.collect` (fixed tuned-family teacher;
   `target/learning/v2/train-*.npz`, seeds 0–9,999; validation 10,000–10,199).
2. Imitate: `python -m mjers.train --architecture suit-resnet-v2 --augment symmetry
   --value-weight 0.5` on MPS → `resnet-10k` (teacher level).
3. Reinforce: `python -m mjers.reinforce` (PPO, Rust `Arena`, lr 3e-5,
   `--snapshot-every 0` for the fixed field) → `ppo-2` → `ppo-4`.
4. Evaluate in Rust: neural strategy JSON + `mj compare` on fresh seed blocks
   (≥ 20,000 walls; dev/reporting blocks are listed below). Hybrid strategy files
   attribute gains to turns or claims.
5. Play or ship: `mj play --opponents <strategy.json>` (or `--hint-network`),
   `mj advise --network <strategy.json>` beside the advisor, `mjers.loadNetwork` in npm.
6. Expert iteration (done once): imitate ppo-4 on `target/learning/v3/` →
   `resnet-20k` (≈ tuned-family), self-play PPO → `ppo-5` (≈ ppo-4). Continued
   fixed-field PPO from ppo-5 is the current run.

Strongest player: `target/learning/v3/ppo-11-search-s128m1/strategy.json`
(ppo-11 checked by 128 paired playouts where it is below 90% sure), +0.028 ±
0.013 over ppo-11 in the tuned-family field it models; about 3 CPU-seconds per
seat-hand. Bigger networks (10 blocks, width 192) and gentler PPO did not beat
ppo-11 (see the capacity and width tests).

Best model: `target/learning/v3/ppo-11/policy.safetensors`, +0.103 ± 0.015
over tuned-family on fresh walls (+0.110 on the shared curve block), level
with or slightly above ppo-7/ppo-8 (+0.009 ± 0.010 over ppo-8); wall-level
errors on 20,000 walls each.
Earlier: `v2/ppo-4`, +0.061 ± 0.015 over tuned-family, all from turn decisions. Seed blocks used so far: training 0–10,199; Python evaluation
50,000–70,999; PPO 100,000,000+ … 400,122,879, 500,000,000–500,122,879, 600,000,000+ … 800,000,000+ (81,920 hands each), 900,000,000+ … 1,200,000,000+ (81,920 hands each), 1,300,000,000–1,300,163,839, 1,400,000,000.. to 2,000,000,000.. (81,920 each); variance probe 7,000,000,000–7,000,004,095; v3 collection 3,000,000–3,100,299; expert-1 collection 4,000,000–4,059,999; `mj compare` 1,000,000–1,519,999,
1,600,000–1,999,999, 2,000,000+ (heuristic studies) and 2,600,000–2,884,039, 2,900,000–2,999,999 and 3,200,000–3,219,999. Pick new blocks above these.

**Teacher change after the results below** (`0a76d94`): the heuristic now
counts a discard that leaves no live improving tiles (a dead wait, when ready)
as one step slower, so it reshapes instead of waiting on nothing. This affects
about 0.2% of seat-hands (all-efficiency tables still win 25.0% with 0.2%
draws) and was not measured in paired play. Shards and checkpoints above were
labeled and compared before it; their `teacher_json` is unchanged.

## Goals (from the user)

- Train a policy in Python and **compare it with the hand-written engine**
  (the heuristic bots and the advisor) on equal terms.
- Use it to study play, not just to win: insights should still flow into
  `docs/LEARNINGS.md`.
- Later, ship it to the browser alongside the WASM engine (the web app comes
  after the neural net). The reference architecture is `~/programming/projects/gills`:
  a `shark` C++ core with `pyshark` Python bindings, an `npm` WASM package, and a
  `gills-pro` web app.

## Plan as it stands (TIMELINE M8)

1. **`mjers-py` (implemented):** pyo3/maturin bindings with turn-based `reset`/`step`,
   legal-action masks, and an observation tensor encoding. Accepts engine ruleset
   presets or custom Ruleset JSON. Teacher and rules are required constructor inputs.
2. **Vectorised environments:** many games stepped per call in Rust, for training throughput.
3. **Imitation learning (MLP and suit-aware CNN implemented):** train from
   teacher-played Rust environments. Python collection, typed datasets, training,
   checkpoints, disagreement diagnostics and paired duplicate evaluation work;
   large/vectorized collection remains.
4. **Self-play reinforcement learning** (PPO or similar).
5. **The net plays from Rust:** export to ONNX and load it with `tract`
   (pure Rust, WASM-capable) as a `Policy`, so it runs in `mj compare` and later the browser.
6. **Duplicate head-to-head** against engine bots on held-out seeds.

Prior art: Mortal (riichi mahjong) and Suphx (Microsoft) for observation
encoding and training. Suphx used oracle guiding (training with hidden information, then
removing it) and run-time policy adaptation.

## Interfaces already in place

- **Players see only an `Observation`:** `Game::observe(seat)` gives a borrowed,
  information-limited view: own hand; every seat's melds, bonus tiles and
  concealed-tile count; all discards with who threw them and whether they were
  claimed; public tile counts; live and total wall size; dealer; the current
  `Decision`; and legal actions.
- **Every decision goes through the same three calls:** `to_act()`,
  `legal_actions(seat)` and `apply(seat, action)`. Claim windows ask several
  seats; every asked seat decides before the window resolves (win > pong/kong >
  chow, ties to the closest seat). An environment must handle multi-seat
  decision points.
- **`Policy`** (`mjers_core::bot::policy`): `act(&Observation, &[Action]) -> Action`.
  `BotSpec` (JSON) builds policies; a neural-net policy should become another
  `BotSpec` kind.
- **`Action`:** `Discard(t)`, `SelfDrawWin`, `ConcealedKong(t)`,
  `UpgradeKong(t)`, `Pass`, `Chow(lowest)`, `Pong`, `Kong`, `Win`.
- **Determinism:** the wall, dealer, and every policy seed derive from a `u64`
  hand seed (in-crate xoshiro256**, output pinned by a test). Traces (`Trace`)
  replay exactly.
- **Mid-hand positions:** `Game::position()` / `Game::resume()` start play
  mid-hand. The advisor's `sample_position` resamples hidden tiles consistently
  with an observation. This is useful for oracle/perfect-information training
  and for search on top of the net.
- **Data tools:** `mj sim --features` writes JSON Lines rows per seat per hand
  (outcomes, melds, wait shapes). `mj trace` writes full traces. Python's
  `mjers.collect` now exports per-decision observations, masks, teacher actions,
  final returns, seats and hand seeds to an NPZ shard.

## Initial encoding (version 1, implemented)

**Action space, fixed size (110 logits) with a legal mask:**
- Discard × 34, concealed kong × 34, upgrade kong × 34.
- Chow × 3: the claimed tile as the low, middle, or high tile, rather than 34.
- One logit each for pong, kong, win, self-draw win, and pass.

The encoder lives in `mjers-core/src/bot/encoding.rs`, shared by Python training
and future Rust inference. It accepts only `Observation`, never `Game` or `Position`.
Relative seat order is self, next, opposite, previous.

`tiles` is float32 `[50, 34]`; `context` is float32 `[22]`:

| Tile planes | Meaning |
|---|---|
| 0–3 | Own concealed counts: at least 1, 2, 3, 4 copies |
| 4–7 | Public counts, same thresholds |
| 8–23 | All discards per relative seat, including claimed ones, same thresholds |
| 24–27 | Claimed discard counts per seat, divided by 4 |
| 28–31 | Latest discard occurrence per seat/tile, position divided by history length; 0 if absent |
| 32–47 | Meld anchors per seat: chow's lowest tile, pong, exposed kong, concealed kong; counts / 4 |
| 48–49 | Offered tile and own last draw, one-hot |

Context: bonus counts / 8 (4), concealed counts / 14 (4), dealer one-hot (4),
offering seat one-hot or zero on turns (4), turn/discard-claim/rob-kong flags (3),
after-claim flag (1), live and total wall size / 144 (2).

Threshold planes distinguish one, two, three and four copies without asking
the first network layer to learn those thresholds from a single count. Relative
seats avoid learning the same position four times. Separate meld types retain
kong and sequence information. Discard recency is deliberately a summary:
we can later compare full-history inputs if this representation limits learning.
No shanten, ukeire, or heuristic scores are provided as inputs initially.

Action indices: discard 0–33, concealed kong 34–67, upgrade kong 68–101,
chow with offered tile low/middle/high 102–104, pong 105, kong 106,
win 107, self-draw win 108, pass 109. Decode by matching the engine's legal
actions, so a masked index cannot mutate the game. `ENCODING_VERSION = 1`
is saved in every shard; changes to representation or action order require a bump.

The first network is a fully connected MLP: 1,722 inputs, two ReLU hidden layers
of width 256, and 110 logits (535,150 trainable parameters). Width is an explicit
training parameter. Illegal logits are masked before cross-entropy training and
action selection. This is the baseline for later comparisons with suit-aware
convolution or history models.

The same input shape works across current engine rulesets. Version 1 does not
encode payouts or rule flags: train a separate model per ruleset initially, and
record the resolved rules in its dataset and eventual checkpoint. A shared model
that adapts to multiple rulesets would also need explicit rule inputs and training
on those variants. Different rules can assign different values to identical board
states and legal actions; the legal mask alone cannot communicate that difference.

Under family rules all honors are interchangeable, and suits are symmetric.
That allows data augmentation by permuting suits and by permuting honors
within winds and within dragons. Check `RULES.md` first: nothing there
distinguishes them.

## Running the first Python slice

```sh
uv sync --locked --python 3.13
uv run --no-sync mypy
uv run --no-sync ruff check python
uv run --no-sync ruff format --check python
uv run --no-sync python -m unittest discover -s python/tests -v
uv run --no-sync python -m mjers.collect target/learning/train-000.npz \
  --teacher strategies/tuned-family.json --rules family --hands 100 --start-seed 0
```

`uv sync` builds the native extension through maturin. Re-run after Rust edits;
for release rebuilding during development, `uv run --no-sync maturin develop --release --locked`.

```python
from mjers._native import Env

env = Env(teacher="efficiency", rules="family")  # both also accept JSON
observation = env.reset(seed=42, dealer=0)
while observation is not None:
    transition = env.step(env.teacher_action())
    observation = transition.observation
print(transition.rewards)  # four absolute-seat settlement totals
```

`Env(teacher="efficiency", rules="discarder-pays")` selects another engine preset. `rules` also accepts
a serialized Ruleset JSON object. Collection accepts `--rules` with a preset or
JSON file path and saves resolved rules and teacher configuration in the shard.

Each step answers one seat's decision. The next observation can be for another
seat; claim responses do not resolve until every asked seat has answered.
There is no auto-reset. Rewards are zero until termination, then the full
settlement, including kongs. Environments are bound to their creating Python
thread. Arrays are independent snapshots, so writes cannot alter the engine.

Collection holds one shard in memory; use modest hand counts and multiple shards
for larger datasets. Every row records the acting seat, the hand seed, teacher
action and that seat's final return. Split **whole hand seeds**, not decision rows,
between training and validation. Returns are training targets, never policy inputs.
For future RL, keep trajectories separate per seat; consecutive environment steps
are not necessarily consecutive decisions by the same player.

Smoke collection on 2026-10-19 produced 6,333 decisions from 100 tuned-family
hands (seeds 0–99). This verifies the data path, not policy learning or throughput.
Rust tests cover legal-action round trips, relative-seat rotation, hidden-information
invariance and private pending claims; Python tests cover full hands, reproducibility,
illegal inputs, snapshot ownership, outcome attribution and custom-rule payouts.

## Training and evaluation (first MLP, 2026-10-20)

Python 3.13+, with strict Mypy and Ruff checks covering source, tests and stubs.
All owned function parameters are required; CLI configuration flags are explicit.
Frozen dataclasses describe datasets, model metadata, training configuration and
results. NumPy annotations specify dtypes; dataset boundaries validate dimensions,
legal labels and finite values. Typed JSON uses msgspec. Model weights and metadata
use safetensors; no pickle is needed to load datasets or policies.

Training uses masked cross-entropy, Adam, and a deterministic CPU shuffle stream.
The chosen checkpoint minimizes validation cross-entropy. Training rejects shared
hand seeds or differing rules/teachers between splits. Evaluation rejects seeds
seen in training or validation and rejects a ruleset mismatch. Four seat rotations
share a wall, so comparison uncertainty is estimated across independent walls.
The model plays complete hands from Python against Rust teachers; Rust/ONNX model
loading is still future work. All current inputs are observations, including claim
decisions; terminal returns are recorded but not used for imitation's loss.

Reproduce the first experiment:

```sh
uv run --no-sync python -m mjers.collect target/learning/mlp/train.npz \
  --teacher strategies/tuned-family.json --rules family --start-seed 0 --hands 1000
uv run --no-sync python -m mjers.collect target/learning/mlp/validation.npz \
  --teacher strategies/tuned-family.json --rules family --start-seed 10000 --hands 200
uv run --no-sync python -m mjers.train \
  --training target/learning/mlp/train.npz --validation target/learning/mlp/validation.npz \
  --output target/learning/mlp/policy.safetensors --architecture mlp-relu-v1 \
  --hidden-width 256 --epochs 12 \
  --batch-size 256 --learning-rate 0.001 --seed 42 --device cpu --threads 4
uv run --no-sync python -m mjers.evaluate \
  --checkpoint target/learning/mlp/policy.safetensors --baseline strategies/tuned-family.json \
  --rules family --start-seed 20000 --hands 200 --device cpu --threads 4 \
  --output target/learning/mlp/comparison.json
```

Measured on this Mac, PyTorch 2.14.1, CPU, four threads:

- 1,000 training hands (seeds 0–999): 64,102 decisions.
- 200 validation hands (10,000–10,199): 12,793 decisions.
- Before training: validation accuracy 17.7%, cross-entropy 1.8774.
- Selected epoch 4: training accuracy 73.7%; validation accuracy **64.8%**,
  cross-entropy **0.9847**. Uniform-legal cross-entropy is 1.8786.
- Epoch 12: training accuracy 91.6%, validation accuracy 60.5%, validation
  cross-entropy 1.4276. This run overfits; later epochs were not selected.
- All decisions in these two shards have multiple legal actions; the separately
  recorded choice accuracy equals overall accuracy here.
- Duplicate evaluation: seeds 20,000–20,199, 200 walls × four candidate seats
  (800 candidate hands), against three tuned-family opponents. The selected MLP
  is **−0.926 units/hand relative to tuned-family**, paired standard error **0.167**.
  This is a small initial baseline, not a competitive neural policy yet.

Artifacts live under ignored `target/learning/mlp/`, with training metrics in
`policy.training.json`. Configuration and seed splits are stored with the
checkpoint. No model or dataset is committed.
The Linux dependency lock selects CPU PyTorch, so CI avoids CUDA runtime packages.
CUDA training would require an explicitly configured CUDA-enabled installation.

## Suit-aware CNN and diagnostics (2026-10-20)

The encoding audit found no missing information needed to imitate tuned-family:
its hand shape, public availability, claim tile and legal actions are already
represented. The teacher does not use full ordered discard history. Bonus
identities are irrelevant under current payouts. Keep encoding version 1 and
separate models per ruleset; no additional engine-derived policy inputs yet.

`suit-cnn-relu-v1` shares two 48-channel, width-3 ReLU convolutions across
the three nine-rank suits. Each suit is processed separately, so filters never
treat 9m and 1p as neighbors. Honors use separate pointwise convolutions with
no rank adjacency. The resulting 48 × 34 features plus the existing 22 context
values feed two width-256 ReLU layers and 110 masked logits. The final dense
layers can still distinguish suits and individual honors. This has **536,750
parameters**, compared with the MLP's 535,150 (+0.3%).

The CNN used the exact same training/validation shards, seed 42, batch size 256,
Adam learning rate 0.001, 12 epochs, CPU and four threads. This matches the data,
optimizer steps and approximate parameter budget, not runtime or operation count.
The selected epoch is 5: training accuracy 75.53%, validation accuracy **72.05%**,
validation cross-entropy **0.8066**. The MLP achieved 64.79% and 0.9847.
CNN validation loss also rises later (1.0057 at epoch 12); best-checkpoint selection
remains necessary. Both models have only one initialization/training seed.

```sh
uv run --no-sync python -m mjers.train \
  --training target/learning/mlp/train.npz --validation target/learning/mlp/validation.npz \
  --output target/learning/cnn/policy.safetensors --architecture suit-cnn-relu-v1 \
  --hidden-width 256 --epochs 12 --batch-size 256 --learning-rate 0.001 \
  --seed 42 --device cpu --threads 4
uv run --no-sync python -m mjers.diagnose \
  --checkpoint target/learning/cnn/policy.safetensors \
  --teacher strategies/tuned-family.json --rules family --examples-per-group 3 \
  --device cpu --threads 1 --output target/learning/cnn/diagnosis.json
uv run --no-sync python -m mjers.evaluate \
  --checkpoint target/learning/cnn/policy.safetensors \
  --baseline strategies/tuned-family.json --rules family \
  --start-seed 40000 --hands 500 --device cpu --threads 2 \
  --output target/learning/comparison/cnn-confirmation.json
```

Repeat diagnosis/evaluation for the MLP checkpoint to compare saved models.
Evaluation retains every wall's four candidate and baseline returns.
`mjers.evaluate.paired_difference(first, second)` verifies matching seeds,
rules, opponents and baseline returns, then estimates the difference and its
standard error across paired independent walls.

| Evaluation block | MLP vs tuned-family | CNN vs tuned-family | CNN minus MLP |
|---|---:|---:|---:|
| 200 walls, seeds 30,000–30,199 | −1.1363 ± 0.1732 | +0.8475 ± 0.2208 | +1.9838 ± 0.2624 |
| Separate confirmation: 500 walls, 40,000–40,499 | −1.1680 ± 0.1113 | +0.6615 ± 0.1367 | +1.8295 ± 0.1607 |

Values are settlement units per candidate hand; ± is one standard error,
not a confidence interval. Each wall supplies four candidate hands, one per
seat, against three tuned-family opponents. The 500-wall check therefore plays
2,000 candidate hands per model. Neither evaluation block influenced training
or checkpoint selection. This is evidence for these saved policies under family
rules against this opponent field; robustness across training seeds, other
opponents and rulesets remains unmeasured.

`Env.rank_actions()` now exposes the existing observation-only advisor's action
descriptions, shanten, live improving tile counts, danger, immediate payments and
reasons. `Env.concealed_hand` describes only the acting seat's hand. These are
diagnostic outputs, never extra policy features. `mjers.diagnose` replays the
checkpoint's validation seeds following the teacher, counts agreement by action
type, and saves bounded disagreement examples with seed/step/seat/hand and both
action assessments. The teacher is called exactly once per decision; analysis
does not change its RNG or the game.

On the same 12,793 validation decisions:

| Diagnostic | MLP | CNN |
|---|---:|---:|
| Discard agreement | 6,467 / 10,201 (63.40%) | 7,082 / 10,201 (69.42%) |
| Pass agreement | 303 / 780 (38.85%) | 493 / 780 (63.21%) |
| Chow agreement | 518 / 793 (65.32%) | 644 / 793 (81.21%) |
| Discard disagreements with higher shanten | 685 | 252 |
| Claims when teacher passed | 477 | 287 |
| Passes when teacher claimed | 252 | 129 |
| Available wins taken | 193 / 193 | 193 / 193 |

These categories describe disagreement, not proven loss of EV. The report groups
by the first differing proxy (shanten, then live improvements, payment, danger)
and preserves raw measurements in examples. Claim assessments assume the
advisor's best follow-up discard. The teacher trades off shanten against ukeire;
the CNN also selects lower-shanten discards than the teacher 1,059 times, versus
777 for the MLP. Settlement comparisons remain the measure of playing strength.

The diagnostic review found and fixed an advisor visibility bug: hypothetical
melds removed tiles from the concealed hand without adding those newly exposed
copies to public counts. This inflated live improving counts after chow, pong,
exposed kong, concealed kong and upgraded kong. Counts now match actual engine
transitions, covered by a regression for all five meld types. The offered tile
and an existing pong were already public and are counted only once. Training
labels, teacher policy and encoding were unaffected; diagnosis reports were
regenerated after the fix. The separate fifth-copy shanten approximation described
below remains; availability counts still exclude fifth copies.

Artifacts: `target/learning/cnn/` contains checkpoint, training metrics/log and
diagnosis; the MLP diagnosis is in `target/learning/mlp/diagnosis.json`.
`target/learning/comparison/` contains both evaluation blocks and their paired
difference reports (`paired.json`, `paired-confirmation.json`). All stay ignored.
Old MLP checkpoints still load. Older training reports lack the newly required
architecture field, and older comparison reports lack per-wall returns; retain
them as historical JSON rather than decoding them into the new report types.

### Repeated training protocol

After committing the CNN/diagnostics work as `b37a500`, repeat both architectures
with training seeds **7 and 123**, keeping the original seed-42 models as the third
replicate. The 1,000/200-hand dataset split, optimizer, width, 12-epoch budget,
batch size and four training threads remain fixed. Each run selects its own
minimum-validation-loss checkpoint. No new training data or model inputs are added.

Evaluate all six saved models on the same **500 fresh walls, seeds 50,000–50,499**,
against three tuned-family opponents. Evaluation uses one CPU thread per process.
No checkpoint or hyperparameter is selected using this block. Report each seed
pair separately and keep training-seed variation distinct from wall uncertainty.
Because all models share the same walls, the aggregate wall error must average
the three model differences *within each wall* before computing its standard
error; pooling the results as 1,500 independent walls would understate uncertainty.

Repeated runs and reports live under `target/learning/repeated/{mlp,cnn}/seed-N/`.
Seed-42 directories contain fresh evaluation reports and reference the original
checkpoints. `target/learning/repeated/summarize.py` is a typed local analysis
script that writes `summary.json`; artifacts and notes stay uncommitted.

All planned runs completed:

| Training seed | MLP validation agreement | CNN validation agreement | MLP units/hand | CNN units/hand | CNN minus MLP |
|---|---:|---:|---:|---:|---:|
| 7 | 64.57% | 72.12% | −1.1065 | +0.9220 | +2.0285 ± 0.1537 |
| 42 | 64.79% | 72.05% | −0.9800 | +0.8750 | +1.8550 ± 0.1605 |
| 123 | 65.00% | 72.21% | −1.0795 | +0.8840 | +1.9635 ± 0.1477 |

Mean across saved models: MLP **−1.0553**, CNN **+0.8937**, difference
**+1.9490 units/hand**. The paired wall standard error of this average,
conditional on these six saved models, is **0.1057**. The standard deviation
of the three seed-pair differences is **0.0877**; it includes finite-wall noise
and should not be treated as a precise population estimate of training variance
from only three replicates. All three CNN runs improve validation agreement,
beat the tuned-family opponent field, and beat their corresponding MLP run.
The result is consistent across the tested training seeds, still on one fixed
training dataset and one ruleset. The six models played 12,000 candidate hands
on this block; the number of independent walls is **500**, not 12,000.

The original seed-42 checkpoints also played the same 500 fresh walls against
two other opponent fields, with no retraining or checkpoint selection:

| Opponent field (three opponents) | MLP units/hand | CNN units/hand | CNN minus MLP |
|---|---:|---:|---:|
| efficiency | −0.9435 ± 0.1086 | +0.8915 ± 0.1288 | +1.8350 ± 0.1549 |
| defensive | +0.0740 ± 0.1071 | +2.3115 ± 0.1369 | +2.2375 ± 0.1656 |

Errors are one wall-based standard error. These additional fields were checked
only for the original training seed, under family rules. They show the observed
CNN advantage is not confined to the tuned-family field; they do not establish
performance against arbitrary policies or under other rulesets. Reports are in
`target/learning/repeated/opponents/{efficiency,defensive}/`.

## The teacher was broken (2026-10-20)

**Every result above was measured against a heuristic that broke ready hands.**
`HeuristicBot::discard` scored `slower + 0.3 * fewer` with
`fewer = (best.total - live) / best.total`. For a slower option, `live` counts
tiles that improve a 1-shanten hand, which are not comparable with a ready
hand's winning tiles. With 3 outs, any 1-shanten shape with 14+ improving tiles
outscored staying ready. Example (seed 70,001): `5m3337p666s` + 2 melds; 7p
waits on three 5m, but the bot threw 3p (score 1 + 0.3 × (−12) = −2.6).

How it was found: the CNN beat its own teacher by +0.89 at 72% agreement.
Hybrid duplicate play on 1,000 fresh walls (70,000–70,999) located the edge:
CNN on turn decisions only +1.39 ± 0.10, kongs only +0.008, claims only
−0.23 ± 0.07. Replacing the teacher's turn decisions with the advisor's
lexicographic ranking was **+3.38 ± 0.10**. The CNN had only partly learned the
discontinuous penalty, so it "fixed" about 40% of the bug by smoothing.

Fixed in `158ce93` (clamp `fewer` at zero) with a regression test. All-efficiency
tables: win 22.6% → 25.0% per seat, no-winner 9.7% → 0.2%, chows/h 0.94 → 0.58.
Re-tuning adopted exposed kong `always` too (`e716f95`). Strategy studies in
`experiments.md` and `docs/LEARNINGS.md` were re-run; directions held but sizes
changed (defence −1.25 → −0.58, never-chow −0.47 → −0.66, kong-always
+0.033 → +0.015). The MLP/CNN numbers above are historical: they imitate and
are compared against the buggy teacher.

### Fixed-teacher baselines

Same protocol: 1,000 training hands (seeds 0–999), 200 validation (10,000–10,199),
evaluation on 500 walls (50,000–50,499) against three tuned-family opponents.
Artifacts in `target/learning/v2/`.

| Model | Data | Augment | Best epoch | Val. agreement | Val. CE | Units/hand vs teacher |
|---|---|---|---:|---:|---:|---:|
| MLP 256 | 1k hands | none | 5 | 69.96% | 0.860 | −2.241 ± 0.073 |
| suit CNN 256 | 1k hands | none | 9 | 83.21% | 0.496 | −0.918 ± 0.095 |
| suit CNN 256 | 1k hands | symmetry | 22 | 81.33% | 0.470 | −0.510 ± 0.102 |
| suit CNN 256 | 10k hands (0–9,999) | symmetry | 15 | 85.47% | 0.319 | −0.140 ± 0.099 |
| suit-resnet-v2 96 | 10k hands | symmetry, value 0.5 | 11 | 91.59% | 0.196 | −0.015 ± 0.091 |

Imitation now has a ceiling at the teacher (~0). Symmetry augmentation
(`mjers.augment`: random suit permutation, rank reversal, honor permutation)
lowers agreement but improves loss and play: the teacher breaks exact ties by
tile index, which augmentation deliberately makes unlearnable and which does
not affect value. Agreement is therefore a weaker metric under augmentation.

### suit-resnet-v2 (`b6223e2`)

Residual tile network: a pointwise stem over the 50 planes plus broadcast
context, six residual blocks, and two heads. Suits are a `[B, C, 3, 9]` grid
with `(1, 3)` kernels (shared across suits, never crossing them); honors use
pointwise convolutions. Each block adds a bias computed from the mean and max
over all 34 tiles, so tiles see the whole hand. The second convolution of each
block starts at zero (identity blocks). Discard, concealed-kong and upgrade-kong
logits come from a per-tile `1 × 1` head; claims, wins and pass come from pooled
features, the offered tile's features and context. A value head predicts the
acting seat's settlement / 8 (`VALUE_SCALE`). Width 96: about 0.6M parameters;
suit and honor permutations leave its tile logits exactly equivariant (tested).
Throughput on this Mac's MPS: about 7k training samples/s at width 96 (the v1
CNN does about 100k/s); CPU training is impractically slow for v2.
float16 autocast on MPS gave only 7.8k vs 7.0k samples/s, so training stays float32.
Training data stays in host memory and `load_datasets` streams shards into
preallocated arrays (`8f15a95`), so the 6.2 GB v3 set is held once.

```sh
uv run --no-sync python -m mjers.train \
  --training target/learning/v2/train-1k.npz target/learning/v2/train-{1..9}.npz \
  --validation target/learning/v2/validation.npz \
  --output target/learning/v2/resnet-10k/policy.safetensors --architecture suit-resnet-v2 \
  --hidden-width 96 --epochs 12 --batch-size 256 --learning-rate 0.001 --seed 42 \
  --device mps --threads 4 --augment symmetry --value-weight 0.5 --soft-teacher none
```

Shards: `train-1k.npz` is seeds 0–999, `train-N.npz` is seeds N000–N999,
all teacher `strategies/tuned-family.json` after `e716f95`.

**Development block.** From here on, choices between variants use walls
60,000–60,499; 50,000–50,499 is the report block. `--candidate-decisions`
(`53abcf6`) restricts which of the candidate's decisions the model makes.

| Model (dev block 60,000–60,499) | all | turns only | claims only |
|---|---:|---:|---:|
| suit CNN 256, 10k, symmetry | +0.002 ± 0.099 | +0.123 ± 0.103 | −0.083 ± 0.044 |
| suit-resnet-v2 96, 10k | +0.117 ± 0.091 | +0.102 ± 0.090 | −0.005 ± 0.022 |

v2 minus CNN on the dev block: +0.116 ± 0.118 (paired). v2 closes the claim
gap (the CNN loses 0.08 there); both are at the teacher's level overall, which
is the ceiling for imitation. The CNN's diagnosis on validation: chow 89%, pass
95%, pong 98%, discard 83% agreement, with most discard disagreements equal in
shanten and live tiles (teacher tie-breaks).

### Monte Carlo advisor as a teacher (rejected for now)

`BotSpec::Advisor` (`c225322`, `strategies/advisor.json`) plays `advise(..).best()`
with fresh samples per decision: the top 4 ranked actions, 50 determinized
playouts each, tuned-family rollouts. Duplicate comparison against tuned-family
in a tuned-family field, 1,000 walls (seeds 50,000..): **−0.359 ± 0.102 units/hand**
(z = −3.5), win 22.5% vs 25.0%, chows/h 0.85 vs 0.58. It claims too much and
wins less. Likely causes: picking any candidate that leads by one standard error
among several noisy estimates (winner's curse), and sampled opponent hands that
ignore what opponents have discarded and claimed. It is about 0.75 CPU-seconds
per candidate hand. Expert iteration on this advisor would imitate a weaker
policy, so the route beyond the teacher is reinforcement learning (`mjers.reinforce`).

### PPO against the teacher field (`c808b76`, `c97cb5d`)

`mjers.reinforce` fine-tunes a `suit-resnet-v2` checkpoint. Each hand the
student plays one random seat against three field bots (the `Env` teacher),
sampling from its masked softmax; wins are always taken and single-action
decisions are skipped. Reward is the student's settlement / 8 at hand end;
advantages are GAE (γ = 1) from the value head; clipped PPO with an entropy
bonus and gradient-norm clipping at 1. 256 hands play concurrently so student
decisions are batched on MPS: about 135 hands/s including updates. Reinforcement
seeds are recorded in checkpoint metadata (`PolicyMetadata.reinforcement`), and
evaluation refuses overlapping walls. `--checkpoint-every` saves partial runs.

```sh
uv run --no-sync python -m mjers.reinforce \
  --checkpoint target/learning/v2/resnet-10k/policy.safetensors \
  --field strategies/tuned-family.json --rules family \
  --output target/learning/v2/ppo-2/policy.safetensors \
  --iterations 40 --hands-per-iteration 2048 --environments 256 --epochs 3 \
  --minibatch-size 1024 --learning-rate 0.00003 --clip 0.2 --value-weight 0.5 \
  --entropy-weight 0.001 --claim-entropy-weight 0 --gae-lambda 0.95 --seed 1 --start-seed 200000000 \
  --device mps --threads 4 --checkpoint-every 10
```

| Run | Hands | LR | Epochs | Sampled units/hand (last 10 it.) | Greedy, dev block | Greedy minus start (paired) |
|---|---:|---:|---:|---:|---:|---:|
| start: resnet-10k | — | — | — | — | +0.117 ± 0.091 | — |
| ppo-2 | 81,920 | 3e-5 | 3 | −0.025 ± 0.040 | +0.108 ± 0.090 | −0.009 ± 0.085 |
| ppo-3 | 81,920 | 3e-4 | 4 | −0.103 ± 0.039 | +0.050 ± 0.106 | −0.068 ± 0.120 |

At 3e-5 the policy barely moved (approximate KL ≈ 5e-4 per update, entropy
0.185 → 0.164). The sampled policy is near the field's level; greedy play is
unchanged.
At 3e-4 it moves (KL 0.02–0.03, 5–8% clipped) without measurable gain. The
per-hand settlement has a standard deviation of about 6 units, so a 0.1-unit
improvement is far below what 80k hands of policy-gradient signal can resolve.
Plausible next steps, none tried yet: millions of hands (mains power), a larger
value head or return normalization, self-play fields, and lower-variance
targets such as paired counterfactual rollouts of candidate actions in Rust.
These runs were made on battery; the laptop idle-sleeps between commands
unless jobs run under `caffeinate -i`.

### The network plays from Rust (`8e305ad`, `b8a3720`)

`mjers_core::bot::neural` runs `suit-resnet-v2` without dependencies (WASM-safe):
callers supply named tensors (`Weights`), `ResidualNet::evaluate` returns masked
logits and predicted settlement, and `NeuralBot` plays the best legal action.
Activations are tile-major so convolutions are contiguous dot products with
eight-lane accumulation; about **0.56 ms per decision** single-threaded at width
96 (PyTorch CPU: 0.47 ms), and 0.45 ms after `1a6fe8d` shares weight loads
across four tiles in the rank convolutions. Parity with PyTorch is tested from Python
(`Env.evaluate_network`, `mjers.model.native_network`) to 1e-4 over full hands;
on the trained model the largest logit difference was 2e-5, and argmax differed
only on exact ties between interchangeable honors (23 of 2,458 decisions).

The CLI loads `{"kind": "neural", "name": ..., "network": "<checkpoint.safetensors>"}`
strategy files (`BotSpec::Neural`; reports record the checkpoint path). It
checks architecture and encoding version, and `mj compare`/`mj sim` refuse
other rules or hand seeds the checkpoint trained, validated or reinforced on.
Keep neural strategy files out of `strategies/` (those must parse without
weights). 500 walls take about 3 s on ten cores instead of minutes in Python.

```sh
printf '{"kind": "neural", "name": "ppo-2", "network": "target/learning/v2/ppo-2/policy.safetensors"}\n' \
  > target/learning/v2/ppo-2/strategy.json
cargo run --release -- compare target/learning/v2/ppo-2/strategy.json strategies/tuned-family.json \
  --field strategies/tuned-family.json --hands 20000 --seed 1000000
```

Until `estimate comparison uncertainty across walls` (after `bed4a93`),
`mj compare` treated each (wall, seat) pair as independent. Its errors are now
computed across walls, like the Python evaluation. The old estimates were not
systematically optimistic: seat rotations of a wall can be negatively
correlated (no-chow at 5,000 walls: ±0.028 wall-level). ppo-4 vs tuned-family
on 1,200,000.. re-measured with wall-level errors: **+0.061 ± 0.015 (z = 4.2)**.
Other `mj compare` rows in this file use the older per-pair errors.

**Precise comparisons** (20,000 walls × 4 seats against a tuned-family field):

| Candidate vs baseline | Seeds | Difference |
|---|---|---:|
| resnet-10k (imitation) vs tuned-family | 1,000,000.. | −0.017 ± 0.015 |
| ppo-2 vs tuned-family | 1,000,000.. | +0.022 ± 0.016 |
| ppo-3 vs tuned-family | 1,000,000.. | −0.002 ± 0.019 |
| **ppo-2 vs resnet-10k** | 1,100,000.. | **+0.033 ± 0.012 (z = 2.7)** |

So PPO at 3e-5 did improve the policy slightly; the 500-wall Python
evaluations could not resolve it. Imitation alone is level with the teacher.

**ppo-4: the first network that beats the teacher.** Continued PPO from ppo-2
with the same settings (seed 4, hands 400,000,000..), stopped after 60 of 80
iterations because the laptop slept on battery; the iteration-60 checkpoint
covers 122,880 more hands (204,800 PPO hands in total, both stages recorded in
its metadata).

| ppo-4 vs | Field | Seeds | Difference |
|---|---|---|---:|
| resnet-10k (its imitation start) | tuned-family | 1,100,000.. | +0.079 ± 0.014 (z = 5.7) |
| tuned-family | tuned-family | 1,000,000.. | +0.044 ± 0.017 (z = 2.6) |
| tuned-family (confirmation) | tuned-family | 1,200,000.. | **+0.061 ± 0.017 (z = 3.7)** |
| efficiency | efficiency | 1,300,000.. | +0.095 ± 0.017 (z = 5.7) |
| tuned-family | defensive | 1,600,000.. (10,000 walls) | +0.099 ± 0.028 (z = 3.5), wall-level |

**Where the gain is** (`BotSpec::Hybrid`, `a484af5`: turn decisions from one
strategy, claim responses from another), same 20,000 walls (1,200,000..):
ppo-4 on turns only **+0.063 ± 0.016**, on claims only −0.002 ± 0.005. All of
the improvement is in discards and kongs. Hybrid files name their parts by
preset or path, e.g. `{"kind": "hybrid", "name": "ppo-4-turns", "turns":
"target/learning/v2/ppo-4/strategy.json", "claims": "strategies/tuned-family.json"}`.

**Why claims did not move** (v3 validation, 13,349 decisions, of which 2,812
are claim responses with pass legal; scratchpad `claims.py`): claim responses
are almost deterministic from the start, so PPO has nothing to explore there.

| Checkpoint | Turns: mean top prob / top < 0.9 / entropy | Claims: mean top prob / top < 0.9 / entropy |
|---|---|---|
| resnet-10k | 0.879 / 27.0% / 0.226 | 0.980 / 4.8% / 0.040 |
| ppo-4 | 0.913 / 20.5% / 0.163 | 0.987 / 4.3% / 0.028 |
| ppo-5 | 0.929 / 16.8% / 0.138 | 0.992 / 2.4% / 0.022 |

The non-pass argmax rate on claims is unchanged (1,794 → 1,789 → 1,790 of
2,812). PPO sharpens turns and leaves claims as imitated. An entropy weight of
0.001 cannot reopen choices at entropy 0.02–0.04. Any claim gain would need
claim-specific exploration (a separate entropy weight or sampling temperature
for claim rows), and only if the teacher's tuned claim thresholds leave room.

**What PPO changed** (`mjers.diagnose` on the 9,051 validation decisions,
teacher replayed): agreement with the teacher fell from 90.6% (resnet-10k) to
89.5% (ppo-4). Discard disagreements with equal shanten and live tiles but
*less* danger rose 182 → 252, and discards giving up some live tiles rose
76 → 120; more-danger disagreements stayed flat (237 → 227). Chow agreement
rose (467 → 482 of 514). Tie-breaking by danger alone does not explain the
gain: tuned-family with caution 0.01 or 0.05 (danger only breaks near-ties)
scored −0.000 ± 0.015 and −0.001 ± 0.015 on 20,000 walls (1,500,000..). The
edge is more likely in the live-tile trades or finer shape choices; examples
are in `target/learning/v2/ppo-4/diagnosis.json`.

For scale, tuned-family is +0.018 over efficiency. ppo-4 wins 25.5% of hands
vs the teacher's 24.9% with unchanged deal-ins (19.0% vs 19.1%). PPO was still
improving when stopped; the obvious next run is more iterations on mains power
(caffeinate keeps an open laptop awake, not a closed one), then decision-scope
attribution of where the gain comes from and a self-play field.

### Rust arena and self-play (`60c3e5b`, `bed4a93`)

`mjers._native.Arena` holds many concurrent hands, each with one student seat.
The field, available wins and single-action decisions run in Rust; `pending()`
returns only real student choices as batched arrays, `act()` applies them, and
`take_finished()` reports settlements. A parity test replays `Env`-driven play
exactly. `mjers.reinforce` uses it (about 160 hands/s on MPS, previously 140;
GPU inference and updates now dominate). `Arena.set_field_network` makes new
hands' field seats play a network natively; `--snapshot-every N` uses this for
self-play against a frozen copy of the student refreshed every N iterations
(0 keeps `--field`). Snapshot stages are recorded as
`{"kind":"snapshot","refresh_every":N}` in checkpoint metadata. Self-play has
not been run at scale yet. Field play for the hands that just acted runs in
parallel with rayon (after `5a9e624`): against a ppo-4 field, 48 hands/s on one
thread and 259 hands/s on ten, so a network field no longer limits PPO.

### Networks as teachers (`bd70de9`): the expert-iteration loop

`mjers.collect`, `mjers.dagger`, `mjers.diagnose` and `mjers.evaluate` accept a
neural strategy file wherever they take a teacher or baseline
(`collect.make_env`; `Env.set_teacher_network`). Labels are the network's argmax
from Rust inference (about 26 ms per hand), and `teacher_json` records the
checkpoint path. This allows imitating ppo-4 with far more data than PPO saw,
then reinforcing the student again. Not run yet (needs mains power):

```sh
for i in 0 1 2 3 4 5 6 7 8 9; do
  uv run --no-sync python -m mjers.collect target/learning/v3/train-$i.npz \
    --teacher target/learning/v2/ppo-4/strategy.json --rules family \
    --start-seed $((3000000 + i * 2000)) --hands 2000 &
done; wait
uv run --no-sync python -m mjers.collect target/learning/v3/validation.npz \
  --teacher target/learning/v2/ppo-4/strategy.json --rules family --start-seed 3100000 --hands 300
# then mjers.train (suit-resnet-v2, symmetry, value 0.5) and mjers.reinforce from the result
```

The value head then learns ppo-4's returns, and the opponents in collected
hands are ppo-4 as well (self-play data).

**Collected 2026-10-20:** `target/learning/v3/train-{0..9}.npz`, 20,000 hands
(seeds 3,000,000–3,019,999), 910,395 decisions; `validation.npz`, 300 hands
(3,100,000–3,100,299), 13,349 decisions; about 35 ms per hand per process.
In float32 the training set is about 6.2 GB in memory. Since `a6d9306` each
improving epoch saves the checkpoint, so a stopped run keeps its best epoch.
The first attempt, on battery, was stopped by the guard before epoch 1 ended;
the run below happened on mains power through a power-gated script.

```sh
uv run --no-sync python -m mjers.train \
  --training target/learning/v3/train-{0..9}.npz --validation target/learning/v3/validation.npz \
  --output target/learning/v3/resnet-20k/policy.safetensors --architecture suit-resnet-v2 \
  --hidden-width 96 --epochs 8 --batch-size 256 --learning-rate 0.001 --seed 42 \
  --device mps --threads 4 --augment symmetry --value-weight 0.5 --soft-teacher none
uv run --no-sync python -m mjers.reinforce \
  --checkpoint target/learning/v3/resnet-20k/policy.safetensors --field strategies/tuned-family.json --rules family \
  --output target/learning/v3/ppo-5/policy.safetensors --iterations 60 --hands-per-iteration 2048 --environments 256 \
  --epochs 3 --minibatch-size 1024 --learning-rate 0.00003 --clip 0.2 --value-weight 0.5 \
  --entropy-weight 0.001 --claim-entropy-weight 0 --gae-lambda 0.95 --seed 5 --start-seed 500000000 --device mps --threads 4 \
  --checkpoint-every 10 --snapshot-every 10
```

**Results (2026-10-20, wall-level errors, 20,000 walls, tuned-family field):**

| Candidate vs baseline | Seeds | Difference |
|---|---|---:|
| resnet-20k (imitates ppo-4) vs tuned-family | 1,700,000.. | −0.011 ± 0.014 |
| resnet-20k vs ppo-4 | 1,720,000.. | −0.017 ± 0.013 |
| ppo-5 (60 self-play iterations from resnet-20k) vs tuned-family | 1,740,000.. | **+0.066 ± 0.015 (z = 4.5)** |
| ppo-5 vs ppo-4 | 1,760,000.. | +0.005 ± 0.013 |

Training took 35 minutes for 8 epochs (validation agreement 95.2% with ppo-4,
loss 0.157, still falling slowly); PPO took 27 minutes (about 78 hands/s with a
network field). Imitation reproduces the teacher's *labels* almost exactly but
not its strength: resnet-20k is no better than tuned-family even though it
agrees with ppo-4 on 95% of decisions. The PPO gain lives in the remaining 5%
(the close calls), which argmax imitation blurs. Self-play PPO restores it, so
the expert-iteration loop returns to the ppo-4 level but does not pass it yet.
A second distillation round is not worth its cost; continuing PPO is.

**Fixed-field PPO from ppo-5 (2026-10-20):** three 40-iteration chunks against
the tuned-family field (81,920 hands each, seeds 600,000,000.., 700,000,000..,
800,000,000..; same settings as above but `--snapshot-every 0`), about 10
minutes per chunk at 130 hands/s. All rows are wall-level, 20,000 walls,
tuned-family field.

| Candidate vs baseline | Seeds | Difference |
|---|---|---:|
| ppo-6 vs tuned-family | 1,800,000.. | +0.062 ± 0.015 (z = 4.2) |
| ppo-7 vs tuned-family | 1,820,000.. | +0.099 ± 0.015 (z = 6.6) |
| ppo-8 vs tuned-family | 1,840,000.. | +0.076 ± 0.015 (z = 5.1) |
| **ppo-7 vs ppo-4** | 1,860,000.. | **+0.041 ± 0.013 (z = 3.1)** |
| ppo-8 vs ppo-4 | 1,880,000.. | +0.018 ± 0.014 (z = 1.3) |
| **ppo-8 vs ppo-5** | 1,900,000.. | **+0.049 ± 0.011 (z = 4.6)** |

Continued PPO is still improving the policy: 245,760 more hands moved ppo-5
clearly (ppo-8 vs ppo-5, z = 4.6), and ppo-7 now beats ppo-4 on fresh walls.
Rows on different seed blocks differ by about ±0.015 each, so ppo-7 vs ppo-8 is
unresolved. A shared block makes the learning curve comparable.

**Learning curve on one shared block** (2026-10-23, `pipeline3.sh` in the
scratchpad; every row vs tuned-family on the same 20,000 walls 1,920,000..,
so rows share wall luck and differences between them are tighter than the
±0.015 each). Each chunk is 40 iterations × 2,048 hands from the previous one;
ppo-n trains on hand seeds n × 100,000,000.. (chunks 9–12 continue the chain
from ppo-8).

| Checkpoint | PPO hands after resnet-20k | vs tuned-family |
|---|---:|---:|
| ppo-5 (self-play) | 122,880 | +0.059 ± 0.015 |
| ppo-6 | 204,800 | +0.073 ± 0.015 |
| ppo-7 | 286,720 | +0.083 ± 0.015 |
| ppo-8 | 368,640 | +0.078 ± 0.015 |
| ppo-9 | 450,560 | +0.103 ± 0.015 |
| ppo-10 | 532,480 | +0.108 ± 0.015 |
| ppo-11 | 614,400 | **+0.110 ± 0.015** |
| ppo-12 | 696,320 | +0.106 ± 0.015 |

Fixed-field PPO gained about +0.05 over ppo-5 and flattens after ppo-10 at
these settings (lr 3e-5, entropy 0.001). Chunks took 15 minutes each on mains
power (ppo-9 took 86 minutes, presumably while the machine was in use).

**Confirmation on fresh walls:** ppo-11 vs tuned-family on 1,960,000..
**+0.103 ± 0.015 (z = 6.7)**; ppo-11 vs ppo-8 head to head on 1,980,000..
+0.009 ± 0.010. The shared-block curve suggests +0.032 between them; read
together, chunks 9–12 added roughly +0.01 to +0.03, so the plateau is real.

**Claim exploration does not help at 0.02** (`87f1d92`). `ppo-9c` trained from
ppo-8 on the same hands as ppo-9 with `--claim-entropy-weight 0.02`: claim
entropy 0.021 vs 0.017 (ppo-9), mean top claim probability 0.992 vs 0.993, and
22 more non-pass argmax claims of 2,812. Head to head, ppo-9c vs ppo-9 on
1,940,000..: **−0.007 ± 0.008**; vs tuned-family on the shared block +0.093
(ppo-9 +0.103). The claim bonus is averaged over all rows (claims are about a
fifth), so 0.02 is a weak push, and even that bought nothing. Claims remain
tuned-family's; a larger weight or claim-only sampling temperature are untried.

**Other fields and a gentler chunk (2026-10-23, `pipeline4.sh`):**

| Comparison | Field | Seeds | Difference |
|---|---|---|---:|
| ppo-11 vs tuned-family | defensive | 2,600,000.. (10,000 walls) | +0.145 ± 0.030 (z = 4.8) |
| ppo-11 vs tuned-family | efficiency | 2,620,000.. | +0.065 ± 0.015 (z = 4.3) |
| ppo-13 vs tuned-family | tuned-family | 1,920,000.. (curve block) | +0.110 ± 0.016 |
| **ppo-13 vs ppo-11** | tuned-family | 2,640,000.. | **−0.000 ± 0.007** |

ppo-13 is 40 iterations from ppo-11 at lr 1e-5 with 4,096 hands per iteration
(163,840 hands, seeds 1,300,000,000..): a third of the step size and half the
gradient noise changed nothing. The edge carries to other fields: ppo-4 had
+0.099 against the defensive field.

**Paired wall-luck baselines do not pay (2026-10-23, not committed).** Idea:
make PPO's advantages duplicate-scored, as evaluation is. A paired `Arena` also
played each wall with the field in the student's seat, and every advantage in
the hand had `baseline / VALUE_SCALE − V(s₀)` subtracted. This is
action-independent, so the gradient stays unbiased, and the value head keeps
raw returns. Measured on 4,096 hands played by ppo-11 (45,091 decisions,
scratchpad `variance.py`; patch `paired-baseline.patch`): the student's
settlement correlates only 0.56 with the field's on the same wall (sd 5.75 vs
5.33 for the difference), because the games diverge after the first different
discard. The full correction *raises* advantage variance by 52% (first decisions
0.19 → 0.33), since the value head already prices the deal. The best scaled
correction (β ≈ 0.35) cuts variance by 21% (30% on first decisions), worth
about 1.3× more hands. ppo-13 already showed that less noise does not move the
plateau, so the feature was reverted. Duplicate scoring works in evaluation
because it averages paired seat rotations over whole walls; a single seat's
counterfactual is too loose a control variate.

**Where this leaves PPO.** Neither less noise (ppo-13) nor claim exploration
(ppo-9c) moves the policy past about +0.11 over tuned-family. What remains is
the model or the information it sees: a wider or deeper network (distill
ppo-11 with soft targets, since argmax labels lost the close calls before),
richer inputs (encoding v2: full discard order, where version 1 keeps each
tile's latest discard per seat; whether a discard was the tile just drawn would
need the engine's `Discard` record to keep it first), or search at decision time with the network as the rollout
policy.

**Capacity test (2026-10-23, `pipeline5.sh`).** `python -m mjers.deepen` (`67e563f`)
appends identity residual blocks to a suit-resnet-v2 checkpoint, so the deeper
network plays exactly like the original and keeps its seed history
(`PolicyMetadata.residual_blocks`, default six for older checkpoints; Rust and
npm already infer the block count from the weights). ppo-11 → 10 blocks
(`v3/ppo-11-deep`), then chunks `ppo-14d` and `ppo-15d` (seeds 1,400,000,000..,
1,500,000,000..; 22 minutes per chunk, 63 hands/s vs 95 for six blocks (ppo-12)).

| Comparison | Seeds | Difference |
|---|---|---:|
| ppo-14d vs tuned-family | 1,920,000.. (curve block) | +0.101 ± 0.015 |
| ppo-15d vs tuned-family | 1,920,000.. (curve block) | +0.116 ± 0.015 |
| ppo-15d vs ppo-11 (its start) | 2,660,000.. | +0.015 ± 0.009 (z = 1.6) |
| ppo-15d vs ppo-13 (6 blocks, same start) | 2,680,000.. | −0.007 ± 0.010 |

Four more blocks and 163,840 hands gave at most +0.015 over the start, no
more than the six-block control, so depth is not what holds PPO at the plateau.
Width is untested; this test does not rule out capacity in general, only depth
added after training.

```sh
uv run --no-sync python -m mjers.deepen --checkpoint target/learning/v3/ppo-11/policy.safetensors \
  --output target/learning/v3/ppo-11-deep/policy.safetensors --residual-blocks 10
```

**Soft-target distillation (2026-10-23, `pipeline6.sh`).** `mjers.train
--soft-teacher <checkpoint>` (`d58ccd1`) replaces the dataset's labels with the
teacher's full move probabilities on the same positions (cross-entropy against
the teacher's softmax; symmetry relabeling moves the targets with the mask;
`none` keeps label training). The student inherits the teacher's training seeds
and reinforcement stages, so `check_unseen` still guards everything the teacher
learned from. Accuracy columns still count agreement with the dataset's labels
(ppo-4 for v3). Both runs used ppo-11 as teacher on the v3 positions (ppo-4's
games), 8 epochs, otherwise the resnet-20k settings.

| Student | Width | Time | Validation soft loss | vs ppo-11 (20,000 walls) |
|---|---:|---:|---:|---:|
| soft-96 | 96 | 64 min | 0.1425 | +0.018 ± 0.012 (2,700,000..) |
| soft-192 | 192 | 144 min | 0.1380 | +0.011 ± 0.012 (2,720,000..) |

soft-192 vs tuned-family on the curve block: +0.102 ± 0.015 (ppo-11 +0.110).
**Soft targets keep the teacher's strength**; argmax labels did not
(resnet-20k was −0.017 ± 0.013 against its teacher ppo-4 and no better than
tuned-family). Distillation is now a lossless way to change architecture. Width
192 matches the teacher, as a copy should; whether it helps further is a PPO
question.

**Width test (2026-10-24, `pipeline7.sh`).** PPO from soft-192, chunks `ppo-16w`
and `ppo-17w` (seeds 1,600,000,000.., 1,700,000,000..; 33 minutes per chunk).

| Comparison | Seeds | Difference |
|---|---|---:|
| ppo-16w vs tuned-family | 1,920,000.. (curve block) | +0.098 ± 0.015 |
| ppo-17w vs tuned-family | 1,920,000.. (curve block) | +0.114 ± 0.015 |
| ppo-17w vs ppo-11 | 2,740,000.. | +0.015 ± 0.012 (z = 1.2) |
| ppo-17w vs ppo-13 (6 blocks, width 96) | 2,760,000.. | +0.007 ± 0.013 |

Twice the width matches the extra depth: about +0.015 over ppo-11, which is no
more than continued training at the original size. **Capacity is not the
limit** at this training budget. Both bigger networks are also slower: 63 hands/s for ten
blocks and 41 for width 192, against 95.

**Faster inference (`5f6334e`).** Rank and honor convolutions are now a
register-blocked matrix product: weights stored `[input][output]`, 4 tiles × 16
output channels of sums kept in registers, one broadcast multiply-add per input
channel. `neural_decision` (64 positions, width 96, six blocks) fell from 56.5 to
35.7 ms, about 0.55 ms per decision (≈34 GFLOP/s on one core). 6×16, 8×8 and
3×32 blocks were slower; so was handling the leftover tiles three at a time
(code layout). ppo-11 still matches PyTorch to 3e-5 over 247 decisions.

**Search where the network is unsure (2026-10-24, `eb1ecf3`).** ppo-11's
favourite has less than 0.9 probability on about 1.1 decisions per seat-hand
(9.5% of non-forced decisions; 11% of turns, 3% of claims; measured on 300
efficiency hands). `BotSpec::Search` (`advisor::search`) plays the favourite
otherwise; at unsure decisions it plays out the network's top `candidates` on
the same `samples` deals (`montecarlo::estimate`, now with separate playout
policies for the advised seat and the field: here the network itself and
tuned-family), and switches only when a candidate leads the favourite by more
than `margin` standard errors of the paired difference. Unlike the old
advisor, the candidates come from the policy, playouts use the policy, and the
favourite is the default. Strategy file (`target/learning/v3/ppo-11-search`):

```json
{"kind": "search", "name": "ppo-11-search", "network": "target/learning/v3/ppo-11/policy.safetensors",
 "search": {"samples": 64, "candidates": 2, "confident": 0.9, "margin": 2.0, "seed": 0, "field": "strategies/tuned-family.json"}}
```

About 1.6 CPU-seconds per seat-hand (500 walls in 7.8 minutes on ten cores).
Both arms agree on almost every decision, so paired errors are small.

| Comparison | Seeds | Difference |
|---|---|---:|
| ppo-11-search vs ppo-11 (pilot) | 2,780,000.. (500 walls) | +0.030 ± 0.015 (z = 1.9) |
| ppo-11-search vs ppo-11 | 2,800,000.. (4,000 walls) | +0.012 ± 0.007 (z = 1.9) |

Pooled, about +0.015 ± 0.006. **Why so little:** a probe (256 samples, 200
walls on 2,820,000..; 817 unsure decisions, scratchpad `probe.py`) logged the
second candidate's paired lead over the favourite. Observed leads have sd 0.47
with a median standard error of 0.30, so the true leads have sd about 0.32
units and mean +0.02 ± 0.02: close calls are worth a lot either way, and the
favourite is not systematically right. At 64 samples (error about 0.6) a
2-error margin switches only on leads above about 1.2. With a Gaussian prior
of sd 0.32, that rule should gain 0.012 per decision, about +0.013 per hand,
which is what the comparison measured, so the playout differences look
unbiased. With two symmetric candidates the best rule is to take the higher
estimate (margin 0): predicted gain about 0.06 per decision, +0.06 per hand.
Higher favourite probabilities had smaller leads (mean +0.04 to +0.11 for
p 0.6–0.9, −0.01 below 0.6).

| Comparison | Seeds | Difference |
|---|---|---:|
| margin 0, 64 samples (`ppo-11-search-m0`) vs ppo-11 | 2,840,000.. (4,000 walls) | +0.017 ± 0.025 (z = 0.7) |

Inconclusive: frequent switches make the arms' games diverge, so the paired
error grew fourfold, and the predicted +0.06 is 1.7 errors above the result.
Part of the playout leads may be bias (sampled opponent hands ignore their
discards and claims) rather than signal.

| Comparison | Seeds | Difference |
|---|---|---:|
| 128 samples, margin 1 (`ppo-11-search-s128m1`) vs ppo-11 | 2,860,000.. (6,000 walls) | **+0.028 ± 0.013 (z = 2.2)** |

About 3.2 CPU-seconds per seat-hand (6,000 walls in 3.1 hours on ten cores).
All three settings are positive (+0.012, +0.017, +0.028); **search over
ppo-11 is the strongest player so far**, roughly +0.13 over tuned-family, at
the price of knowing the field's policy for playouts. `mj advise --network` and
`mj play --hint-network` accept search strategy files (`cd610a8`) and print the
playout leads and the search's choice at searched decisions, e.g.

```sh
cargo run --release -- advise --seed 2900001 --step 15 --samples 0 --network target/learning/v3/ppo-11-search-s128m1/strategy.json
```

| s128m1 vs ppo-11 in an **efficiency** field (playouts still model tuned-family) | 2,880,000.. (4,000 walls) | +0.012 ± 0.016 (z = 0.7) |

With the wrong field model the gain is not resolved: consistent with the
tuned-family result or with nothing. Using the network itself as the playout field
(`ppo-11-search-self`) needs no field model, but neural opponents evaluate the
network at every claim window too: at least 50 CPU-seconds per seat-hand,
about 15 times the tuned-family playouts, so it was stopped untested. A hybrid playout field
(`v3/ppo-11-hybrid`: ppo-11 turns, tuned-family claims, which ppo-11's claims
already match) costs about 10 CPU-seconds per seat-hand (40 walls in 4.4
minutes); resolving a +0.03 effect would take 6,000+ walls, about 10 hours.
Cheaper field-free search needs batched playout inference (many playouts'
network calls in one matrix product) before that test is worth running.

**Expert iteration on search labels (2026-10-24, `53d38b2`, `pipeline12.sh`).**
`python -m mjers.expert collect` plays ppo-11 in one random seat per hand
against the field (ten worker processes, each an `Env`), and at decisions below
`--confident` stores the second candidate's paired lead and error from
`Env.estimate_actions` (the student in its seat, the env's teacher elsewhere),
plus `--anchors-per-hand` other student decisions. `refine` fine-tunes the
checkpoint on soft targets: its own probabilities, with each searched row's
two-candidate mass split by the posterior P(second better) =
Φ(d·τ / (e·√(τ² + e²))) for prior τ (0.32 from the probe). Symmetry
augmentation, no value loss; the collected hands join `training_seeds`.
About 3 CPU-seconds per searched decision at 128 samples (0.85 searched per
seat-hand). Run: 60,000 hands, seeds 4,000,000–4,059,999, 64 samples, 2 anchors
per hand (`target/learning/expert-1`); refine at lr 1e-5 and 3e-5, 3 epochs,
batch 512; each compared with ppo-11 on 20,000 walls (2,900,000.., 2,920,000..).

| Refined from ppo-11 | Difference vs ppo-11 |
|---|---:|
| lr 1e-5 | −0.003 ± 0.008 |
| lr 3e-5 | +0.003 ± 0.009 |

**Search labels do not distill.** 181,785 positions (61,884 searched, 0.34 per
hand at 64 samples; 45 minutes per 10,000 hands). On held-out hands the
network's preference between the two candidates agrees with the posterior
50.8% before and 50.6% after training, and searched validation loss does not
fall (0.882 → 0.890 / 0.874) while anchor loss rises (0.009 → 0.020 / 0.037).
Even a perfect predictor of the true sign would agree only about 65% with
labels this noisy (signal 0.32 against error about 0.6), but the network finds
none of it: playout leads at close calls are not a function it learns from
60,000 examples. Search stays a run-time tool.

**Close calls lean on discard order.** Zeroing input planes of ppo-11 on
those positions (`positions-0.npz`, 30,159 rows) changes its choice at:

| Planes zeroed | Unsure decisions | Anchors |
|---|---:|---:|
| 28–31, latest discard position per seat/tile | 30.8% | 0.5% |
| 8–23, discards per seat | 30.1% | 0.3% |
| 24–27, claimed discards | 7.3% | 0.0% |
| 32–47, melds | 8.9% | 0.0% |

Confident decisions ignore the discard history; close calls depend on it
(partly out-of-distribution, but anchors barely move). This is the strongest
hint so far that richer discard information (encoding v2: full order, whether
each discard was the tile just drawn) could matter where the policy is weakest.

**Encoding v2 (2026-10-25, `231f0f1`, `a70036c`, `3ffc151`).** `Discard`
records `drawn`: the seat discarded the tile kind it had just drawn (never
after a claim). Version 2 appends 8 planes to version 1's unchanged 50 (58 in
all): 50–53, each relative seat's first six discards (counts / 4); 54–57, each
seat's just-drawn discards (counts / 4). Rust inference infers the planes a
network reads from its stem (`ResidualNet::planes`), and the CLI and npm loader
check them against the checkpoint's version (`encoding::planes`), so version 1
checkpoints such as ppo-11 keep playing exactly as before (a 200-wall compare
and PyTorch parity unchanged). Python networks take explicit `planes` and slice
observations; version 1 datasets no longer load for training. `python -m
mjers.upgrade` gives a version 1 checkpoint zero stem weights for the new planes
(`target/learning/v4/ppo-11-v2`, identical play), so PPO can learn to use them
without re-collecting or distilling.

Test (`pipeline13.sh`): three PPO chunks from ppo-11-v2 (`v4/ppo-18v2` …
`ppo-20v2`, seeds 1,800,000,000.., 1,900,000,000.., 2,000,000,000..) and a
version 1 control from ppo-11 on the same hand and PPO seeds (`v4/ppo-18c` …
`ppo-20c`); then ppo-20v2 and ppo-20c vs ppo-11 (2,960,000.., 2,980,000..) and
ppo-20v2 vs ppo-20c (3,200,000..). Each chunk took about 10 minutes on AC.

| Comparison | Seeds | Difference |
|---|---|---:|
| ppo-20v2 vs ppo-11 | 2,960,000.. | −0.006 ± 0.010 |
| ppo-20c vs ppo-11 | 2,980,000.. | +0.011 ± 0.009 |
| ppo-20v2 vs ppo-20c | 3,200,000.. | +0.008 ± 0.010 |

**Null, because PPO barely touched the new inputs.** The mean absolute stem
weight on planes 50–57 grew 0 → 0.0018 → 0.0025 → 0.0030 over the three chunks,
against 0.169 on the old planes (unchanged). About 7,000 Adam steps per chunk at
lr 3e-5 could have moved each weight by up to 0.2; moving 0.003 means the
policy gradient's sign on those weights keeps flipping. The PPO signal is too
noisy to discover new features, so a fair test of the information needs a
dense target. Three more fixed-field chunks from ppo-11 add about +0.01 (the
control), the same plateau as before.

**Reading opponents (`2c78fbf`, scratchpad `danger.py`).** `Env.opponent_waits()`
and `Arena.pending_waits()` give hidden-information training labels: for each
other seat (next, across, previous), the tile kinds that would complete its hand.
ppo-11 (sampled) vs tuned-family, 40,000 hands (seeds 5,000,000–5,039,999,
444,632 student decisions) for training and 5,000 (5,040,000–5,044,999) for
validation; 16% of opponent rows are tenpai. Six epochs each, AUC on held-out
positions (deal-in = any opponent waits on a legal discard):

| Predictor | Waits AUC | Deal-in AUC | Deal-in BCE |
|---|---:|---:|---:|
| linear probe on frozen ppo-11 tile features | 0.69 | 0.66 | 0.118 |
| fresh trunk (width 96, 6 blocks), v1 planes | 0.925 | 0.904 | 0.086 |
| fresh trunk, v2 planes | 0.925 | 0.904 | 0.086 |

Version 2's planes add **nothing** to reading waits, so the PPO null above is not
just slow learning. ppo-11's features barely encode danger, although the inputs
support 0.90 AUC. That is rational here: a deal-in costs the discarder 2
against 1 when another seat deals in and 3 when the winner self-draws, so
avoiding one rarely pays (folding loses in `experiments.md`). An auxiliary danger
loss is not worth adding under family payouts.

### In the browser (`4315b70`)

`mjers-wasm` exposes `Network` (`evaluate(game, seat)` → settlement and legal
action probabilities) and `Bot.fromNetwork(network)`. The npm wrapper's
`mjers.loadNetwork(bytes)` parses the safetensors header with `JSON.parse`,
checks architecture and encoding version, and adds each float32 tensor through
`NetworkWeights`, so no JSON parser enters the Rust build. Tested in
`npm/tests/engine.test.mjs` with an in-memory checkpoint; documented in
`npm/README.md`. A width-96 checkpoint is about 2.4 MB.

## Reward and evaluation

- **The objective is EV in units per hand:** the seat's settlement total, which
  is zero-sum across seats. Win rate alone is the wrong target, because kong
  income and the extra discarder payment matter.
- **Variance is large:** a per-hand standard deviation of about 6 units. Use
  duplicate comparison (`mjers_sim::compare`), where the candidate and baseline
  play every seat of the same walls. It resolved differences of ±0.003 units/hand
  with 20k seeds × 4 seats.
- **Keep validation seeds disjoint:** never select on seeds you report.
  `mj tune` already validates on the next seed block.

**Baselines,** in units per hand against an efficiency field, family rules:

| Policy | Units per hand |
|---|---|
| `tuned-family` (efficiency + always concealed/upgrade kong) | +0.041 ± 0.007 vs efficiency (held out) |
| `kong-always` | +0.033 |
| Efficiency | 0 |
| Defensive | −1.25 |
| Fully concealed | −1.59 |
| Random | about −2.8 in mixed tables |

The neural net should at least beat `tuned-family` in duplicate play.

## What the experiments imply for learning (`experiments.md`, `docs/LEARNINGS.md`)

- **Speed dominates.** Defence and folding lose about 1.2 units/hand under family
  payouts. A learned policy that discovers useful defence would be a genuine
  finding, because the heuristic danger model may simply be poor.
- **Kongs are paid immediately and pay even with no winner,** worth +4 units per
  kong in winning hands. The net must learn their value; the reward signal arrives at hand end.
- **Claiming:** chow and pong when it lowers shanten; never-chow costs 0.47;
  late-only claiming costs 0.67. The heuristic's shanten-improvement rule is a
  strong baseline to imitate first.
- **Wait quality:** live tile count predicts winning (8+ live: 73%,
  2 live: 28%). On an eye wait, 1/9 and honors (40%) beat middle tiles (30%),
  likely because opponents discard them more. These are observational
  hypotheses the net could test.
- **Dealer** advantage is about +0.14/hand. About 9% of hands end with no
  winner; robbing the kong almost never happens.
- **Bot behaviour baked into the data:** heuristic bots always take a win when
  offered, and the defence model's "passed tile" safety assumes that. A net
  might decline wins (it never should under these rules), so keep win-forcing
  as an option.

## Performance numbers (2026-10-19, release, Apple Silicon)

Measured at revision `cac74b0`; full methodology and profile findings are in
`docs/PERFORMANCE.md`.

- Shanten: 236 ns per batch of four fixed hands (about 59 ns/hand).
- Ranking discards: 33.3 µs per batch of four fixed hands (about 8.3 µs/hand).
  These repeatedly use the same hands and warm suit caches; simulation sees
  more varied shapes and cache misses.
- A whole hand with efficiency bots: about 475 µs single-threaded; defensive
  bots: 590 µs. Engine alone with random legal actions: 32.6 µs/hand.
- CLI simulation: about 2,030 hands/s with one Rayon worker and 10,980
  hands/s with ten workers (10,000 hands, median of three runs).
- Advisor instant ranking: 12.4 µs on the benchmark position. Monte Carlo with
  3 candidates × 20 samples: 18.8 ms on that position. This is not a new
  measurement of the older 300-sample × 6-candidate workload, nor a guarantee
  about latency on other positions. The earlier experiment's roughly ±0.4-unit
  paired standard error at 300 samples is a statistical observation, not a
  speed measurement.
- A 20,000-hand single-worker CPU profile puts about 91% of samples under
  discard ranking, including about 87% under improving-draw evaluation.
  The largest leaf costs are `shanten_with` (about 40%) and `Summary::combine`
  (15%); recursive suit search is about 11%. Summary combination is the next
  place to investigate; rollout speed still limits self-play and search.

After consolidation, efficiency-bot hands measured about 483 µs and the same
advisor rollout benchmark 18.8 ms. CLI throughput measured about 2,060 hands/s
with one worker and 10,140 with ten; parallel results varied more than before.
See `docs/PERFORMANCE.md` for the full comparison and the small increase in
random-play engine overhead.

## Pitfalls to remember

- **Observations must never leak hidden information.** Opponents' concealed tiles
  and the wall order are reachable only through `Game` internals. Oracle training
  should use a separate, explicit encoder.
- **Shanten ignores the four-copy limit:** a hand holding all four copies can
  have theoretical shanten 0 with no physically possible winning draw. Waits
  and ukeire never report a fifth copy, but policies also compare shanten,
  so do not assume this approximation cannot affect rankings or claiming.
- **Claim windows:** a seat's decision is recorded before others decide.
  Resolution needs all asked seats, so batching needs care.
- **Honour the WASM constraint** for anything the browser will run: no
  threads, no OS randomness, no file I/O in `mjers-core`. Verify with
  `cargo build --locked -p mjers-core --target wasm32-unknown-unknown`. Neural-net
  inference must also compile for this target.
- **The feature log is observational** (selection effects are everywhere, e.g. "4 exposed
  pongs wins 35%"). Test policy questions with duplicate head-to-head.
- **Commit conventions:** lowercase short messages, no trailers. Documentation
  files stay uncommitted.
