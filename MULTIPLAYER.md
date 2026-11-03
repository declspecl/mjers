# Multiplayer

Design record and implementation brief for online multiplayer. Written 2026-11-02 from a design discussion with the project owner. **Nothing in this file is implemented yet.** The decisions below are made; the open questions at the end have recommended defaults. An implementing agent should read this file, `CLAUDE.md`, `RULES.md`, `STYLE.md`, and `web/CLAUDE.md` first.

## 1. Goal and constraints

Let up to four people play one hand (then a series of hands) of the family ruleset together in a browser, with bots filling empty seats.

Constraints stated by the owner, in their words or close to them:

- **Cost:** originally "genuinely 0 cost". Then relaxed: the owner already has "plenty of AWS resources" and "even a $0.10 bill will go unnoticed". So a few cents a month is fine. Anything with a real recurring bill is not.
- **Performance:** the owner does not want weak bots or a slow engine just to save money. Running the Rust engine natively was a deciding factor.
- **Scale:** a family and friends game. Tens of tables at most, four players each. No public matchmaking is planned.
- **Infrastructure preference:** the owner uses AWS and CDK in other projects (see section 12). They asked directly whether Lambda plus API Gateway WebSockets plus DynamoDB could work, and were told yes.
- **Hidden information must stay hidden.** Each player may see only their own concealed tiles. This forces a server that runs the rules.

## 2. Decision

**Run the Rust engine natively in AWS Lambda behind an API Gateway WebSocket API, with DynamoDB for state and EventBridge Scheduler for timers.**

```
 browser ──wss──▶ API Gateway WebSocket API ──invokes──▶ Lambda (Rust, arm64, mjers-core native)
    ▲                    │  holds the connection                │ load table, replay, apply, step bots,
    │                    │  ($connect/$disconnect/$default)     │ write back with a version check
    └────── PostToConnection (management API) ◀─────────────────┤
                                                                 ├─▶ DynamoDB (tables, connections)
 browser ──https──▶ HTTP endpoint (Lambda Function URL or HTTP API): create table / join
 EventBridge Scheduler ──one-time schedule per turn──▶ Lambda (timeout event with the version it was set for)
```

API Gateway, not Lambda, holds the socket. Lambda runs once per message and pushes replies with `POST https://{api}.execute-api.{region}.amazonaws.com/{stage}/@connections/{connectionId}`. The Rust SDK crate is `aws-sdk-apigatewaymanagement`. Lambda Function URLs do not support WebSockets.

### Why this and not the alternatives

| Option | Verdict | Reason |
|---|---|---|
| **Lambda + API GW WebSocket + DynamoDB** | **Chosen** | Native Rust and full-strength bots, scales to zero, pennies per month, fits the owner's AWS and CDK setup. Timers and claim windows have clean solutions (section 6). |
| Always-on Rust server (axum) on Fly.io or Fargate | Rejected | It was the first recommendation when cost was flexible. Fly.io appears to need a card and can bill; Fargate is always-on cost. Simple mental model, but not free. |
| Cloudflare Workers + Durable Objects (or PartyKit) | Rejected | The best truly $0 option (see below) but the engine would run as WASM and heavy bots (search playouts, about 3 CPU-seconds per decision) do not fit. The owner said "the lack of performance is unfortunate". |
| Free container hosts (Render and similar) | Rejected | They sleep after about 15 minutes idle, with roughly a minute of cold start, and in-memory tables vanish. Card requirements unclear. |
| Peer-to-peer WebRTC, host browser authoritative | Rejected | No backend, but the host's browser holds every hand and the game ends when the host leaves. Still needs signaling and sometimes a TURN relay. |
| Oracle Cloud Always Free, Railway credit, Koyeb | Not investigated in depth | The search did not cover Koyeb or Oracle. Railway is a one-time $5 credit, not a free tier. |

What was learned about Cloudflare, in case the project ever needs a hard $0 ceiling (figures are mostly from third-party sites; verify on Cloudflare's own pricing page before relying on them):

- Workers Free allows only SQLite-backed Durable Objects.
- Free limits: 100,000 requests/day, 13,000 GB-seconds/day, 5 GB storage. Incoming WebSocket messages bill 20:1; outgoing are free. Over a limit, requests fail until 00:00 UTC. They do not bill.
- Hibernation removes duration charges while a table is idle.
- Docs conflict on Durable Object CPU limits (some say the 10 ms Workers Free limit applies, others say 30 s by default). Test before committing.
- Estimate: about 10 billed requests per four-player hand, so thousands of hands per day fit in the free tier.
- Cloudflare is fail-closed (stops at the limit). AWS is fail-open (bills on overage). Hence the budget alarm in section 11.

## 3. Facts about the engine that the server depends on

All paths are relative to the repo root. Verify against the code before relying on a detail.

- **`Game` is the only state machine** (`mjers-core/src/game/mod.rs`). Everything goes through `to_act()`, `legal_actions(seat)`, `apply(seat, action)`, and `observe(seat)`.
  - `Game::new(rules: Arc<Ruleset>, seed: u64, dealer: Seat) -> Result<Game, GameError>` shuffles from `seed` and deals.
  - `to_act()` returns a `SeatSet`. In a turn phase it holds one seat. In a **claim window it can hold several seats**: every seat with an option that has not decided yet. The server must collect each claimant's response. Each response is applied with `apply(seat, action)`. The engine resolves priority once all have answered.
  - `apply` returns `GameError::{HandOver, NotAsked(seat), Illegal{seat, action}}` and leaves state unchanged on error. A client sending an illegal action must produce an error message, not a corrupted table.
  - `game.steps()` returns the applied steps (`Step { seat, action }`). `game.seed()` returns `Option<u64>`.
- **Determinism.** The wall, the dealer, and every bot's random stream derive from a `u64`. Randomness is xoshiro256** in `mjers-core/src/game/rng.rs`, and its first output is pinned by a test because changing it changes every experiment. **Never change the RNG.**
- **Replay.** `Game::trace() -> Trace` and `Trace::replay() -> Result<Game, ReplayError>` exist in `mjers-core/src/game/trace.rs` (`Trace { version, rules, dealer, seed, wall, steps, events, result }`, `TRACE_VERSION = 1`). Persisting `(rules, dealer, seed, steps)` is enough to rebuild any hand by `Game::new` plus applying each step. A full hand replays in well under a millisecond (engine benchmark: a complete efficiency-bot hand is about 480 µs; see `docs/PERFORMANCE.md`).
- **`Observation<'a>`** (`mjers-core/src/game/observation.rs`) is a read-only borrowed view. It exposes only public information plus that seat's own concealed tiles. Other hands' tiles and the wall order are unreachable through it. Policies only ever receive an `Observation`.
- **Serialized observation shape.** The browser already consumes it. The reference serializer is `Snapshot` in `mjers-wasm/src/observation.rs`, and the TypeScript types are in `npm/types.d.ts` (`Observation`, `Action`, `Meld`, `Discard`, `Decision`, `HandResult`, `Settlement`, `Advice`). Fields: `seat, dealer, rules, hand{concealed,melds,bonus}, players[4]{seat,concealed_count,melds,bonus}, discards[], public_tiles, wall_remaining, live_wall, to_act[], legal_actions[], decision{kind: turn|claim|none}`. **Recommendation:** move `Snapshot` into `mjers-core` (it only needs serde, so the core stays WASM-safe and dependency-light) so the WASM package and the server share one serializer.
- **Actions** (JSON, serde externally tagged): `{discard:"5m"}`, `"self_draw_win"`, `{concealed_kong:"5m"}`, `{upgrade_kong:"5m"}`, `"pass"`, `{chow:"3p"}` (the chow **starting at** that tile), `"pong"`, `"kong"`, `"win"`. Tile notation: `1m`-`9m`, `1p`-`9p`, `1s`-`9s`, `1z`-`7z` (E S W N White Green Red). Bonus tiles `1F`-`4F` (plum, orchid, chrysanthemum, bamboo) and `1S`-`4S` (spring, summer, autumn, winter).
- **Claim rules** (`RULES.md`): chow only from the player before you (your left); pong and exposed kong from anyone; win from any discard. Priority win > pong/kong > chow. If several can win off one discard, only the closest in turn order after the discarder wins. Robbing the kong is off by default (`allow_rob_kong`). The dealer rotates counter-clockwise **every hand regardless of outcome**. Payments are settled at hand end in units (discarder pays 2, other non-winners 1; self-draw 3 from each; flowers 1 each; exposed kong 1, concealed kong 2, paid whether or not the owner wins).
- **Bots.** `BotSpec::preset(name)` accepts `"random"`, `"efficiency"`, `"defensive"` (`mjers-core/src/bot/spec.rs`). `spec.build(seed) -> Box<dyn Policy>`, and a policy decides with `policy.act(&observation, &legal_actions)`. Give each bot seat its own policy and seed so streams are independent. **Only these three presets exist in production.** Neural and search bots need `.safetensors` checkpoints, and all `target/` directories (including every checkpoint) were deleted. `BotSpec::Search` and `Advisor` are slow (about 3 CPU-seconds per decision for search). Do not use them in v1.
- **Advice.** `advise(&observation, &AdviceConfig { samples, candidates, seed, rollout })` in `mjers-core/src/advisor/mod.rs` needs only an observation. `samples: 0` is instant and heuristic. Playouts sample hidden tiles consistent with the observation. Run hints server-side so clients never hold the full game.
- **Position and resume.** `Game::position()` and `Game::resume()` (`mjers-core/src/game/position.rs`) snapshot and restart mid-hand. A resumed game has no deal, so its trace does not replay. Prefer seed plus steps for persistence.
- **WASM safety.** The entire `mjers-core` library must stay WASM-safe: no threads, OS randomness, or file I/O (CI builds it for `wasm32-unknown-unknown`). Server-only dependencies belong in new crates.

### Things that leak hidden information. Never send these to a client

- `Game::seed()`, `initial_wall()`, `wall()`: the seed or wall reveals every hand.
- `Game::position()`, `Game::hand(seat)` for other seats, `Game::events()` and `Game::trace()`: all contain hidden tiles.
- Another seat's `legal_actions` or `Observation`.
- After a hand ends it is fine to reveal hands (the web UI shows the winner's hand), but only in the explicit `hand_over` message.

## 4. Proposed crate layout

Current workspace (`Cargo.toml`): `mjers-core`, `mjers-sim`, `mjers-py`, `mjers-wasm`, plus the root `mjers-cli`. Add:

- **`mjers-table`**: pure table logic. No AWS, no async, no I/O. WASM-safe if possible.
- **`mjers-lambda`**: thin AWS shell: DynamoDB, API Gateway management API, EventBridge Scheduler, Lambda handlers.
- **`mjers-dev-server`**: local axum WebSocket server wrapping `mjers-table` with an in-memory store, for development and integration tests. Not deployed.
- **`infra/`** (CDK, TypeScript): stack definition. Follow the owner's other CDK projects (section 12).

Why the split: Lambda is stateless and awkward to run locally. If the core is a pure function from `(state, command)` to `(new state, outbox)`, the Lambda adapter is trivial, and full multiplayer hands can be tested in-process, like the existing 80-bot-hand test in `web/tests/event-log.test.ts`.

### Sketch of `mjers-table` (suggested, not binding)

```rust
pub struct Table {
    pub id: TableId,                 // short random code; also the invite code
    pub version: u64,                // bumped on every applied command; the compare-and-set key
    pub config: TableConfig,         // ruleset, bot preset, timer policy
    pub seats: [SeatSlot; 4],        // Open | Human { token_hash, name } | Bot { preset }
    pub hand: Option<HandRecord>,    // { seed, dealer, steps } for the live hand
    pub scores: [i32; 4],            // running totals across hands
    pub hands_played: u32,
}

pub enum Command {
    Join { name: String },                       // returns a seat token (shown once)
    Connect { seat: Seat },                      // presence
    Act { seat: Seat, action: Action, steps_seen: usize },
    Hint { seat: Seat, samples: u32 },
    NextHand,
    Timeout { seat: Seat, version: u64 },
    Disconnect { seat: Seat },
}

pub struct Outbox { pub to_seat: [Vec<ServerMessage>; 4], pub timer: Option<TimerRequest> }

pub fn handle(table: &mut Table, command: Command, rng: &mut impl SeedSource) -> Result<Outbox, TableError>
```

Rules for `handle`:

1. Rebuild the `Game` from `(rules, dealer, seed, steps)`. Reject a command whose `steps_seen` does not equal `steps.len()` with a `stale` error that carries the fresh state (this also makes client retries idempotent).
2. Validate that the seat is in `to_act()`. Call `apply`. On `GameError`, return the error and **change nothing**.
3. Append the step, then loop: while `to_act()` contains only bot seats, ask each bot (one policy per seat, seeded from the table seed and seat so the choice is reproducible on replay) and apply. Stop when a human must act or the hand ends.
4. Build each seat's `ServerMessage::State` from `game.observe(seat)` and structured events for what just happened. At hand end, build `HandOver` with `result()`, `settlement()`, and revealed hands.
5. Bump `version`. Return the outbox. The caller persists and sends.

Bot reproducibility: derive each bot's seed deterministically (for example `derive_seed(table_seed, seat)`, see `mjers-core/src/game/rng.rs`). Replay re-asks the bots in order, so the same seeds give the same moves. Alternatively store bot-chosen steps in the log (they are steps anyway) and never re-ask bots on replay. The second is simpler and safer: **the log is the truth; bots are only consulted for new decisions.**

The table seed comes from OS randomness **on the server only** (`getrandom` in the Lambda crate, passed into the pure core through a trait). The core must not call OS randomness itself.

## 5. Wire protocol (proposal)

JSON text frames. Use `{ "type": "...", ... }`. Reuse the existing observation, action, result, and advice shapes from `npm/types.d.ts` verbatim.

Authentication happens at connect: `wss://.../prod?table=ABCD&token=...`. The `$connect` Lambda looks up the seat by token hash, stores `connectionId -> {table, seat}`, and returns 403 to reject. There are no accounts in v1 (see open questions).

Client to server:

| Message | Meaning |
|---|---|
| `{type:"act", action, steps_seen}` | Play an action. `steps_seen` is how many steps the client has applied; it guards against stale or duplicate sends. |
| `{type:"hint", samples}` | Ask for advice for the sender's seat. `samples: 0` is instant. |
| `{type:"next_hand"}` | Ready for the next hand (host or all humans, see open questions). |
| `{type:"ping"}` | Keepalive, every 5 minutes or less. |

Server to client:

| Message | Meaning |
|---|---|
| `{type:"state", table, seats, scores, hand_number, steps, observation}` | Full per-seat state after every change. `seats` lists name, kind (human or bot), and connected. Sending the whole observation each time is simplest (a few KB). |
| `{type:"events", events:[{seat, event}]}` | Structured events (`discard`, `claim`, `kong`, `win`, `draw`) so the client can write its log. Keep text formatting in the client (`web/src/engine/names.ts`). |
| `{type:"hand_over", result, settlement, revealed:[Hand\|null x4]}` | The only message that reveals other hands. |
| `{type:"hint", advice}` | Reply to `hint`. |
| `{type:"error", code, message}` | Codes: `illegal_action`, `stale`, `not_your_turn`, `table_full`, `bad_token`, `internal`. After `stale`, send fresh `state`. |
| `{type:"pong"}` | Reply to `ping`. |

A new connection for a seat should replace the old one (close the old `connectionId`) and immediately receive the current `state`. That is the reconnect path.

HTTP (Lambda Function URL or API Gateway HTTP API), no WebSocket needed:

- `POST /tables` -> `{ table: "ABCD", seat_token, seat: 0 }`. Creates a table, takes seat 0, fills the other seats with bots by default.
- `POST /tables/{id}/join` `{ name }` -> `{ seat_token, seat }`. Replaces a bot seat before the hand starts, or takes an open seat.

## 6. State, concurrency, and timers on AWS

### DynamoDB

**`tables`**: primary key `table_id` (S).

| Attribute | Notes |
|---|---|
| `version` (N) | Compare-and-set key |
| `config` | Ruleset name or JSON, bot preset, timer policy |
| `seats` | List of `{kind, name, token_hash}`. Store only a SHA-256 hash of each seat token. |
| `seed` (N), `dealer`, `steps` (list of `{seat, action}`) | The live hand. A hand is a few hundred small steps, far under the 400 KB item limit. |
| `scores`, `hands_played`, `history` | Running match state. Finished hands' seeds are safe to keep for replays. |
| `ttl` | Epoch seconds. Expire idle tables. |

**`connections`**: primary key `connection_id` (S), attributes `table_id`, `seat`, `ttl`. Global secondary index `by_table` on `table_id` so the server can find a table's live connections to broadcast to.

Alternative worth knowing: store each step as its own item (`table_id` + `step_no`, written with `attribute_not_exists(step_no)`). That gives compare-and-set for free and unbounded history. A single item with a `version` condition is simpler and enough for v1.

### Per-message flow in the Lambda

1. Read the table item with **`ConsistentRead: true`**.
2. Call the pure `handle`.
3. Write back with `ConditionExpression: version = :expected` (or `attribute_exists(table_id) AND version = :expected`). On `ConditionalCheckFailedException`, re-read and re-run (a few retries). This is what makes two humans answering a claim window at the same moment safe, because API Gateway can run two Lambdas concurrently for one table.
4. For each outbox message, find the target connections (query `by_table`, filter by seat) and `PostToConnection`. A `GoneException` means the client left: delete that connection row.
5. If the outbox has a timer request, create the schedule (below).

Messages from one client can be processed concurrently. Do not assume ordering. The `steps_seen` check and the version condition are the ordering mechanism.

### Timers (add after the basics work)

Use **EventBridge Scheduler one-time schedules**: `CreateSchedule` with schedule expression `at(YYYY-MM-DDTHH:MM:SS)`, target the same Lambda (or a separate one) with input `{ table_id, version, seat }`, `ActionAfterCompletion: DELETE`. When it fires, load the table. **If its `version` differs from the input, the timer is stale and the handler does nothing.** There is no need to cancel old schedules. If the version matches, auto-act for that seat: pass a claim, discard the drawn tile on a turn, or hand the seat to a bot. A DynamoDB TTL stream is too imprecise for turn timers. Step Functions is heavier than needed.

**v1 recommendation: no timers.** The table waits for humans, and a "let a bot play my seat" action covers walkaways. Add timers once the rest is solid.

### Bots and slow work

Heuristic bots take microseconds and run inside the same invocation as the human's action. The API Gateway WebSocket integration timeout is 29 seconds, so anything heavier (search bots, Monte Carlo hints with large samples) should self-invoke asynchronously (`InvocationType: Event`) and push the result when done. Not needed in v1.

### API Gateway limits to design around (from memory; confirm in the AWS docs)

- Idle connection timeout 10 minutes; maximum connection duration 2 hours. The client must send `ping` at least every 5 minutes and **reconnect automatically**, resuming with its seat token.
- Lambda integration timeout 29 seconds.
- Payload up to 128 KB, with frames of 32 KB. Observations are a few KB.
- The Lambda role needs `execute-api:ManageConnections` on the API.

## 7. Cost

All from memory; check current pricing. Rough shape: API Gateway WebSocket about $1.00 per million messages plus $0.25 per million connection-minutes. Lambda's free tier (1M requests, 400,000 GB-seconds per month) and DynamoDB's (25 GB storage) are permanent. EventBridge Scheduler has a large monthly free allowance. A four-player hour is a fraction of a cent. **Add an AWS Budgets alert at about $1 as the guardrail**, since AWS bills on overage instead of stopping.

## 8. Web client changes

Current state of the web app (`web/`, a single Vite and React app, see `web/CLAUDE.md`):

- `web/src/engine/use-hand.ts` runs a local `Game` through the WASM package `@mjers/mjers` (linked from `../npm`), with three `efficiency` bots, the human at seat 0, and a bot delay of 650 ms per turn and 80 ms for claim responses. It exposes `{ view, advice, error, act, hint }` where `view` is `{ observation, humanToAct, over, result, settlement, winnerHand, events }`.
- Components consume `HandView`: `table.tsx`, `opponent.tsx`, `pond.tsx`, `your-side.tsx`, `side-panel.tsx`, `result.tsx`. Keep `HandView` as the seam: write `useTable(connection)` that returns the same shape, and the components barely change.
- `web/src/engine/event-log.ts` builds log lines client-side by diffing meld counts after each move. It is tested over 80 bot hands. With a server, either keep it (the client sees every player's melds in each observation) or switch to the structured `events` message. If the server sends events, port `describeMeld` and `describeAction` from `names.ts` and keep the "log a claim only when it takes effect" rule. A claim is a bid and may lose to a higher-priority claim; logging bids as results was a real bug found while playtesting.
- The seed is currently in the URL (`?seed=`). **Remove it for multiplayer.** The seed is secret.
- The browser WASM `Game` is not used in multiplayer. Keep `@mjers/mjers` for types and `analyze` only. Hints (`advise`) move server-side.
- Needed UI: create table, share invite link, join with a name, fill seats with bots, connection status, reconnect, "waiting for X", hand-over screen with revealed hands, next hand, running scores, dealer and wind labels that rotate each hand. Seat names currently assume you are seat 0 (`windName`, `seatName` in `web/src/engine/names.ts`); with real seats, rotate the view so "you" is always at the bottom.
- Hosting the built app: S3 plus CloudFront, or whatever the owner already uses.

Design and tooling rules for the web app (enforced by tests and lint, see `web/CLAUDE.md`):

- No all-caps text, gradients, glass, glow, hover lift, emoji, template punctuation, or hype copy. `pnpm test` runs `web/tests/design-rules.test.ts`, which scans `web/src` and `index.html`. Use the `frontend-design` skill (`.agents/skills/frontend-design`) before reshaping a screen.
- Commands: `pnpm install`, `pnpm dev` (rebuilds the WASM first, serves :5173), `pnpm test`, `pnpm run lint:check`, `pnpm run check-types`, `pnpm run prettier:check`, `pnpm build`.
- Tile artwork is real PNGs in `web/src/assets/tiles/` from Anisimova's Mahjong game pack. Terms and provenance are in `NOTICE.md` there. The author asks only that the files not be resold. Public redistribution is not addressed, so get the author's OK if the repo ever goes public. Regenerate with `web/scripts/make-tiles.py` (needs Ghostscript and poppler). Do not redraw tiles.
- The previous sessions tested the UI like a human: dev server in the background, the T3 browser preview, scripted discards through `preview_evaluate`, and screenshots at 1280 px, 390 px (phone), and dark mode. Keep doing that for the multiplayer UI.

## 9. Testing plan

- **Pure core (`mjers-table`) unit tests.** Join, seat tokens, illegal actions leave state unchanged, stale `steps_seen` is rejected, dealer rotation across hands, scores sum to zero per hand.
- **Replay equality.** For many seeds, play a table with bots, then rebuild from `(rules, dealer, seed, steps)` and assert the same final observation, result, and settlement for every seat.
- **Bot-only tables.** Play 200 or more hands through `handle` with all four seats as bots and assert every hand finishes. The workspace denies `unwrap`, `expect`, `panic`, and friends in non-test code (`Cargo.toml` `[workspace.lints.clippy]`), and `unsafe_code` is forbidden. Tests may opt back in.
- **Claim races.** Use a fake store that implements compare-and-set and interleave two claimants' commands. Assert exactly one resolution and no corrupted state.
- **Leak test.** Serialize every outgoing message for seat A across many hands. Assert none contains a `seed` or `wall` key, none reveals another seat's concealed tiles before `hand_over`, and `hand_over` is the only message with other hands.
- **Protocol golden tests.** Fixed JSON for each message type, serialized and parsed.
- **Local integration.** `mjers-dev-server` plus the web client in four browser tabs, played to the end of a hand, including a claim race.
- **AWS smoke test** after deploy: create a table over HTTP, connect two sockets, play a few moves, kill one socket and reconnect.

## 10. Milestones

1. **Pure core and local harness.** `mjers-table`, the shared `Snapshot` in core, `mjers-dev-server` (in-memory store), protocol types. Acceptance: four tabs play a full hand against the dev server, bots fill empty seats, all section 9 core tests pass.
2. **Web client against the local server.** `useTable`, create and join UI, reconnect, hand-over with revealed hands. Acceptance: the existing design-rules and event-log tests still pass; the UX is checked in the T3 browser at desktop and phone width.
3. **AWS.** `mjers-lambda`, DynamoDB tables, API Gateway WebSocket and HTTP, CDK stack, budget alarm. Acceptance: the AWS smoke test passes; a deployed hand completes.
4. **Series play.** Running scores, dealer rotation (counter-clockwise every hand), next-hand flow, optional table history.
5. **Timers and takeover.** EventBridge Scheduler timeouts, bot takeover for absent players, explicit "bot plays my seat".
6. **Later, optional.** Spectators; stronger bots (needs retraining checkpoints, see `NN.md`, and likely asynchronous Lambda invocations); accounts.

## 11. Pitfalls and warnings

- **Seed secrecy** is the top risk. See the leak list in section 3 and the leak test in section 9.
- **Do not change the RNG** (`mjers-core/src/game/rng.rs`); a test pins its first output.
- The core must stay WASM-safe. Put AWS, async, and OS-randomness code in the new crates.
- `Game::resume` games cannot be replayed from a trace. Persist seed plus steps instead.
- The server never trusts client claims about whose turn it is. Always check `to_act()`.
- DynamoDB reads must be consistent, writes conditional on `version`.
- Close the old connection when a seat reconnects, or two browsers can act as one seat.
- Seat tokens are shown once, stored only as hashes, and compared in constant time.
- `PostToConnection` fails with `GoneException` for dead connections. Handle it and clean up.
- A budget alarm is not optional. AWS bills on overage.
- Neural and search bots cannot be used in production now. No checkpoints exist.

## 12. Environment and reference

- Repository conventions are in `CLAUDE.md`: commit messages are short, plain, and lowercase with no trailers; `cargo fmt --all` uses nightly rustfmt with the repo's `rustfmt.toml` (vertical params and imports, width 200); CI runs `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`, and a `wasm32-unknown-unknown` build of `mjers-core`. `STYLE.md` describes code style preferences. Read it before writing Rust.
- `CLAUDE.md` says project notes stay uncommitted, but the owner committed them in `999a4b3` ("docs?"). Do not assume either way; ask whether to commit this file.
- The commit hook `block-no-verify` once flagged `git commit -q` as a false positive. Plain `git commit -m "..."` worked.
- The owner's other repos, useful as CDK and tooling templates (same machine): `~/programming/projects/classiq/classiq` (`apps/infra` CDK, Vite website; the closest layout match), `~/programming/projects/remindyrs`, `~/programming/projects/ourlil` (`apps/cdk`), and `~/programming/web/lavdiakaemen`. They use pnpm 12, mise, Node 24, catalog-pinned dependencies, and Prettier with the sort-imports plugin.
- Local tools present: Node 25 via mise, pnpm 12, Rust nightly, `gh`, and (installed this session) Ghostscript 10.08 via Homebrew.
- Not verified, state them as assumptions: current AWS pricing and limits (section 6 and 7), Cloudflare free-tier details, and whether the AWS account has any quirks (region, existing CDK bootstrap).

## 13. Open questions and recommended defaults

| Question | Recommended default |
|---|---|
| Accounts or anonymous? | Anonymous: invite code plus per-seat token. Add accounts later (`better-auth` is used in the owner's other projects). |
| Turn timers? | None in v1; add EventBridge timers and bot takeover in milestone 5. |
| Who may start the next hand? | The table creator, or all humans must ready up. Pick one; creator-only is simplest. |
| Ruleset selection? | Fixed `family` preset in v1; the `Ruleset` is already data, so a table option is cheap later. |
| Bot strength? | Presets only: `efficiency` default, `defensive` optional per seat. |
| Spectators? | No in v1. |
| Region and CDK bootstrap? | Use the owner's existing AWS account and region; ask before deploying. |
| Persist finished hands? | Yes, keep `history` with seed, steps, and settlement. It is cheap and enables replays. |

Ask the owner before deploying anything to AWS, creating schedules, or committing this file.

## 14. Sources

Research done on 2026-11-02 (third-party figures; verify on official pages):

- Cloudflare Durable Objects limits: https://developers.cloudflare.com/durable-objects/platform/limits
- Cloudflare Durable Objects pricing: https://developers.cloudflare.com/durable-objects/platform/pricing
- Durable Objects free tier summary: https://agentdeals.dev/vendor/cloudflare-durable-objects
- Durable Objects pricing calculator: https://flarecalc.com/calculators/durable-objects/
- Free backend hosting comparison: https://snapdeploy.dev/blog/free-backend-hosting-2026-apis-servers
- Free tier platforms overview: https://render.com/articles/platforms-with-a-real-free-tier-for-developers-in-2026
- Tile artwork source: https://anisimova.itch.io/anisimovas-mahjong-game-pack
