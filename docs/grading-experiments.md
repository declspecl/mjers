# Grading experiments, 2026-11-03

## Decision

Use immediate coaching about visible hand progress, kong payments, and explicit
safety/value tradeoffs. Do not use full-hand Monte Carlo uncertainty bands for
routine live labels. The current simulator's hidden-state model is not a reliable
reference for expert move quality. The labels deliberately say what they measure;
there is no claim of optimal-play accuracy.

Production `analyzeMove` requests `samples: 0` once. `grade` compares shanten and
unseen improving copies, recognizes wins, accounts for paid kongs, distinguishes
postponing a kong from breaking it, and describes meaningful safety sacrifices as
tradeoffs. Modest same-shanten acceptance losses receive gentle feedback. Unknown
or forced decisions remain ungraded. The toast shows the basis and a short reason;
the side panel contains the explanation and alternative. Old interval grading is
retained in `playout-grade.ts` as an offline diagnostic, never as live feedback.

## Evidence

Three parallel studies investigated policy strength, evaluator behavior, and
feedback stability. A separate hidden-state test and held-out run checked the new
approach. No external data or opponent private tiles are used for live grading.

- Policy matches: 4 paired experiments, 10,000 walls per experiment, four seat
  rotations, both policies: 320,000 complete hands. Standard errors are over paired
  wall means. Defensive versus efficiency in an efficiency field lost 0.6081 ±
  0.02231 units per hand. Tuned-family versus efficiency in a tuned field gained
  0.0170 ± 0.00325. See `target/grading-policy-study/*.json` and
  `docs/grading-policy-study.md`.
- Hidden-state diagnostic: 2,048 complete efficiency hands, seeds 6000–8047.
  In middle-game snapshots, actual opponents were ready 9.35% of the time versus
  0.13% in uniform hidden samples. Late snapshots: 58.29% versus 0.90%.
  Late snapshots are conditioned on a hand surviving that long; the actual and
  sampled groups share those same positions. Opponents within a hand and samples
  within a snapshot are correlated. These are descriptive results, not confidence
  intervals or human-population estimates.
- Feedback study: four complete hands, 18 positions, 49 alternatives. The old
  exact-band rule was unclear on 31/49 decisions even at 64 playouts. Relaxing the
  rule caused a false accusation against a static-best move that an independently
  seeded 768-playout run considered close to best. That deeper reference shares
  the same policy and sampling flaws; agreement is not expert validation.
- Policy/evaluator study: six complete mixed-policy hands, 12 turn/claim positions,
  256 samples per policy and selected independent 1,024-sample reruns. Most modest
  acceptance differences remained unresolved. One additional shanten without
  increased payment was a useful warning, but is not a calibrated settlement loss.
- Held-out check: 256 complete hands, seeds 90000–90255, with efficiency and random
  trajectories; compare efficiency, defensive, and random choices at each actual
  non-forced decision. 15,012 decisions all produced an explanation; 1,016
  reconstructed observations produced identical grades. Desktop median assessment
  plus three policy choices was 0.050 ms, p95 0.085 ms; this excludes worker startup,
  network delay, and mobile performance. Efficiency choices received 13,563
  Excellent labels; random choices received 3,677 Excellent, 5,948 Bad, and 100
  Terrible. This is a coverage/sanity check, not independent accuracy validation:
  the coaching intentionally shares efficiency features with that bot.

## Important counterexamples preserved as regression fixtures

- Seed 1003, step 33: declaring concealed kong 6s pays six, but discarding 2p
  preserves the quad and may collect it later. Do not call that a missed payment.
  Discarding 6s actually destroys the quad.
- Seed 1008, step 30: kong sacrifices readiness but pays three. Explain compensation
  rather than mechanically condemning the shanten increase.
- Seed 1015, step 47: upgrading 3z preserves readiness and collects three;
  discarding the fourth tile instead forgoes that payment.
- Seed 1017, step 22: sacrificing one shanten reduces rough danger. Different
  rollout policies disagree on its value, so present a safety tradeoff.
- Seed 601, step 38: chow 4s reaches ready while chow 5s or pass does not. Progress
  feedback must cover claims, not just discards.

## Reproduction

Run from `web/` after building the local engine with `npm run build --prefix npm`
from the repository root:

```sh
pnpm exec tsx scripts/experiment-hidden-state.ts
pnpm exec tsx scripts/experiment-feedback.ts
pnpm exec tsx scripts/experiment-policy.ts
pnpm exec tsx scripts/experiment-policy-edge.ts
pnpm exec tsx scripts/experiment-folds.ts
pnpm exec tsx scripts/experiment-live-feedback.ts
```

Outputs are in `web/.cache/*experiment*.json`; raw policy match reports are in
`target/grading-policy-study/`. The comparison chart is in
`web/.cache/grading-experiment-report.html`. Fixed real observation fixtures live in
`web/tests/fixtures/grading-positions.json`. Scripts encode seeds and configurations.

## Remaining limitations

Thresholds for improving-copy ratios and risk differences are coaching choices,
not a fitted mapping to expected units. Risk is a rough estimate, not a probability.
Improving copies include tiles in unknown opponent hands as well as the wall.
No suit/pattern/fan premium exists in this family's rules; paid kongs and bonuses do.
Defensive folding that is sensible in another mahjong variant can lose here.
Expert-reviewed positions and a behavior-conditioned hidden-state model remain
necessary before claiming calibrated overall strategic grades.
