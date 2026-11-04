import type { LogEvent } from "../engine/event-log";
import { setSeatNames } from "../engine/names";
import { prefersReducedMotion, type Settings } from "../engine/settings";
import type { TableState } from "../engine/use-table";
import { useGrader } from "../grading/use-grader";
import { layout } from "../table/layout";
import {
    advance,
    arrange,
    createModel,
    matches,
    restoreModel,
    reveal,
    sortHand,
    type TableModel,
    takeDraw,
    wallBreak,
} from "../table/model";
import { randomInt } from "../table/random";
import { setupFrames } from "../table/setup";
import { socketUrl } from "./config";
import { describeEvent } from "./events";
import { confirmsMove, type PendingGrade } from "./grading";
import type { SavedSeat } from "./identity";
import type { ClientMessage, ConnectionStatus, PublicEvent, RoomInfo, ServerMessage, StateMessage } from "./protocol";
import { rotateEvent, rotateHands, rotateObservation, rotateResult, rotateSeats } from "./rotate";
import { serverMessageSchema } from "./wire-schema";
import type { Action, Advice, Seat, Settlement } from "@mjers/mjers/types";
import { useCallback, useEffect, useRef, useState } from "react";

/** API Gateway drops idle sockets after ten minutes; ping well inside that. */
const PING_MS = 4 * 60 * 1000;
const BACKOFF_MS = [500, 1000, 2000, 4000, 8000, 15_000, 30_000];
const LOG_LENGTH = 14;

interface Tracker {
    version: number;
    me: Seat;
    table?: TableState;
    log: LogEvent[];
    /** Versions whose events are already in the log, so a repeated update is not logged twice. */
    logged: Set<number>;
    /** The tile you picked, so that same tile leaves your hand when the server confirms the discard. */
    discardId?: number;
    awaiting: boolean;
    pendingGrade?: PendingGrade;
    autoplay: boolean;
}

function freshTable(message: StateMessage, me: Seat, settings: Settings): TableState {
    const observation = rotateObservation(message.observation!, me);
    const cosmetic = { random: Math.floor(Math.random() * 2_147_483_646) + 1 };
    const roll = [0, 0].map(() => 1 + randomInt(cosmetic, 6));
    const wall = wallBreak(observation.dealer, roll[0]! + roll[1]!);
    const fromDeal = message.steps === 0;
    const animate = fromDeal && settings.opening && !prefersReducedMotion();
    let model: TableModel;
    let frames: TableState["frames"] = [];

    if (fromDeal) {
        const created = createModel(observation, wall, cosmetic.random);

        model = created.model;

        if (animate) {
            const final = layout(model, { order: model.hands[0]!, drawn: null, selected: null });

            frames = setupFrames(model, created.plan, roll, final, { random: cosmetic.random ^ 0x5e_ed });
        }
    } else {
        model = restoreModel(observation, wall, cosmetic.random);
    }

    return {
        // Practice counts hands from 0; the server counts from 1.
        hand: message.hand_number - 1,
        seed: 0n,
        dealer: observation.dealer,
        roll,
        model,
        observation,
        humanToAct: false,
        phase: animate ? "opening" : "play",
        frames,
        moves: message.steps,
        result: null,
        settlement: null,
        winnerHand: undefined,
        events: [],
    };
}

function flushDraws(model: TableModel): TableModel {
    let next = model;

    while (next.pending.length > 0) {
        next = takeDraw(next);
    }

    return next;
}

function roomInfo(message: StateMessage, me: Seat): RoomInfo {
    return {
        code: message.table,
        mySeat: me,
        seats: message.seats,
        humanPlayers: message.human_players,
        passwordProtected: message.password_protected,
        scores: message.scores,
        handNumber: message.hand_number,
    };
}

function errorText(code: string, message: string): string {
    switch (code) {
        case "stale": {
            return "The table moved on before your move arrived. Look at the new position and choose again.";
        }
        case "not_your_turn": {
            return "It is not your turn any more. Wait for your next decision.";
        }
        case "bad_token": {
            return `${message} Reconnect to take your seat back, or leave the room.`;
        }
        case "rate_limited": {
            return "Too many requests. Wait a minute, then try again.";
        }
        default: {
            return message;
        }
    }
}

/** Plays a hand held by the multiplayer server. The server owns the game; this only shows its observations for your seat. */
export function useRemoteTable(seat: SavedSeat, settings: Settings) {
    const { reset: resetGrader, snapshot: gradeSnapshot, graded, failure: gradingFailure } = useGrader();
    const [status, setStatus] = useState<ConnectionStatus>("connecting");
    const [room, setRoom] = useState<RoomInfo>();
    const [table, setTable] = useState<TableState>();
    const [advice, setAdvice] = useState<Advice>();
    const [error, setError] = useState<string>();
    const [attempt, setAttempt] = useState(0);
    const socket = useRef<WebSocket>(undefined);
    const tracker = useRef<Tracker>({ version: -1, me: seat.seat, log: [], logged: new Set(), awaiting: false, autoplay: false });
    const settingsRef = useRef(settings);

    settingsRef.current = settings;

    const commit = useCallback((next: TableState | undefined) => {
        tracker.current.table = next;
        setTable(next);
    }, []);

    const send = useCallback((message: ClientMessage): boolean => {
        const open = socket.current;

        if (open?.readyState !== WebSocket.OPEN) {
            return false;
        }

        open.send(JSON.stringify(message));

        return true;
    }, []);

    const humanToAct = useCallback((next: TableState) => {
        const state = tracker.current;

        return next.phase !== "over" && !state.awaiting && !state.autoplay && next.observation.to_act.includes(0);
    }, []);

    const onState = useCallback(
        (message: StateMessage) => {
            const state = tracker.current;
            const me = state.me;
            const seatView = message.seats[me];

            state.autoplay = seatView?.autoplay ?? false;
            setRoom(roomInfo(message, me));
            setSeatNames(rotateSeats(message.seats, me).map((view, index) => (index === 0 ? "You" : (view.name ?? "Open seat"))));

            if (message.observation === null) {
                state.pendingGrade = undefined;
                resetGrader();
                commit(undefined);

                return;
            }

            const previous = state.table;
            let next: TableState;

            if (!previous || previous.hand !== message.hand_number - 1) {
                state.pendingGrade = undefined;
                resetGrader();
                state.log = [];
                state.logged = new Set();
                state.discardId = undefined;
                setAdvice(undefined);
                next = freshTable(message, me, settingsRef.current);
            } else if (message.steps === previous.moves) {
                next = { ...previous, observation: rotateObservation(message.observation, me) };
            } else {
                const observation = rotateObservation(message.observation, me);
                let model =
                    message.steps > previous.moves && !previous.model.revealed
                        ? advance(previous.model, observation, { discardId: state.discardId })
                        : undefined;

                if (!model || !matches(model, observation)) {
                    model = restoreModel(
                        observation,
                        wallBreak(observation.dealer, previous.roll[0]! + previous.roll[1]!),
                        previous.model.random,
                    );
                }

                state.discardId = undefined;
                setAdvice(undefined);
                next = { ...previous, model, observation, moves: message.steps };
            }

            const pending = state.pendingGrade;

            if (pending && confirmsMove(pending, next.hand, next.moves, next.observation)) {
                state.pendingGrade = undefined;
                gradeSnapshot(pending.hand, pending.decision, pending.observation, pending.chosen);
            }

            if (state.autoplay && next.model.pending.length > 0) {
                next = { ...next, model: flushDraws(next.model) };
            }

            state.awaiting = false;
            commit({ ...next, humanToAct: humanToAct(next) });
        },
        [commit, humanToAct, gradeSnapshot, resetGrader],
    );

    const onEvents = useCallback(
        (version: number, handNumber: number, events: PublicEvent[]) => {
            const state = tracker.current;
            const current = state.table;

            if (!current || current.hand !== handNumber - 1 || state.logged.has(version)) {
                return;
            }

            state.logged.add(version);

            for (const event of events) {
                const entry = describeEvent(rotateEvent(event, state.me));

                if (entry) {
                    state.log.push({ id: (state.log.at(-1)?.id ?? 0) + 1, ...entry });
                }
            }

            commit({ ...current, events: state.log.slice(-LOG_LENGTH) });
        },
        [commit],
    );

    const onMessage = useCallback(
        (message: ServerMessage) => {
            const state = tracker.current;

            if ("version" in message) {
                if (message.version < state.version) {
                    return;
                }

                state.version = message.version;
            }

            switch (message.type) {
                case "state": {
                    setStatus("connected");
                    onState(message);
                    break;
                }
                case "events": {
                    onEvents(message.version, message.hand_number, message.events);
                    break;
                }
                case "hand_over": {
                    const current = state.table;

                    if (!current || current.hand !== message.hand_number - 1 || current.phase === "over") {
                        break;
                    }

                    const result = rotateResult(message.result, state.me);
                    const hands = rotateHands(message.revealed, state.me);
                    const win = result.win;
                    const model = reveal(current.model, hands, win?.winner ?? null, win !== null && win.source !== "SelfDraw");
                    const settlement: Settlement = {
                        win: rotateSeats(message.settlement.win, state.me) as Settlement["win"],
                        kongs: rotateSeats(message.settlement.kongs, state.me) as Settlement["kongs"],
                        total: rotateSeats(message.settlement.total, state.me) as Settlement["total"],
                    };

                    commit({
                        ...current,
                        model,
                        phase: "over",
                        frames: [],
                        humanToAct: false,
                        result,
                        settlement,
                        winnerHand: win ? hands[win.winner] : undefined,
                    });
                    break;
                }
                case "hint": {
                    if (state.table?.hand === message.hand_number - 1) {
                        setAdvice(message.advice);
                    }

                    break;
                }
                case "error": {
                    state.pendingGrade = undefined;
                    state.awaiting = false;
                    setError(errorText(message.code, message.message));

                    if (state.table) {
                        commit({ ...state.table, humanToAct: humanToAct(state.table) });
                    }

                    if (message.code === "bad_token") {
                        setStatus("replaced");
                        socket.current?.close(4000, "seat lost");
                    }

                    break;
                }
                case "pong": {
                    break;
                }
            }
        },
        [commit, humanToAct, onEvents, onState],
    );

    useEffect(() => {
        tracker.current = { version: -1, me: seat.seat, log: [], logged: new Set(), awaiting: false, autoplay: false };
        resetGrader();
        commit(undefined);
        setRoom(undefined);

        return () => setSeatNames(null);
    }, [seat.table, seat.seat, commit, resetGrader]);

    useEffect(() => {
        let disposed = false;
        let failures = 0;
        let retry: ReturnType<typeof setTimeout> | undefined;
        let ping: ReturnType<typeof setInterval> | undefined;
        let stopped = false;
        let opened = false;
        const channel = typeof BroadcastChannel === "undefined" ? undefined : new BroadcastChannel("mjers.rooms");
        const tabId = Math.random().toString(36).slice(2);

        const connect = () => {
            if (disposed || stopped) {
                return;
            }

            setStatus(failures === 0 ? "connecting" : "reconnecting");

            const ws = new WebSocket(socketUrl(seat.table, seat.token));

            socket.current = ws;
            ws.addEventListener("open", () => {
                failures = 0;
                opened = true;
                setStatus("syncing");
                ws.send(JSON.stringify({ type: "sync" } satisfies ClientMessage));
                channel?.postMessage({ table: seat.table, seat: seat.seat, tab: tabId });
                clearInterval(ping);
                ping = setInterval(() => ws.readyState === WebSocket.OPEN && ws.send(JSON.stringify({ type: "ping" })), PING_MS);
            });
            ws.addEventListener("message", (event) => {
                try {
                    onMessage(serverMessageSchema.parse(JSON.parse(String(event.data))));
                } catch {
                    // A message the client does not understand; the next sync replaces the state anyway.
                }
            });
            ws.addEventListener("close", () => {
                clearInterval(ping);

                if (socket.current === ws) {
                    socket.current = undefined;
                }

                if (disposed || stopped) {
                    return;
                }

                tracker.current.pendingGrade = undefined;
                tracker.current.awaiting = false;

                if (!opened && failures >= 2) {
                    stopped = true;
                    setStatus("disconnected");
                    setError(
                        "Could not connect to the room. It may have expired or your seat may no longer be valid. Reconnect to try again, or leave the room.",
                    );

                    return;
                }

                setStatus("reconnecting");
                retry = setTimeout(connect, BACKOFF_MS[Math.min(failures, BACKOFF_MS.length - 1)]);
                failures++;
            });
        };

        const stop = (next: ConnectionStatus) => {
            stopped = true;
            clearTimeout(retry);
            clearInterval(ping);
            socket.current?.close(4001, "replaced");
            setStatus(next);
        };

        // Another tab took this seat; stand down rather than take it back and start a tug of war.
        channel?.addEventListener("message", (event: MessageEvent<{ table: string; seat: Seat; tab: string }>) => {
            if (event.data.table === seat.table && event.data.seat === seat.seat && event.data.tab !== tabId) {
                stop("replaced");
            }
        });

        const wake = () => {
            if (!stopped && !disposed && globalThis.document.visibilityState === "visible" && socket.current === undefined) {
                clearTimeout(retry);
                failures = 0;
                connect();
            }
        };

        globalThis.addEventListener("online", wake);
        globalThis.document.addEventListener("visibilitychange", wake);
        connect();

        return () => {
            disposed = true;
            clearTimeout(retry);
            clearInterval(ping);
            channel?.close();
            globalThis.removeEventListener("online", wake);
            globalThis.document.removeEventListener("visibilitychange", wake);
            socket.current?.close(1000, "left");
            socket.current = undefined;
        };
    }, [seat.table, seat.token, seat.seat, attempt, onMessage]);

    const update = useCallback(
        (change: (model: TableModel) => TableModel) => {
            const current = tracker.current.table;

            if (current) {
                commit({ ...current, model: change(current.model) });
            }
        },
        [commit],
    );

    const act = useCallback(
        (action: Action, discardId?: number) => {
            const state = tracker.current;
            const current = state.table;

            if (!current || !current.humanToAct || current.phase !== "play") {
                return;
            }

            if (!send({ type: "act", action, hand_number: current.hand + 1, steps_seen: current.moves })) {
                setError("You are not connected, so the move was not sent. Wait for the connection to come back, then choose again.");

                return;
            }

            state.pendingGrade =
                settingsRef.current.feedback && current.observation.legal_actions.length > 1
                    ? { hand: current.hand, decision: current.moves, observation: current.observation, chosen: action }
                    : undefined;
            state.awaiting = true;
            state.discardId = discardId;
            setError(undefined);
            commit({ ...current, humanToAct: false });
        },
        [commit, send],
    );

    const finishOpening = useCallback(() => {
        const current = tracker.current.table;

        if (current?.phase === "opening") {
            commit({ ...current, phase: "play", frames: [] });
        }
    }, [commit]);

    const setAutoplay = useCallback(
        (enabled: boolean) => {
            const current = tracker.current.table;

            if (
                !send({
                    type: "autoplay",
                    enabled,
                    hand_number: current ? current.hand + 1 : (room?.handNumber ?? 0),
                    steps_seen: current?.moves ?? 0,
                })
            ) {
                setError("You are not connected. Wait for the connection to come back, then try again.");
            }
        },
        [send, room],
    );

    return {
        status,
        room,
        table,
        advice,
        graded,
        gradingFailure,
        error,
        act,
        draw: useCallback(() => update(takeDraw), [update]),
        move: useCallback((id: number, index: number) => update((model) => arrange(model, id, index)), [update]),
        sort: useCallback(() => update(sortHand), [update]),
        finishOpening,
        hint: useCallback(() => {
            if (!send({ type: "hint", samples: 0 })) {
                setError("You are not connected, so no hint was requested.");
            }
        }, [send]),
        start: useCallback(() => send({ type: "start_hand" }), [send]),
        nextHand: useCallback(() => {
            const current = tracker.current.table;

            if (current) {
                send({ type: "next_hand", hand_number: current.hand + 1 });
            }
        }, [send]),
        setAutoplay,
        reconnect: useCallback(() => {
            setError(undefined);
            setAttempt((value) => value + 1);
        }, []),
    };
}
