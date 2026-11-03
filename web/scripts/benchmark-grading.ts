/** Run after `npm run build --prefix npm`, from web: `pnpm exec tsx scripts/benchmark-grading.ts`.
 * This is a repeatable evaluator diagnostic, not expert-reviewed ground truth.
 * The independent reference uses the same rollout policy, so shared policy bias remains.
 */
import createMjers from "../../npm/index.js";
import { analyzeMove } from "../src/grading/analyze";
import { grade } from "../src/grading/grade";

const mjers = await createMjers();
const cases = [];

for (const [seed, steps] of [
    [7, 0],
    [42, 0],
    [123, 0],
    [7, 24],
    [42, 24],
    [123, 24],
] as const) {
    const game = new mjers.Game(BigInt(seed), 0);
    const bots = [0, 1, 2, 3].map((seat) => new mjers.Bot("efficiency", BigInt(seat)));

    try {
        for (let step = 0; step < steps && !game.isOver(); step++) {
            const seat = game.toAct()[0]!;
            game.apply(seat, bots[seat]!.choose(game, seat));
        }
        const seat = game.toAct()[0];
        if (seat === undefined) continue;
        const before = game.observe(seat);
        const candidates = before.legal_actions.length;
        if (candidates < 2) continue;
        // Choose from the static ranking before any simulation: avoids selecting
        // benchmark moves based on the reference's sampled outcomes.
        const ranked = game.advise(seat, { samples: 0 }).ranked;
        const chosen = ranked.at(-1)!.action;
        const started = performance.now();
        const result = analyzeMove((samples) => game.advise(seat, { samples, candidates, seed: 991n }), chosen);
        const elapsedMs = performance.now() - started;
        const referenceAdvice = game.advise(seat, { samples: 1536, candidates, seed: 1777n });
        const reference = grade(referenceAdvice, chosen);
        if (JSON.stringify(game.observe(seat)) !== JSON.stringify(before)) throw new Error("Analysis changed the game");
        cases.push({ seed, steps, seat, chosen, result, reference, elapsedMs });
    } finally {
        game.free();
        for (const bot of bots) bot.free();
    }
}

process.stdout.write(
    `${JSON.stringify(
        {
            objective: "expected settlement units",
            rollout: "efficiency",
            referenceSamples: 1536,
            note: "Independent seeds, same evaluator; thresholds remain provisional. This does not establish human playing strength.",
            cases,
        },
        null,
        2,
    )}\n`,
);
