import createMjers, { type Action, type MjersModule } from "@mjers/mjers";
import type { Advice, Analysis, Observation, Ruleset, Seat, Settlement } from "@mjers/mjers/types";
import * as typeExports from "@mjers/mjers/types";

const mjers: MjersModule = await createMjers();
const rules: Ruleset = mjers.ruleset("family");
const game = new mjers.Game(42n, 0, rules);
const bot = new mjers.Bot("efficiency", 0n);
const seat: Seat = game.toAct()[0];
const observation: Observation = game.observe(seat);
const actions: Action[] = game.legalActions(seat);
const advice: Advice = game.advise(seat, { samples: 0, seed: 1n });
const analysis: Analysis = mjers.analyze(observation.hand.concealed, 0, observation.public_tiles);
const settlement: Settlement | null = game.settlement();
try {
    const choice: Action = bot.choose(game, seat);
    game.apply(seat, advice.recommended);
    if (Object.keys(typeExports).length !== 0) throw new Error("types export must be empty at runtime");
    if (mjers.analyze("123m456p78s11122z").shanten !== 0) throw new Error("expected a ready hand");
    void choice;
} finally {
    bot.free();
    game.free();
}

// These examples are type-checked but never executed by the installed consumer.
function invalidInputs() {
    // @ts-expect-error seeds must retain u64 precision
    new mjers.Game(42, 0);
    // @ts-expect-error actions must match the engine's tagged encoding
    game.apply(seat, { discard: 5 });
    // @ts-expect-error options are typed
    game.advise(seat, { samplez: 0 });
}
void [actions, analysis, settlement];
