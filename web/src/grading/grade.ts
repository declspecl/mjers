import type { Action, Advice, Assessment, Estimate } from "@mjers/mjers/types";

export const GRADES = ["Perfect", "Good", "Okay", "Bad", "Terrible"] as const;
export type Grade = (typeof GRADES)[number];

export interface GradeResult {
    grade: Grade;
    /** The move the engine liked best. */
    best: Action;
    /** Expected units lost against the best move, after allowing for playout noise. Undefined without playouts. */
    loss?: number;
    reason: string;
}

export function sameAction(a: Action, b: Action): boolean {
    return JSON.stringify(a) === JSON.stringify(b);
}

/** Units of expected loss, beyond noise, that each grade allows. */
const LOSS_STEPS = [0.15, 0.4, 0.8, 1.5];

function byLoss(loss: number): number {
    const index = LOSS_STEPS.findIndex((step) => loss <= step);

    return index === -1 ? 4 : index;
}

/** The instant ranking: how far the move set the hand back and how many winning chances it gave up. */
function byShape(chosen: Assessment, best: Assessment): number {
    const steps = chosen.shanten - best.shanten;

    if (steps >= 2) {
        return 4;
    }

    if (steps === 1) {
        return 3;
    }

    const kept = best.live_improving === 0 ? 1 : chosen.live_improving / best.live_improving;

    if (kept >= 0.9 && chosen.danger <= best.danger + 0.05) {
        return 0;
    }

    return kept >= 0.7 ? 1 : 2;
}

function isWin(action: Action): boolean {
    return action === "win" || action === "self_draw_win";
}

/**
 * Grades a move against the advisor. Playout estimates are paired, so the loss is the gap in mean units to the best candidate,
 * shrunk by one standard error so a coin flip between near-equal moves is not called a mistake. The instant ranking catches
 * moves that break the hand's shape, and either signal saying the move was fine is enough.
 */
export function grade(advice: Advice, chosen: Action): GradeResult {
    const best = advice.recommended;

    if (sameAction(best, chosen)) {
        return { grade: "Perfect", best, loss: 0, reason: "The engine's choice." };
    }

    const winAvailable = advice.ranked.find((entry) => isWin(entry.action));

    if (winAvailable && !isWin(chosen)) {
        return { grade: "Terrible", best: winAvailable.action, reason: "You passed up a winning hand." };
    }

    const estimates: Estimate[] = advice.estimates;
    const mine = estimates.find((entry) => sameAction(entry.action, chosen));
    const top = estimates.reduce<Estimate | undefined>(
        (leader, entry) => (leader === undefined || entry.difference > leader.difference ? entry : leader),
        undefined,
    );
    const rankedBest = advice.ranked[0];
    const rankedMine = advice.ranked.find((entry) => sameAction(entry.action, chosen));
    const shape = rankedBest && rankedMine ? byShape(rankedMine, rankedBest) : 2;

    if (mine && top) {
        const gap = top.difference - mine.difference;
        const noise = Math.max(mine.difference_stderr, top.difference_stderr);
        const loss = Math.max(0, gap - noise);
        const level = Math.min(byLoss(loss), shape);

        return {
            grade: GRADES[level]!,
            best: top.action,
            loss,
            reason: `About ${gap.toFixed(2)} units worse on average over ${mine.samples} playouts.`,
        };
    }

    return { grade: GRADES[shape]!, best, reason: "Judged by hand shape alone." };
}
