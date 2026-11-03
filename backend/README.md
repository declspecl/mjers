# Multiplayer backend

The backend runs the native engine in both a local WebSocket server and AWS
Lambda. The room UI and browser connection are a separate next step.

| Directory | Responsibility |
| --- | --- |
| `table/` | Pure room state, engine replay, bots, scoring and wire types |
| `dynamodb/` | Consistent reads, conditional writes, connection ownership, request counters and replay archives |
| `service/` | Shared create/join/command orchestration and credential generation |
| `dev-server/` | Local HTTP and WebSocket transport using DynamoDB Local |
| `lambda/` | HTTP API v2 and WebSocket proxy Lambda handlers |
| `infra/` | TypeScript CDK app and infrastructure assertions |
| `scripts/` | Build, packaged-runtime verification and deployed smoke test |

The Rust crates belong to the root Cargo workspace. `mjers-core` stays WASM-safe
and shares its observation serializer with WASM and the backend.

## Room behavior

Every room has four seats. The host chooses one to four humans, defaulting to two
humans and two efficiency bots. `random` and `defensive` bots are also supported.
Each human chooses a username; reserved human seats stay open until friends join.
Only the host starts the first and subsequent hands. Dealer rotation and scores
continue across hands.

Rooms have a six-character invite code and a join path. Password protection is
optional. The password is verified when joining; an existing seat reconnects
with its private token. Joining is allowed only before the first hand starts.

An authenticated human can enable or disable `autoplay` for their own seat. The
seat retains its name, token and human identity; an efficiency bot makes its
new decisions while autoplay is enabled. Disconnects alone do not enable bots.
There are no turn timers or EventBridge schedules in v1.

## Run locally

From the repository root:

```sh
docker compose -f backend/compose.yaml up -d
cargo run --locked -p mjers-dev-server
```

DynamoDB Local uses a named volume and binds only to loopback. Room state survives
both server and container restarts. The server provisions its local room table
on startup and fails if DynamoDB is unavailable; there is no in-memory fallback.
Only active socket handles and presence live in the local process.

| Variable | Default |
| --- | --- |
| `MJERS_LISTEN` | `127.0.0.1:3001` |
| `MJERS_DYNAMODB_ENDPOINT` | `http://127.0.0.1:8000` |
| `MJERS_DYNAMODB_TABLE` | `mjers-local-tables` |

The local SDK client uses dummy credentials with an explicit region and endpoint.
It does not read your AWS credential profile. Lambda injects a production SDK
client into the same persistence adapter and uses its execution role.

Stop DynamoDB with `docker compose -f backend/compose.yaml down`. The volume is
retained. Adding `--volumes` intentionally erases local data.

## HTTP protocol

`POST /tables`:

```json
{
  "name": "Alex",
  "human_players": 2,
  "password": "optional room password",
  "bot": "efficiency"
}
```

`human_players` defaults to 2, and `bot` defaults to `efficiency`. Omit `password`
for an unprotected room. Usernames are trimmed, nonempty and at most 40
characters. Passwords accept at most 128 bytes.

`POST /tables/{code}/join` accepts `name` and an optional `password`. Both endpoints
return `table`, `seat_token`, `seat`, and `join_path` (`/?room=CODE`). The future
web client combines that path with its origin for copy-link. Passwords and seat
tokens must never be included in share links. Responses use `Cache-Control:
no-store`. The current web app does not consume these links yet.

Passwords use salted Argon2id hashes. Seat tokens contain 32 random bytes and
are returned once, persisted only as SHA-256 hashes, and compared in constant
time. OS randomness and password work live outside the engine; password hashing
and verification run outside the async runtime.

## WebSocket protocol

Local: `/ws?table=CODE&token=SEAT_TOKEN`. AWS: use the CDK WebSocket endpoint with
the same query parameters. Authentication fixes the seat; client bodies cannot
supply another seat. Allowed browser origins must match `SITE_ORIGINS` on AWS.

Send `sync` after **every** socket open, including reconnects. API Gateway's
`$connect` handler saves authentication/ownership but does not send to a socket
before the handshake completes. `sync` marks the connection ready and sends full
state. The local server also sends initial state automatically.

```json
{"type":"sync"}
{"type":"start_hand"}
{"type":"act","action":{"discard":"5m"},"hand_number":1,"steps_seen":0}
{"type":"autoplay","enabled":true,"hand_number":1,"steps_seen":0}
{"type":"hint","samples":0}
{"type":"next_hand","hand_number":1}
{"type":"ping"}
```

Send `ping` at least every five minutes and reconnect automatically when the
socket closes. Keep the seat token on the client so it can restore the seat.

`table/src/protocol.rs` defines the wire types. `state` includes room settings,
names, presence, scores, hand number, step count, table version and the receiving
seat's observation (null in the lobby). Actions guard both hand number and step
count. A stale action returns an error followed by fresh state; the player can
reconsider using that state.

`events` describes resolved public events. Deals are omitted, and draw events
omit the drawn tile. `hand_over` alone includes the four revealed hands and
settlement, including `total`; reconnecting after a finished hand also sends it.
Hints use the existing advice shape, and only instant, zero-sample hints are
supported. Hints and pings do not mutate game state.

State, events, hand-over and hint messages carry `version` and `hand_number`.
Concurrent Lambdas can deliver messages out of order. The client must discard
messages older than its current table version; multiple messages at the same
version belong to the same update. Presence can change without a game version
change, so equal-version state messages must still be accepted. Use `sync` to
recover if a callback is missed. Delivery is best effort; persisted state is the
truth, and callback failures do not roll back committed actions or joins.

## Persistence and connection ownership

The room item uses a string `table_id` key, native numeric `version`, and a JSON
`document` string. Seeds remain exact `u64` values. Persisted documents contain
hidden data and credentials and must never be returned to clients.

Every operation reads with `ConsistentRead`. Mutations commit against an expected
version; conflicts cause a fresh read and command re-evaluation, with at most
four attempts. Commands and bots replay only recorded actions. Bots are asked
only for new decisions, including bot claims in mixed human/bot claim windows.

Rooms expire 24 hours after their last mutation. Reads and writes enforce expiry
immediately, because DynamoDB TTL deletion is asynchronous. Old local items
without a TTL acquire one on their next write.

A room retains at most four finished hand records inline to bound item size.
A separate `hand#ROOM#NUMBER` item archives each completed hand for seven days.
The archive, final state and score commit in one DynamoDB transaction. These
server-only records can replay the hand; no public replay endpoint is exposed.

AWS connections use a second table with a `connection_id` string key and TTL.
Each socket has a record and each seat has an authoritative ownership record.
Replacement atomically revokes the old socket record before attempting to close
it. A late disconnect cannot delete the replacement. Game mutations include an
ownership condition in their DynamoDB transaction, so a replaced invocation
cannot commit. Strongly consistent reads of four seat records avoid a GSI's
propagation delay. `GoneException` removes dead connection records.

The connection table also stores expiring atomic request counters. The hosted
adapter allows 5 creates per IP/minute, 10 joins per room/IP/minute, 20 connection
attempts per IP/minute, and 120 messages per connection/minute. IP scopes are
hashed before storage. API Gateway also applies aggregate throttles; the Lambda
has bounded reserved concurrency. These limits are not hard spending caps.

The local transport is one process: it serializes its broadcasts and keeps socket
handles locally. It shares room orchestration and persistence with Lambda.
Distributed socket ownership is exercised by Lambda integration tests using
DynamoDB Local and a local management callback endpoint.

## Verification

```sh
docker compose -f backend/compose.yaml up -d
cargo test --locked --workspace
cargo test --locked -p mjers-dynamodb --test local -- --ignored
cargo test --locked -p mjers-lambda --test gateway -- --ignored
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked -p mjers-core --target wasm32-unknown-unknown
node --test backend/dev-server/tests/rooms.test.mjs
pnpm --dir backend/infra install --frozen-lockfile
pnpm --dir backend/infra check-types
pnpm --dir backend/infra test
pnpm --dir backend/infra format:check
```

The two Rust integration suites are explicitly ignored by ordinary `cargo test`
because they need DynamoDB Local; the commands above run them and fail if it is
unavailable. They provision and delete isolated tables. Storage tests cover
conditional writes, simultaneous joins/claims, expiry and malformed records.
Lambda tests exercise real signed SDK requests, passwords, per-seat privacy,
replacement, late disconnects, transactional ownership, rate limits, dead
sockets, autoplay, full hands, score archives and next-hand rotation.

The Node 24+ transport tests restart the server mid-hand and compete for a seat
from independent server processes. Their default `mjers-test-sockets` table
retains test rooms until local data is reset; override it with
`MJERS_DYNAMODB_TABLE`. Pure table tests stay independent of storage.

CI runs these checks with DynamoDB Local and synthesizes CDK offline. It also
builds and executes the packaged Linux arm64 bootstrap in an AL2023 container.

## AWS packaging and deployment

The CDK structure follows `ourlil`, `remindyrs` and `unifit`: thin app entrypoint,
explicit configuration, one stack, and a focused storage construct.

Build the deployable archive (requires Rust, rustup and uv):

```sh
backend/scripts/build-lambda.sh
node backend/scripts/check-lambda.mjs
```

The build pins Cargo Lambda through uv and cross-compiles for Linux arm64. Its
Zig dependency is installed in uv's tool environment. The output is
`target/lambda/mjers-lambda/bootstrap.zip`. The packaging check requires Docker:
it verifies the ELF architecture/executable bit and runs the extracted bootstrap
in Lambda's AL2023 base image against a local Runtime API. It makes no AWS calls.

From `backend/infra`, set deployment configuration explicitly:

```sh
export DEPLOYMENT_STAGE=dev
export AWS_REGION=us-east-1
export SITE_ORIGINS=https://mjers.pages.dev,https://mjers.gavindhondt.com
pnpm synth
```

`.env.example` lists the variables; the app does not load `.env` files implicitly.
`LAMBDA_ARCHIVE` can override the built ZIP path. Offline synthesis requires no
AWS credentials. Real deployment uses the selected AWS profile and CDK account.
**Ask the owner to select/approve the account, region and allowed origins
before bootstrapping or deploying.** No AWS resources have been deployed.

The stack defines:

- A `provided.al2023` arm64 Lambda, 512 MB, 25-second timeout, concurrency 8.
- HTTP create/join routes with origin-specific CORS and throttling.
- WebSocket connect/default/disconnect routes, management permissions and throttling.
- On-demand room and connection DynamoDB tables with TTL. Production tables are
  retained when the stack is removed; development tables are destroyed.
- One-week Lambda log retention. Request bodies, tokens and query strings are
  never logged by application code; WebSocket data tracing is disabled.

No AWS budgets or budget notifications are provisioned.

After owner approval, use `pnpm diff`, `pnpm exec cdk bootstrap` only if needed,
and `pnpm deploy`. CDK outputs the HTTP and WebSocket endpoints and table names.

After deploying, run the real AWS smoke test:

```sh
node backend/scripts/smoke-aws.mjs HTTPS_ENDPOINT WSS_ENDPOINT
```

It creates a password-protected two-human room, joins over HTTP, opens two
sockets, completes a hand using explicit autoplay, reconnects and starts the next
hand. It leaves the room to expire naturally. IAM, real API Gateway routing and
cloud delivery remain unverified until that test runs on an approved deployment.

## Deferred work

Room UI and client connection come next. Timers, automatic takeover for absent
players, spectators, accounts, public replay UI, and stronger neural/search bots
remain outside v1.
