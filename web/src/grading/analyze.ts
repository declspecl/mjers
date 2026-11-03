import { grade, type GradeResult } from "./grade";
import type { Action, Advice } from "@mjers/mjers/types";

/** A small first pass, then a bounded deeper pass only for unresolved decisions.
 * The caller uses the same seed, so the deeper pass extends the sample prefix.
 * The current engine recomputes that prefix; budgets include that extra work.
 */
export const SAMPLE_BUDGETS = [48, 384] as const;

export function analyzeMove(advise: (samples: number) => Advice, chosen: Action): GradeResult {
    let result: GradeResult | undefined;

    for (const samples of SAMPLE_BUDGETS) {
        result = grade(advise(samples), chosen);

        if (result.grade !== "Unclear" && result.grade !== "Mistake") {
            return result;
        }
    }

    return result!;
}
