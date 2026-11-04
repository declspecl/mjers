/** Held-out full-hand coverage and latency; this does not establish expert accuracy. */
import createMjers from "../../npm/index.js";
import { analyzeMove } from "../src/grading/analyze";
import { grade } from "../src/grading/grade";
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";

const mjers = await createMjers();
const policies = ["efficiency", "defensive", "random"];
const counts: Record<string, Record<string, number>> = Object.fromEntries(policies.map((name) => [name, {}]));
const latency: number[] = [];
const examples = [];
let decisions = 0;
let viewChecks = 0;
for (let seed = 90_000; seed < 90_256; seed++) {
    const game = new mjers.Game(BigInt(seed), seed % 4);
    const bots = policies.map((policy, index) => new mjers.Bot(policy, BigInt(seed * 3 + index)));
    try {
        for (let step = 0; step < 1000 && !game.isOver(); step++) {
            const seat = game.toAct()[0]!;
            const observation = game.observe(seat);
            if (observation.legal_actions.length > 1) {
                const started = performance.now();
                const advice = game.advise(seat, { samples: 0 });
                const actions = bots.map((bot) => bot.choose(game, seat));
                const results = actions.map((action) =>
                    analyzeMove((samples) => {
                        assert.equal(samples, 0);

                        return advice;
                    }, action),
                );
                latency.push(performance.now() - started);
                decisions++;
                for (const [i, policy] of policies.entries()) {
                    const result = results[i]!;
                    const tally = counts[policy!]!;
                    tally[result.grade] = (tally[result.grade] ?? 0) + 1;
                    assert.ok(result.summary && result.reason && result.basis);
                    assert.notEqual(result.grade, "Unclear");
                }
                if (step % 17 === 0) {
                    const reconstructed = mjers.Game.fromObservation(observation, 888n);
                    try {
                        const remoteAdvice = reconstructed.advise(seat, { samples: 0 });
                        assert.deepEqual(grade(remoteAdvice, actions[0]!), results[0]);
                        viewChecks++;
                    } finally {
                        reconstructed.free();
                    }
                }
                if (examples.length < 12 && results[2]!.grade !== results[0]!.grade) {
                    examples.push({
                        seed,
                        step,
                        actions,
                        feedback: results.map(({ grade, basis, summary }) => ({ grade, basis, summary })),
                    });
                }
                // Play all the way to settlement, alternating strong and random tables.
                game.apply(seat, actions[seed % 3 === 0 ? 2 : 0]!);
            } else {
                game.apply(seat, observation.legal_actions[0]!);
            }
        }
        assert.ok(game.isOver());
    } finally {
        game.free();
        for (const bot of bots) bot.free();
    }
}
latency.sort((a, b) => a - b);
const report = {
    seeds: [90_000, 90_255],
    completeHands: 256,
    decisions,
    viewChecks,
    note: "Labels describe observable hand progress/value, not human-expert optimality. Timing includes assessment and three policy choices on this desktop.",
    milliseconds: { median: latency[Math.floor(latency.length / 2)], p95: latency[Math.floor(latency.length * 0.95)], max: latency.at(-1) },
    counts,
    examples,
};
await mkdir(".cache", { recursive: true });
await writeFile(".cache/live-feedback-experiment.json", JSON.stringify(report, null, 2));
process.stdout.write(`${JSON.stringify({ ...report, examples: undefined }, null, 2)}\n`);
