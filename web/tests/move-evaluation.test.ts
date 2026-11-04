import { actionLabel } from "../src/engine/action-label";
import { evaluateMoves, grade } from "../src/grading/grade";
import fixtures from "./fixtures/grading-positions.json";
import createMjers from "@mjers/mjers";
import type { Action, Advice, Assessment, Observation } from "@mjers/mjers/types";
import assert from "node:assert/strict";
import test from "node:test";

const mjers = await createMjers();
function assessment(action: Action, shanten: number, live: number): Assessment {
    return { action, shanten, live_improving: live, danger: 0, immediate_units: 0, reasons: [] };
}

test("only the engine recommendation is Perfect; close alternatives are Good", () => {
    const best: Action = { discard: "1m" };
    const ranked = [assessment(best, 1, 20), assessment({ discard: "2m" }, 1, 19), assessment({ discard: "3m" }, 2, 20)];
    const advice: Advice = { recommended: best, ranked, estimates: [] };
    assert.deepEqual(
        evaluateMoves(advice).map((entry) => entry.grade),
        ["Perfect", "Good", "Bad"],
    );
    assert.deepEqual(grade(advice, { discard: "3m" }).best, best);
});

test("all real legal choices receive an evaluation and the same best-move suggestion", () => {
    for (const fixture of Object.values(fixtures)) {
        const observation = fixture as Observation;
        const game = mjers.Game.fromObservation(observation, 2n);
        try {
            const advice = game.advise(observation.seat, { samples: 0 });
            const evaluations = evaluateMoves(advice);
            assert.equal(evaluations.length, observation.legal_actions.length);
            assert.equal(evaluations.filter((entry) => entry.grade === "Perfect").length, 1);
            assert.deepEqual(evaluations[0]!.action, advice.recommended);
            for (const [index, entry] of evaluations.entries()) {
                assert.deepEqual(entry.best, advice.recommended);
                assert.equal(entry.rank, index + 1);
                assert.equal(entry.choices, evaluations.length);
                assert.ok(["Perfect", "Good", "Okay", "Bad", "Terrible"].includes(entry.grade));
                assert.deepEqual(grade(advice, entry.action), entry);
            }
        } finally {
            game.free();
        }
    }
});

test("the actual recommendation wins even when it differs from the static ranking", () => {
    const ranked = [assessment({ discard: "1m" }, 1, 20), assessment({ discard: "2m" }, 1, 19)];
    const advice: Advice = { recommended: ranked[1]!.action, ranked, estimates: [] };
    assert.deepEqual(evaluateMoves(advice)[0]!.action, advice.recommended);
    assert.equal(grade(advice, ranked[1]!.action).grade, "Perfect");
    assert.equal(grade(advice, ranked[0]!.action).grade, "Good");
});

test("the best move is short and readable for discards and calls", () => {
    assert.equal(actionLabel({ discard: "5p" }), "Discard 5 dots");
    assert.equal(actionLabel("pong"), "Pong");
    assert.equal(actionLabel("pass"), "Pass");
    assert.match(actionLabel({ chow: "3s" }), /3 bamboo/);
});
