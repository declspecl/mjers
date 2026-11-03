import type { Action, Advice } from "@mjers/mjers/types";

export const GRADES = ["Excellent", "Good", "Okay", "Bad", "Terrible", "Mistake", "Unclear"] as const;
export type Grade = (typeof GRADES)[number];

export interface GradeResult {
    grade: Grade;
    /** The move with the highest estimated return, or the advisor's fallback. */
    best: Action;
    /** Estimated expected units lost against the best simulated move, without subtracting noise. */
    loss?: number;
    lossRange?: [number, number];
    /** Simulation precision, not a guarantee that the rollout policy is correct. */
    certainty?: "resolved" | "severity" | "unresolved";
    reason: string;
}

export function sameAction(a: Action, b: Action): boolean {
    return JSON.stringify(a) === JSON.stringify(b);
}

/** Provisional loss thresholds in settlement units; these still need calibration against reviewed positions. */
const LOSS_STEPS = [0.15, 0.4, 0.8, 1.5];

function byLoss(loss: number): number {
    const index = LOSS_STEPS.findIndex((step) => loss <= step);

    return index === -1 ? 4 : index;
}

function isWin(action: Action): boolean {
    return action === "win" || action === "self_draw_win";
}

/**
 * Grade settlement regret from observation-only simulations. Prefer direct paired
 * errors; older engines use a conservative sum of baseline-relative errors.
 * Three standard errors allow more caution for multiple candidates and repeated
 * analysis. These ranges remain approximate, not calibrated confidence intervals.
 */
export function grade(advice: Advice, chosen: Action): GradeResult {
    const winAvailable = advice.ranked.find((entry) => isWin(entry.action));

    if (winAvailable && !isWin(chosen)) {
        return { grade: "Terrible", best: winAvailable.action, reason: "You passed up a winning hand." };
    }

    if (isWin(chosen) && winAvailable) {
        return { grade: "Excellent", best: chosen, reason: "You took the winning hand." };
    }

    if (advice.ranked.length === 1 && sameAction(advice.ranked[0]!.action, chosen)) {
        return { grade: "Unclear", best: chosen, reason: "Only one legal move; there was no decision to grade." };
    }

    const estimates = advice.estimates;
    const mine = estimates.find((entry) => sameAction(entry.action, chosen));
    const top = estimates.reduce<(typeof estimates)[number] | undefined>(
        (leader, entry) => (leader === undefined || entry.difference > leader.difference ? entry : leader),
        undefined,
    );
    const complete = advice.ranked.every((entry) => estimates.some((estimate) => sameAction(estimate.action, entry.action)));

    if (
        !mine ||
        !top ||
        !complete ||
        estimates.some(
            (entry) =>
                entry.samples < 2 ||
                !Number.isFinite(entry.difference) ||
                !Number.isFinite(entry.difference_stderr) ||
                entry.difference_stderr < 0,
        )
    ) {
        return { grade: "Unclear", best: advice.recommended, reason: "Not enough simulation evidence to grade this move." };
    }

    let lower = 0;
    let upper = 0;

    for (const alternative of estimates) {
        if (sameAction(alternative.action, chosen)) {
            continue;
        }

        const paired = alternative.comparisons?.find((entry) => sameAction(entry.action, chosen));
        const valid = paired && Number.isFinite(paired.difference) && Number.isFinite(paired.stderr) && paired.stderr >= 0;
        const gap = valid ? paired.difference : alternative.difference - mine.difference;
        const margin = 3 * (valid ? paired.stderr : alternative.difference_stderr + mine.difference_stderr);
        lower = Math.max(lower, gap - margin);
        upper = Math.max(upper, gap + margin);
    }

    const loss = Math.max(0, top.difference - mine.difference);
    const level = byLoss(lower);
    const certain = level === byLoss(upper);
    const mistake = lower > LOSS_STEPS[1]!;
    const qualification = certain
        ? ""
        : mistake
          ? " This move loses value, but its exact severity is uncertain."
          : " More analysis is needed to establish the quality of this move.";

    return {
        grade: certain ? GRADES[level]! : mistake ? "Mistake" : "Unclear",
        certainty: certain ? "resolved" : mistake ? "severity" : "unresolved",
        lossRange: [lower, upper],
        best: top.action,
        loss,
        reason: `Estimated loss: ${loss.toFixed(2)} units per hand; approximate range ${lower.toFixed(2)}–${upper.toFixed(2)} over ${mine.samples} playouts.${qualification}`,
    };
}
