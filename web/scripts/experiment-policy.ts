/** Real-position evaluator diagnostic, not expert-labelled ground truth. */
import createMjers from "../../npm/index.js";
import type { Advice, Observation } from "../../npm/types.js";
import { writeFileSync } from "node:fs";

const mjers = await createMjers();
const positions: { seed: number; step: number; observation: Observation }[] = [];
const hands = [];
for (const seed of [101, 202, 303, 404, 505, 606]) {
    const game = new mjers.Game(BigInt(seed), seed % 4);
    const bots = [0, 1, 2, 3].map((seat) => new mjers.Bot(seat % 2 ? "defensive" : "efficiency", BigInt(seed + seat)));
    let turns = 0;
    let claims = 0;
    let step = 0;
    try {
        while (!game.isOver() && step < 500) {
            const seat = game.toAct()[0]!;
            const obs = game.observe(seat);
            if (obs.legal_actions.length > 1) {
                if (
                    obs.decision.kind === "claim" &&
                    claims < 1 &&
                    positions.filter((p) => p.observation.decision.kind === "claim").length < 4
                ) {
                    positions.push({ seed, step, observation: obs });
                    claims++;
                } else if (obs.decision.kind === "turn" && turns < 2 && ((turns === 0 && step >= 20) || (turns === 1 && step >= 70))) {
                    positions.push({ seed, step, observation: obs });
                    turns++;
                }
            }
            game.apply(seat, bots[seat]!.choose(game, seat));
            step++;
        }
        hands.push({ seed, steps: step, result: game.result(), settlement: game.settlement() });
    } finally {
        game.free();
        for (const bot of bots) bot.free();
    }
}
function summarize(advice: Advice) {
    const leader = advice.estimates[0]!;
    const first = advice.ranked[0]!;
    const top = advice.ranked.find((a) => JSON.stringify(a.action) === JSON.stringify(leader.action))!;

    return {
        recommended: advice.recommended,
        leader: leader.action,
        leaderShape: top,
        staticLeader: first,
        staticLeaderLoss: leader.comparisons?.find((c) => JSON.stringify(c.action) === JSON.stringify(first.action)),
        candidates: advice.ranked.map((a) => {
            const e = advice.estimates.find((entry) => JSON.stringify(entry.action) === JSON.stringify(a.action))!;

            return {
                ...a,
                ev: e.ev,
                stderr: e.ev_stderr,
                samples: e.samples,
                winRate: e.win_rate,
                dealInRate: e.deal_in_rate,
                versusStatic: e.comparisons?.find((c) => JSON.stringify(c.action) === JSON.stringify(first.action)),
            };
        }),
    };
}
const results = [];
for (const [index, position] of positions.entries()) {
    const sampled = mjers.Game.fromObservation(position.observation, 987n);
    try {
        const configs = [{ samples: 256, seed: 112_233n }, ...(index % 4 === 0 ? [{ samples: 1024, seed: 998_877n }] : [])];
        const analyses = [];
        for (const config of configs)
            for (const rollout of ["efficiency", "defensive"]) {
                const start = performance.now();
                const advice = sampled.advise(position.observation.seat, {
                    ...config,
                    rollout,
                    candidates: position.observation.legal_actions.length,
                });
                analyses.push({
                    rollout,
                    samples: config.samples,
                    analysisSeed: String(config.seed),
                    elapsedMs: performance.now() - start,
                    ...summarize(advice),
                });
            }
        results.push({ ...position, analyses });
        process.stderr.write(`position ${index + 1}/${positions.length} ${position.observation.decision.kind} done\n`);
    } finally {
        sampled.free();
    }
}
writeFileSync(
    new URL("../.cache/policy-experiment.json", import.meta.url),
    JSON.stringify(
        {
            note: "Same hidden-state samples per analysis seed; independent deeper seed on every fourth position. Self and opponent policies both change. No expert labels.",
            hands,
            results,
        },
        null,
        2,
    ),
);
