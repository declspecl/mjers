import { z } from "zod";

export const seatSchema = z.union([z.literal(0), z.literal(1), z.literal(2), z.literal(3)]);
export const roomCodeSchema = z.string().trim().toUpperCase().length(6, "Enter the six-character room code.");
export const usernameSchema = z
    .string()
    .trim()
    .min(1, "Enter the name you want your friends to see.")
    .max(40, "Use at most 40 characters for your name.");
export const passwordSchema = z
    .string()
    .refine((value) => new TextEncoder().encode(value).length <= 128, "Choose a shorter password, using at most 128 bytes.");
export const botSchema = z.enum(["efficiency", "defensive", "random"]);

export const createRequestSchema = z.object({
    name: usernameSchema,
    human_players: z.number().int().min(1).max(4),
    password: passwordSchema.optional().transform((value) => value || undefined),
    bot: botSchema.optional(),
});
export const joinRequestSchema = createRequestSchema.pick({ name: true, password: true });
export const seatGrantSchema = z.object({
    table: roomCodeSchema,
    seat: seatSchema,
    seat_token: z.string().min(1),
    join_path: z.string().startsWith("/"),
});
export const savedSeatSchema = z.object({
    table: roomCodeSchema,
    seat: seatSchema,
    token: z.string().min(1),
    name: z.string().optional(),
    saved: z.number().int().nonnegative(),
});
export const errorResponseSchema = z.object({
    code: z.enum([
        "illegal_action",
        "stale",
        "not_your_turn",
        "table_full",
        "bad_token",
        "bad_password",
        "invalid_room",
        "not_host",
        "not_ready",
        "hand_in_progress",
        "internal",
        "rate_limited",
    ]),
    message: z.string(),
});
