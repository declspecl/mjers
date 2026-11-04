import createMjers from "../../npm/index.js";
import type { Observation } from "../../npm/types.js";
import { writeFileSync } from "node:fs";

const mjers = await createMjers();
const picks: { kind: string; seed: number; step: number; efficiency: unknown; defensive: unknown; observation: Observation }[] = [];
const key = (a: unknown) => JSON.stringify(a);
let played = 0;
for (let seed = 1000; seed < 1128; seed++) {
    const game = new mjers.Game(BigInt(seed), seed % 4);
    const bots = [0, 1, 2, 3].map((s) => new mjers.Bot(s % 2 ? "defensive" : "efficiency", BigInt(seed + s)));
    const fast = new mjers.Bot("efficiency", 1n);
    const safe = new mjers.Bot("defensive", 1n);
    try {
        let step = 0;
        while (!game.isOver() && step < 500) {
            const seat = game.toAct()[0]!;
            const obs = game.observe(seat);
            const efficiency = fast.choose(game, seat);
            const defensive = safe.choose(game, seat);
            if (obs.legal_actions.length > 1) {
                const kong = obs.legal_actions.some(
                    (a) => a === "kong" || (typeof a === "object" && ("concealed_kong" in a || "upgrade_kong" in a)),
                );
                const kind = kong ? "kong" : key(efficiency) !== key(defensive) && obs.decision.kind === "turn" ? "defense" : "";
                if (kind && picks.filter((p) => p.kind === kind).length < 6 && !picks.some((p) => p.seed === seed && p.kind === kind))
                    picks.push({ kind, seed, step, efficiency, defensive, observation: obs });
            }
            game.apply(seat, bots[seat]!.choose(game, seat));
            step++;
        }
        played++;
    } finally {
        game.free();
        fast.free();
        safe.free();
        for (const b of bots) b.free();
    }
}
const results = [];
for (const [index, p] of picks.entries()) {
    const g = mjers.Game.fromObservation(p.observation, 123n);
    const analyses = [];
    try {
        const budgets = [{ samples: 256, seed: 4455n }, ...(index === 0 || index === 1 ? [{ samples: 1024, seed: 7788n }] : [])];
        for (const budget of budgets)
            for (const rollout of ["efficiency", "defensive"]) {
                const a = g.advise(p.observation.seat, { ...budget, rollout, candidates: p.observation.legal_actions.length });
                const leader = a.estimates[0]!;
                analyses.push({
                    samples: budget.samples,
                    rollout,
                    ranked: a.ranked,
                    estimates: a.estimates.map((e) => ({
                        action: e.action,
                        ev: e.ev,
                        stderr: e.ev_stderr,
                        versusBest: leader.comparisons?.find((c) => key(c.action) === key(e.action)),
                    })),
                });
            }
        results.push({ ...p, analyses });
        process.stderr.write(`${index + 1}/${picks.length} ${p.kind} done\n`);
    } finally {
        g.free();
    }
}
writeFileSync(new URL("../.cache/policy-edge-experiment.json", import.meta.url), JSON.stringify({ played, results }, null, 2));
