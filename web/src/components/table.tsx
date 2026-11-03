import type { HandView } from "../engine/use-hand";
import { Opponent } from "./opponent";
import { Pond } from "./pond";
import type { Seat } from "@mjers/mjers/types";

interface TableProps {
    view: HandView;
    dealer: Seat;
}

/** The three bots, all four discard ponds, and the wall count between them. */
export function Table({ view, dealer }: TableProps) {
    const { observation } = view;
    const { players, discards, to_act: toAct } = observation;
    const latest = discards.at(-1);
    const [, right, across, left] = players;

    return (
        <div className="table">
            {across && (
                <div className="area-across">
                    <Opponent player={across} dealer={dealer} acting={toAct.includes(2)} />
                </div>
            )}
            {left && (
                <div className="area-left">
                    <Opponent player={left} dealer={dealer} acting={toAct.includes(3)} side />
                </div>
            )}
            {right && (
                <div className="area-right">
                    <Opponent player={right} dealer={dealer} acting={toAct.includes(1)} side />
                </div>
            )}
            <div className="ponds">
                <div className="pond-across">
                    <p className="pond-caption">Across bot</p>
                    <Pond seat={2} discards={discards} latest={latest} />
                </div>
                <div className="pond-left">
                    <p className="pond-caption">Left bot</p>
                    <Pond seat={3} discards={discards} latest={latest} />
                </div>
                <div className="pond-right">
                    <p className="pond-caption">Right bot</p>
                    <Pond seat={1} discards={discards} latest={latest} />
                </div>
                <div className="pond-you">
                    <p className="pond-caption">You</p>
                    <Pond seat={0} discards={discards} latest={latest} />
                </div>
            </div>
        </div>
    );
}
