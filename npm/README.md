# @mjers/mjers

Mahjong game engine, hand analysis, bots, and advisor, running locally in
WebAssembly. Uses the same deterministic engine as `mj` and the Python package.
ES modules with TypeScript declarations, following the Shark npm package layout.

```js
import createMjers from "@mjers/mjers";

const mjers = await createMjers();
const game = new mjers.Game(42n, 0); // u64 seed, dealer seat 0–3
const bot = new mjers.Bot("efficiency", 1n);

try {
    const seat = game.toAct()[0];
    console.log(game.observe(seat));
    const advice = game.advise(seat, { samples: 0 });
    console.log(advice.recommended, advice.ranked[0].reasons);
    game.apply(seat, bot.choose(game, seat));
    console.log(mjers.analyze("123m456p78s11122z"));
} finally {
    bot.free();
    game.free();
}
```

`createMjers` lazily initializes one shared runtime; importing the package does
not load or instantiate WASM. Concurrent calls share initialization. A failed
initialization can be retried. The first successful call's options apply.
Browser loading uses the adjacent `mjers_wasm_bg.wasm`; Node reads it from disk.
Use `createMjers({ wasmBinary: bytes })` for bundler-managed assets, or
`createMjers({ locateFile: (path, prefix) => new URL(path, cdnUrl) })` for a CDN.
Serve WASM with `application/wasm` for streaming compilation.

Types are also available from `@mjers/mjers/types` without loading WASM.
Object fields and action encodings follow the engine's Serde format: for
example `{ discard: "5m" }`, `{ chow: "3p" }`, or `"pass"`. Tile counts use
grouped notation. Observations include only the selected seat's concealed hand;
other players expose melds, bonuses, and concealed counts. The local game owner
can request observations for any seat.

`game.advise(seat, options)` accepts `samples` (default 200, 0 for instant hints),
`candidates` (default 6), a bigint `seed` (default 0n), and `rollout` (default
`"efficiency"`). Advice and bot choices require a seat currently in `toAct()`.
Advice includes the recommendation, explained heuristic rankings, and playout
estimates. Analysis and advice are synchronous; run expensive playouts in a Web
Worker to keep the UI responsive.

`analyze(concealed, melds = 0, visible = "")` accepts a 13- or 14-tile hand,
counting three per declared meld. Visible tiles include all declared melds and
discards, excluding your concealed hand. It returns shanten, live improving
tiles, and ranked discards for non-winning 14-tile hands. A live copy is unseen,
so it may be held by an opponent rather than left in the wall.

`ruleset("family")` and `ruleset("discarder-pays")` return rules objects; pass a
modified object as the third `Game` constructor argument for custom rules.
Bot presets are `"efficiency"`, `"defensive"`, and `"random"`. Seeds use unsigned
64-bit bigints, preserving Rust's complete seed range. Invalid inputs and illegal
actions throw JavaScript Errors. `game.result()` and `game.settlement()` are
`null` until the hand ends; settlement includes win, kong, and total unit changes.
Always release `Game` and `Bot` instances with `free()`.

## Build and verify

From the repository root:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.125 --locked
npm ci --prefix npm
npm test --prefix npm
npm run test:package --prefix npm
npm pack ./npm --pack-destination npm
```

`npm run build --prefix npm` builds the release WASM and stages JavaScript,
WASM, and generated declarations in `npm/`. Generated artifacts are ignored by
git and included in the npm tarball. Packing runs the build, TypeScript checks,
and real WASM tests first. The installed package has no runtime dependencies or
install scripts and needs no Rust toolchain. Nothing is published by building
or packing the package.

## Publishing

`.github/workflows/publish-wasm.yml` publishes `@mjers/mjers` to the public npm
registry on `v*` tags. The tag must match `npm/package.json`, for example
`v0.1.0`. The job builds the release WASM, runs TypeScript and JavaScript tests,
verifies a clean tarball installation, and publishes with provenance.

For the first release, set the repository's `NPM_TOKEN` Actions secret to an npm
token allowed to publish under `@mjers`. Alternatively, configure an npm trusted
publisher for GitHub user `declspecl`, repository `mjers`, and workflow filename
`publish-wasm.yml`. Trusted publishing uses OIDC and requires no token secret.
The workflow supports either method; merely owning the npm organization does
not configure authentication for GitHub Actions. See the
[npm trusted publishing documentation](https://docs.npmjs.com/trusted-publishers/).

Update the package version and npm lockfile together before creating a release
tag. Building and testing locally do not publish anything.
