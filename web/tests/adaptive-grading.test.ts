import { analyzeMove } from "../src/grading/analyze";
import { gradeByPlayouts as grade } from "../src/grading/playout-grade";
import type { Action, Advice, Estimate } from "@mjers/mjers/types";
import assert from "node:assert/strict";
import test from "node:test";

const best: Action = { discard: "1m" };
const chosen: Action = { discard: "5p" };

function advice(loss: number, error: number, samples = 48): Advice {
    const estimate = (action: Action, difference: number, stderr: number): Estimate => ({
        action,
        difference,
        difference_stderr: stderr,
        samples,
        ev: difference,
        ev_stderr: stderr,
        win_rate: 0,
        deal_in_rate: 0,
    });

    return {
        recommended: best,
        ranked: [best, chosen].map((action) => ({ action, shanten: 1, live_improving: 20, danger: 0, immediate_units: 0, reasons: [] })),
        estimates: [estimate(best, 0, 0), estimate(chosen, -loss, error)],
    };
}

test("a known loss is retained when severity crosses grade boundaries", () => {
    const result = grade(advice(1.5, 0.2), chosen);
    assert.equal(result.grade, "Mistake");
    assert.equal(result.certainty, "severity");
    assert.ok(result.lossRange![0] > 0.4);
});

test("direct pairing resolves noise that baseline-relative errors cannot", () => {
    const position = advice(2, 1);
    assert.equal(grade(position, chosen).grade, "Unclear");
    Object.assign(position.estimates[0]!, { comparisons: [{ action: chosen, difference: 2, stderr: 0.01 }] });
    assert.equal(grade(position, chosen).grade, "Terrible");
});

test("live feedback requests one instant assessment and no playouts", () => {
    const calls: number[] = [];
    const result = analyzeMove((samples) => {
        calls.push(samples);

        return advice(2, 1, samples);
    }, chosen);
    assert.deepEqual(calls, [0]);
    assert.notEqual(result.grade, "Unclear");
    assert.equal(result.basis, "Hand progress");
    assert.ok(result.summary);
});

test("invalid paired errors fall back to conservative baseline comparisons", () => {
    const position = advice(2, 1);
    Object.assign(position.estimates[0]!, { comparisons: [{ action: chosen, difference: 2, stderr: Number.NaN }] });
    assert.equal(grade(position, chosen).grade, "Unclear");
});
