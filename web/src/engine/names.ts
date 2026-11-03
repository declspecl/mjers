import { meldTiles, parseTile } from "./tiles";
import type { Action, Meld, Seat, Tile } from "@mjers/mjers/types";

export const HUMAN: Seat = 0;

const SEAT_NAMES = ["You", "Right bot", "Across bot", "Left bot"] as const;
const WINDS = ["East", "South", "West", "North"] as const;
const SUITS: Record<string, string> = { m: "characters", p: "dots", s: "bamboo" };
const HONORS = ["East wind", "South wind", "West wind", "North wind", "White dragon", "Green dragon", "Red dragon"];
const FLOWERS = ["Plum", "Orchid", "Chrysanthemum", "Bamboo flower"];
const SEASONS = ["Spring", "Summer", "Autumn", "Winter"];

let customNames: readonly string[] | null = null;

/** Names for a multiplayer table, in view order (you first, then right, across, left). Null restores the practice names. */
export function setSeatNames(names: readonly string[] | null) {
    customNames = names;
}

export function seatName(seat: Seat): string {
    return customNames?.[seat] ?? SEAT_NAMES[seat];
}

export function windName(seat: Seat, dealer: Seat): string {
    return WINDS[(seat - dealer + 4) % 4] ?? "East";
}

export function tileName(tile: Tile): string {
    const { rank, suit } = parseTile(tile);

    if (suit === "z") {
        return HONORS[rank - 1] ?? tile;
    }

    if (suit === "F") {
        return FLOWERS[rank - 1] ?? tile;
    }

    if (suit === "S") {
        return SEASONS[rank - 1] ?? tile;
    }

    return `${rank} ${SUITS[suit] ?? suit}`;
}

/** What a turn action did, for the event log. Claims return nothing: they are bids and may lose to a higher claim. */
export function describeAction(action: Action): string | undefined {
    if (action === "self_draw_win") {
        return "won by self-draw";
    }

    if (typeof action === "string") {
        return undefined;
    }

    if ("discard" in action) {
        return `discarded ${tileName(action.discard)}`;
    }

    if ("concealed_kong" in action) {
        return `declared a concealed kong of ${tileName(action.concealed_kong)}`;
    }

    if ("upgrade_kong" in action) {
        return `added to a pong to make a kong of ${tileName(action.upgrade_kong)}`;
    }

    return undefined;
}

/** A meld that a claim just put on the table. */
export function describeMeld(meld: Meld): string {
    if ("chow" in meld) {
        return `claimed a chow of ${meldTiles(meld)
            .map((tile) => tileName(tile))
            .join(", ")}`;
    }

    if ("pong" in meld) {
        return `claimed a pong of ${tileName(meld.pong)}`;
    }

    return `claimed a kong of ${tileName(meld.kong[0])}`;
}
