/// <reference lib="webworker" />
import type { GradeRequest, GraderMessage } from "./protocol";
import createMjers from "@mjers/mjers";

type Engine = Awaited<ReturnType<typeof createMjers>>;

// eslint-disable-next-line unicorn/prefer-global-this -- a worker's global scope is typed through `self`.
const scope = self as DedicatedWorkerGlobalScope;
let game: InstanceType<Engine["Game"]> | undefined;
let queue: Promise<void> = Promise.resolve();

/** Mirrors the hand by replaying every move, then runs playouts for your decisions without blocking the table. */
async function handle(message: GraderMessage) {
    const mjers = await createMjers();

    if (message.type === "start") {
        game?.free();
        game = new mjers.Game(message.seed, message.dealer);

        return;
    }

    if (!game) {
        return;
    }

    if (message.type === "apply") {
        game.apply(message.seat, message.action);

        return;
    }

    const request: GradeRequest = message;

    try {
        const advice = game.advise(0, { samples: request.samples, candidates: request.candidates, seed: BigInt(request.decision) });

        scope.postMessage({ type: "advice", decision: request.decision, hand: request.hand, advice });
    } catch (error) {
        scope.postMessage({ type: "failed", decision: request.decision, hand: request.hand, error: String(error) });
    }
}

scope.addEventListener("message", (event: MessageEvent<GraderMessage>) => {
    queue = queue
        .then(() => handle(event.data))
        .catch((error: unknown) => {
            scope.postMessage({ type: "failed", decision: -1, hand: -1, error: String(error) });
        });
});
