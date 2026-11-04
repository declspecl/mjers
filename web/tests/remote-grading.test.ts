import { confirmsMove, type PendingGrade } from "../src/remote/grading";
import { rotateObservation } from "../src/remote/rotate";
import createMjers from "@mjers/mjers";
import type { Seat } from "@mjers/mjers/types";
import assert from "node:assert/strict";
import test from "node:test";

const mjers = await createMjers();

test("a grade uses the pre-move view for every multiplayer seat", () => {
    for (const seat of [0, 1, 2, 3] as Seat[]) {
        const game = new mjers.Game(42n, seat);
        try {
            const before = rotateObservation(game.observe(seat), seat);
            const analysis = mjers.Game.fromObservation(before, 99n);
            try {
                assert.deepEqual(analysis.observe(0), before);
            } finally {
                analysis.free();
            }
            const chosen = before.legal_actions.find((action) => typeof action === "object" && "discard" in action)!;
            const pending: PendingGrade = { hand: 0, decision: 0, observation: before, chosen };
            assert.equal(confirmsMove(pending, 0, 0, before), false);
            game.apply(seat, chosen);
            const after = rotateObservation(game.observe(seat), seat);
            assert.equal(confirmsMove(pending, 0, 1, after), true);
            assert.notEqual(pending.observation.hand.concealed, after.hand.concealed);
            assert.equal(pending.observation.seat, 0);
            assert.equal(confirmsMove(pending, 1, 1, after), false);
        } finally {
            game.free();
        }
    }
});

test("another player's response in the same claim window is not our confirmation", () => {
    const game = new mjers.Game(42n, 0);
    try {
        const observation = game.observe(0);
        const claim = { ...observation, decision: { kind: "claim" as const, tile: "1m", from: 1 as const, offer: "discard" as const } };
        const pending: PendingGrade = { hand: 0, decision: 5, observation: claim, chosen: "pass" };
        assert.equal(confirmsMove(pending, 0, 6, claim), false);
        assert.equal(confirmsMove(pending, 0, 6, { ...claim, to_act: [2] }), true);
    } finally {
        game.free();
    }
});
