import { seatName, tileName } from "../engine/names";
import { meldTiles } from "../engine/tiles";
import type { TableState } from "../engine/use-table";
import { sameAction } from "../grading/grade";
import { TileFace } from "./tile";
import type { Action, Advice, Tile } from "@mjers/mjers/types";

interface ActionBarProps {
    table: TableState;
    advice: Advice | undefined;
    selectedTile: Tile | null;
    onAct: (action: Action) => void;
    onDiscard: () => void;
    onDraw: () => void;
}

function isChow(action: Action): action is { chow: Tile } {
    return typeof action === "object" && "chow" in action;
}

function isConcealedKong(action: Action): action is { concealed_kong: Tile } {
    return typeof action === "object" && "concealed_kong" in action;
}

function isUpgradeKong(action: Action): action is { upgrade_kong: Tile } {
    return typeof action === "object" && "upgrade_kong" in action;
}

/** What you can do right now, in the family's words: draw, discard, pong, seung, gong, am gong, win or pass. */
export function ActionBar({ table, advice, selectedTile, onAct, onDiscard, onDraw }: ActionBarProps) {
    const { observation, humanToAct, model, phase } = table;
    const { decision, legal_actions: legal } = observation;

    if (phase !== "play") {
        return null;
    }

    const suggested = (action: Action) => advice !== undefined && sameAction(advice.recommended, action);

    if (model.pending.length > 0) {
        const next = model.pending[0]!;

        return (
            <div className="action-bar" role="group" aria-label="Your move">
                <button type="button" className="primary" onClick={onDraw}>
                    {next.source === "back" ? "Draw a replacement" : "Draw tile"}
                </button>
            </div>
        );
    }

    if (!humanToAct) {
        return null;
    }

    if (decision.kind === "claim") {
        return (
            <div className="action-bar" role="group" aria-label="Claim options">
                <p className="claim-offer">
                    {seatName(decision.from)} {decision.offer === "kong_upgrade" ? "adds" : "discards"} <TileFace tile={decision.tile} />
                </p>
                {legal.includes("win") && (
                    <button type="button" className="primary" data-suggested={suggested("win")} onClick={() => onAct("win")}>
                        Win
                    </button>
                )}
                {legal.includes("kong") && (
                    <button type="button" data-suggested={suggested("kong")} onClick={() => onAct("kong")}>
                        Gong
                    </button>
                )}
                {legal.includes("pong") && (
                    <button type="button" data-suggested={suggested("pong")} onClick={() => onAct("pong")}>
                        Pong
                    </button>
                )}
                {legal
                    .filter((action) => isChow(action))
                    .map((action) => (
                        <button
                            type="button"
                            key={action.chow}
                            data-suggested={suggested(action)}
                            onClick={() => onAct(action)}
                            aria-label={`Seung ${meldTiles(action)
                                .map((tile) => tileName(tile))
                                .join(", ")}`}
                        >
                            Seung
                            <span className="run">
                                {meldTiles(action).map((tile, index) => (
                                    <TileFace key={index} tile={tile} />
                                ))}
                            </span>
                        </button>
                    ))}
                <button type="button" data-suggested={suggested("pass")} onClick={() => onAct("pass")}>
                    Pass
                </button>
            </div>
        );
    }

    if (decision.kind !== "turn") {
        return null;
    }

    const discardable =
        selectedTile !== null &&
        legal.some((action) => typeof action === "object" && "discard" in action && action.discard === selectedTile);

    return (
        <div className="action-bar" role="group" aria-label="Your move">
            {legal.includes("self_draw_win") && (
                <button
                    type="button"
                    className="primary"
                    data-suggested={suggested("self_draw_win")}
                    onClick={() => onAct("self_draw_win")}
                >
                    Win
                </button>
            )}
            {legal
                .filter((action) => isConcealedKong(action))
                .map((action) => (
                    <button type="button" key={action.concealed_kong} data-suggested={suggested(action)} onClick={() => onAct(action)}>
                        Am gong <TileFace tile={action.concealed_kong} />
                    </button>
                ))}
            {legal
                .filter((action) => isUpgradeKong(action))
                .map((action) => (
                    <button type="button" key={action.upgrade_kong} data-suggested={suggested(action)} onClick={() => onAct(action)}>
                        Gong <TileFace tile={action.upgrade_kong} />
                    </button>
                ))}
            <button
                type="button"
                className={legal.includes("self_draw_win") ? undefined : "primary"}
                disabled={!discardable}
                onClick={onDiscard}
            >
                {selectedTile === null ? "Discard" : `Discard ${tileName(selectedTile)}`}
            </button>
        </div>
    );
}
