import type { Action, Observation } from "@mjers/mjers/types";

/** Capture the decision before submission; never analyze a post-move or revealed hand. */
export interface PendingGrade {
    hand: number;
    decision: number;
    observation: Observation;
    chosen: Action;
}

/** Other players can answer the same claim window while our move is in flight.
 * Wait until our own hand or decision changes, not merely the room version.
 * Rejections and disconnects discard the pending request in the connection hook.
 */
export function confirmsMove(pending: PendingGrade, hand: number, steps: number, next: Observation): boolean {
    if (hand !== pending.hand || steps <= pending.decision) {
        return false;
    }

    return (
        !next.to_act.includes(next.seat) ||
        JSON.stringify(next.hand) !== JSON.stringify(pending.observation.hand) ||
        JSON.stringify(next.decision) !== JSON.stringify(pending.observation.decision)
    );
}
