import { grade } from "../src/grading/grade";
import type { Action, Advice, Assessment, Estimate } from "@mjers/mjers/types";
import assert from "node:assert/strict";
import test from "node:test";

function assessment(action: Action, shanten: number, live: number): Assessment {
    return { action, shanten, live_improving: live, danger: 0, immediate_units: 0, reasons: [] };
}

function estimate(action: Action, difference: number, stderr = 0.1): Estimate {
    return { action, samples: 48, ev: difference, ev_stderr: 0.5, win_rate: 0, deal_in_rate: 0, difference, difference_stderr: stderr };
}

const ranked = [assessment({ discard: "1m" }, 1, 20), assessment({ discard: "5p" }, 1, 19), assessment({ discard: "9s" }, 2, 30)];

test("the engine's own choice is perfect", () => {
    const advice: Advice = { recommended: { discard: "1m" }, ranked, estimates: [] };

    assert.equal(grade(advice, { discard: "1m" }).grade, "Perfect");
});

test("a near-equal move is not punished for playout noise", () => {
    const advice: Advice = {
        recommended: { discard: "1m" },
        ranked,
        estimates: [estimate({ discard: "1m" }, 0, 0), estimate({ discard: "5p" }, -0.2, 0.3), estimate({ discard: "9s" }, -2, 0.3)],
    };

    assert.equal(grade(advice, { discard: "5p" }).grade, "Perfect");
});

test("a clear loss that also breaks the hand is terrible", () => {
    const advice: Advice = {
        recommended: { discard: "1m" },
        ranked: [assessment({ discard: "1m" }, 1, 20), assessment({ discard: "9s" }, 3, 30)],
        estimates: [estimate({ discard: "1m" }, 0, 0), estimate({ discard: "9s" }, -2.5, 0.3)],
    };

    assert.equal(grade(advice, { discard: "9s" }).grade, "Terrible");
});

test("passing up a win is terrible", () => {
    const advice: Advice = { recommended: "win", ranked: [assessment("win", -1, 0), assessment("pass", 0, 10)], estimates: [] };

    assert.equal(grade(advice, "pass").grade, "Terrible");
});

test("without playouts the hand's shape decides", () => {
    const advice: Advice = { recommended: { discard: "1m" }, ranked, estimates: [] };

    assert.equal(grade(advice, { discard: "9s" }).grade, "Bad");
});
