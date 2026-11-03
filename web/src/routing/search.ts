import { z } from "zod";

/** Keep identifiers and u64 practice seeds as strings instead of JSON numbers. */
export function parseSearch(search: string): Record<string, string> {
    return Object.fromEntries(new URLSearchParams(search));
}

export function stringifySearch(search: Record<string, unknown>): string {
    const params = new URLSearchParams();

    for (const [key, value] of Object.entries(search)) {
        if (typeof value === "string" && value !== "") {
            params.set(key, value);
        }
    }

    const value = params.toString();

    return value ? `?${value}` : "";
}

const seedSchema = z.string().regex(/^\d+$/);
const roomSchema = z.string().trim().toUpperCase();

export function validatePracticeSearch(search: Record<string, unknown>): { seed?: string } {
    const seed = seedSchema.safeParse(search.seed);

    return seed.success ? { seed: seed.data } : {};
}

export function validateRoomSearch(search: Record<string, unknown>): { room?: string; seed?: string } {
    const room = roomSchema.safeParse(search.room);

    return { ...validatePracticeSearch(search), ...(room.success && room.data ? { room: room.data } : {}) };
}
