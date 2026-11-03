import type { GradeResult } from "./grade";
import type { GraderMessage, GraderReply } from "./protocol";
import type { Action, Seat } from "@mjers/mjers/types";
import { useCallback, useEffect, useRef, useState } from "react";

export interface GradedMove {
    hand: number;
    decision: number;
    action: Action;
    result: GradeResult;
}

function key(hand: number, decision: number): string {
    return `${hand}:${decision}`;
}

/** Mirrors the hand in a worker; analyze the chosen move before replaying it. */
export function useGrader() {
    const worker = useRef<Worker>(undefined);
    const session = useRef(0);
    const requested = useRef(new Map<string, number>());
    const [graded, setGraded] = useState<GradedMove[]>([]);
    const [failure, setFailure] = useState<string>();

    useEffect(() => {
        const created = new Worker(new URL("grader.worker.ts", import.meta.url), { type: "module" });

        created.addEventListener("message", (event: MessageEvent<GraderReply>) => {
            const reply = event.data;

            if (reply.session !== session.current) {
                return;
            }

            if (reply.type === "failed") {
                setFailure(reply.error);

                return;
            }

            setGraded((list) => [...list, { hand: reply.hand, decision: reply.decision, action: reply.action, result: reply.result }]);
        });
        worker.current = created;

        return () => {
            created.terminate();
            worker.current = undefined;
        };
    }, []);

    const post = useCallback((message: GraderMessage) => worker.current?.postMessage(message), []);

    const start = useCallback(
        (seed: bigint, dealer: Seat) => {
            session.current += 1;
            requested.current.clear();
            setGraded([]);
            setFailure(undefined);
            post({ type: "start", session: session.current, seed, dealer });
        },
        [post],
    );

    const apply = useCallback((seat: Seat, action: Action) => post({ type: "apply", session: session.current, seat, action }), [post]);

    const request = useCallback((hand: number, decision: number, candidates: number) => {
        requested.current.set(key(hand, decision), candidates);
    }, []);

    /** Must be posted before the chosen action is applied to the worker's mirror. */
    const choose = useCallback(
        (hand: number, decision: number, action: Action) => {
            const id = key(hand, decision);
            const candidates = requested.current.get(id);

            if (candidates === undefined) {
                return;
            }

            requested.current.delete(id);
            post({ type: "grade", session: session.current, hand, decision, candidates, chosen: action });
        },
        [post],
    );

    return { start, apply, request, choose, graded, failure };
}
