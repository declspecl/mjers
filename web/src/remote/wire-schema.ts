import type { ServerMessage } from "./protocol";
import { errorResponseSchema, roomCodeSchema, seatSchema } from "./schemas";
import { z } from "zod";

const count = z.number().int().nonnegative();
const tile = z.string();
const units = z.tuple([z.number(), z.number(), z.number(), z.number()]);
const kind = z.enum(["exposed", "concealed"]);
const meld = z.union([z.object({ chow: tile }), z.object({ pong: tile }), z.object({ kong: z.tuple([tile, kind]) })]);
const action = z.union([
    z.enum(["self_draw_win", "pass", "pong", "kong", "win"]),
    z.object({ discard: tile }),
    z.object({ concealed_kong: tile }),
    z.object({ upgrade_kong: tile }),
    z.object({ chow: tile }),
]);
const hand = z.object({ concealed: z.string(), melds: z.array(meld), bonus: z.array(z.string()) });
const source = z.union([z.literal("SelfDraw"), z.object({ Discard: seatSchema })]);
const player = z.object({ seat: seatSchema, concealed_count: count, melds: z.array(meld), bonus: z.array(z.string()) });
const seat = z.object({
    seat: seatSchema,
    name: z.string().nullable(),
    kind: z.enum(["open", "human", "bot"]),
    connected: z.boolean(),
    autoplay: z.boolean(),
});
const rules = z.object({
    name: z.string(),
    flower_wall_stacks: count,
    allow_rob_kong: z.boolean(),
    payouts: z.object({
        discard_win: count,
        discard_win_discarder: count,
        self_draw_win: count,
        per_bonus: count,
        exposed_kong: count,
        concealed_kong: count,
        kongs_paid_without_winner: z.boolean(),
    }),
});
const observation = z.object({
    seat: seatSchema,
    dealer: seatSchema,
    rules,
    hand,
    players: z.tuple([player, player, player, player]),
    discards: z.array(z.object({ seat: seatSchema, tile, claimed: z.boolean(), drawn: z.boolean() })),
    public_tiles: z.string(),
    wall_remaining: count,
    live_wall: count,
    to_act: z.array(seatSchema),
    legal_actions: z.array(action),
    decision: z.discriminatedUnion("kind", [
        z.object({ kind: z.literal("turn"), after_claim: z.boolean(), drawn: tile.nullable() }),
        z.object({ kind: z.literal("claim"), tile, from: seatSchema, offer: z.enum(["discard", "kong_upgrade"]) }),
        z.object({ kind: z.literal("none") }),
    ]),
});
const event = z.discriminatedUnion("event", [
    z.object({ event: z.literal("bonus"), seat: seatSchema, bonus: z.string() }),
    z.object({ event: z.literal("draw"), seat: seatSchema }),
    z.object({ event: z.literal("discard"), seat: seatSchema, tile }),
    z.object({ event: z.literal("claim"), seat: seatSchema, from: seatSchema, meld }),
    z.object({ event: z.literal("kong"), seat: seatSchema, meld }),
    z.object({ event: z.literal("win"), seat: seatSchema, source, tile }),
    z.object({ event: z.literal("exhausted") }),
]);
const result = z.object({
    win: z.object({ winner: seatSchema, source, bonus_tiles: count }).nullable(),
    kongs: z.array(z.object({ owner: seatSchema, kind })),
});
const assessment = z.object({
    action,
    shanten: z.number(),
    live_improving: count,
    danger: z.number(),
    immediate_units: z.number(),
    reasons: z.array(z.string()),
});
const estimate = z.object({
    action,
    samples: count,
    ev: z.number(),
    ev_stderr: z.number(),
    win_rate: z.number(),
    deal_in_rate: z.number(),
    difference: z.number(),
    difference_stderr: z.number(),
    comparisons: z.array(z.object({ action, difference: z.number(), stderr: z.number() })).optional(),
});
const position = { version: count, hand_number: count };

/** Parse every network frame before it reaches the client state machine. */
export const serverMessageSchema: z.ZodType<ServerMessage> = z.discriminatedUnion("type", [
    z.object({
        type: z.literal("state"),
        ...position,
        table: roomCodeSchema,
        seats: z.tuple([seat, seat, seat, seat]),
        human_players: z.number().int().min(1).max(4),
        password_protected: z.boolean(),
        scores: units,
        steps: count,
        observation: observation.nullable(),
    }),
    z.object({ type: z.literal("events"), ...position, events: z.array(event) }),
    z.object({
        type: z.literal("hand_over"),
        ...position,
        result,
        settlement: z.object({ win: units, kongs: units, total: units }),
        revealed: z.tuple([hand, hand, hand, hand]),
    }),
    z.object({
        type: z.literal("hint"),
        ...position,
        advice: z.object({ recommended: action, ranked: z.array(assessment), estimates: z.array(estimate) }),
    }),
    errorResponseSchema.extend({ type: z.literal("error") }),
    z.object({ type: z.literal("pong") }),
]);
