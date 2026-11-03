import { seatName } from "../engine/names";
import { expandCounts, meldTiles } from "../engine/tiles";
import { TileFace } from "./tile";
import type { Hand, HandResult, Seat, Settlement, Tile } from "@mjers/mjers/types";

function headline(result: HandResult): string {
    const { win } = result;

    if (win === null) {
        return "Nobody won. The wall ran out.";
    }

    const winner = seatName(win.winner);

    if (win.source === "SelfDraw") {
        return `${winner} won by self-draw.`;
    }

    const discarder = win.source.Discard;

    return `${winner} won off ${discarder === 0 ? "your" : `${seatName(discarder)}'s`} discard.`;
}

function signed(units: number): string {
    return units > 0 ? `+${units}` : String(units);
}

interface ResultProps {
    result: HandResult;
    settlement: Settlement;
    winnerHand: Hand | undefined;
    /** The discard the winner took, which is marked inside their hand. */
    winningTile: Tile | undefined;
    onNext: () => void;
}

/** The outcome of a finished hand and the units each seat won or lost. */
export function Result({ result, settlement, winnerHand, winningTile, onNext }: ResultProps) {
    const seats: Seat[] = [0, 1, 2, 3];
    const concealed = winnerHand ? expandCounts(winnerHand.concealed) : [];
    const winningIndex = winningTile === undefined ? -1 : concealed.lastIndexOf(winningTile);

    return (
        <section className="result" aria-label="Hand result">
            <h2>{headline(result)}</h2>
            {winnerHand && result.win && (
                <div className="winning-hand" role="group" aria-label={`${seatName(result.win.winner)}'s hand`}>
                    <ul>
                        {concealed.map((tile, index) => (
                            <li key={index} data-winning={index === winningIndex}>
                                <TileFace tile={tile} />
                            </li>
                        ))}
                    </ul>
                    {winnerHand.melds.length > 0 && (
                        <ul>
                            {winnerHand.melds.flatMap((meld, meldIndex) =>
                                meldTiles(meld).map((tile, index) => (
                                    <li key={`${meldIndex}-${index}`}>
                                        <TileFace tile={tile} />
                                    </li>
                                )),
                            )}
                        </ul>
                    )}
                </div>
            )}
            <table>
                <thead>
                    <tr>
                        <th scope="col">Player</th>
                        <th scope="col">Win</th>
                        <th scope="col">Kongs</th>
                        <th scope="col">Total</th>
                    </tr>
                </thead>
                <tbody>
                    {seats.map((seat) => (
                        <tr key={seat}>
                            <th scope="row">{seatName(seat)}</th>
                            <td>{signed(settlement.win[seat])}</td>
                            <td>{signed(settlement.kongs[seat])}</td>
                            <td>{signed(settlement.total[seat])}</td>
                        </tr>
                    ))}
                </tbody>
            </table>
            <button type="button" className="primary" onClick={onNext}>
                Deal the next hand
            </button>
        </section>
    );
}
