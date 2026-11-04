/** Real-hand stability/latency diagnostic; deeper same-policy simulations are not expert ground truth. */
import createMjers from "../../npm/index.js";
import type { Action, Advice, Observation } from "../../npm/types.js";
import { writeFileSync, mkdirSync } from "node:fs";

const mjers = await createMjers();
const key = (a: Action) => JSON.stringify(a);
const positions: { seed: number; step: number; observation: Observation }[] = [];
const hands = [];
let claims = 0;
for (const seed of [7, 42, 123, 601]) {
    const game = new mjers.Game(BigInt(seed), 0);
    const bots = [0, 1, 2, 3].map((s) => new mjers.Bot("efficiency", BigInt(seed + s)));
    const taken = new Set<number>();
    let step = 0;
    const tail: { seed: number; step: number; observation: Observation }[] = [];
    try {
        while (!game.isOver() && step < 1000) {
            const seat = game.toAct()[0]!;
            const observation = game.observe(seat);
            const bucket = step < 25 ? 0 : step < 80 ? 1 : 2;
            if (observation.legal_actions.length > 1) tail.push({ seed, step, observation });
            if (observation.legal_actions.length > 1 && (!taken.has(bucket) || (observation.decision.kind === "claim" && claims < 3))) {
                positions.push({ seed, step, observation });
                taken.add(bucket);
                if (observation.decision.kind === "claim") claims++;
            }
            game.apply(seat, bots[seat]!.choose(game, seat));
            step++;
        }
        for (const position of [tail.at(-3), tail.at(-1)])
            if (position && !positions.some((p) => p.seed === seed && p.step === position.step)) positions.push(position);
        hands.push({ seed, decisions: step, completed: game.isOver(), result: game.result() });
    } finally {
        game.free();
        for (const b of bots) b.free();
    }
}
function regret(advice: Advice, chosen: Action, z: number) {
    const mine = advice.estimates.find((e) => key(e.action) === key(chosen))!;
    const top = advice.estimates.reduce((a, b) => (a.ev > b.ev ? a : b));
    let lower = 0,
        upper = 0;
    for (const entry of advice.estimates) {
        const pair = entry.comparisons!.find((c) => key(c.action) === key(chosen))!;
        lower = Math.max(lower, pair.difference - z * pair.stderr);
        upper = Math.max(upper, pair.difference + z * pair.stderr);
    }

    return { loss: top.ev - mine.ev, lower, upper };
}
function structural(advice: Advice, chosen: Action) {
    const mine = advice.ranked.find((e) => key(e.action) === key(chosen))!;
    const win = advice.ranked.find((e) => e.action === "win" || e.action === "self_draw_win");
    if (win) return { grade: chosen === "win" || chosen === "self_draw_win" ? "Excellent" : "Terrible", why: "available win" };
    const kong = advice.ranked.find((e) => e.immediate_units > 0);
    if (mine.immediate_units > 0) return { grade: "Good", why: "paid kong" };
    if (kong) return { grade: "Okay", why: "declined paid kong" };
    const discards = advice.ranked.filter((e) => typeof e.action === "object" && "discard" in e.action);
    if (typeof chosen !== "object" || !("discard" in chosen)) return { grade: "Okay", why: "claim tradeoff" };
    const min = Math.min(...discards.map((e) => e.shanten));
    const bestLive = Math.max(...discards.filter((e) => e.shanten === min).map((e) => e.live_improving));
    const steps = mine.shanten - min;
    const ratio = bestLive === 0 ? 1 : mine.live_improving / bestLive;
    const safer = discards.find(
        (e) => e.shanten === mine.shanten && e.live_improving >= mine.live_improving && e.danger + 0.05 < mine.danger,
    );

    return {
        grade:
            steps >= 2 ? "Terrible" : steps === 1 ? "Bad" : ratio >= 0.9 ? "Good" : ratio >= 0.7 ? "Okay" : ratio >= 0.4 ? "Okay" : "Bad",
        why: "discard efficiency",
        steps,
        ratio,
        safer: safer?.action,
        danger: mine.danger,
    };
}
/** Freeze the pre-experiment exact-band rule so future production edits do not change the baseline. */
function grade(advice: Advice, chosen: Action) {
    const win = advice.ranked.find((entry) => entry.action === "win" || entry.action === "self_draw_win");
    if (win) return { grade: chosen === "win" || chosen === "self_draw_win" ? "Excellent" : "Terrible" };
    const range = regret(advice, chosen, 3);
    const bands = [0.15, 0.4, 0.8, 1.5];
    const level = (loss: number) => {
        const found = bands.findIndex((step) => loss <= step);

        return found === -1 ? 4 : found;
    };
    const same = level(range.lower) === level(range.upper);

    return {
        grade: same ? ["Excellent", "Good", "Okay", "Bad", "Terrible"][level(range.lower)]! : range.lower > 0.4 ? "Mistake" : "Unclear",
    };
}

function coarse(advice: Advice, chosen: Action, z: number) {
    const r = regret(advice, chosen, z);

    return { ...r, label: r.lower > 0.4 && r.loss > 0.8 ? "Clear mistake" : r.loss > 0.4 ? "Possible mistake" : "No significant loss" };
}
const cases = [];
for (const [index, p] of positions.entries()) {
    const game = mjers.Game.fromObservation(p.observation, BigInt(index + 900));
    try {
        const candidates = p.observation.legal_actions.length;
        const fast: Record<number, Advice> = {};
        const timings: Record<number, number> = {};
        for (const samples of [16, 32, 64]) {
            const start = performance.now();
            fast[samples] = game.advise(p.observation.seat, { candidates, samples, seed: 9001n });
            timings[samples] = performance.now() - start;
        }
        const start = performance.now();
        const reference = game.advise(p.observation.seat, { candidates, samples: 768, seed: 765_432n });
        timings[768] = performance.now() - start;
        const ranked = reference.ranked;
        const seen = new Set<string>();
        for (const assessment of [ranked[0]!, ranked[Math.floor(ranked.length / 2)]!, ranked.at(-1)!]) {
            const chosen = assessment.action;
            if (seen.has(key(chosen))) continue;
            seen.add(key(chosen));
            cases.push({
                seed: p.seed,
                step: p.step,
                decision: p.observation.decision,
                wall: p.observation.live_wall,
                candidates,
                chosen,
                assessment,
                timings,
                reference: regret(reference, chosen, 2.5),
                structural: structural(reference, chosen),
                estimates: [16, 32, 64].map((samples) => ({
                    samples,
                    current: grade(fast[samples]!, chosen),
                    coarse1: coarse(fast[samples]!, chosen, 1),
                    coarse164: coarse(fast[samples]!, chosen, 1.64),
                    pointLoss: regret(fast[samples]!, chosen, 0).loss,
                })),
            });
        }
        process.stderr.write(`Position ${index + 1}/${positions.length} complete\n`);
    } finally {
        game.free();
    }
}
const summary = [16, 32, 64].map((samples) => {
    const records = cases.map((c) => ({ reference: c.reference, ...c.estimates.find((e) => e.samples === samples)! }));

    return {
        samples,
        total: records.length,
        currentUnclear: records.filter((e) => e.current.grade === "Unclear").length,
        currentClearMistakes: records.filter((e) => ["Bad", "Terrible", "Mistake"].includes(e.current.grade)).length,
        pointFalsePraise: records.filter((e) => e.pointLoss <= 0.4 && e.reference.lower > 0.8).length,
        coarse: (["coarse1", "coarse164"] as const).map((name) => ({
            name,
            clear: records.filter((e) => e[name].label === "Clear mistake").length,
            possible: records.filter((e) => e[name].label === "Possible mistake").length,
            noSignificantLoss: records.filter((e) => e[name].label === "No significant loss").length,
            falseClear: records.filter((e) => e[name].label === "Clear mistake" && e.reference.upper < 0.4).length,
            clearNotConfirmed: records.filter((e) => e[name].label === "Clear mistake" && e.reference.lower <= 0.4).length,
            missedReferenceClear: records.filter((e) => e[name].label !== "Clear mistake" && e.reference.lower > 0.8).length,
        })),
        medianMs: positions
            .map((p) => cases.find((c) => c.seed === p.seed && c.step === p.step)!.timings[samples]!)
            .toSorted((a, b) => a - b)[Math.floor(positions.length / 2)],
    };
});
mkdirSync(new URL("../.cache/", import.meta.url), { recursive: true });
writeFileSync(
    new URL("../.cache/feedback-experiment.json", import.meta.url),
    JSON.stringify(
        {
            note: "Same efficiency rollout; independent reference seed, not expert ground truth. Moves within positions are correlated.",
            hands,
            positions: positions.length,
            summary,
            cases,
        },
        null,
        2,
    ),
);
process.stdout.write(JSON.stringify(summary, null, 2) + "\n");
