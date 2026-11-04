import createMjers from "../../npm/index.js";
import { writeFileSync } from "node:fs";

const m = await createMjers();
const positions = [];
const key = (a: unknown) => JSON.stringify(a);
for (let seed = 1000; seed < 1128; seed++) {
    const g = new m.Game(BigInt(seed), seed % 4);
    const bots = [0, 1, 2, 3].map((s) => new m.Bot(s % 2 ? "defensive" : "efficiency", BigInt(seed + s)));
    const f = new m.Bot("efficiency", 1n),
        d = new m.Bot("defensive", 1n);
    try {
        let step = 0;
        while (!g.isOver() && step < 500) {
            const seat = g.toAct()[0]!;
            const obs = g.observe(seat);
            const fast = f.choose(g, seat),
                safe = d.choose(g, seat);
            if (obs.decision.kind === "turn" && key(fast) !== key(safe) && positions.length < 4) {
                const rank = g.advise(seat, { samples: 0 }).ranked;
                const a = rank.find((x) => key(x.action) === key(fast))!,
                    b = rank.find((x) => key(x.action) === key(safe))!;
                if (b.shanten > a.shanten && !positions.some((x) => x.seed === seed)) positions.push({ seed, step, obs, fast, safe, rank });
            }
            g.apply(seat, bots[seat]!.choose(g, seat));
            step++;
        }
    } finally {
        g.free();
        f.free();
        d.free();
        for (const b of bots) b.free();
    }
}
const results = [];
for (const p of positions) {
    const g = m.Game.fromObservation(p.obs, 1n);
    const analyses = [];
    try {
        for (const rollout of ["efficiency", "defensive"]) {
            const a = g.advise(p.obs.seat, { samples: 512, candidates: p.obs.legal_actions.length, rollout, seed: 6633n });
            const fast = a.estimates.find((e) => key(e.action) === key(p.fast))!;
            analyses.push({
                rollout,
                fastToSafe: fast.comparisons?.find((c) => key(c.action) === key(p.safe)),
                estimates: a.estimates.map((e) => ({ action: e.action, ev: e.ev, deal: e.deal_in_rate, win: e.win_rate })),
            });
        }
        results.push({ ...p, analyses });
    } finally {
        g.free();
    }
}
writeFileSync(new URL("../.cache/fold-experiment.json", import.meta.url), JSON.stringify({ results }, null, 2));
