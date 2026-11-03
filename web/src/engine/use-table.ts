import { useGrader } from "../grading/use-grader";
import { layout } from "../table/layout";
import { advance, arrange, createModel, reveal, SEATS, type TableModel, takeDraw, wallBreak } from "../table/model";
import { randomInt } from "../table/random";
import { type Frame, setupFrames } from "../table/setup";
import type { Mjers } from "./engine";
import { createLog, type EventLog, type LogEvent, recordMove } from "./event-log";
import { HUMAN } from "./names";
import { prefersReducedMotion, type Settings } from "./settings";
import type { Action, Advice, Hand, HandResult, Observation, Seat, Settlement } from "@mjers/mjers/types";
import { useCallback, useEffect, useRef, useState } from "react";

type GameHandle = InstanceType<Mjers["Game"]>;
type BotHandle = InstanceType<Mjers["Bot"]>;

/** A bot's turn is slow enough to follow; claim responses are not worth waiting for. */
const TURN_DELAY_MS = 700;
const CLAIM_DELAY_MS = 90;

export type Phase = "opening" | "play" | "over";

export interface TableState {
    hand: number;
    seed: bigint;
    dealer: Seat;
    /** The dealer's two dice: their total picks the setter and how many stacks the setter counts. */
    roll: number[];
    model: TableModel;
    observation: Observation;
    humanToAct: boolean;
    phase: Phase;
    /** The opening sequence, played before the first move. */
    frames: Frame[];
    /** Moves applied so far this hand; names each decision for grading. */
    moves: number;
    result: HandResult | null;
    settlement: Settlement | null;
    winnerHand: Hand | undefined;
    events: LogEvent[];
}

export interface MatchState {
    hand: number;
    baseSeed: bigint;
    dealer: Seat | null;
    totals: number[];
}

interface Live {
    game: GameHandle;
    bots: Map<Seat, BotHandle>;
    log: EventLog;
}

function startingSeed(): bigint {
    const given = new URLSearchParams(globalThis.location.search).get("seed");

    return given !== null && /^\d+$/.test(given) ? BigInt(given) : BigInt(Math.floor(Math.random() * 1_000_000));
}

function rollDice(state: { random: number }): number[] {
    return [0, 0].map(() => 1 + randomInt(state, 6));
}

function sum(values: number[]): number {
    return values.reduce((total, value) => total + value, 0);
}

/** Plays a match: you at seat 0 and the engine's bots at the other three, with the dealer passing round every hand. */
export function useTable(mjers: Mjers, settings: Settings) {
    const [match, setMatch] = useState<MatchState>(() => ({ hand: 0, baseSeed: startingSeed(), dealer: null, totals: [0, 0, 0, 0] }));
    const [table, setTable] = useState<TableState>();
    const [advice, setAdvice] = useState<Advice>();
    const [error, setError] = useState<string>();
    const live = useRef<Live>(undefined);
    const current = useRef<TableState>(undefined);
    const grader = useGrader();
    const { start: startGrader, apply: mirror, request, choose } = grader;

    const commit = useCallback((next: TableState) => {
        current.current = next;
        setTable(next);
    }, []);

    useEffect(() => {
        const seed = match.baseSeed + BigInt(match.hand);
        const dice = { random: Number(seed % 2_147_483_647n) || 1 };
        const dealer = match.dealer ?? (randomInt(dice, 4) as Seat);
        const roll = rollDice(dice);
        const game = new mjers.Game(seed, dealer);
        const bots = new Map(
            SEATS.filter((seat) => seat !== HUMAN).map((seat) => [seat, new mjers.Bot("efficiency", seed * 4n + BigInt(seat))] as const),
        );
        const observation = game.observe(HUMAN);
        const { model, plan } = createModel(observation, wallBreak(dealer, sum(roll)), dice.random);
        const final = layout(model, { order: model.hands[0]!, drawn: null, selected: null });
        const animate = settings.opening && !prefersReducedMotion();

        live.current = { game, bots, log: createLog() };
        startGrader(seed, dealer);
        commit({
            hand: match.hand,
            seed,
            dealer,
            roll,
            model,
            observation,
            humanToAct: game.toAct().includes(HUMAN),
            phase: animate ? "opening" : "play",
            frames: animate ? setupFrames(model, plan, roll, final, { random: dice.random ^ 0x5e_ed }) : [],
            moves: 0,
            result: null,
            settlement: null,
            winnerHand: undefined,
            events: [],
        });
        setAdvice(undefined);
        setError(undefined);

        return () => {
            live.current = undefined;
            game.free();

            for (const bot of bots.values()) {
                bot.free();
            }
        };
        // Settings only shape how a hand opens; changing them mid-hand must not redeal.
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [mjers, match.hand, match.baseSeed, commit, startGrader]);

    const apply = useCallback(
        (seat: Seat, action: Action, discardId?: number) => {
            const state = current.current;
            const handle = live.current;

            if (!state || !handle || state.phase !== "play") {
                return;
            }

            try {
                handle.game.apply(seat, action);
                mirror(seat, action);

                if (seat === HUMAN) {
                    choose(state.hand, state.moves, action);
                }

                const observation = handle.game.observe(HUMAN);
                const over = handle.game.isOver();
                let model = advance(state.model, observation, { discardId });

                recordMove(handle.log, observation.players, seat, action);

                const result = handle.game.result();
                const settlement = handle.game.settlement();

                if (over) {
                    const win = result?.win ?? null;

                    model = reveal(
                        model,
                        SEATS.map((other) => handle.game.observe(other).hand),
                        win?.winner ?? null,
                        win !== null && win.source !== "SelfDraw",
                    );

                    if (settlement) {
                        setMatch((previous) => ({
                            ...previous,
                            totals: previous.totals.map((total, index) => total + (settlement.total[index] ?? 0)),
                        }));
                    }
                }

                commit({
                    ...state,
                    model,
                    observation,
                    humanToAct: handle.game.toAct().includes(HUMAN),
                    phase: over ? "over" : "play",
                    moves: state.moves + 1,
                    result,
                    settlement,
                    winnerHand: result?.win ? handle.game.observe(result.win.winner).hand : undefined,
                    events: handle.log.events.slice(-14),
                });
                setAdvice(undefined);
                setError(undefined);
            } catch (error_) {
                setError(String(error_));
            }
        },
        [choose, commit, mirror],
    );

    useEffect(() => {
        const handle = live.current;

        if (!table || !handle || table.phase !== "play" || table.model.pending.length > 0) {
            return;
        }

        const seat = handle.game.toAct().find((candidate) => candidate !== HUMAN);
        const bot = seat === undefined ? undefined : handle.bots.get(seat);

        if (seat === undefined || !bot) {
            return;
        }

        const claiming = handle.game.observe(seat).decision.kind === "claim";
        const timer = setTimeout(() => apply(seat, bot.choose(handle.game, seat)), claiming ? CLAIM_DELAY_MS : TURN_DELAY_MS);

        return () => clearTimeout(timer);
    }, [table, apply]);

    useEffect(() => {
        if (!table || table.phase !== "play" || !table.humanToAct || !settings.feedback || table.observation.legal_actions.length < 2) {
            return;
        }

        request(table.hand, table.moves, table.observation.legal_actions.length);
    }, [table, settings.feedback, request]);

    const update = useCallback(
        (change: (model: TableModel) => TableModel) => {
            const state = current.current;

            if (state) {
                commit({ ...state, model: change(state.model) });
            }
        },
        [commit],
    );

    const finishOpening = useCallback(() => {
        const state = current.current;

        if (state?.phase === "opening") {
            commit({ ...state, phase: "play", frames: [] });
        }
    }, [commit]);

    const draw = useCallback(() => update(takeDraw), [update]);
    const move = useCallback((id: number, index: number) => update((model) => arrange(model, id, index)), [update]);
    const act = useCallback((action: Action, discardId?: number) => apply(HUMAN, action, discardId), [apply]);

    const hint = useCallback(() => {
        try {
            setAdvice(live.current?.game.advise(HUMAN, { samples: 0 }));
        } catch (error_) {
            setError(String(error_));
        }
    }, []);

    const nextHand = useCallback(() => {
        setMatch((previous) => ({ ...previous, hand: previous.hand + 1, dealer: (((current.current?.dealer ?? 0) + 1) % 4) as Seat }));
    }, []);

    return {
        match,
        table,
        advice,
        error,
        graded: grader.graded,
        gradingFailure: grader.failure,
        act,
        draw,
        move,
        hint,
        finishOpening,
        nextHand,
    };
}
