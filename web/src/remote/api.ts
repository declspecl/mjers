import { httpBase } from "./config";
import type { CreateRequest, JoinRequest, RoomError, SeatGrant } from "./protocol";
import { createRequestSchema, joinRequestSchema, seatGrantSchema, errorResponseSchema } from "./schemas";

export class RoomRequestError extends Error {
    readonly room: RoomError;

    constructor(room: RoomError) {
        super(room.message);
        this.room = room;
    }
}

function describeStatus(status: number, message: string | undefined): RoomError {
    if (status === 429) {
        return { code: "rate_limited", status, message: "Too many requests. Wait a minute, then try again." };
    }

    if (status === 404) {
        return { code: "invalid_room", status, message: "Room not found. Check the invite code." };
    }

    return { code: "http", status, message: message ?? `The server answered ${status}. Try again in a moment.` };
}

/** One request, never retried: create and join are not idempotent. */
async function post(path: string, body: unknown): Promise<SeatGrant> {
    let response: Response;

    try {
        response = await fetch(`${httpBase()}${path}`, {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify(body),
        });
    } catch {
        throw new RoomRequestError({
            code: "network",
            message: "Could not reach the server. Check your connection; if a room was created, it may already exist.",
        });
    }

    let data: unknown;

    try {
        data = await response.json();
    } catch {
        data = undefined;
    }

    if (!response.ok) {
        const parsed = errorResponseSchema.safeParse(data);
        const error = parsed.success ? parsed.data : undefined;
        if (error && response.status !== 429) {
            throw new RoomRequestError({ code: error.code, message: error.message, status: response.status });
        }
        throw new RoomRequestError(describeStatus(response.status, error?.message));
    }

    const parsed = seatGrantSchema.safeParse(data);
    if (!parsed.success) {
        throw new RoomRequestError({
            code: "http",
            message: "The server returned invalid room details. Check your saved rooms before trying again.",
        });
    }

    return parsed.data;
}

export function createRoom(request: CreateRequest): Promise<SeatGrant> {
    return post("/tables", createRequestSchema.parse(request));
}

export function joinRoom(code: string, request: JoinRequest): Promise<SeatGrant> {
    return post(`/tables/${encodeURIComponent(code)}/join`, joinRequestSchema.parse(request));
}
