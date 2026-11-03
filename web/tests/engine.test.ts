import createMjers from "@mjers/mjers";
import assert from "node:assert/strict";
import test from "node:test";

test("the engine deals a repeatable hand", async () => {
    const mjers = await createMjers();
    const first = new mjers.Game(42n, 0);
    const second = new mjers.Game(42n, 0);

    try {
        const seat = first.toAct()[0];

        assert.notEqual(seat, undefined);
        assert.deepEqual(first.observe(seat!), second.observe(seat!));
    } finally {
        first.free();
        second.free();
    }
});
