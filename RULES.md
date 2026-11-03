# Family Ruleset

The house rules this engine is built around. Items marked **OPEN** are not yet
pinned down. Each is a `Ruleset` parameter, so the simulator can compare the
alternatives.

## Tiles and dealing

- 144 tiles: 3 suits × 9 ranks × 4, 4 winds × 4, 3 dragons × 4, 4 flowers, 4 seasons.
- 13-tile hands; the dealer starts with 14.
- Flowers/seasons are set aside when drawn and replaced from the flower wall.
- The flower wall is 8 stacks (16 tiles), always kept at that size: replacements
  (flowers and kongs) come from it and it is topped up from the live wall.
  Equivalently, normal draws stop when 16 tiles remain.
  - Parameter `flower_wall_stacks` (8; 9 is a variant worth comparing).
- If the live wall runs out, the hand ends with no winner.

## Claiming discards

- **Chow (sheung):** only from the player on your left (the player before you).
- **Pong / exposed kong:** from any player.
- **Win:** from any player's discard, completing any part of the hand,
  including the eye.
- Priority: win > pong/kong > chow. If several players can win off one discard,
  only the closest in turn order after the discarder wins.
- Upgrading your exposed pong to a kong with a tile you drew counts as an
  exposed kong.
- **OPEN:** robbing the kong (another player wins with the tile you add to your
  pong). Assumed not allowed (parameter: `allow_rob_kong`).

## Winning

- Exactly 4 sets (chow, pong, or kong) plus 1 eye. No minimum points or other
  conditions. Special hands (seven pairs, thirteen orphans) are not wins.

## Payment

All payments are settled at the end of the hand, in units.

| Event | Who pays | Amount |
|---|---|---|
| Win off a discard | discarder | 2 |
| Win off a discard | each other non-winner | 1 |
| Self-drawn win | each non-winner | 3 |
| Winner's flowers/seasons | each non-winner | 1 per flower |
| Exposed kong | each other player → kong owner | 1 |
| Concealed (am) kong | each other player → kong owner | 2 |

- Kongs are paid whether or not their owner wins, including when the wall runs
  out with no winner.

## Dealer

- The deal rotates counter-clockwise every hand, regardless of outcome.
