import type { GradeResult } from "./grade";
import type { Action, Observation, Seat } from "@mjers/mjers/types";

export interface GradeRequest {
    type: "grade";
    session: number;
    hand: number;
    /** Moves applied before this decision, which names it within the hand. */
    decision: number;
    candidates: number;
    chosen: Action;
    observation?: Observation;
}

export type GraderMessage =
    | { type: "start"; session: number; seed: bigint; dealer: Seat }
    | { type: "apply"; session: number; seat: Seat; action: Action }
    | GradeRequest;

export type GraderReply =
    | { type: "graded"; session: number; hand: number; decision: number; action: Action; result: GradeResult }
    | { type: "failed"; session: number; hand: number; decision: number; error: string };
