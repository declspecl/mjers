import { grade, type GradeResult } from "./grade";
import type { Action, Advice } from "@mjers/mjers/types";

/** Live coaching is a single observation-only assessment. Full hand simulations
 * remain offline diagnostics until their hidden-state model is validated.
 */
export function analyzeMove(advise: (samples: number) => Advice, chosen: Action): GradeResult {
    return grade(advise(0), chosen);
}
