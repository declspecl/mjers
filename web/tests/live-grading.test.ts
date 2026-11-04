import { grade } from "../src/grading/grade";
import fixtures from "./fixtures/grading-positions.json";
import createMjers from "@mjers/mjers";
import type { Action, Advice, Assessment, Observation } from "@mjers/mjers/types";
import assert from "node:assert/strict";
import test from "node:test";

const mjers = await createMjers();
function realGrade(id: keyof typeof fixtures, chosen: Action) {
    const observation = fixtures[id] as Observation;
    const game = mjers.Game.fromObservation(observation, 1n);
    try {
        return grade(game.advise(observation.seat, { samples: 0 }), chosen);
    } finally {
        game.free();
    }
}
function assessment(action: Action, shanten: number, live: number, danger = 0): Assessment {
    return { action, shanten, live_improving: live, danger, immediate_units: 0, reasons: [] };
}
function advice(ranked: Assessment[]): Advice {
    return { recommended: ranked[0]!.action, ranked, estimates: [] };
}

test("real paid kong is explained as compensation for lost readiness", () => {
    const result = realGrade("1008:30", "kong");
    assert.equal(result.grade, "Tradeoff");
    assert.equal(result.basis, "Kong value");
    assert.match(result.summary!, /3 units/);
});

test("delaying a concealed kong is different from destroying it", () => {
    const delayed = realGrade("1003:33", { discard: "2p" });
    assert.equal(delayed.grade, "Tradeoff");
    assert.match(delayed.summary!, /available for later/);
    const broken = realGrade("1003:33", { discard: "6s" });
    assert.equal(broken.grade, "Bad");
    assert.match(broken.summary!, /6 units/);
});

test("real upgrade opportunity is not missed when the fourth tile is retained", () => {
    const result = realGrade("1015:47", { discard: "3z" });
    assert.equal(result.grade, "Bad");
    assert.match(result.summary!, /3 units/);
    assert.equal(realGrade("1015:47", { upgrade_kong: "3z" }).grade, "Excellent");
});

test("real defensive sacrifice receives a safety tradeoff rather than automatic condemnation", () => {
    const result = realGrade("1017:22", { discard: "8s" });
    assert.equal(result.grade, "Tradeoff");
    assert.equal(result.basis, "Safety tradeoff");
    assert.match(result.reason, /extra step/);
});

test("claims that miss readiness receive concrete progress feedback", () => {
    const ranked = [assessment({ chow: "4s" }, 0, 7), assessment("pass", 1, 14)];
    const result = grade(advice(ranked), "pass");
    assert.equal(result.grade, "Bad");
    assert.equal(result.summary, "Breaks a ready hand.");
});

test("small differences in improving copies are treated gently", () => {
    const ranked = [assessment({ discard: "1m" }, 1, 20), assessment({ discard: "2m" }, 1, 18), assessment({ discard: "3m" }, 1, 15)];
    assert.equal(grade(advice(ranked), { discard: "2m" }).grade, "Excellent");
    assert.equal(grade(advice(ranked), { discard: "3m" }).grade, "Good");
});

test("same efficiency can still have an avoidable risk difference", () => {
    const ranked = [assessment({ discard: "1m" }, 0, 8, 0), assessment({ discard: "2m" }, 0, 8, 0.3)];
    const result = grade(advice(ranked), { discard: "2m" });
    assert.equal(result.grade, "Okay");
    assert.match(result.summary!, /safer/);
});

test("missed wins and missing assessments remain explicit", () => {
    const ranked = [assessment("win", -1, 0), assessment("pass", 0, 8)];
    assert.equal(grade(advice(ranked), "pass").grade, "Terrible");
    assert.equal(grade(advice(ranked), "win").grade, "Excellent");
    assert.equal(grade(advice(ranked), { discard: "1m" }).grade, "Unclear");
});
