import { expandCounts, meldTiles, removeOne } from "../src/engine/tiles";
import assert from "node:assert/strict";
import test from "node:test";

test("grouped notation expands to one entry per tile", () => {
    assert.deepEqual(expandCounts("1156m88p7z"), ["1m", "1m", "5m", "6m", "8p", "8p", "7z"]);
    assert.deepEqual(expandCounts(""), []);
});

test("melds expand to the tiles they show", () => {
    assert.deepEqual(meldTiles({ chow: "3p" }), ["3p", "4p", "5p"]);
    assert.deepEqual(meldTiles({ pong: "5z" }), ["5z", "5z", "5z"]);
    assert.deepEqual(meldTiles({ kong: ["2s", "concealed"] }), ["2s", "2s", "2s", "2s"]);
});

test("removeOne takes a single copy and ignores a missing tile", () => {
    assert.deepEqual(removeOne(["1m", "1m", "2m"], "1m"), ["1m", "2m"]);
    assert.deepEqual(removeOne(["1m"], "9s"), ["1m"]);
});
