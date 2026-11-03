import type { LogEvent } from "../engine/event-log";
import { describeMeld, tileName } from "../engine/names";
import type { PublicEvent } from "./protocol";

/** Log text for a resolved public event, already turned to view seats. Draws are left out: they say nothing about the tile. */
export function describeEvent(event: PublicEvent): Omit<LogEvent, "id"> | undefined {
    switch (event.event) {
        case "bonus": {
            return { seat: event.seat, text: `set aside ${tileName(event.bonus)}` };
        }
        case "discard": {
            return { seat: event.seat, text: `discarded ${tileName(event.tile)}` };
        }
        case "claim": {
            return { seat: event.seat, text: describeMeld(event.meld) };
        }
        case "kong": {
            const tile = "kong" in event.meld ? event.meld.kong[0] : "pong" in event.meld ? event.meld.pong : event.meld.chow;
            const concealed = "kong" in event.meld && event.meld.kong[1] === "concealed";

            return {
                seat: event.seat,
                text: concealed ? `declared a concealed kong of ${tileName(tile)}` : `added to a pong to make a kong of ${tileName(tile)}`,
            };
        }
        case "win": {
            return { seat: event.seat, text: event.source === "SelfDraw" ? "won by self-draw" : `won on ${tileName(event.tile)}` };
        }
        case "exhausted": {
            return { seat: 0, text: "saw the wall run out" };
        }
        default: {
            return undefined;
        }
    }
}
