import type { Action, Advice, Seat } from "@mjers/mjers/types";

export interface GradeRequest {
    type: "grade";
    hand: number;
    /** Moves applied before this decision, which names it within the hand. */
    decision: number;
    samples: number;
    candidates: number;
}

export type GraderMessage = { type: "start"; seed: bigint; dealer: Seat } | { type: "apply"; seat: Seat; action: Action } | GradeRequest;

export type GraderReply =
    { type: "advice"; hand: number; decision: number; advice: Advice } | { type: "failed"; hand: number; decision: number; error: string };
