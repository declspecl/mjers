# Remaining multiplayer UI work

Updated 2026-11-03. The v1 backend is implemented, deployed and smoke-tested.
The browser still plays locally. This document is the handoff for connecting it.

## References and endpoints

- Backend behavior and setup: [`backend/README.md`](backend/README.md).
- Deployment configuration: [`backend/deployment.prod.json`](backend/deployment.prod.json).
- Authoritative wire types: [`backend/table/src/protocol.rs`](backend/table/src/protocol.rs).
- Read `web/CLAUDE.md` and use `.agents/skills/frontend-design/SKILL.md` before reshaping screens.

Production HTTP: `https://ovx79phvqa.execute-api.us-east-1.amazonaws.com`

Production WebSocket: `wss://21g3gv0b56.execute-api.us-east-1.amazonaws.com/prod`

Allowed origins: `https://mjers.pages.dev` and `https://mjers.gavindhondt.com`.
Local development should use the dev server and DynamoDB Local; localhost is not
on the production origin allowlist. No AWS budget resources are configured.

## Current frontend integration points

- `web/src/engine/use-table.ts` already exists: it runs a local WASM game and bots,
  owns `TableState`/`MatchState`, sequences animations and integrates grading.
  Do not assume the older `use-hand.ts`/`HandView` sketch in `MULTIPLAYER.md`
  describes the current code.
- `web/src/app.tsx` consumes that hook and coordinates selection, actions,
  immersive/analytic views, hints and results.
- `web/src/table/` owns the visual tile model, layout, setup and animations.
- `web/src/engine/names.ts` and several view/model operations assume you are seat 0.
- The local hook reads `?seed=` and starts the grader from a seed. Neither is
  suitable for multiplayer, where the hand seed stays on the server.

Keep local practice available. Add a remote transport/state hook, for example
`useRemoteTable`, and adapt its observations into the existing presentation.
Re-read these files before implementation: the local table UI is evolving.

## Screens and controls

- [ ] Entry: choose local practice, create a room, or join a room.
- [ ] Create: username, human count 1–4 (default 2), optional password, and bot
  preset (default efficiency; random and defensive supported). Four total seats
  are fixed; explain the remaining bot count.
- [ ] Join: invite code and username, with optional password input. Support
  `/?room=CODE` directly. There is no room lookup/metadata HTTP endpoint before
  joining, so do not depend on one to decide whether to ask for a password.
- [ ] Lobby: show names, open human seats, bots and connection status. Provide
  copy-code and copy-link controls with visible success/failure feedback.
- [ ] Host start: enable when all reserved human seats are filled. The creator
  is seat 0; dealer rotation does not change who is host.
- [ ] Play: waiting-for-player names, connection status, legal actions, hints,
  and explicit "Let a bot play my seat" / resume control.
- [ ] Results: settlement, revealed hands, running scores, and host-only next
  hand. Other players see that they are waiting for the host.
- [ ] Errors: wrong password, missing/expired room, full room, already-started
  room, rejected action, expired seat token, rate limiting and connection loss.

Room settings and usernames are chosen on creation/join. There are no rename,
kick-player, change-room-settings, late-join or ready-up endpoints in v1.

## HTTP and seat identity

- [ ] Add typed create/join requests and responses matching the backend.
  `POST /tables` accepts `{name, human_players, password?, bot?}`;
  `POST /tables/{code}/join` accepts `{name, password?}`.
- [ ] Save the returned `{table, seat, seat_token}` so refresh can restore the
  seat. Key saved identity by room and handle storage failures. Do not log tokens.
- [ ] If a room already has a saved identity, attempt resume rather than joining
  again and consuming another seat. Give an explicit recovery path if it fails.
- [ ] Build invite links from the current site's origin and returned `join_path`.
  Never put a password, seat token or seed in a shared link.
- [ ] Disable duplicate submissions. Do not blindly replay create/join requests
  after an uncertain network failure; these endpoints are not idempotent.
- [ ] Handle HTTP 429 with a clear wait/retry state. Avoid retry loops against
  password checks or room creation.

Browser storage is origin-specific: identities on the two production domains
are separate. Choose a clear UX for an invite opened on the other domain.

## WebSocket client

- [ ] Open the configured endpoint with `?table=CODE&token=SEAT_TOKEN`.
- [ ] Send `{type:"sync"}` after every open, including reconnect. AWS does not
  send the initial snapshot during `$connect`.
- [ ] Send `{type:"ping"}` at least every five minutes. Reconnect after idle
  timeout, maximum connection duration, sleep/wake or temporary network loss.
  Use bounded backoff and clean up sockets, listeners and timers on unmount.
- [ ] Define the replaced-tab UX: a new socket replaces the previous socket for
  that seat. Avoid two tabs continuously reconnecting and replacing each other.
- [ ] Disable actions until synchronized. Do not queue/replay game actions while
  disconnected. Send the latest `hand_number` and `steps_seen` with each action.
- [ ] On `stale`, accept the fresh state and let the player reconsider the action.
- [ ] Discard state/events/hints/results older than the current table version.
  Accept equal-version messages: an update contains several messages, and
  presence can change without changing the game version. Reset hand-specific
  selection, advice, results and animations when the hand number changes.
- [ ] Recover from missed messages with `sync`; never infer a successful action
  solely from the click or reconstruct server game state from client guesses.

Client messages in addition to sync/ping:

```json
{"type":"start_hand"}
{"type":"act","action":{"discard":"5m"},"hand_number":1,"steps_seen":0}
{"type":"hint","samples":0}
{"type":"autoplay","enabled":true,"hand_number":1,"steps_seen":0}
{"type":"next_hand","hand_number":1}
```

Autoplay keeps the human's name/token and uses an efficiency bot. It is explicit;
disconnecting does not enable it. Hints are server-side and support only
`samples: 0` in v1.

## Table, animations and grading

- [ ] Map server observations into the existing tile model. Reconcile concealed
  tile identities locally for sorting, dragging and selection; tile notation,
  not a visual tile ID, is sent in an action.
- [ ] Rotate the view relative to the authenticated seat, so every player is at
  the bottom. Update names, winds, ponds, meld placement and all seat-0 assumptions.
- [ ] Keep opponent concealed tiles as backs/counts. Multiplayer must not create
  a local `Game`, simulate other players' hands, or generate the secret wall.
- [ ] Separate cosmetic opening randomness from engine randomness. Adapt or
  shorten shuffle/deal animations to use public counts and this player's tiles;
  they must not need a server seed or hidden tile faces.
- [ ] Reconcile snapshots immediately, then animate presentation. Several bot
  actions may arrive in one update; a reconnect may skip many moves. Cancel or
  fast-forward obsolete animations instead of delaying authoritative state.
- [ ] Consume structured resolved events for the log. Draw events have no tile;
  claim bids must not be logged as successful melds before they resolve. Prevent
  duplicate events on retries/equal-version updates.
- [ ] Show other concealed hands only from the explicit `hand_over` message.
- [ ] Remove seed URLs/debug output in multiplayer. Disable the seed-based local
  grader there, or design a separate observation-only server grading API later.
  Do not send the full game to the browser to preserve the existing grader.
- [ ] Preserve local sorting/settings/accessibility and practice-mode behavior.

## Verification and completion

- [ ] Test create/join forms, identity restoration, sync/reconnect, stale actions,
  version ordering, equal-version presence, event deduplication and token privacy.
- [ ] Play a complete two-human/two-bot hand in separate browser sessions, then
  another hand with scores and dealer rotation.
- [ ] Check four humans, one human/three bots, password errors, full rooms,
  simultaneous claims, refresh mid-hand, dropped sockets and autoplay/resume.
- [ ] Verify all four seat perspectives and both production domains. Check
  invite links, including a first-time visitor with no saved identity.
- [ ] Check desktop, phone, light/dark, keyboard focus and reduced motion. Follow
  the existing tile artwork and design rules; avoid redesigning the table just
  to add networking.
- [ ] Run `pnpm test`, `pnpm run lint:check`, `pnpm run check-types`,
  `pnpm run prettier:check` and `pnpm build` from `web/`.
- [ ] Deploy the web client and verify the same flows against the production
  backend. Backend-only smoke tests do not prove the room UI works.

Out of scope for this UI pass: accounts, spectators, turn timers, automatic
absence takeover, stronger bots, public matchmaking and a replay browser.
