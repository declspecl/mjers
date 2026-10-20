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

    for (const preset of ["family", "discarder-pays"]) {
        for (let seed = 0n; seed < 24n; seed++) {
            const { result } = play(seed, "efficiency", mjers.ruleset(preset));

            if (result.win) {
                wins++;
            } else {
                draws++;
            }

            kongs += result.kongs.length;
        }
    }

    assert.ok(wins > 0);
    assert.ok(draws > 0);
    assert.ok(kongs > 0);
});
