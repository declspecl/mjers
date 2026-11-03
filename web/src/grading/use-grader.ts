import { grade, type GradeResult } from "./grade";
import type { GraderMessage, GraderReply } from "./protocol";
import type { Action, Advice, Seat } from "@mjers/mjers/types";
import { useCallback, useEffect, useRef, useState } from "react";

export interface GradedMove {
    hand: number;
    decision: number;
    action: Action;
    result: GradeResult;
}

/** Playouts per candidate move: enough to separate clear mistakes, quick enough to answer within a second or so. */
const SAMPLES = 48;

function key(hand: number, decision: number): string {
    return `${hand}:${decision}`;
}

/** Runs the grading engine in a worker that mirrors the hand, and grades your moves once its advice arrives. */
export function useGrader() {
    const worker = useRef<Worker>(undefined);
    const advice = useRef(new Map<string, Advice>());
    const chosen = useRef(new Map<string, Action>());
    const requested = useRef(new Set<string>());
    const [graded, setGraded] = useState<GradedMove[]>([]);
    const [failure, setFailure] = useState<string>();

    const settle = useCallback((hand: number, decision: number) => {
        const id = key(hand, decision);
        const found = advice.current.get(id);
        const action = chosen.current.get(id);

        if (!found || action === undefined) {
            return;
        }

        chosen.current.delete(id);
        setGraded((list) => [...list, { hand, decision, action, result: grade(found, action) }]);
    }, []);

    useEffect(() => {
        const created = new Worker(new URL("grader.worker.ts", import.meta.url), { type: "module" });

        created.addEventListener("message", (event: MessageEvent<GraderReply>) => {
            const reply = event.data;

            if (reply.type === "failed") {
                setFailure(reply.error);

                return;
            }

            advice.current.set(key(reply.hand, reply.decision), reply.advice);
            settle(reply.hand, reply.decision);
        });
        worker.current = created;

        return () => {
            created.terminate();
            worker.current = undefined;
        };
    }, [settle]);

    const post = useCallback((message: GraderMessage) => worker.current?.postMessage(message), []);

    const start = useCallback(
        (seed: bigint, dealer: Seat) => {
            advice.current.clear();
            chosen.current.clear();
            requested.current.clear();
            setGraded([]);
            post({ type: "start", seed, dealer });
        },
        [post],
    );

    const apply = useCallback((seat: Seat, action: Action) => post({ type: "apply", seat, action }), [post]);

    const request = useCallback(
        (hand: number, decision: number, candidates: number) => {
            const id = key(hand, decision);

            if (requested.current.has(id)) {
                return;
            }

            requested.current.add(id);
            post({ type: "grade", hand, decision, samples: SAMPLES, candidates });
        },
        [post],
    );

    /** Records your move for a decision that was sent for grading; the grade appears when the advice is ready. */
    const choose = useCallback(
        (hand: number, decision: number, action: Action) => {
            if (!requested.current.has(key(hand, decision))) {
                return;
            }

            chosen.current.set(key(hand, decision), action);
            settle(hand, decision);
        },
        [settle],
    );

    return { start, apply, request, choose, graded, failure };
}
