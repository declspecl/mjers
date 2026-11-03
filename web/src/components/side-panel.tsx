import { seatName, tileName } from "../engine/names";
import type { TableState } from "../engine/use-table";
import { GRADES } from "../grading/grade";
import type { GradedMove } from "../grading/use-grader";
import type { Action, Advice, Seat } from "@mjers/mjers/types";

function actionLabel(action: Action, offered: string | undefined): string {
    if (typeof action === "string") {
        return { self_draw_win: "Win by self-draw", pass: "Pass", pong: "Pong", kong: "Gong", win: `Win with ${offered ?? "the discard"}` }[
            action
        ];
    }

    if ("discard" in action) {
        return `Discard ${tileName(action.discard)}`;
    }

    if ("chow" in action) {
        return `Seung from ${tileName(action.chow)}`;
    }

    if ("concealed_kong" in action) {
        return `Am gong of ${tileName(action.concealed_kong)}`;
    }

    return `Gong with ${tileName(action.upgrade_kong)}`;
}

function lowerFirst(text: string): string {
    return text.charAt(0).toLowerCase() + text.slice(1);
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

function statusText(table: TableState): string {
    const { observation, humanToAct, phase, model } = table;
    const { decision } = observation;

    if (phase === "opening") {
        return "Building the wall and dealing.";
    }

    if (phase === "over") {
        return "The hand is over.";
    }

    if (model.pending.length > 0) {
        return "Your turn. Draw a tile from the wall.";
    }

    if (humanToAct && decision.kind === "claim") {
        return decision.offer === "kong_upgrade"
            ? `${seatName(decision.from)} is adding to a pong. You can win on it or pass.`
            : `${seatName(decision.from)} discarded ${tileName(decision.tile)}. Claim it or pass.`;
    }

    if (humanToAct) {
        return "Your turn. Select a tile, then discard it.";
    }

    const next = observation.to_act[0];

    return next === undefined ? "Waiting." : `${seatName(next)} is playing.`;
}

interface SidePanelProps {
    table: TableState;
    advice: Advice | undefined;
    error: string | undefined;
    graded: GradedMove[];
    feedback: boolean;
    gradingFailure: string | undefined;
    totals: number[];
    onHint: () => void;
}

/** The analytic view: whose move it is, the wall, the engine's verdicts and hints, and what has happened this hand. */
export function SidePanel({ table, advice, error, graded, feedback, gradingFailure, totals, onHint }: SidePanelProps) {
    const { observation, events, humanToAct, phase, model } = table;
    const best = advice?.ranked[0];
    const offered = observation.decision.kind === "claim" ? tileName(observation.decision.tile) : undefined;
    const latest = graded.at(-1);
    const tally = GRADES.map((grade) => ({ grade, count: graded.filter((entry) => entry.result.grade === grade).length })).filter(
        (entry) => entry.count > 0,
    );

    return (
        <aside className="side" aria-label="Hand status">
            <p className="status" role="status">
                {statusText(table)}
            </p>
            <p className="wall">
                {observation.live_wall} tiles left in the live wall. Hand {table.hand + 1}, you{" "}
                {totals[0]! > 0 ? `+${totals[0]}` : totals[0]} in the match.
            </p>

            {error !== undefined && (
                <p className="problem" role="alert">
                    The engine rejected that move: {error}. Pick another action.
                </p>
            )}

            {feedback && (
                <div className="verdict">
                    <h2>Your moves</h2>
                    {latest ? (
                        <>
                            <p>
                                <strong data-grade={latest.result.grade.toLowerCase()}>{latest.result.grade}</strong> for{" "}
                                {lowerFirst(actionLabel(latest.action, undefined))}.
                                {latest.result.grade !== "Perfect" && (
                                    <> The engine preferred {lowerFirst(actionLabel(latest.result.best, undefined))}.</>
                                )}
                            </p>
                            <p className="muted">{latest.result.reason}</p>
                            <p className="muted">
                                {tally.map((entry) => `${entry.count} ${entry.grade.toLowerCase()}`).join(", ")} this hand.
                            </p>
                        </>
                    ) : (
                        <p className="muted">Each move you make is graded against the engine once its playouts finish.</p>
                    )}
                    {gradingFailure !== undefined && (
                        <p className="problem">Grading stopped: {gradingFailure}. Reload the page to restart it.</p>
                    )}
                </div>
            )}

            {phase === "play" && humanToAct && model.pending.length === 0 && (
                <button type="button" onClick={onHint}>
                    Show a hint
                </button>
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
