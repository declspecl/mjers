import type { Action, Advice, Assessment } from "@mjers/mjers/types";

export const GRADES = ["Excellent", "Good", "Okay", "Bad", "Terrible", "Tradeoff", "Unclear"] as const;
export type Grade = (typeof GRADES)[number];

export interface GradeResult {
    grade: Grade;
    best: Action;
    /** These labels describe observable progress/value, not proven optimal play. */
    basis?: "Hand progress" | "Kong value" | "Winning hand" | "Safety tradeoff";
    /** Brief, actionable explanation for the mobile toast. */
    summary?: string;
    reason: string;
    loss?: number;
    lossRange?: [number, number];
    certainty?: "resolved" | "severity" | "unresolved";
}

export function sameAction(a: Action, b: Action): boolean {
    return JSON.stringify(a) === JSON.stringify(b);
}

function isWin(action: Action): boolean {
    return action === "win" || action === "self_draw_win";
}

function discarded(action: Action): string | undefined {
    return typeof action === "object" && "discard" in action ? action.discard : undefined;
}

function kongTile(action: Action): string | undefined {
    if (typeof action !== "object") return undefined;
    if ("concealed_kong" in action) return action.concealed_kong;
    if ("upgrade_kong" in action) return action.upgrade_kong;

    return undefined;
}

/** Immediate coaching from visible hand consequences. Experiments found full-hand
 * rollouts too slow and biased by uniform hidden-hand sampling for live labels.
 * Shanten and improving copies are facts about hand progress, not expected returns.
 * Payments and meaningful safety compensation are explicit tradeoffs, never silently
 * treated as proof that damaged efficiency was either correct or a blunder.
 */
export function grade(advice: Advice, chosen: Action): GradeResult {
    const win = advice.ranked.find((entry) => isWin(entry.action));
    const mine = advice.ranked.find((entry) => sameAction(entry.action, chosen));
    const result = (grade: Grade, best: Action, basis: GradeResult["basis"], summary: string, reason = summary): GradeResult => ({
        grade,
        best,
        basis,
        summary,
        reason,
    });

    if (!mine) {
        return result("Unclear", advice.recommended, undefined, "This decision could not be assessed.");
    }
    if (win) {
        return isWin(chosen)
            ? result("Excellent", chosen, "Winning hand", "You took the win.")
            : result("Terrible", win.action, "Winning hand", "You passed up a winning hand.");
    }
    if (advice.ranked.length === 1) {
        return result("Unclear", chosen, undefined, "Only one legal move; no choice to grade.");
    }

    const ordinary = advice.ranked.filter((entry) => entry.immediate_units === 0);
    const best = ordinary[0] ?? mine;
    const paid = advice.ranked.filter((entry) => entry.immediate_units > 0);
    const payment = paid.reduce<Assessment | undefined>(
        (leader, entry) => (!leader || entry.immediate_units > leader.immediate_units ? entry : leader),
        undefined,
    );

    if (mine.immediate_units > 0) {
        const cost = mine.shanten - best.shanten;
        const hurtsProgress = ordinary.length > 0 && (cost > 0 || (cost === 0 && mine.live_improving < best.live_improving));
        const summary = `Secures ${mine.immediate_units} units in kong payments.`;

        return result(
            hurtsProgress ? "Tradeoff" : "Excellent",
            chosen,
            "Kong value",
            summary,
            `${summary}${hurtsProgress ? " It gives up hand efficiency in exchange for the payment; the tradeoff is not judged as a mistake." : " It preserves the strongest immediate hand progress."}`,
        );
    }

    const brokenKong = paid.find((entry) => discarded(chosen) !== undefined && discarded(chosen) === kongTile(entry.action));
    if (brokenKong) {
        return result(
            "Bad",
            brokenKong.action,
            "Kong value",
            `Breaks a kong worth ${brokenKong.immediate_units} units.`,
            `Discarding the fourth tile gives up a ${brokenKong.immediate_units}-unit kong payment. These payments count even if the hand ends without a winner.`,
        );
    }

    // Delaying a concealed kong while keeping all four tiles does not forfeit it.
    // A discard claim or an upgrade opportunity cannot be treated the same way.
    if (payment?.action === "kong") {
        return result(
            "Tradeoff",
            payment.action,
            "Kong value",
            `Passes on a ${payment.immediate_units}-unit kong payment.`,
            `This keeps your chosen hand structure instead of taking the kong payment. Hand efficiency alone cannot settle that tradeoff.`,
        );
    }

    const deferred = paid.find((entry) => kongTile(entry.action) !== undefined && discarded(chosen) !== undefined);
    if (deferred) {
        return result(
            "Tradeoff",
            chosen,
            "Kong value",
            `Keeps a ${deferred.immediate_units}-unit kong available for later.`,
            `You retain the fourth tile instead of declaring the kong now. Delaying the payment is not the same as losing it; your chosen discard leaves ${mine.shanten} shanten and ${mine.live_improving} unseen improving copies.`,
        );
    }

    const steps = mine.shanten - best.shanten;
    const safer = mine.danger + 0.1 < best.danger && best.danger >= 0.3;
    if (safer && (steps > 0 || mine.live_improving < best.live_improving)) {
        return result(
            "Tradeoff",
            chosen,
            "Safety tradeoff",
            "Gives up hand progress for a safer-looking discard.",
            `The move lowers the engine's rough risk estimate but ${steps > 0 ? `leaves the hand ${steps} extra step${steps === 1 ? "" : "s"} from ready` : `reduces improving copies from ${best.live_improving} to ${mine.live_improving}`}. Safety estimates are approximate; this is not automatically a mistake.`,
        );
    }
    if (steps > 0) {
        return result(
            steps >= 2 ? "Terrible" : "Bad",
            best.action,
            "Hand progress",
            best.shanten === 0 ? "Breaks a ready hand." : `Leaves the hand ${steps} extra step${steps === 1 ? "" : "s"} from ready.`,
            `Another legal move leaves the hand at ${best.shanten} shanten instead of ${mine.shanten}. This measures hand progress, not a predicted settlement loss.`,
        );
    }

    const saferEquivalent = ordinary.find(
        (entry) => entry.shanten <= mine.shanten && entry.live_improving >= mine.live_improving && entry.danger + 0.05 < mine.danger,
    );
    if (saferEquivalent) {
        return result(
            "Okay",
            saferEquivalent.action,
            "Safety tradeoff",
            "Another discard looks safer with the same progress.",
            "You can keep at least as many improving copies and the same distance from ready with a lower estimated discard risk. The risk estimate is approximate.",
        );
    }
    if (best.live_improving === 0) {
        return result("Okay", best.action, "Hand progress", "No unseen tile currently improves this hand.");
    }

    const kept = mine.live_improving / best.live_improving;
    const level: Grade = kept >= 0.9 ? "Excellent" : kept >= 0.7 ? "Good" : kept >= 0.4 ? "Okay" : "Bad";
    const summary =
        kept >= 0.9
            ? "Preserves nearly all improving copies."
            : `Keeps ${mine.live_improving} improving copies instead of ${best.live_improving}.`;

    return result(
        level,
        best.action,
        "Hand progress",
        summary,
        `${mine.shanten === 0 ? "The hand stays ready" : `The hand stays at ${mine.shanten} shanten`}, with ${mine.live_improving} unseen improving copies versus ${best.live_improving}. Similar efficiency can still lead to different long-term outcomes.`,
    );
}
