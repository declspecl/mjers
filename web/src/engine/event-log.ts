import { describeAction, describeMeld } from "./names";
import type { Action, Observation, Seat } from "@mjers/mjers/types";

export interface LogEvent {
    id: number;
    seat: Seat;
    text: string;
}

export interface EventLog {
    events: LogEvent[];
    /** How many melds each seat showed after the last move, to spot claims that took effect. */
    melds: number[];
}

export function createLog(): EventLog {
    return { events: [], melds: [0, 0, 0, 0] };
}

function push(log: EventLog, seat: Seat, text: string) {
    log.events.push({ id: (log.events.at(-1)?.id ?? 0) + 1, seat, text });
}

/**
 * Logs what a move did. Call it after the engine applied the move, with the players as the table shows them now.
 * A claim is a bid and may lose to a higher one, so it is logged only once it wins, when the claimer's melds grow.
 */
export function recordMove(log: EventLog, players: Observation["players"], seat: Seat, action: Action) {
    const text = describeAction(action);

    if (text !== undefined) {
        push(log, seat, text);
    }

    const concealedKong = typeof action === "object" && "concealed_kong" in action;

    for (const player of players) {
        const before = log.melds[player.seat] ?? 0;
        const meld = player.melds.at(-1);

        log.melds[player.seat] = player.melds.length;

        if (meld && player.melds.length > before && !(concealedKong && player.seat === seat)) {
            push(log, player.seat, describeMeld(meld));
        }
    }
}
