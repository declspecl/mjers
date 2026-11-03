import type { Mjers } from "./engine";
import { createLog, type EventLog, type LogEvent, recordMove } from "./event-log";
import { HUMAN } from "./names";
import type { Action, Advice, Hand, HandResult, Observation, Seat, Settlement } from "@mjers/mjers/types";
import { useCallback, useEffect, useRef, useState } from "react";

type GameHandle = InstanceType<Mjers["Game"]>;
type BotHandle = InstanceType<Mjers["Bot"]>;

const BOT_SEATS: Seat[] = [1, 2, 3];
/** A bot's turn is slow enough to follow; claim responses are not worth waiting for. */
const TURN_DELAY_MS = 650;
const CLAIM_DELAY_MS = 80;

export interface HandView {
    observation: Observation;
    humanToAct: boolean;
    over: boolean;
    result: HandResult | null;
    settlement: Settlement | null;
    /** The winner's hand, shown once the hand is over. */
    winnerHand: Hand | undefined;
    events: LogEvent[];
}

interface Live {
    game: GameHandle;
    bots: Map<Seat, BotHandle>;
    log: EventLog;
}

function snapshot(live: Live): HandView {
    const { game } = live;
    const result = game.result();

    return {
        observation: game.observe(HUMAN),
        humanToAct: game.toAct().includes(HUMAN),
        over: game.isOver(),
        result,
        settlement: game.settlement(),
        winnerHand: result?.win ? game.observe(result.win.winner).hand : undefined,
        events: live.log.events.slice(-10),
    };
}

/** Plays one hand: the human sits at seat 0 and the engine's bots take the other three. */
export function useHand(mjers: Mjers, seed: bigint, dealer: Seat) {
    const live = useRef<Live>(undefined);
    const [view, setView] = useState<HandView>();
    const [advice, setAdvice] = useState<Advice>();
    const [error, setError] = useState<string>();

    useEffect(() => {
        const game = new mjers.Game(seed, dealer);
        const bots = new Map(BOT_SEATS.map((seat) => [seat, new mjers.Bot("efficiency", seed * 4n + BigInt(seat))] as const));
        const current: Live = { game, bots, log: createLog() };

        live.current = current;
        setView(snapshot(current));
        setAdvice(undefined);
        setError(undefined);

        return () => {
            live.current = undefined;
            game.free();

            for (const bot of bots.values()) {
                bot.free();
            }
        };
    }, [mjers, seed, dealer]);

    const apply = useCallback((seat: Seat, action: Action) => {
        const current = live.current;

        if (!current) {
            return;
        }

        try {
            current.game.apply(seat, action);
            recordMove(current.log, current.game.observe(HUMAN).players, seat, action);
            setAdvice(undefined);
            setError(undefined);
            setView(snapshot(current));
        } catch (error_) {
            setError(String(error_));
        }
    }, []);

    useEffect(() => {
        const current = live.current;

        if (!current || !view || view.over || view.humanToAct) {
            return;
        }

        const seat = current.game.toAct().find((candidate) => candidate !== HUMAN);
        const bot = seat === undefined ? undefined : current.bots.get(seat);

        if (seat === undefined || !bot) {
            return;
        }

        const claiming = current.game.observe(seat).decision.kind === "claim";
        const timer = setTimeout(() => apply(seat, bot.choose(current.game, seat)), claiming ? CLAIM_DELAY_MS : TURN_DELAY_MS);

        return () => clearTimeout(timer);
    }, [view, apply]);

    const act = useCallback((action: Action) => apply(HUMAN, action), [apply]);

    const hint = useCallback(() => {
        try {
            setAdvice(live.current?.game.advise(HUMAN, { samples: 0 }));
        } catch (error_) {
            setError(String(error_));
        }
    }, []);

    return { view, advice, error, act, hint };
}
