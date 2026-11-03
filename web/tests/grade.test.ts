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

test("the recommendation alone is insufficient", () => {
    const advice: Advice = { recommended: { discard: "1m" }, ranked, estimates: [] };

    assert.equal(grade(advice, { discard: "1m" }).grade, "Unclear");
});

test("noisy evidence is unclear", () => {
    const advice: Advice = {
        recommended: { discard: "1m" },
        ranked,
        estimates: [estimate({ discard: "1m" }, 0, 0), estimate({ discard: "5p" }, -0.2, 0.3), estimate({ discard: "9s" }, -2, 0.3)],
    };

    assert.equal(grade(advice, { discard: "5p" }).grade, "Unclear");
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

test("without playouts shape does not determine the grade", () => {
    const advice: Advice = { recommended: { discard: "1m" }, ranked, estimates: [] };

    assert.equal(grade(advice, { discard: "9s" }).grade, "Unclear");
});

test("good shape cannot override a clear expected loss", () => {
    const advice: Advice = {
        recommended: { discard: "1m" },
        ranked,
        estimates: [estimate({ discard: "1m" }, 0, 0), estimate({ discard: "5p" }, -2.5, 0.1), estimate({ discard: "9s" }, -3, 0.1)],
    };
    assert.equal(grade(advice, { discard: "5p" }).grade, "Terrible");
    advice.recommended = { discard: "5p" };
    assert.equal(grade(advice, { discard: "5p" }).grade, "Terrible");
});

test("precise estimates determine grades without subtracting noise from loss", () => {
    for (const [loss, expected] of [
        [0.05, "Excellent"],
        [0.25, "Good"],
        [0.6, "Okay"],
        [1.1, "Bad"],
        [2, "Terrible"],
    ] as const) {
        const advice: Advice = {
            recommended: { discard: "1m" },
            ranked,
            estimates: [estimate({ discard: "1m" }, 0, 0), estimate({ discard: "5p" }, -loss, 0.01), estimate({ discard: "9s" }, -3, 0.01)],
        };
        const result = grade(advice, { discard: "5p" });
        assert.equal(result.grade, expected);
        assert.equal(result.loss, loss);
    }
});

test("even the estimated best move needs evidence", () => {
    const advice: Advice = {
        recommended: { discard: "1m" },
        ranked,
        estimates: [estimate({ discard: "1m" }, 0, 0), estimate({ discard: "5p" }, -0.1, 0.4), estimate({ discard: "9s" }, -3, 0.1)],
    };
    assert.equal(grade(advice, { discard: "1m" }).grade, "Unclear");
    advice.estimates = advice.estimates.slice(0, 1);
    assert.equal(grade(advice, { discard: "1m" }).grade, "Unclear");
});

test("taking a win is excellent and a forced move is ungraded", () => {
    const advice: Advice = { recommended: "win", ranked: [assessment("win", -1, 0)], estimates: [] };
    assert.equal(grade(advice, "win").grade, "Excellent");
    advice.recommended = "pass";
    advice.ranked = [assessment("pass", 1, 0)];
    assert.equal(grade(advice, "pass").grade, "Unclear");
});
