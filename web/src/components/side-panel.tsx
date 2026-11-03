import { seatName, tileName } from "../engine/names";
import type { HandView } from "../engine/use-hand";
import type { Action, Advice, Seat } from "@mjers/mjers/types";

function actionLabel(action: Action, offered: string | undefined): string {
    if (typeof action === "string") {
        return { self_draw_win: "Declare win", pass: "Pass", pong: "Pong", kong: "Kong", win: `Win with ${offered ?? "the tile"}` }[action];
    }

    if ("discard" in action) {
        return `Discard ${tileName(action.discard)}`;
    }

    if ("chow" in action) {
        return `Chow starting at ${tileName(action.chow)}`;
    }

    if ("concealed_kong" in action) {
        return `Concealed kong of ${tileName(action.concealed_kong)}`;
    }

    return `Make ${tileName(action.upgrade_kong)} a kong`;
}

const SEATS: Seat[] = [0, 1, 2, 3];

/** Rewrites the advisor's shorthand (shanten counts, tile lists, seat numbers) as plain sentences. */
function plainReason(reason: string): string {
    const ready = /^(\d+)-shanten, (\d+) live improving tiles/.exec(reason);

    if (ready?.[1] !== undefined && ready[2] !== undefined) {
        const steps = Number(ready[1]);
        const readiness =
            steps === 0 ? "One tile from winning" : steps < 0 ? "Complete hand" : `${steps} ${steps === 1 ? "step" : "steps"} from ready`;

        return `${readiness}. ${ready[2]} unseen tiles would improve it.`;
    }

    return reason.replace(/^danger ([\d.]+)/, "Deal-in risk $1").replaceAll(/seat (\d)/g, (match, digit: string) => {
        const seat = SEATS.find((candidate) => String(candidate) === digit);

        return seat === undefined ? match : seatName(seat);
    });
}

function statusText(view: HandView): string {
    const { observation, humanToAct, over } = view;
    const { decision } = observation;

    if (over) {
        return "The hand is over.";
    }

    if (humanToAct && decision.kind === "claim") {
        return decision.offer === "kong_upgrade"
            ? `${seatName(decision.from)} is adding to a pong. You can win on it or pass.`
            : `${seatName(decision.from)} discarded ${tileName(decision.tile)}. Claim it or pass.`;
    }

    if (humanToAct) {
        return decision.kind === "turn" && decision.after_claim ? "Discard a tile." : "Your turn. Select a tile, then discard it.";
    }

    const next = observation.to_act[0];

    return next === undefined ? "Waiting." : `${seatName(next)} is playing.`;
}

interface SidePanelProps {
    view: HandView;
    advice: Advice | undefined;
    error: string | undefined;
}

/** Whose move it is, the wall, an optional hint, and what has happened this hand. */
export function SidePanel({ view, advice, error }: SidePanelProps) {
    const { observation, events } = view;
    const best = advice?.ranked[0];
    const offered = observation.decision.kind === "claim" ? tileName(observation.decision.tile) : undefined;

    return (
        <aside className="side" aria-label="Hand status">
            <p className="status" role="status">
                {statusText(view)}
            </p>
            <p className="wall">{observation.live_wall} tiles left in the wall</p>

            {error !== undefined && (
                <p className="problem" role="alert">
                    The engine rejected that move: {error}. Pick another action.
                </p>
            )}

            {advice !== undefined && (
                <div className="advice">
                    <h2>Suggested move</h2>
                    <p className="suggestion">{actionLabel(advice.recommended, offered)}</p>
                    {best !== undefined && (
                        <ul>
                            {best.reasons.map((reason) => (
                                <li key={reason}>{plainReason(reason)}</li>
                            ))}
                        </ul>
                    )}
                </div>
            )}

            <div className="log">
                <h2>This hand</h2>
                {events.length === 0 ? (
                    <p>Nothing has been played yet.</p>
                ) : (
                    <ol>
                        {events.toReversed().map((event) => (
                            <li key={event.id}>
                                <strong>{seatName(event.seat satisfies Seat)}</strong> {event.text}
                            </li>
                        ))}
                    </ol>
                )}
            </div>
        </aside>
    );
}
