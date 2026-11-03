# Multiplayer backend

The local implementation has pure room logic in `table/`, a reusable AWS SDK
DynamoDB adapter in `dynamodb/`, and an HTTP/WebSocket runtime in `dev-server/`.
All three crates belong to the root Cargo workspace. The engine remains in
`mjers-core`; its owned observation serializer is shared with WASM.

Rooms always have four seats. The host chooses one to four humans (default: two),
with bots filling the rest. Human seats remain open until friends join. Every
human chooses a username; only the host can start the first or subsequent hand.
The optional room password is verified on joining. A seat token restores an
existing seat without asking for the password again.

## Run locally

From the repository root:

```sh
docker compose -f backend/compose.yaml up -d
cargo run --locked -p mjers-dev-server
```

The default address is `127.0.0.1:3001`; override it with `MJERS_LISTEN`.
Room state is stored in DynamoDB Local and survives server and container
restarts. The Compose service binds only to loopback and uses a named volume.
The server creates its local table on startup; it fails if DynamoDB is
unavailable, with no in-memory fallback. Active socket handles and presence
remain local to the running server. This is a development harness; room expiry
and public-service request limits are still pending.

Configuration:

| Variable | Default |
| --- | --- |
| `MJERS_LISTEN` | `127.0.0.1:3001` |
| `MJERS_DYNAMODB_ENDPOINT` | `http://127.0.0.1:8000` |
| `MJERS_DYNAMODB_TABLE` | `mjers-local-tables` |

The local SDK client supplies dummy credentials and an explicit region and
endpoint. It does not read your AWS credential profile. The future hosted
runtime will inject its own SDK client into the same `TableStore`.

Stop DynamoDB with `docker compose -f backend/compose.yaml down`. The named
volume is retained. Add `--volumes` only when intentionally erasing local data.

## Create and join

`POST /tables` accepts:

```json
{
  "name": "Alex",
  "human_players": 2,
  "password": "optional room password",
  "bot": "efficiency"
}
```

`human_players` defaults to 2; `bot` defaults to `efficiency` and also accepts
`random` or `defensive`. Omit `password` for an unprotected room.

The response contains `table` (a six-character invite code), `seat_token`, `seat`,
and `join_path` (`/?room=CODE`). The web client will combine that path with its
own origin for the copy-link control. Passwords and seat tokens must not be
included in share links. The current web app does not consume these links yet.

`POST /tables/{code}/join` accepts `name` and an optional `password`, returning
the same response shape. Joining is allowed only before the first hand starts,
and only for reserved human seats. Names are trimmed, nonempty and at most 40
characters. Passwords accept at most 128 bytes.

Passwords are salted Argon2id hashes. Seat tokens contain 32 random bytes, are
returned once, and are persisted only as SHA-256 hashes with constant-time
comparison. Random bytes come from the adapter, never the engine or table crate.

## WebSocket protocol

Connect to `/ws?table=CODE&token=SEAT_TOKEN`. Authentication fixes the seat for
the connection; client messages cannot supply a different seat. Reconnecting
replaces and closes the previous socket immediately. A bounded outgoing queue
closes slow consumers rather than accumulating messages indefinitely.

The full types are in `table/src/protocol.rs`. Client messages:

```json
{"type":"start_hand"}
{"type":"act","action":{"discard":"5m"},"hand_number":1,"steps_seen":0}
{"type":"hint","samples":0}
{"type":"next_hand","hand_number":1}
{"type":"ping"}
```

`state` includes room settings, names, presence, running scores, hand number,
step count, table version and the receiving seat's `observation`. The observation
is null in the lobby. A stale action produces an error followed by fresh state;
the client should let the player reconsider using that state. Hand numbers guard
against late messages from earlier hands whose step counts happen to match.

`events` contains only resolved public events. Draw events omit the tile, and
deals are excluded. `hand_over` alone includes the four revealed hands and
settlement, including `total`. Reconnecting after a hand ends also sends
`hand_over`. `hint` uses the existing advice shape; only instant (zero sample)
hints are enabled. Hints and pings do not mutate table state.

The persisted table is a separate server-only model. It contains credentials,
seeds and action logs and must never be returned to a client. Each action
rebuilds the hand from its recorded seed and steps. Bots are consulted only for
new decisions, including bot claim responses while a human claim is pending.
Failed commands do not change the table. The local adapter serializes mutations
under each room's lock for broadcast ordering. Every operation reads from
DynamoDB with `ConsistentRead`, and mutations commit using a conditional
`version` check before broadcasting. A conflict causes a fresh read and command
re-evaluation, with up to four attempts. No game state is cached in memory.

The DynamoDB item uses `table_id` as its string partition key, `version` as a
native number for conditional writes, and `document` as a JSON string containing
the server-only table. This preserves full-width `u64` seeds without JavaScript
number rounding. Reads check identity/version agreement. Conditional creates
protect invite-code collisions; updates cannot recreate a missing record.

The local server is a single WebSocket runtime: socket replacement and broadcasts
operate within that process. DynamoDB tests also exercise writes from independent
clients/processes. Distributed connection routing belongs to the Lambda/API
Gateway adapter.

## Verify

```sh
docker compose -f backend/compose.yaml up -d
cargo test --locked --workspace
cargo test --locked -p mjers-dynamodb --test local -- --ignored
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked -p mjers-core --target wasm32-unknown-unknown
node --test backend/dev-server/tests/rooms.test.mjs
```

The storage integration test is explicitly marked ignored for ordinary
`cargo test` runs because it requires DynamoDB Local. The command above runs it
against the real service and fails if it is unavailable. It creates and deletes
an isolated DynamoDB table, testing exact persistence, conditional creates,
competing joins and claims, stale versions, malformed records and missing tables.

The Node 24+ socket tests require the same running DynamoDB Local service. They
start and stop local servers on ephemeral ports, restart mid-hand and verify
that observations, scores, credentials and steps survive. They also finish a
two-human/two-bot hand and race joins from two server processes. They use the
separate `mjers-test-sockets` DynamoDB table by default (override with
`MJERS_DYNAMODB_TABLE`); test rooms remain there until local data is reset.

CI starts DynamoDB Local and runs both integration suites. Pure table tests stay
independent of storage and cover 200 bot hands, replay equality, claim conflicts,
per-seat privacy, scoring and dealer rotation.

## Next slices

- Connect the web app: room creation and joining, copy code/link, lobby, remote
  hand state, reconnect, hints and series play.
- Add `lambda/` as the AWS adapter and `infra/` as the TypeScript CDK project.
  The CDK reference projects are `ourlil`, `remindyrs` and `unifit`: thin app
  entrypoints, explicit deployment configuration, stacks composed from focused
  constructs, stage-scoped resources and least-privilege grants. Start with one
  multiplayer stack rather than their larger deployment/pipeline structures.
- Reuse the DynamoDB adapter with production SDK configuration, API Gateway
  WebSockets, native arm64 Lambda and an AWS budget alert. Deploy only after
  the owner selects the account/region and approves deployment.
- Add room expiry, bounded persisted history, production request limits and
  password-attempt throttling in the hosted adapter. No timers or schedules in v1.
