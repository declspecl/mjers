/** Actual hidden hands are used only in this offline sampling diagnostic. */
import createMjers from "../../npm/index.js";
import { mkdir, writeFile } from "node:fs/promises";

const mjers = await createMjers();
const buckets = Object.fromEntries(
    ["early", "middle", "late"].map((name) => [
        name,
        {
            actual: { count: 0, shanten: 0, ready: 0 },
            sampled: { count: 0, shanten: 0, ready: 0 },
        },
    ]),
);
for (let seed = 6000; seed < 8048; seed++) {
    const game = new mjers.Game(BigInt(seed), seed % 4);
    const bots = [0, 1, 2, 3].map((seat) => new mjers.Bot("efficiency", BigInt(seed * 4 + seat)));
    const seen = new Set<string>();
    try {
        for (let step = 0; step < 600 && !game.isOver(); step++) {
            const seat = game.toAct()[0]!;
            const observation = game.observe(seat);
            const band = observation.live_wall > 55 ? "early" : observation.live_wall > 25 ? "middle" : "late";
            if (!seen.has(band) && observation.decision.kind === "turn") {
                seen.add(band);
                const bucket = buckets[band]!;
                const record = (position: typeof game, kind: "actual" | "sampled") => {
                    for (const other of [0, 1, 2, 3].filter((other) => other !== seat)) {
                        const hand = position.observe(other).hand;
                        const shanten = mjers.analyze(hand.concealed, hand.melds.length).shanten;
                        bucket[kind].count++;
                        bucket[kind].shanten += shanten;
                        bucket[kind].ready += Number(shanten === 0);
                    }
                };
                record(game, "actual");
                for (let sample = 0; sample < 8; sample++) {
                    const sampled = mjers.Game.fromObservation(observation, BigInt(seed * 100 + sample));
                    try {
                        record(sampled, "sampled");
                    } finally {
                        sampled.free();
                    }
                }
            }
            game.apply(seat, bots[seat]!.choose(game, seat));
        }
    } finally {
        game.free();
        for (const bot of bots) bot.free();
    }
}
const summary = Object.fromEntries(
    Object.entries(buckets).map(([name, groups]) => [
        name,
        Object.fromEntries(
            Object.entries(groups).map(([kind, group]) => [
                kind,
                {
                    count: group.count,
                    meanShanten: group.shanten / group.count,
                    readyFraction: group.ready / group.count,
                },
            ]),
        ),
    ]),
);
await mkdir(".cache", { recursive: true });
await writeFile(".cache/hidden-state-experiment.json", JSON.stringify({ seeds: [6000, 8047], rollout: "efficiency", summary }, null, 2));
process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
