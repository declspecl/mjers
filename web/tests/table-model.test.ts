import { expandCounts } from "../src/engine/tiles";
import { advance, breakStackFor, createModel, reveal, SEATS, type TableModel, takeDraw, TILE_COUNT } from "../src/table/model";
import createMjers from "@mjers/mjers";
import type { Observation, Seat } from "@mjers/mjers/types";
import assert from "node:assert/strict";
import test from "node:test";

function placed(model: TableModel): number[] {
    return [
        ...model.hands.flat(),
        ...model.drawn.filter((id): id is number => id !== null),
        ...model.melds.flat().flatMap((meld) => meld.ids),
        ...model.bonus.flat(),
        ...model.pond.map((tile) => tile.id),
        ...model.pending.map((step) => step.id),
    ];
}

function check(model: TableModel, observation: Observation, label: string) {
    const ids = placed(model);

    assert.equal(new Set(ids).size, ids.length, `${label}: a tile is in two places`);
    assert.ok(ids.every((id) => id >= 0 && id < TILE_COUNT));

    for (const seat of SEATS) {
        const player = observation.players[seat];
        const pendingKept = seat === 0 ? model.pending.filter((step) => !step.bonus).length : 0;
        const pendingBonus = seat === 0 ? model.pending.filter((step) => step.bonus).length : 0;

        assert.equal(
            model.hands[seat]!.length + (model.drawn[seat] === null ? 0 : 1) + pendingKept,
            player.concealed_count,
            `${label}: seat ${seat} concealed`,
        );
        assert.equal(model.bonus[seat]!.length + pendingBonus, player.bonus.length, `${label}: seat ${seat} bonus`);
        assert.equal(model.melds[seat]!.length, player.melds.length, `${label}: seat ${seat} melds`);
    }

    const mine = [
        ...model.hands[0]!,
        ...(model.drawn[0] === null ? [] : [model.drawn[0]!]),
        ...model.pending.filter((step) => !step.bonus).map((step) => step.id),
    ];

    assert.deepEqual(
        mine.map((id) => model.faces[id]).toSorted(),
        expandCounts(observation.hand.concealed).toSorted(),
        `${label}: your faces`,
    );
    assert.equal(ids.length + observation.wall_remaining, TILE_COUNT, `${label}: wall count`);
    assert.equal(model.pond.length, observation.discards.filter((discard) => !discard.claimed).length, `${label}: pond`);
}

test("the table follows every tile through whole hands", async () => {
    const mjers = await createMjers();

    for (let hand = 0; hand < 60; hand++) {
        const seed = BigInt(1000 + hand);
        const dealer = (hand % 4) as Seat;
        const game = new mjers.Game(seed, dealer);
        const bots = SEATS.map((seat) => new mjers.Bot(hand % 2 === 0 ? "efficiency" : "random", seed * 4n + BigInt(seat)));
        let { model } = createModel(game.observe(0), breakStackFor(dealer, 3 + (hand % 16)), hand);

        try {
            check(model, game.observe(0), `hand ${hand} deal`);

            for (let step = 0; !game.isOver() && step < 500; step++) {
                while (model.pending.length > 0) {
                    model = takeDraw(model);
                }

                const seat = game.toAct()[0]!;
                const action = bots[seat]!.choose(game, seat);
                const discardId =
                    seat === 0 && typeof action === "object" && "discard" in action
                        ? model.hands[0]!.find((id) => model.faces[id] === action.discard)
                        : undefined;

                game.apply(seat, action);
                model = advance(model, game.observe(0), { discardId });
                if (!game.isOver()) check(model, game.observe(0), `hand ${hand} step ${step} seat ${seat} ${JSON.stringify(action)}`);
            }

            const result = game.result();
            const winner = result?.win?.winner ?? null;
            const fromDiscard = result?.win !== null && result?.win !== undefined && result.win.source !== "SelfDraw";

            model = reveal(
                model,
                SEATS.map((seat) => game.observe(seat).hand),
                winner,
                fromDiscard,
            );

            for (const seat of SEATS) {
                const ids = [...model.hands[seat]!, ...(model.drawn[seat] === null ? [] : [model.drawn[seat]!])];

                assert.deepEqual(
                    ids.map((id) => model.faces[id]).toSorted(),
                    expandCounts(game.observe(seat).hand.concealed).toSorted(),
                    `hand ${hand} reveal ${seat}`,
                );
            }
        } finally {
            game.free();
            for (const bot of bots) bot.free();
        }
    }
});
