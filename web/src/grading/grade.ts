import { assessMove, sameAction } from "./assess-move";
import type { GradeResult as AssessmentResult } from "./assess-move";
import type { Action, Advice } from "@mjers/mjers/types";

export { sameAction } from "./assess-move";
export const GRADES = ["Perfect", "Good", "Okay", "Bad", "Terrible", "Unclear"] as const;
export type Grade = (typeof GRADES)[number];

export interface GradeResult extends Omit<AssessmentResult, "grade"> {
    grade: Grade;
    /** Position in the engine's ordering; its recommendation is always first. */
    rank?: number;
    choices?: number;
}

export interface EvaluatedMove extends GradeResult {
    action: Action;
}

/** Grade all choices against a single engine recommendation. Perfect means
 * matching that recommendation, not a claim of mathematically optimal play.
 * The underlying assessment separates serious damage from acceptable alternatives.
 */
export function evaluateMoves(advice: Advice): EvaluatedMove[] {
    const ordered = advice.ranked.toSorted(
        (a, b) => Number(sameAction(b.action, advice.recommended)) - Number(sameAction(a.action, advice.recommended)),
    );

    return ordered.map((entry, index) => {
        const assessed = assessMove(advice, entry.action);
        const grade: Grade = sameAction(entry.action, advice.recommended)
            ? "Perfect"
            : assessed.grade === "Excellent"
              ? "Good"
              : assessed.grade === "Tradeoff"
                ? "Okay"
                : assessed.grade;

        return { ...assessed, action: entry.action, grade, best: advice.recommended, rank: index + 1, choices: ordered.length };
    });
}

export function grade(advice: Advice, chosen: Action): GradeResult {
    const evaluated = evaluateMoves(advice).find((entry) => sameAction(entry.action, chosen));

    return evaluated ?? { grade: "Unclear", best: advice.recommended, reason: "This decision could not be assessed." };
}
