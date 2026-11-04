/// <reference lib="webworker" />
import { analyzeMove } from "./analyze";
import type { GradeRequest, GraderMessage } from "./protocol";
import createMjers from "@mjers/mjers";

type Engine = Awaited<ReturnType<typeof createMjers>>;

// eslint-disable-next-line unicorn/prefer-global-this -- a worker's global scope is typed through `self`.
const scope = self as DedicatedWorkerGlobalScope;
let game: InstanceType<Engine["Game"]> | undefined;
let queue: Promise<void> = Promise.resolve();

/** Mirrors the hand by replaying every move, then assesses your decisions without blocking the table. */
async function handle(message: GraderMessage) {
    const mjers = await createMjers();

    if (message.type === "start") {
        game?.free();
        game = new mjers.Game(message.seed, message.dealer);

        return;
    }

    if (message.type === "apply") {
        game?.apply(message.seat, message.action);

        return;
    }

    const request: GradeRequest = message;

    let sampled: InstanceType<Engine["Game"]> | undefined;

    try {
        sampled = request.observation ? mjers.Game.fromObservation(request.observation, BigInt(request.decision)) : undefined;
        const current = sampled ?? game;

        if (!current) {
            return;
        }
        const result = analyzeMove(
            (samples) =>
                current.advise(request.observation?.seat ?? 0, { samples, candidates: request.candidates, seed: BigInt(request.decision) }),
            request.chosen,
        );

        scope.postMessage({
            type: "graded",
            session: request.session,
            decision: request.decision,
            hand: request.hand,
            action: request.chosen,
            result,
        });
    } catch (error) {
        scope.postMessage({
            type: "failed",
            session: request.session,
            decision: request.decision,
            hand: request.hand,
            error: String(error),
        });
    } finally {
        sampled?.free();
    }
}

scope.addEventListener("message", (event: MessageEvent<GraderMessage>) => {
    queue = queue
        .then(() => handle(event.data))
        .catch((error: unknown) => {
            scope.postMessage({ type: "failed", session: event.data.session, decision: -1, hand: -1, error: String(error) });
        });
});
