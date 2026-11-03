import { seatName } from "../engine/names";
import type { HandResult, Seat, Settlement } from "@mjers/mjers/types";

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
    totals: number[];
    onNext: () => void;
}

const SEATS: Seat[] = [0, 1, 2, 3];

/** The outcome of a finished hand, laid over the middle of the table so the revealed hands stay in view. */
export function Result({ result, settlement, totals, onNext }: ResultProps) {
    return (
        <section className="result" aria-label="Hand result">
            <h2>{headline(result)}</h2>
            <table>
                <thead>
                    <tr>
                        <th scope="col">Player</th>
                        <th scope="col">This hand</th>
                        <th scope="col">Match</th>
                    </tr>
                </thead>
                <tbody>
                    {SEATS.map((seat) => (
                        <tr key={seat}>
                            <th scope="row">{seatName(seat)}</th>
                            <td>{signed(settlement.total[seat])}</td>
                            <td>{signed(totals[seat] ?? 0)}</td>
                        </tr>
                    ))}
                </tbody>
            </table>
            <button type="button" className="primary" onClick={onNext} autoFocus>
                Deal the next hand
            </button>
        </section>
    );
}
