import assert from "node:assert/strict";
import { test } from "node:test";
import createMjers, { createMjers as namedFactory } from "@mjers/mjers";
import "@mjers/mjers/types";

const mjers = await createMjers();

test("initialization is shared and observations hide other hands", async () => {
    assert.equal(await namedFactory(), mjers);

    const game = new mjers.Game(42n, 0);

    try {
        const own = game.observe(0);

        assert.deepEqual(game.toAct(), [0]);
        assert.equal(own.decision.kind, "turn");
        assert.deepEqual(own.legal_actions, game.legalActions(0));
        assert.equal(own.players[0].concealed_count, 14);

        for (const player of own.players) {
            assert.deepEqual(Object.keys(player).sort(), ["bonus", "concealed_count", "melds", "seat"]);
        }

        for (const seat of [1, 2, 3]) {
            const other = game.observe(seat);

            assert.equal(other.players[seat].concealed_count, 13);
            assert.equal(other.decision.kind, "none");
            assert.deepEqual(other.legal_actions, []);
            assert.notEqual(other.hand.concealed, own.hand.concealed);
        }

        assert.equal(game.result(), null);
        assert.equal(game.settlement(), null);

        // Returned objects are detached snapshots.
        own.hand.concealed = "";
        assert.notEqual(game.observe(0).hand.concealed, "");
        assert.equal("wall" in own, false);
        assert.equal("seed" in own, false);
    } finally {
        game.free();
    }
});

test("analysis finds waits, accounts for visible copies, and ranks discards", () => {
    const ready = mjers.analyze("123m456p78s11122z");

    assert.equal(ready.shanten, 0);
    assert.deepEqual(ready.improvements, [
        { tile: "6s", live: 4 },
        { tile: "9s", live: 4 },
    ]);
    assert.equal(ready.live_improving, 8);
    assert.deepEqual(ready.discards, []);

    const visible = mjers.analyze("123m456p78s11122z", 0, "6666s");

    assert.deepEqual(visible.improvements, [{ tile: "9s", live: 4 }]);

    const melded = mjers.analyze("456p78s11122z", 1, "123m");

    assert.deepEqual(melded.improvements, ready.improvements);

    const complete = mjers.analyze("123m456p678s11122z");

    assert.equal(complete.shanten, -1);
    assert.deepEqual(complete.discards, []);

    const options = mjers.analyze("123m456p78s11122z5z").discards;
    const { discards, ...acceptance } = ready;

    assert.deepEqual(options[0], { tile: "5z", ...acceptance });

    for (let i = 1; i < options.length; i++) {
        const a = options[i - 1];
        const b = options[i];

        assert.ok(a.shanten < b.shanten || (a.shanten === b.shanten && a.live_improving >= b.live_improving));
    }
});

test("invalid input throws Errors and rejected actions leave the game intact", () => {
    assert.throws(() => mjers.analyze("123m"), /13 or 14/);
    assert.throws(() => mjers.analyze("11111m456p78s112z"), Error);
    assert.throws(() => mjers.analyze("123m456p78s11122z", 5), /at most 4 melds/);
    assert.throws(() => mjers.analyze("123m456p78s11122z", 0, "111z"), /four copies/);
    assert.throws(() => mjers.ruleset("unknown"), /unknown ruleset/);
    assert.throws(() => new mjers.Game(0n, 4), /seat must be 0-3/);
    assert.throws(() => new mjers.Bot("unknown", 0n), /unknown bot/);

    const rules = mjers.ruleset("family");

    rules.flower_wall_stacks = 100;
    assert.throws(() => new mjers.Game(0n, 0, rules), /flower wall/);

    const game = new mjers.Game(0n, 0);
    const bot = new mjers.Bot("efficiency", 0n);

    try {
        const before = game.observe(0);

        assert.throws(() => game.observe(256), /seat must be 0-3/);
        assert.throws(() => game.apply(0, "pass"), /cannot/);
        assert.throws(() => game.apply(0, { discard: "0m" }), Error);
        assert.throws(() => game.apply(1, "pass"), /no decision/);
        assert.throws(() => game.advise(1, { samples: 0 }), /no decision/);
        assert.throws(() => bot.choose(game, 1), /no decision/);
        assert.throws(() => game.advise(0, { samples: -1 }), Error);
        assert.throws(() => game.advise(0, { samples: 0, rollout: "unknown" }), /unknown bot/);

        assert.deepEqual(game.observe(0), before);
    } finally {
        bot.free();
        game.free();
    }
});

test("advice covers legal actions and seeded playouts are repeatable", () => {
    const game = new mjers.Game(42n, 0);

    try {
        const before = game.observe(0);
        const instant = game.advise(0, { samples: 0 });

        assert.equal(instant.ranked.length, game.legalActions(0).length);
        assert.deepEqual(instant.estimates, []);
        assert.deepEqual(instant.recommended, instant.ranked[0].action);
        assert.ok(instant.ranked.every((a) => a.reasons.length > 0));

        const config = { samples: 8, candidates: 2, seed: 18446744073709551615n };
        const sampled = game.advise(0, config);

        assert.deepEqual(game.advise(0, config), sampled);
        assert.equal(sampled.estimates.length, 2);

        for (const estimate of sampled.estimates) {
            assert.ok(estimate.samples > 0 && estimate.samples <= config.samples);
            assert.ok(Number.isFinite(estimate.ev));
            assert.ok(estimate.win_rate >= 0 && estimate.win_rate <= 1);
        }

        assert.ok(game.legalActions(0).some((a) => JSON.stringify(a) === JSON.stringify(sampled.recommended)));
        assert.deepEqual(game.observe(0), before);
    } finally {
        game.free();
    }
});

function play(seed, preset, rules) {
    const game = new mjers.Game(seed, Number(seed % 4n), rules);
    const bots = [0, 1, 2, 3].map((i) => new mjers.Bot(preset, BigInt(i)));
    const actions = [];
    const decisions = new Set();

    try {
        while (!game.isOver()) {
            assert.ok(actions.length < 1000, "the hand must terminate");
            const seat = game.toAct()[0];
            decisions.add(game.observe(seat).decision.kind);
            const action = bots[seat].choose(game, seat);
            assert.ok(game.legalActions(seat).some((a) => JSON.stringify(a) === JSON.stringify(action)));
            actions.push({ seat, action });
            game.apply(seat, action);
        }

        const settlement = game.settlement();

        assert.deepEqual(game.toAct(), []);
        assert.ok(game.result());
        assert.equal(
            settlement.total.reduce((a, b) => a + b),
            0,
        );
        assert.deepEqual(
            settlement.total,
            settlement.win.map((win, i) => win + settlement.kongs[i]),
        );
        assert.equal(game.observe(0).decision.kind, "none");
        assert.throws(() => game.advise(0), /no decision/);
        assert.throws(() => game.apply(0, "pass"), /hand is over/);

        return { actions, decisions, result: game.result(), settlement };
    } finally {
        for (const bot of bots) {
            bot.free();
        }

        game.free();
    }
}

test("full games and bot random streams repeat for the entire u64 seed range", () => {
    for (const preset of ["efficiency", "defensive", "random"]) {
        const first = play(18446744073709551615n, preset);

        assert.deepEqual(play(18446744073709551615n, preset), first);
        assert.ok(first.decisions.has("turn"));
        assert.ok(first.decisions.has("claim"));
    }
});

test("both rulesets finish games, including wins, draws, and kongs", () => {
    let wins = 0;
    let draws = 0;
    let kongs = 0;

    // Efficiency tables almost always produce a winner; random play exhausts the wall.
    for (const preset of ["family", "discarder-pays"]) {
        for (const strategy of ["efficiency", "random"]) {
            for (let seed = 0n; seed < 24n; seed++) {
                const { result } = play(seed, strategy, mjers.ruleset(preset));

                if (result.win) {
                    wins++;
                } else {
                    draws++;
                }

                kongs += result.kongs.length;
            }
        }
    }

    assert.ok(wins > 0);
    assert.ok(draws > 0);
    assert.ok(kongs > 0);
});

/** A tiny suit-resnet-v2 checkpoint with deterministic weights. */
function checkpoint({ architecture = "suit-resnet-v2", drop } = {}) {
    const width = 2;
    const layers = [];
    const layer = (name, weight) => {
        layers.push([`${name}.weight`, weight], [`${name}.bias`, [weight[0]]]);
    };

    layer("stem", [width, 72, 1]);
    layer("blocks.0.suit_first", [width, width, 1, 3]);
    layer("blocks.0.suit_second", [width, width, 1, 3]);
    layer("blocks.0.honor_first", [width, width, 1]);
    layer("blocks.0.honor_second", [width, width, 1]);
    layer("blocks.0.pooled", [width, 2 * width]);
    layer("tile_actions", [3, width, 1]);
    layer("summary", [width, 3 * width + 22]);
    layer("other_actions", [8, width]);
    layer("value_layer", [1, width]);

    const metadata = { architecture, dataset: { encoding_version: 1 } };
    const header = { __metadata__: { mjers: JSON.stringify(metadata) } };
    const values = [];

    for (const [i, [name, shape]] of layers.entries()) {
        if (name === drop) {
            continue;
        }

        const count = shape.reduce((a, b) => a * b, 1);
        header[name] = { dtype: "F32", shape, data_offsets: [values.length * 4, (values.length + count) * 4] };

        for (let j = 0; j < count; j++) {
            values.push(((i * 7 + j * 3) % 11) / 20 - 0.25);
        }
    }

    const json = new TextEncoder().encode(JSON.stringify(header));
    const bytes = new Uint8Array(8 + json.length + values.length * 4);

    new DataView(bytes.buffer).setBigUint64(0, BigInt(json.length), true);
    bytes.set(json, 8);
    bytes.set(new Uint8Array(Float32Array.from(values).buffer), 8 + json.length);

    return bytes;
}

test("networks load from checkpoint bytes, evaluate, and play legal hands", () => {
    const network = mjers.loadNetwork(checkpoint());
    const bot = mjers.Bot.fromNetwork(network);
    const field = [1, 2, 3].map((i) => new mjers.Bot("efficiency", BigInt(i)));
    const game = new mjers.Game(5n, 0);

    try {
        let evaluated = false;

        while (!game.isOver()) {
            const seat = game.toAct()[0];
            const policy = seat === 0 ? bot : field[seat - 1];

            if (seat === 0) {
                const view = network.evaluate(game, seat);
                const total = view.actions.reduce((sum, choice) => sum + choice.probability, 0);

                assert.ok(Number.isFinite(view.settlement));
                assert.equal(view.actions.length, game.legalActions(seat).length);
                assert.ok(Math.abs(total - 1) < 1e-5);
                assert.ok(view.actions.every((c, i) => i === 0 || view.actions[i - 1].probability >= c.probability));
                assert.deepEqual(bot.choose(game, seat), view.actions[0].action);
                evaluated = true;
            }

            game.apply(seat, policy.choose(game, seat));
        }

        assert.ok(evaluated);
        assert.ok(game.settlement());
    } finally {
        for (const other of field) {
            other.free();
        }

        bot.free();
        network.free();
        game.free();
    }

    assert.throws(() => mjers.loadNetwork(checkpoint({ architecture: "mlp-relu-v1" })), /suit-resnet-v2/);
    assert.throws(() => mjers.loadNetwork(checkpoint({ drop: "summary.bias" })), /missing tensor summary.bias/);
});
