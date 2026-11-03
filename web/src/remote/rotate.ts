import type { PublicEvent } from "./protocol";
import type { Hand, HandResult, Observation, Seat } from "@mjers/mjers/types";

/**
 * The table always draws you at the bottom as seat 0. A multiplayer seat can be any of the four, so server seats turn into view
 * seats before the table model sees them. Actions carry tiles, never seats, so nothing needs turning back.
 */
export function toView(seat: Seat, me: Seat): Seat {
    return ((seat - me + 4) % 4) as Seat;
}

export function fromView(seat: Seat, me: Seat): Seat {
    return ((seat + me) % 4) as Seat;
}

/** Reorders a per-seat array so index 0 is you. */
export function rotateSeats<T>(values: readonly T[], me: Seat): T[] {
    return [0, 1, 2, 3].map((view) => values[fromView(view as Seat, me)]!);
}

export function rotateObservation(observation: Observation, me: Seat): Observation {
    const turn = (seat: Seat) => toView(seat, me);
    const decision =
        observation.decision.kind === "claim" ? { ...observation.decision, from: turn(observation.decision.from) } : observation.decision;

    return {
        ...observation,
        seat: turn(observation.seat),
        dealer: turn(observation.dealer),
        players: rotateSeats(observation.players, me).map((player) => ({ ...player, seat: turn(player.seat) })) as Observation["players"],
        discards: observation.discards.map((discard) => ({ ...discard, seat: turn(discard.seat) })),
        to_act: observation.to_act.map((seat) => turn(seat)),
        decision,
    };
}

export function rotateResult(result: HandResult, me: Seat): HandResult {
    const win = result.win && {
        ...result.win,
        winner: toView(result.win.winner, me),
        source: result.win.source === "SelfDraw" ? result.win.source : { Discard: toView(result.win.source.Discard, me) },
    };

    return { ...result, win, kongs: result.kongs.map((kong) => ({ ...kong, owner: toView(kong.owner, me) })) };
}

export function rotateHands(hands: readonly Hand[], me: Seat): Hand[] {
    return rotateSeats(hands, me);
}

export function rotateEvent(event: PublicEvent, me: Seat): PublicEvent {
    if (event.event === "exhausted") {
        return event;
    }

    if (event.event === "claim") {
        return { ...event, seat: toView(event.seat, me), from: toView(event.from, me) };
    }

    if (event.event === "win") {
        return {
            ...event,
            seat: toView(event.seat, me),
            source: event.source === "SelfDraw" ? event.source : { Discard: toView(event.source.Discard, me) },
        };
    }

    return { ...event, seat: toView(event.seat, me) };
}
