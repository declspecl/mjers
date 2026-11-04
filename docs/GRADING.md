# Move grading

The current objective is expected settlement units for this hand. Analysis only
uses the acting seat's observation; the actual hidden hands and subsequent result
are not inputs. Unknown tiles are sampled uniformly subject to visible tile counts.
This does not yet infer opponents' concealed shapes from their discard choices.

## Decision analysis

The web worker waits for the player's chosen action, analyzes the position before
replaying that action, and compares every legal candidate. It first uses 48 paired
playouts per candidate. If the chosen move's grade or severity remains unresolved,
it repeats with the same seed and 384 samples. The second pass includes the same
sample prefix but currently recomputes it, so the total cap is 432 attempted
playouts per candidate. This keeps the UI thread free but can delay feedback.

The engine supplies each candidate's mean return, plus direct mean differences and
standard errors against every other candidate using the same samples. Keeping this
covariance matters: two alternatives can move together relative to the baseline,
while their difference is precisely known. Earlier engine packages omit these
comparisons; the web app then uses a conservative sum of baseline-relative errors.
Publishing the updated npm engine and updating the web dependency is needed to
activate direct comparisons in a normal deployed web build. Building the engine
locally is sufficient for the benchmark below.

Loss is the best candidate's estimated return minus the chosen move's. For each
alternative the grader constructs an approximate three-standard-error range of its
advantage, then takes the maximum lower and upper endpoints, clipped at zero.
A precise label requires the whole resulting range to occupy one loss band.
If the lower endpoint exceeds 0.4 units but severity spans bands, the label is
"Mistake". Otherwise unresolved evidence is "Unclear". Matching a recommendation
and preserving hand shape do not override this evidence. Taking or declining an
available win is handled directly; forced decisions are not requested for grading.

Three standard errors is a cautious heuristic for repeated analysis and multiple
candidates, not a formally calibrated simultaneous or sequential confidence bound.
Rare outcomes and rollout policy bias are not captured by sampling error alone.

## Repeatable diagnostic

From the repository root:

```sh
npm test --prefix npm
cd web
pnpm exec tsx scripts/benchmark-grading.ts > .cache/grading-benchmark.json
```

The diagnostic uses fixed opening and midgame positions, selects a move before
simulating, and compares adaptive feedback against a separately seeded 1,536-sample
reference. It also verifies analysis leaves the game observation unchanged. The
reference uses the same efficiency rollout policy, so agreement does not establish
human playing strength. The regression tests separately cover exact wins, clear
losses despite good shape, covariance cancellation, candidate ordering, missing or
invalid evidence, and analysis budget exhaustion.

## Calibration work still required

Create a versioned, expert-reviewed position corpus including efficient alternatives,
valuable slower hands, dangerous discards, defensive folds, calls, and kongs at
several wall depths. Record only information visible at the decision. Annotate
acceptable moves and the reason an inferior move loses value; retain disagreement.
Use independently seeded, higher-budget runs and multiple strong rollout policies
to distinguish sampling instability from evaluator bias. Reserve positions for a
held-out check rather than tuning thresholds on the entire corpus.

The existing loss bands (0.15, 0.4, 0.8, and 1.5 units) remain provisional. Evaluate
false praise, false accusations, unresolved rate, and feedback latency before
changing them. Confidence wording should be calibrated against repeated independent
runs and expert verdicts. Simulation return differences alone do not establish a
causal explanation such as "safer"; explanations need their own supporting evidence.

Incremental sample accumulation, contender elimination, stronger rollout policies,
behavior-informed hidden-state sampling, and a match-standing objective remain
separate improvements. None is implied by the current grade labels.
