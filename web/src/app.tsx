import { ActionBar } from "./components/action-bar";
import { GradeToast } from "./components/grade-toast";
import { Result } from "./components/result";
import { SettingsMenu } from "./components/settings-menu";
import { SidePanel } from "./components/side-panel";
import { Stage } from "./components/stage";
import { loadEngine, type Mjers } from "./engine/engine";
import { useSettings } from "./engine/settings";
import { useTable } from "./engine/use-table";
import { isHandSorted } from "./table/model";
import type { Tile } from "@mjers/mjers/types";
import { useCallback, useEffect, useMemo, useState } from "react";

function Match({ mjers }: { mjers: Mjers }) {
    const { settings, update } = useSettings();
    const { match, table, advice, error, graded, gradingFailure, act, draw, move, sort, hint, finishOpening, nextHand } = useTable(
        mjers,
        settings,
    );
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
        return <p className="loading">Dealing the hand.</p>;
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
                    {settings.feedback && <GradeToast latest={latest?.hand === table.hand ? latest : undefined} />}
                    {table.phase === "over" && table.result && table.settlement && (
                        <Result result={table.result} settlement={table.settlement} totals={match.totals} onNext={nextHand} />
                    )}
                </Stage>
                <SettingsMenu settings={settings} onChange={update} onSkipOpening={table.phase === "opening" ? finishOpening : undefined} />
            </div>
            {analytic && (
                <SidePanel
                    table={table}
                    advice={advice}
                    error={error}
                    graded={graded}
                    feedback={settings.feedback}
                    gradingFailure={gradingFailure}
                    totals={match.totals}
                    onHint={hint}
                />
            )}
            {!analytic && error !== undefined && (
                <p className="problem floating" role="alert">
                    The engine rejected that move: {error}. Pick another action.
                </p>
            )}
        </div>
    );
}

export default function App() {
    const [mjers, setMjers] = useState<Mjers>();
    const [error, setError] = useState<string>();

    useEffect(() => {
        loadEngine().then(setMjers, (error_: unknown) => setError(String(error_)));
    }, []);

    return (
        <main>
            {error !== undefined && <p className="problem">The engine did not load: {error}. Reload the page to try again.</p>}
            {error === undefined && mjers === undefined && <p className="loading">Loading the engine.</p>}
            {mjers !== undefined && <Match mjers={mjers} />}
        </main>
    );
}
