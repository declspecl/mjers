import createMjers, { type Action, type MjersModule } from "@mjers/mjers";
import type { Advice, Analysis, Observation, Ruleset, Seat, Settlement } from "@mjers/mjers/types";

createMjers() satisfies Promise<MjersModule>;

declare const mjers: MjersModule;

const rules = mjers.ruleset("family") satisfies Ruleset;
const game = new mjers.Game(42n, 0, rules);
const bot = new mjers.Bot("efficiency", 0n);

const seat = game.toAct()[0] satisfies Seat;
const observation = game.observe(seat) satisfies Observation;

const advice = game.advise(seat, { samples: 0, seed: 1n }) satisfies Advice;
const choice = bot.choose(game, seat) satisfies Action;

game.legalActions(seat) satisfies Action[];
mjers.analyze(observation.hand.concealed, 0, observation.public_tiles) satisfies Analysis;
game.settlement() satisfies Settlement | null;

game.apply(seat, choice);
game.apply(seat, advice.recommended);

// @ts-expect-error seeds must retain u64 precision
new mjers.Game(42, 0);

// @ts-expect-error actions must match the engine's tagged encoding
game.apply(seat, { discard: 5 });

// @ts-expect-error options are typed
game.advise(seat, { samplez: 0 });
