# Mahjong Learnings

Practical lessons from simulating millions of hands of our family's mahjong.

## Where these numbers come from

Computer players played hundreds of thousands of hands under our house rules.
To test a strategy, we replayed the exact same deals with only that one thing
changed, so the luck of the tiles cancels out. "Units" are the payment units
from the rules below. All results are statistically solid; small differences
are only reported when they are real.

**Caveats:**
- These are the best strategies *for the computer players we tested*. A person
  who reads opponents well may do better in places, especially defence.
- "+0.5 units per hand" sounds small, but over an evening of 50 hands it is
  about 25 units.

**Our rules, in short:**
- 13-tile hands, with flowers and seasons.
- Chow only from the player on your left; pong from anyone; win off anyone's discard.
- Any 4 sets + 1 eye wins. There are no special hands or points.
- **A win off a discard** pays 1 unit from each other player, and the discarder pays 2.
- **A self-drawn win** pays 3 units from each other player.
- **Each flower or season** adds 1 unit from every payer.
- **Kongs pay immediately**, from every other player: 1 unit for an exposed
  kong, 2 for a concealed kong, even if nobody wins the hand.

---

## The big six

1. **Play for speed.** Get to "ready" (one tile from winning) as fast as you can.
2. **Once ready, stay ready.** Never break a ready hand to chase a "wider"
   shape that is one step from ready again. Our computer players originally did
   this whenever the wider shape had many more useful tiles, and fixing it alone
   was worth about **+3.4 units per hand**, the biggest effect we have measured.
3. **Claim chows and pongs whenever they bring you closer to ready**, early or late.
4. **Always declare kongs**, especially concealed ones (6 units, paid even if nobody wins).
5. **Don't fold.** Throwing someone the winning tile costs you only 1 unit more than
   if someone else had thrown it. Giving up on your own hand costs far more.
6. **Wait on two or more different tiles**, ideally a two-sided chow (like 45
   waiting on 3 or 6). Avoid single-tile waits.

---

## Claiming (chow and pong)

| If you... | You win or lose per hand |
|---|---|
| Claim when it gets you closer to ready (the baseline) | — |
| Pong every pair you can, even when it doesn't help | −0.10 |
| Chow everything you can, even when it doesn't help | −0.33 |
| Never chow | **−0.66** |
| Only claim once you're one tile from ready | **−0.86** |
| Never chow or pong (fully concealed hand) | **−1.72** |

- Chows are worth a lot here: there's no bonus for a concealed hand, so the speed is free.
- **Claim early.** Holding back until late is worse than never chowing at all.
- Claim selectively: only when the meld actually helps your hand.

## Kongs

| If you... | Per hand |
|---|---|
| Always declare every kong | **+0.015** (the best strategy found) |
| Kong only when it doesn't hurt your hand | — |
| Never declare concealed kongs | −0.08 |
| Never kong | −0.27 |

- A kong pays you on the spot: 3 units if exposed, 6 if concealed.
- Winning hands that include a kong pay about 4 more units on average.
- Upgrading your exposed pong with the 4th tile is worth it too.

## Defence (avoiding throwing the winning tile)

- Playing defensively cut "dealing in" (throwing someone's winning tile) from
  19% to 16% of hands, but cut the win rate from 25% to 20%. Net: **−0.6 units per hand**.
- This is because of our payment rules: dealing in costs 2 instead of the 1 you'd
  pay anyway, while your own win collects 4 to 9+.
- Even if the discarder paid much more for a win, defence still lost (−0.3 per hand).
- **Most dangerous discards:** middle tiles (3–7). **Safest:** honors, 1s and 9s.
  A tile someone has just let pass (didn't win on) is safe against them for now.

---

## Waiting for the last tile

**Win rate after becoming ready, by wait shape:**

| Wait | Example | Win rate | Turns to win |
|---|---|---|---|
| 3+ different tiles | 34567 → 2, 5, 8 | **63%** | 7.1 |
| Two-sided chow | 11 + 78 → 6, 9 | **53%** | 7.9 |
| Pair to pong | 11 + 55 → 1, 5 | 44% | 9.3 |
| Gap chow | 79 → 8 | 38% | 10.2 |
| Single tile as the eye | 789 + 1 → 1 | 38% | 9.9 |
| Edge chow | 12 → 3 | 34% | 10.1 |

- **The number of copies still out there is what matters.** With 8+ live winning
  tiles you win 63% of the time; with 4 live, 41%; with 2 live, 27%; with 1, 17%.
- **Number of different tiles you can win on:** 1 → 37%, 2 → 52%, 3 → 63%,
  4+ → 68%.
- Turning 11 + 78 into "789 + 1 waiting on the eye" drops you from ~53% to ~38%.
- Most wins (about 75%) come off someone's discard, not your own draw.

## Eyes (the pair)

- If you must wait on a single tile for your eye, **pick 1, 2, 9, or an honor**:
  those wins happen ~38–42% of the time vs ~31–34% for 4–7. Others throw those tiles away.
- Middle tiles (3–7) are better used in chows than held as a lone eye.

---

## Tile values

| Tile | How often thrown away | How often used in a set or win |
|---|---|---|
| Winds | 81% | 13% |
| Dragons | 79% | 14% |
| 1, 9 | 46–52% | 20% |
| 2, 8 | 34–37% | 24% |
| 3–7 | **25–28%** | **27–28%** |

- **Honors are weak tiles here.** They can only become pongs (never chows) and
  score nothing extra in our rules. Isolated honors are usually the first discard.
- **3–7 are the most flexible tiles**, the most useful, and also the most dangerous to discard.
- Middle tiles often end a lost hand still in someone's hand, because they
  stayed useful to the end, not because they were dead.

## What winning hands look like

- **Typical split:** 3 chows + 1 pong (39% of wins), 4 chows (31%), 2 chows + 2 pongs (18%).
  All pongs is rare (0.2%).
- **Average winning hand:**

  | | Exposed | Concealed |
  |---|---|---|
  | Chows | ~0.7 | ~2.4 |
  | Pongs | ~0.6 | ~0.3 |

  Pongs get claimed because anyone can pong; chows mostly come from your own draws.
- **Chows usually spread across suits:** 2 suits 57%, 3 suits 37%, one suit 7%.
- **Runs:** 39% of hands with 2+ chows have 4+ in a row in one suit
  (like 345 + 456, or 456 + 789). A full 1–9 straight is rare (0.6%).

---

## Smaller facts

- **The dealer** gains about +0.19 units per hand from starting with 14 tiles.
- **Almost every hand has a winner** (99.8%) when everyone plays for speed.
  Keeping 9 stacks instead of 8 in the flower wall barely changes that (99.4%)
  and changes no strategy above.
- **Robbing a kong** (winning with the tile someone adds to their pong) is so
  rare that the rule barely matters.
