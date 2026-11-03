import { createLog, recordMove } from "../src/engine/event-log";
import createMjers from "@mjers/mjers";
import type { Seat } from "@mjers/mjers/types";
import assert from "node:assert/strict";
import test from "node:test";

const SEATS: Seat[] = [0, 1, 2, 3];

test("the log shows each discard and each meld exactly once across 80 bot hands", async () => {
    const mjers = await createMjers();

    for (let seed = 0; seed < 80; seed++) {
        const game = new mjers.Game(BigInt(seed), 0);
        const bots = SEATS.map((seat) => new mjers.Bot("efficiency", BigInt(seed * 4 + seat)));
        const log = createLog();

        try {
            for (let step = 0; step < 2000 && !game.isOver(); step++) {
                const seat = game.toAct()[0];

                assert.notEqual(seat, undefined, `seed ${seed}: nobody to act in an unfinished hand`);

                const action = bots[seat!]!.choose(game, seat!);

                game.apply(seat!, action);
                recordMove(log, game.observe(0).players, seat!, action);
            }

            assert.ok(game.isOver(), `seed ${seed}: the hand did not finish`);

            const observation = game.observe(0);
            const texts = log.events.map((event) => event.text);
            const melds = observation.players.reduce((total, player) => total + player.melds.length, 0);
            const claimed = texts.filter((text) => text.startsWith("claimed a ")).length;
            const concealedKongs = texts.filter((text) => text.startsWith("declared a concealed kong")).length;

            assert.equal(claimed + concealedKongs, melds, `seed ${seed}: logged claims and kongs differ from the melds on the table`);
            assert.equal(
                texts.filter((text) => text.startsWith("discarded ")).length,
                observation.discards.length,
                `seed ${seed}: discards differ`,
            );
            assert.ok(
                texts.every((text) => !text.includes("undefined")),
                `seed ${seed}: a log line has a missing value`,
            );
        } finally {
            game.free();

            for (const bot of bots) {
                bot.free();
            }
        }
    }
});
