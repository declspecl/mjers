import type { Mjers } from "../engine/engine";

interface HandViewProps {
    mjers: Mjers;
    seed: bigint;
}

/** Deals one hand and shows the first seat to act, as the engine reports it. */
export function HandView({ mjers, seed }: HandViewProps) {
    const game = new mjers.Game(seed, 0);

    try {
        const seat = game.toAct()[0];

        if (seat === undefined) {
            return <p>The hand is already over.</p>;
        }

        return (
            <section>
                <h2>Seat {seat} to act</h2>
                <pre>{JSON.stringify(game.observe(seat), undefined, 2)}</pre>
            </section>
        );
    } finally {
        game.free();
    }
}
