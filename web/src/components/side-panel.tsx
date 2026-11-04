import { actionLabel } from "../engine/action-label";
import { seatName, tileName } from "../engine/names";
import type { TableState } from "../engine/use-table";
import { GRADES, evaluateMoves } from "../grading/grade";
import type { GradedMove } from "../grading/use-grader";
import type { Advice, Seat } from "@mjers/mjers/types";

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
                    {error}
                </p>
            )}

            {feedback && (
                <div className="verdict">
                    <h2>Your moves</h2>
                    {latest ? (
                        <>
                            <p>
                                <strong data-grade={latest.result.grade.toLowerCase()}>{latest.result.grade}</strong>
                            </p>
                            {latest.result.grade !== "Perfect" && <p>Best: {actionLabel(latest.result.best)}</p>}
                            <p className="muted">
                                {tally.map((entry) => `${entry.count} ${entry.grade.toLowerCase()}`).join(", ")} this hand.
                            </p>
                        </>
                    ) : (
                        <p className="muted">Your grade and the engine’s best move appear after each decision.</p>
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
                    <ol className="move-rankings" aria-label="Engine move rankings">
                        {evaluateMoves(advice).map((entry) => (
                            <li key={JSON.stringify(entry.action)}>
                                {actionLabel(entry.action, offered)} <strong data-grade={entry.grade.toLowerCase()}>{entry.grade}</strong>
                            </li>
                        ))}
                    </ol>
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
