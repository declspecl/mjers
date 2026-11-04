# Grading policy study, 2026-11-03

Fresh duplicate matches use family rules, seeds 41,000,000 through 41,009,999.
Each candidate plays the same 10,000 walls in each of four seat positions against
the same field. Candidate A and B together therefore complete 80,000 hands per
comparison; the four comparisons complete 320,000 hands. All runs finished
without errors. Two experiments ran concurrently, each with two Rayon workers.

The errors below are standard errors over independent wall means. Four seat
rotations share a wall and are not treated as independent observations.

| Candidate A versus B | Other three seats | A minus B units/hand | Standard error | z |
|---|---|---:|---:|---:|
| defensive vs efficiency | efficiency | -0.608100 | 0.022307 | -27.26 |
| tuned-family vs efficiency | tuned-family | +0.017000 | 0.003246 | 5.24 |
| tuned-family vs defensive | tuned-family | +0.636325 | 0.022429 | 28.37 |
| tuned-family vs efficiency | defensive | +0.012925 | 0.003851 | 3.36 |

The defensive policy really does deal in less: 15.58% versus efficiency's 19.19%
in the efficiency field. But its win rate falls from 24.95% to 20.00%, and its
settlement suffers. A grading mechanism that assumes a safer discard or a fold is
inherently better would therefore systematically teach worse play for this
ruleset. This does not prove every fold is bad; it rejects this preset as the
stronger overall baseline.

Tuned-family differs from efficiency mainly by declaring every kong rather than
only those the heuristic considers free. In a tuned field it wins slightly less
(24.94% versus 25.07%) yet collects enough kong value to improve settlement.
This is a useful counterexample to grading only shanten, improving tiles, or
win probability: the objective here is settlement units. The same small kong
advantage persists against the defensive field.

Family-rule grounding matters. Concealed kongs collect six units and exposed or
upgraded kongs collect three units immediately, even if the hand later draws.
There is no closed-hand, pure-suit, or fancy-pattern premium. A discard win
collects four base units and self-draw collects nine, plus bonus payments. A
deal-in is only one extra base unit worse than another opponent discarding the
winning tile. Generic riichi-style folding or closed-hand value assumptions are
not interchangeable with these rules.

Existing experiments.md provides a further caution, measured on different seeds:
the old Monte Carlo advisor with four candidates and 50 playouts lost
0.359 +/- 0.102 units/hand against tuned-family, and the two-candidate/100-playout
advisor lost 0.156 +/- 0.134. Its sampling ignores opponents' discard-conditioned
concealed shapes, and selecting a noisy winner adds selection bias. Those are
prior results, not rerun as part of this study. Current fresh matches concern
complete heuristic policies, not the production move grader.

No neural checkpoint was available under target/learning in this workspace.
The project records prior learned-policy gains, but those cannot be replicated
or recommended as the available browser baseline from the current checkout.
Tuned-family is the strongest available preset tested here, with only a small
edge over efficiency; it is still a simple heuristic, not a human-expert oracle.

Raw reproducible reports (resolved strategies, rules, seeds, and outcomes) are in:

- target/grading-policy-study/defensive-vs-efficiency.json
- target/grading-policy-study/tuned-vs-efficiency.json
- target/grading-policy-study/tuned-vs-defensive.json
- target/grading-policy-study/tuned-vs-efficiency-defensive-field.json
- target/grading-policy-study/summary.json

Reproduce each with the corresponding command, for example:

```sh
RAYON_NUM_THREADS=2 target/release/mj compare strategies/tuned-family.json efficiency --field strategies/tuned-family.json --hands 10000 --seed 41000000 --json
```

Implementation implications:

1. Keep settlement units as the objective; do not silently switch to winning rate.
2. Account explicitly for paid kongs before declaring structural damage a blunder.
3. Use efficient hand development as the default, but describe defensive-risk
   estimates as heuristic evidence rather than definitive probabilities.
4. Do not replace the efficiency baseline with the defensive preset under the
   assumption that it is stronger. An ensemble can expose model disagreement,
   but averaging a clearly weaker policy's values is not automatically better.
5. Use actual comparisons to calibrate the direction of advice; strategy labels
   and reassuring explanations alone are not evidence of stronger play.
