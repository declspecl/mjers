import type { SeatGrant } from "./protocol";
import { savedSeatSchema } from "./schemas";
import { z } from "zod";
import { createStore } from "zustand/vanilla";

export type SavedSeat = z.infer<typeof savedSeatSchema>;
type SeatStorage = Pick<Storage, "getItem" | "setItem">;
const KEY = "mjers.rooms";
const MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000;
const rawSeatsSchema = z.record(z.string(), z.unknown());

function readAll(storage: SeatStorage): Record<string, SavedSeat> {
    try {
        const raw = rawSeatsSchema.safeParse(JSON.parse(storage.getItem(KEY) ?? "{}"));
        if (!raw.success) {
            return {};
        }
        const seats: Record<string, SavedSeat> = {};
        for (const value of Object.values(raw.data)) {
            const parsed = savedSeatSchema.safeParse(value);
            if (parsed.success && Date.now() - parsed.data.saved < MAX_AGE_MS) {
                seats[parsed.data.table] = parsed.data;
            }
        }

        return seats;
    } catch {
        return {};
    }
}

interface SeatState {
    seats: Record<string, SavedSeat>;
    persisted: boolean;
    refresh: () => void;
    remember: (seat: SavedSeat) => boolean;
    forget: (table: string) => void;
}

/** Keeps identities reactive and in memory even if persistence is unavailable.
 * The existing storage format is retained so deployed seats continue to work. */
export function createSeatStore(storage: SeatStorage) {
    return createStore<SeatState>((set, get) => {
        function commit(seats: Record<string, SavedSeat>): boolean {
            let persisted = true;
            try {
                storage.setItem(KEY, JSON.stringify(seats));
            } catch {
                persisted = false;
            }
            set({ seats, persisted });

            return persisted;
        }

        return {
            seats: readAll(storage),
            persisted: true,
            refresh: () => set({ seats: get().persisted ? readAll(storage) : { ...readAll(storage), ...get().seats } }),
            remember: (seat) => commit({ ...get().seats, [seat.table]: seat }),
            forget: (table) => {
                const seats = { ...get().seats };
                delete seats[normalizeCode(table)];
                commit(seats);
            },
        };
    });
}

export const seatStore = createSeatStore({
    getItem: (key) => globalThis.localStorage.getItem(key),
    setItem: (key, value) => globalThis.localStorage.setItem(key, value),
});

if (typeof globalThis.addEventListener === "function") {
    globalThis.addEventListener("storage", (event: StorageEvent) => {
        if (event.key === KEY || event.key === null) {
            seatStore.getState().refresh();
        }
    });
}

export function normalizeCode(code: string): string {
    return code.trim().toUpperCase();
}
export function savedSeat(table: string): SavedSeat | undefined {
    const seat = seatStore.getState().seats[normalizeCode(table)];

    return seat && Date.now() - seat.saved < MAX_AGE_MS ? seat : undefined;
}
export function savedSeats(): SavedSeat[] {
    return Object.values(seatStore.getState().seats)
        .filter((seat) => Date.now() - seat.saved < MAX_AGE_MS)
        .toSorted((a, b) => b.saved - a.saved);
}
export function saveSeat(grant: SeatGrant, name: string): boolean {
    return seatStore
        .getState()
        .remember({ table: normalizeCode(grant.table), seat: grant.seat, token: grant.seat_token, name, saved: Date.now() });
}
export function forgetSeat(table: string) {
    seatStore.getState().forget(table);
}
export function storageAvailable(): boolean {
    try {
        globalThis.localStorage.setItem("mjers.probe", "1");
        globalThis.localStorage.removeItem("mjers.probe");

        return true;
    } catch {
        return false;
    }
}
