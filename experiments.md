# Strategy studies

> **Re-measured 2026-10-20 after the discard fix (`158ce93`).** Before that commit
> every heuristic bot broke ready hands: its discard score rewarded a slower
> option's improving-tile count against the best option's winning tiles, so
> a narrow wait lost to almost any 1-shanten shape. All bots were affected,
> which produced the old 9.7% no-winner rate (now 0.2%) and inflated defence's
> cost. The tables below are the corrected runs on the same seeds; the old
> values are kept in parentheses where the conclusion's size changed.
> The `mj study` section was re-run as well.

All runs: release build, family rules unless stated, efficiency field.
"Δev" is a duplicate comparison: the candidate and the efficiency baseline
each play every seat of the same 20,000 walls (seeds 1,000,000..1,020,000),
so the difference is paired and luck of the deal cancels. Units are per hand.
|z| > 2 is unlikely to be noise; every result below is far beyond that.
Standard errors in these tables were computed per (wall, seat) pair; since
`b1002c4` `mj compare` computes them across walls, which is the correct unit
(the values are similar; seat rotations need not be positively correlated).

Reproduce any row with, for example:

```sh
cargo run --release -- compare strategies/no-chow.json efficiency --hands 20000 --seed 1000000
```

These are results for *these bots*. A human who reads opponents better than
the heuristic danger model may defend more profitably than the bots do.

## Claiming (commit plan 37)

| strategy | Δev | ± se | z | win% | dealin% | chows/h | pongs/h |
|---|---|---|---|---|---|---|---|
| efficiency (baseline) | 0 | | | 25.0 | 19.2 | 0.58 | 0.51 |
| pong whenever legal | −0.097 (−0.125) | 0.011 | −8.5 | 24.1 | 19.9 | 0.57 | 0.72 |
| chow whenever legal | −0.326 (−0.185) | 0.015 | −21.1 | 22.7 | 19.2 | 1.10 | 0.51 |
| claim everything | −0.390 (−0.230) | 0.017 | −22.3 | 22.0 | 19.8 | 1.05 | 0.70 |
| never chow | −0.661 (−0.471) | 0.016 | −40.3 | 18.9 | 19.9 | 0 | 0.58 |
| claim only at ≤1-shanten | −0.861 (−0.671) | 0.019 | −46.1 | 17.7 | 19.4 | 0.13 | 0.09 |
| fully concealed (no chow/pong/exposed kong) | −1.717 (−1.587) | 0.020 | −84.2 | 11.7 | 20.2 | 0 | 0 |

- **Claim when it brings you closer to ready, not otherwise.** Both extremes lose:
  refusing chows costs ~0.66/hand, claiming indiscriminately ~0.39.
- **Chows are valuable** under these rules: there is no concealed-hand bonus,
  so the speed of a chow is nearly free. Never chowing drops win rate 25.0% → 18.9%.
- **Claim early.** Waiting until 1-shanten to claim is worse than never chowing.
- A fully concealed hand is the worst strategy tested: −1.7 units/hand.

## Kongs (commit plan 38)

| strategy | Δev | ± se | z | kong units/h |
|---|---|---|---|---|
| kong always (even if it costs shape) | **+0.015** (+0.033) | 0.002 | +6.3 | +0.029 |
| efficiency: kong when it doesn't hurt shanten | 0 | | | 0 |
| never concealed kong | −0.079 (−0.102) | 0.003 | −22.8 | −0.071 |
| never kong | −0.274 (−0.421) | 0.007 | −38.3 | −0.254 |

- **Always declare kongs.** Kongs pay at once (3 units exposed, 6 concealed) and
  are paid even with no winner; that outweighs the occasional shape loss.
- Concealed kongs alone are worth ~0.08/hand; all kongs together ~0.27/hand.

## Defence vs. speed (commit plan 39)

| strategy | Δev | ± se | z | win% | dealin% |
|---|---|---|---|---|---|
| efficiency | 0 | | | 25.0 | 19.2 |
| cautious (weigh danger, never fold) | −0.544 (−1.215) | 0.020 | −27.6 | 20.4 | 16.1 |
| defensive (weigh danger, fold at ≥2-shanten) | −0.582 (−1.247) | 0.020 | −29.6 | 20.1 | 15.7 |

- Defence cuts deal-ins by ~3.5 points but costs ~5 points of win rate (it
  halved the win rate with the buggy discards). Under family
  payouts a deal-in costs only **1 extra unit** over another player's discard
  win (2 vs 1), while a win collects 4–9+, so speed dominates.
- Even under `discarder-pays` (deal-in costs 4) defence still loses (−0.30/hand, was −0.8).
- Caveat: caution weight 1.0 is clearly too strong; see tuning for whether a
  small weight helps.

## Ruleset sensitivity (commit plan 40)

All-efficiency tables, 50,000 hands (seeds 2,000,000..).

| ruleset | win% (per seat) | self-draw% | no-winner% | dealer ev |
|---|---|---|---|---|
| family (8 stacks) | 22.70 | 5.47 | 9.19 | +0.142 |
| family, 9 stacks | 22.29 | 5.37 | 10.85 | +0.140 |
| family, rob the kong | 22.70 | 5.47 | 9.18 | +0.142 |
| discarder-pays | 22.70 | 5.47 | 9.19 | +0.102 |

- **8 vs 9 stacks:** 9 stacks makes no-winner hands ~18% more common
  (9.2% → 10.9%) and wins slightly rarer; strategy conclusions are unchanged
  (never-chow −0.47, kong-always +0.034, defence −1.2 under 9 stacks).
- **Robbing the kong** almost never arises with these bots.
- **The dealer** gains ~+0.14 units/hand from the 14th tile.
- **Discarder-pays** narrows defence's deficit (−1.2 → −0.8) and the cost of
  never chowing (−0.47 → −0.36), but changes no conclusion.

Corrected spot checks (duplicate, 20,000 walls, seeds 1,000,000..): under
`discarder-pays`, defensive −0.304 ± 0.018, never-chow −0.495 ± 0.015,
kong-always +0.019 ± 0.002; under 9 stacks, defensive −0.580, never-chow −0.659,
kong-always +0.015. The no-winner rate is now about 0.2% (0.6% with 9 stacks).
The ruleset table above predates the fix.

## Tuning (commit plan 36)

```sh
cargo run --release -- tune --hands 5000 --rounds 3 --seed 3000000 --name tuned-family --output strategies/tuned-family.json
```

Coordinate search from the efficiency preset, with each trial a duplicate
comparison on 5,000 training walls; changes adopted at z ≥ 2. Stopped after
round 1 with no further changes (48 trials).

Adopted:
- `concealed_kong: always` (+0.025 ± 0.005)
- `upgrade_kong: always` (+0.024 ± 0.005)

Held-out validation on seeds 3,005,000..3,010,000: tuned vs start
**+0.041 ± 0.007 units/hand (z = 6.3)**. The win and deal-in rates are unchanged
(22.5% vs 22.7%, 17.4% vs 17.4%), so the gain is kong income.

**Re-tuned after the discard fix** (same command and seeds): adopted
`exposed_kong`, `concealed_kong` and `upgrade_kong: always`; held-out
**+0.018 ± 0.005 (z = 3.6)**. `strategies/tuned-family.json` now equals
`kong-always` (`e716f95`).

Rejected in the original (buggy-bot) run, with the cost of each alternative against the champion:
- `caution` 0.25 / 0.5 / 1 / 2: −0.51 / −0.92 / −1.28 / −1.54. Any weight on
  danger hurts these bots.
- `claim_max_shanten` 3 / 2 / 1 / 0: −0.05 / −0.32 / −0.78 / −1.33. Claim at
  any distance.
- `pong` never / always: −0.82 / −0.17. `chow` never / always: −0.48 / −0.23.
- `exposed_kong` never: −0.15; `always` is identical to when-free.
- The fold settings showed no effect. That's a limit of the search: folding
  needs both `fold_shanten` and `fold_readiness` changed together, and the
  defensive preset, which does both, loses 1.2/hand.

## Advisor, danger tie-breaks and the neural policy (2026-10-20)

Duplicate comparisons in a tuned-family field (after the discard fix).

| candidate vs tuned-family | walls (seeds) | Δev | ± se |
|---|---|---|---|
| Monte Carlo advisor, 4 candidates × 50 playouts | 1,000 (50,000..) | −0.359 | 0.102 |
| Monte Carlo advisor, 2 candidates × 100 playouts | 500 (51,000..) | −0.156 | 0.134 |
| tuned-family + caution 0.01 (danger breaks near-ties) | 20,000 (1,500,000..) | −0.000 | 0.015 |
| tuned-family + caution 0.05 | 20,000 (1,500,000..) | −0.001 | 0.015 |
| neural `ppo-4` (imitation + PPO) | 20,000 (1,200,000..) | **+0.061** | 0.015 |
| `ppo-4` on turn decisions only | 20,000 (1,200,000..) | +0.063 | 0.016 |
| `ppo-4` on claim decisions only | 20,000 (1,200,000..) | −0.002 | 0.005 |

All rows used `mj compare`. Except the `ppo-4` total, which was re-measured
after `b1002c4`, their errors are per (wall, seat) pair rather than per wall.

- **The playout advisor is worse than the heuristic it rolls out with.** It
  chows far more (0.85/h vs 0.58) and wins less (22.5% vs 25.0%). Picking any
  candidate one standard error ahead of several noisy estimates favours lucky
  ones, and sampled opponent hands ignore what opponents discarded.
- **Danger as a tie-breaker is worth nothing measurable** for these bots.
- **A learned policy beats the tuned heuristic**, entirely through discards
  and kongs. Compared with the teacher it more often throws the safer of two
  equally efficient tiles (especially once ready: a terminal rather than a
  middle tile) and occasionally gives up a couple of live tiles. Details and
  caveats are in `NN.md`; the network's gain is not explained by danger
  tie-breaking alone.

## Takeaways for play

1. Claim chows and pongs whenever they bring you closer to ready, at any stage.
2. Declare every kong, especially concealed ones (6 units, paid even if nobody wins).
3. Don't fold. Under family payouts a deal-in costs 1 unit more than another
   player's discard win, and a win pays 4–9+.
4. A fully concealed hand has no value under these rules.

## Tiles, waits, and set splits (`mj study`)

```sh
cargo run --release -- study --strategy strategies/tuned-family.json --hands 200000 --seed 4000000
```

Tuned bot in every seat, 200,000 hands (199,611 wins; re-run 2026-10-20 after
the discard fix, old values in parentheses). Observational: these describe how
the bots' hands turn out, not causal effects.

- **Winning splits:** 3 chows + 1 pong 38.8% (37.4), 4 chows 31.1% (24.6),
  2 + 2 17.9% (20.7), 1 chow + 3 pongs 3.2% (4.6), 4 pongs 0.2% (0.4). About 9%
  of wins include a kong (12%); each kong adds about 4 units (8.5 → 12.4 → 16.2).
- **Wait shape when first ready → eventual win rate:** 3+ tile waits 62.8%,
  two-sided chow 53.3%, pair to pong 44.2%, gap 38.3%, eye (tanki) 37.7%,
  edge 34.4%. More players now reach ready, so each wait wins less often than
  in the buggy run (70.7 / 61.2 / 35.8 / 38.5 / 37.3 / 33.8).
- **Live winning tiles when first ready:** 8+ live 62.7%, 4 live 40.8%,
  2 live 26.5%, 1 live 17.2%, 0 live (dead wait) 8.4% in 1,328 cases. The bot
  still keeps a dead ready hand rather than reshaping; that is a remaining weakness.
- **Eye waits by tile:** 1 ≈ 42%, 2/9 ≈ 38%, 3/8 ≈ 36%, 4–7 ≈ 31–34%; honors
  44–51% on very few cases (101).
- **Discarded:** winds 81%, dragons 79%, 1s 52%, 9s 46%, 2s 37%, 8s 34%, 3–7 25–28%.
- **Realized (melded or in a win):** 3–7 ≈ 27–28%, 2/8 ≈ 24%, 1/9 20%,
  dragons 14%, winds 13%.
- **Average winning hand:** chows 0.65 exposed + 2.35 concealed; pongs 0.61
  exposed + 0.30 concealed; kongs 0.08 exposed + 0.02 concealed. Fewer claims
  are needed now that ready hands are kept.
- **Chows:** 67% inner, 33% touching a terminal. Claimed chows completed a
  two-sided wait 60.0%, a gap 30.5%, an edge 9.4%. Hands with 2+ chows are
  95.5% of wins; their chows span 1 suit 6.6%, 2 suits 56.6%, 3 suits 36.8%.
- **Time from first ready to winning (winners only):** 3+ tile waits 7.1 table
  turns (1.5 own draws), two-sided 7.9 (1.7), pair to pong 9.3, gap 10.2,
  edge 10.1, eye 9.9 (2.2).
- **Exposed melds at the end:** pongs correlate with EV more than chows
  (0c 2p +1.20, 2c 0p +0.08, no melds −0.83), but this is selection, not cause.

## Bootstrapped search: implemented, not measured (2026-11-02)

`SearchConfig.bootstrap` (commit `54ffdc5`) stops each playout at the advised
seat's next real choice and scores it with the network's value head
(`montecarlo::estimate_values`) instead of playing the hand out. Tests and
clippy pass, but no strength comparison exists: every `target/` directory,
including all checkpoints (`ppo-11` and its search files), was deleted, and
retraining was deferred. To measure it, train a `suit-resnet-v2` with a value
head (imitation, then some PPO so the head is calibrated to returns), write two
search strategy files that differ only in `"bootstrap"`, and `mj compare` them
on identical walls. Results on a weaker network may not transfer to ppo-11.
