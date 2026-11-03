import type { Settings } from "../engine/settings";
import type { TableState } from "../engine/use-table";
import type { GradedMove } from "../grading/use-grader";
import { isHandSorted } from "../table/model";
import { ActionBar } from "./action-bar";
import { GradeToast } from "./grade-toast";
import { Result } from "./result";
import { SettingsMenu } from "./settings-menu";
import { SidePanel } from "./side-panel";
import { Stage } from "./stage";
import type { Action, Advice, Tile } from "@mjers/mjers/types";
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";

export interface PlayProps {
    settings: Settings;
    update: (change: Partial<Settings>) => void;
    table: TableState | undefined;
    advice: Advice | undefined;
    error: string | undefined;
    graded: GradedMove[];
    gradingFailure: string | undefined;
    /** Multiplayer has no local grader. */
    grading: boolean;
    totals: number[];
    act: (action: Action, discardId?: number) => void;
    draw: () => void;
    move: (id: number, index: number) => void;
    sort: () => void;
    hint: () => void;
    finishOpening: () => void;
    /** Missing when someone else deals the next hand. */
    nextHand?: () => void;
    waiting?: string;
    loading?: string;
    /** Room status shown over the table in multiplayer. */
    overlay?: ReactNode;
}

/** The table, its controls and the analytic panel, for practice and multiplayer alike. */
export function Play({
    settings,
    update,
    table,
    advice,
    error,
    graded,
    gradingFailure,
    grading,
    totals,
    act,
    draw,
    move,
    sort,
    hint,
    finishOpening,
    nextHand,
    waiting,
    loading,
    overlay,
}: PlayProps) {
    const [selected, setSelected] = useState<number | null>(null);
    const analytic = settings.view === "analytic";

    useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null;

            if (event.metaKey || event.ctrlKey || event.altKey || target?.closest("input, textarea, select")) {
                return;
            }

            if (event.key === "v" || event.key === "V") {
                update({ view: settings.view === "immersive" ? "analytic" : "immersive" });
            }

            if ((event.key === "d" || event.key === "D") && table?.model.pending.length) {
                draw();
            }

            if ((event.key === "s" || event.key === "S") && table?.phase === "play") {
                sort();
            }
        };

        globalThis.addEventListener("keydown", onKey);

        return () => globalThis.removeEventListener("keydown", onKey);
    }, [settings.view, update, table, draw, sort]);

    const faceOf = useCallback((id: number | null): Tile | null => (id === null ? null : (table?.model.faces[id] ?? null)), [table]);

    const canDiscard = useCallback(
        (id: number) => {
            if (
                !table ||
                table.phase !== "play" ||
                !table.humanToAct ||
                table.model.pending.length > 0 ||
                table.observation.decision.kind !== "turn"
            ) {
                return false;
            }

            const face = faceOf(id);

            return table.observation.legal_actions.some(
                (action) => typeof action === "object" && "discard" in action && action.discard === face,
            );
        },
        [table, faceOf],
    );

    const discard = useCallback(
        (id: number) => {
            const face = faceOf(id);

            if (face !== null && canDiscard(id)) {
                setSelected(null);
                act({ discard: face }, id);
            }
        },
        [faceOf, canDiscard, act],
    );

    const suggested = useMemo(() => {
        if (!table || !advice || typeof advice.recommended !== "object" || !("discard" in advice.recommended)) {
            return null;
        }

        const tile = advice.recommended.discard;
        const drawn = table.model.drawn[0];

        return drawn !== null && drawn !== undefined && table.model.faces[drawn] === tile
            ? drawn
            : (table.model.hands[0]!.findLast((id) => table.model.faces[id] === tile) ?? null);
    }, [table, advice]);

    if (!table) {
        return <p className="loading">{loading ?? "Dealing the hand."}</p>;
    }

    const held = new Set([...table.model.hands[0]!, table.model.drawn[0]]);
    const selectedId = selected !== null && held.has(selected) ? selected : null;
    const latest = graded.at(-1);

    return (
        <div className="screen" data-view={settings.view}>
            <div className="play">
                <Stage
                    table={table}
                    analytic={analytic}
                    selected={selectedId}
                    suggested={analytic ? suggested : null}
                    canDiscard={canDiscard}
                    onSelect={setSelected}
                    onDiscard={discard}
                    onDraw={draw}
                    onMove={move}
                    onOpeningDone={finishOpening}
                >
                    <ActionBar
                        table={table}
                        advice={analytic ? advice : undefined}
                        selectedTile={faceOf(selectedId)}
                        onAct={(action) => {
                            setSelected(null);
                            act(action);
                        }}
                        onDiscard={() => selectedId !== null && discard(selectedId)}
                        onDraw={draw}
                    />
                    {table.phase === "play" && (
                        <button
                            type="button"
                            className="sort-hand"
                            disabled={isHandSorted(table.model)}
                            onClick={sort}
                            aria-keyshortcuts="S"
                            title="Sort hand (S)"
                        >
                            Sort hand
                        </button>
                    )}
                    {grading && settings.feedback && <GradeToast latest={latest?.hand === table.hand ? latest : undefined} />}
                    {table.phase === "over" && table.result && table.settlement && (
                        <Result result={table.result} settlement={table.settlement} totals={totals} onNext={nextHand} waiting={waiting} />
                    )}
                </Stage>
                <SettingsMenu settings={settings} onChange={update} onSkipOpening={table.phase === "opening" ? finishOpening : undefined} />
            </div>
            {overlay}
            {analytic && (
                <SidePanel
                    table={table}
                    advice={advice}
                    error={error}
                    graded={graded}
                    feedback={grading && settings.feedback}
                    gradingFailure={gradingFailure}
                    totals={totals}
                    onHint={hint}
                />
            )}
            {!analytic && error !== undefined && (
                <p className="problem floating" role="alert">
                    {error}
                </p>
            )}
        </div>
    );
}
