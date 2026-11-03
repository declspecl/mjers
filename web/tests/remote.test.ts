import { describeEvent } from "../src/remote/events";
import { fromView, rotateEvent, rotateResult, rotateSeats, toView } from "../src/remote/rotate";
import type { Seat } from "@mjers/mjers/types";
import assert from "node:assert/strict";
import test from "node:test";

test("every seat sees itself at the bottom", () => {
    for (const me of [0, 1, 2, 3] as Seat[]) {
        assert.equal(toView(me, me), 0);
        assert.equal(toView(((me + 1) % 4) as Seat, me), 1);

        for (const seat of [0, 1, 2, 3] as Seat[]) {
            assert.equal(fromView(toView(seat, me), me), seat);
        }
    }

    assert.deepEqual(rotateSeats(["a", "b", "c", "d"], 2), ["c", "d", "a", "b"]);
});

test("results and events turn with the seat", () => {
    const result = rotateResult({ win: { winner: 3, source: { Discard: 1 }, bonus_tiles: 0 }, kongs: [{ owner: 1, kind: "exposed" }] }, 1);

    assert.deepEqual(result.win, { winner: 2, source: { Discard: 0 }, bonus_tiles: 0 });
    assert.equal(result.kongs[0]!.owner, 0);
    assert.deepEqual(rotateEvent({ event: "claim", seat: 2, from: 1, meld: { pong: "5m" } }, 1), {
        event: "claim",
        seat: 1,
        from: 0,
        meld: { pong: "5m" },
    });
});

test("draws are not logged and claims are logged as resolved melds", () => {
    assert.equal(describeEvent({ event: "draw", seat: 1 }), undefined);
    assert.equal(describeEvent({ event: "discard", seat: 1, tile: "5m" })?.text, "discarded 5 characters");
    assert.match(describeEvent({ event: "claim", seat: 2, from: 1, meld: { pong: "7z" } })!.text, /claimed a pong/);
});
