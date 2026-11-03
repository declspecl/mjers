import { toStage } from "../table/geometry";
import type { Seat } from "@mjers/mjers/types";

/** Pip positions on a 3 by 3 grid, by face value. */
const PIPS: Record<number, number[]> = {
    1: [4],
    2: [0, 8],
    3: [0, 4, 8],
    4: [0, 2, 6, 8],
    5: [0, 2, 4, 6, 8],
    6: [0, 2, 3, 5, 6, 8],
};

/** Three dice in the middle of the table. While rolling they tumble; then they settle on the values that were rolled. */
export function Dice({ values, rolling, roller }: { values: number[]; rolling: boolean; roller: Seat }) {
    const total = values.reduce((sum, value) => sum + value, 0);
    const spot = toStage(roller, 0, 200);

    return (
        <div
            className="dice"
            role="img"
            aria-label={rolling ? "Rolling the dice" : `The dice show ${total}`}
            data-rolling={rolling}
            style={{ left: spot.x, top: spot.y }}
        >
            {values.map((value, index) => (
                <svg key={index} className="die" viewBox="0 0 30 30" style={{ animationDelay: `${index * 70}ms` }}>
                    <rect x="1" y="1" width="28" height="28" rx="6" />
                    {(PIPS[value] ?? []).map((cell) => (
                        <circle
                            key={cell}
                            cx={7 + (cell % 3) * 8}
                            cy={7 + Math.floor(cell / 3) * 8}
                            r={value === 1 ? 3.6 : 2.6}
                            data-red={value === 1 || value === 4}
                        />
                    ))}
                </svg>
            ))}
            {!rolling && <span className="dice-total">{total}</span>}
        </div>
    );
}
