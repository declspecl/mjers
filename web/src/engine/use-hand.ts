import type { Mjers } from "./engine";
import { describeAction, describeMeld, HUMAN } from "./names";
import type { Action, Advice, Hand, HandResult, Observation, Seat, Settlement } from "@mjers/mjers/types";
import { useCallback, useEffect, useRef, useState } from "react";

type GameHandle = InstanceType<Mjers["Game"]>;
type BotHandle = InstanceType<Mjers["Bot"]>;

const BOT_SEATS: Seat[] = [1, 2, 3];
/** A bot's turn is slow enough to follow; claim responses are not worth waiting for. */
const TURN_DELAY_MS = 650;
const CLAIM_DELAY_MS = 80;

export interface LogEvent {
    id: number;
    seat: Seat;
    text: string;
}

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
    events: LogEvent[];
    /** Melds each seat showed after the last move, to spot claims that took effect. */
    melds: number[];
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
        events: live.events.slice(-10),
    };
}

function log(live: Live, seat: Seat, text: string) {
    live.events.push({ id: (live.events.at(-1)?.id ?? 0) + 1, seat, text });
}

/** Logs what a move did. A claim is only logged once it wins, when the claimer's melds grow. */
function record(live: Live, seat: Seat, action: Action) {
    const text = describeAction(action);

    if (text !== undefined) {
        log(live, seat, text);
    }

    const concealedKong = typeof action === "object" && "concealed_kong" in action;

    for (const player of live.game.observe(HUMAN).players) {
        const before = live.melds[player.seat] ?? 0;
        const meld = player.melds.at(-1);

        live.melds[player.seat] = player.melds.length;

        if (meld && player.melds.length > before && !(concealedKong && player.seat === seat)) {
            log(live, player.seat, describeMeld(meld));
        }
    }
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
        const current: Live = { game, bots, events: [], melds: [0, 0, 0, 0] };

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
            record(current, seat, action);
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
