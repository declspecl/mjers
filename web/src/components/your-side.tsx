import { seatName, tileName, windName } from "../engine/names";
import { expandCounts, meldTiles, removeOne } from "../engine/tiles";
import type { HandView } from "../engine/use-hand";
import { TileFace } from "./tile";
import type { Action, Advice, Seat, Tile } from "@mjers/mjers/types";
import { useState } from "react";

interface YourSideProps {
    view: HandView;
    advice: Advice | undefined;
    dealer: Seat;
    onAct: (action: Action) => void;
    onHint: () => void;
}

interface HandEntry {
    id: string;
    tile: Tile;
}

function isChow(action: Action): action is { chow: Tile } {
    return typeof action === "object" && "chow" in action;
}

function isDiscard(action: Action): action is { discard: Tile } {
    return typeof action === "object" && "discard" in action;
}

function isConcealedKong(action: Action): action is { concealed_kong: Tile } {
    return typeof action === "object" && "concealed_kong" in action;
}

function isUpgradeKong(action: Action): action is { upgrade_kong: Tile } {
    return typeof action === "object" && "upgrade_kong" in action;
}

/** Your hand, the tile you just drew, your melds, and the buttons for what you can do now. */
export function YourSide({ view, advice, dealer, onAct, onHint }: YourSideProps) {
    const [selected, setSelected] = useState<string>();
    const { observation, humanToAct } = view;
    const { decision, legal_actions: legal } = observation;
    const sorted = expandCounts(observation.hand.concealed);
    const drawn = decision.kind === "turn" ? decision.drawn : null;
    const resting: HandEntry[] = (drawn === null ? sorted : removeOne(sorted, drawn)).map((tile, index) => ({ id: `hand-${index}`, tile }));
    const justDrawn: HandEntry | undefined = drawn === null ? undefined : { id: "drawn", tile: drawn };
    const discardable = new Set(legal.filter((action) => isDiscard(action)).map((action) => action.discard));
    const suggested = advice !== undefined && isDiscard(advice.recommended) ? advice.recommended.discard : undefined;
    const selectedTile = [...resting, ...(justDrawn ? [justDrawn] : [])].find((entry) => entry.id === selected)?.tile;
    const claiming = humanToAct && decision.kind === "claim";

    const choose = (entry: HandEntry) => {
        if (!humanToAct || !discardable.has(entry.tile)) {
            return;
        }

        if (selected === entry.id) {
            onAct({ discard: entry.tile });
        } else {
            setSelected(entry.id);
        }
    };

    const renderTile = (entry: HandEntry) => (
        <li key={entry.id}>
            <button
                type="button"
                className="tile-button"
                aria-pressed={selected === entry.id}
                aria-label={`${tileName(entry.tile)}${suggested === entry.tile ? ", suggested" : ""}`}
                data-suggested={suggested === entry.tile}
                disabled={!humanToAct || !discardable.has(entry.tile)}
                data-illegal={humanToAct && decision.kind === "turn" && !discardable.has(entry.tile)}
                onClick={() => choose(entry)}
            >
                <TileFace tile={entry.tile} />
            </button>
        </li>
    );

    return (
        <section className="you" aria-label="Your hand">
            <p className="seat-name">
                You
                <span>{windName(0, dealer)}</span>
                {dealer === 0 && <span>Dealer</span>}
            </p>

            <ul className="hand" aria-label="Concealed tiles">
                {resting.map((entry) => renderTile(entry))}
                {justDrawn && <li className="drawn-gap" aria-hidden="true" />}
                {justDrawn && renderTile(justDrawn)}
            </ul>

            <div className="shown">
                {observation.hand.melds.map((meld, index) => (
                    <div className="meld" key={index}>
                        {meldTiles(meld).map((tile, tileIndex) => (
                            <TileFace key={tileIndex} tile={tile} />
                        ))}
                    </div>
                ))}
                {observation.hand.bonus.length > 0 && (
                    <div className="meld bonus">
                        {observation.hand.bonus.map((tile) => (
                            <TileFace key={tile} tile={tile} />
                        ))}
                    </div>
                )}
            </div>

            {claiming && decision.kind === "claim" && (
                <div className="claim" role="group" aria-label="Claim options">
                    <p>
                        {seatName(decision.from)} discarded <strong>{tileName(decision.tile)}</strong>.
                    </p>
                    <div className="actions">
                        {legal
                            .filter((action) => isChow(action))
                            .map((action) => (
                                <button
                                    type="button"
                                    key={action.chow}
                                    data-suggested={
                                        advice !== undefined && isChow(advice.recommended) && advice.recommended.chow === action.chow
                                    }
                                    onClick={() => onAct(action)}
                                >
                                    Chow
                                    <span className="run">
                                        {meldTiles(action).map((tile, index) => (
                                            <TileFace key={index} tile={tile} />
                                        ))}
                                    </span>
                                </button>
                            ))}
                        {legal.includes("pong") && (
                            <button type="button" data-suggested={advice?.recommended === "pong"} onClick={() => onAct("pong")}>
                                Pong
                            </button>
                        )}
                        {legal.includes("kong") && (
                            <button type="button" data-suggested={advice?.recommended === "kong"} onClick={() => onAct("kong")}>
                                Kong
                            </button>
                        )}
                        {legal.includes("win") && (
                            <button
                                type="button"
                                className="primary"
                                data-suggested={advice?.recommended === "win"}
                                onClick={() => onAct("win")}
                            >
                                Win with {tileName(decision.tile)}
                            </button>
                        )}
                        <button type="button" data-suggested={advice?.recommended === "pass"} onClick={() => onAct("pass")}>
                            Pass
                        </button>
                    </div>
                </div>
            )}

            {humanToAct && (
                <div className="actions" role="group" aria-label="Your move">
                    {decision.kind === "turn" && (
                        <button
                            type="button"
                            className="primary"
                            disabled={selectedTile === undefined}
                            onClick={() => selectedTile && onAct({ discard: selectedTile })}
                        >
                            {selectedTile === undefined ? "Select a tile to discard" : `Discard ${tileName(selectedTile)}`}
                        </button>
                    )}
                    {decision.kind === "turn" && legal.includes("self_draw_win") && (
                        <button
                            type="button"
                            className="primary"
                            data-suggested={advice?.recommended === "self_draw_win"}
                            onClick={() => onAct("self_draw_win")}
                        >
                            Declare win
                        </button>
                    )}
                    {legal
                        .filter((action) => isConcealedKong(action))
                        .map((action) => (
                            <button type="button" key={action.concealed_kong} onClick={() => onAct(action)}>
                                Concealed kong of {tileName(action.concealed_kong)}
                            </button>
                        ))}
                    {legal
                        .filter((action) => isUpgradeKong(action))
                        .map((action) => (
                            <button type="button" key={action.upgrade_kong} onClick={() => onAct(action)}>
                                Make {tileName(action.upgrade_kong)} a kong
                            </button>
                        ))}
                    <button type="button" onClick={onHint}>
                        Show a hint
                    </button>
                </div>
            )}
        </section>
    );
}
