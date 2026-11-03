import { createSeatStore } from "../src/remote/identity";
import { createRequestSchema, seatGrantSchema } from "../src/remote/schemas";
import { serverMessageSchema } from "../src/remote/wire-schema";
import { roomFormSchema } from "../src/rooms/room-schema";
import assert from "node:assert/strict";
import test from "node:test";

const form = { mode: "create", name: " Alex ", code: "", password: "", human_players: 2, bot: "efficiency" };
const held = { table: "ABC234", seat: 0 as const, token: "private-seat-token", name: "Alex", saved: Date.now() };

test("room schemas normalize names and enforce counts, invite codes and password byte limits", () => {
    assert.equal(roomFormSchema.parse(form).name, "Alex");
    assert.equal(createRequestSchema.parse(form).password, undefined);
    assert.equal(roomFormSchema.safeParse({ ...form, human_players: 5 }).success, false);
    assert.equal(roomFormSchema.safeParse({ ...form, mode: "join", code: "" }).success, false);
    assert.equal(roomFormSchema.safeParse({ ...form, mode: "join", code: "abc234" }).success, true);
    assert.equal(roomFormSchema.safeParse({ ...form, password: "界".repeat(43) }).success, false);
    assert.equal(roomFormSchema.safeParse({ ...form, name: "   " }).success, false);
});

test("Zustand retains a seat and notifies subscribers when browser saving fails", () => {
    const store = createSeatStore({
        getItem: () => null,
        setItem: () => {
            throw new Error("Storage blocked");
        },
    });
    let changes = 0;
    const unsubscribe = store.subscribe(() => {
        changes++;
    });
    assert.equal(store.getState().remember(held), false);
    assert.equal(store.getState().seats.ABC234?.token, held.token);
    assert.equal(changes, 1);
    store.getState().refresh();
    assert.equal(store.getState().seats.ABC234?.token, held.token);
    store.getState().forget("abc234");
    assert.equal(store.getState().seats.ABC234, undefined);
    unsubscribe();
});

test("existing saved identities load without accepting corrupt or expired records", () => {
    const store = createSeatStore({
        getItem: () => JSON.stringify({ ABC234: held, broken: { token: "bad" }, stale: { ...held, table: "DEF234", saved: 1 } }),
        setItem: () => {},
    });
    assert.deepEqual(Object.keys(store.getState().seats), ["ABC234"]);
});

test("network schemas reject malformed grants and nested messages before state updates", () => {
    assert.equal(seatGrantSchema.safeParse({ table: "ABC234", seat: 8, seat_token: "token", join_path: "/?room=ABC234" }).success, false);
    assert.deepEqual(serverMessageSchema.parse({ type: "pong" }), { type: "pong" });
    assert.equal(
        serverMessageSchema.safeParse({ type: "events", version: 1, hand_number: 1, events: [{ event: "discard", seat: 8, tile: "5m" }] })
            .success,
        false,
    );
    assert.equal(serverMessageSchema.safeParse({ type: "state", table: "ABC234", version: 1, observation: {} }).success, false);
});
