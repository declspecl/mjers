import { tileName } from "../engine/names";
import { parseTile } from "../engine/tiles";
import type { Tile } from "@mjers/mjers/types";

type Point = readonly [number, number];

interface Layout {
    size: number;
    points: Point[];
}

const NUMERALS = ["一", "二", "三", "四", "五", "六", "七", "八", "九"];
const WIND_GLYPHS = ["東", "南", "西", "北"];
const FLOWER_GLYPHS = ["梅", "蘭", "菊", "竹"];
const SEASON_GLYPHS = ["春", "夏", "秋", "冬"];

/** Dots: circle radius and centers on a 44 by 56 tile face. */
const DOTS: Record<number, Layout> = {
    1: { size: 10, points: [[22, 28]] },
    2: {
        size: 6.5,
        points: [
            [22, 17],
            [22, 39],
        ],
    },
    3: {
        size: 6,
        points: [
            [13, 13],
            [22, 28],
            [31, 43],
        ],
    },
    4: {
        size: 5.5,
        points: [
            [14, 17],
            [30, 17],
            [14, 39],
            [30, 39],
        ],
    },
    5: {
        size: 5.5,
        points: [
            [14, 15],
            [30, 15],
            [14, 41],
            [30, 41],
            [22, 28],
        ],
    },
    6: {
        size: 5,
        points: [
            [14, 12],
            [30, 12],
            [14, 28],
            [30, 28],
            [14, 44],
            [30, 44],
        ],
    },
    7: {
        size: 4.2,
        points: [
            [12, 10],
            [22, 15],
            [32, 20],
            [14, 36],
            [30, 36],
            [14, 47],
            [30, 47],
        ],
    },
    8: {
        size: 4.2,
        points: [
            [14, 9],
            [30, 9],
            [14, 22],
            [30, 22],
            [14, 34],
            [30, 34],
            [14, 47],
            [30, 47],
        ],
    },
    9: {
        size: 4.6,
        points: [
            [12, 12],
            [22, 12],
            [32, 12],
            [12, 28],
            [22, 28],
            [32, 28],
            [12, 44],
            [22, 44],
            [32, 44],
        ],
    },
};

/** Bamboo: stick height and centers. */
const STICKS: Record<number, Layout> = {
    1: { size: 24, points: [[22, 28]] },
    2: {
        size: 15,
        points: [
            [22, 16],
            [22, 40],
        ],
    },
    3: {
        size: 14,
        points: [
            [22, 15],
            [14, 40],
            [30, 40],
        ],
    },
    4: {
        size: 15,
        points: [
            [14, 16],
            [30, 16],
            [14, 40],
            [30, 40],
        ],
    },
    5: {
        size: 15,
        points: [
            [14, 16],
            [30, 16],
            [14, 40],
            [30, 40],
            [22, 28],
        ],
    },
    6: {
        size: 15,
        points: [
            [12, 16],
            [22, 16],
            [32, 16],
            [12, 40],
            [22, 40],
            [32, 40],
        ],
    },
    7: {
        size: 12,
        points: [
            [22, 11],
            [12, 29],
            [22, 29],
            [32, 29],
            [12, 45],
            [22, 45],
            [32, 45],
        ],
    },
    8: {
        size: 15,
        points: [
            [9.5, 16],
            [18.5, 16],
            [27.5, 16],
            [36.5, 16],
            [9.5, 40],
            [18.5, 40],
            [27.5, 40],
            [36.5, 40],
        ],
    },
    9: {
        size: 12,
        points: [
            [12, 11],
            [22, 11],
            [32, 11],
            [12, 28],
            [22, 28],
            [32, 28],
            [12, 45],
            [22, 45],
            [32, 45],
        ],
    },
};

function dotColor(count: number, index: number): string {
    if (count === 1 || (count === 5 && index === 4)) {
        return "red";
    }

    return index % 2 === 0 ? "blue" : "green";
}

function stickColor(count: number, index: number): string {
    return count === 1 || (count === 5 && index === 4) || (count === 7 && index === 0) ? "red" : "green";
}

function Glyph({ tile }: { tile: Tile }) {
    const { rank, suit } = parseTile(tile);

    if (suit === "m") {
        return (
            <>
                <text className="glyph ink" x="22" y="27" fontSize="23">
                    {NUMERALS[rank - 1]}
                </text>
                <text className="glyph red" x="22" y="48" fontSize="16">
                    萬
                </text>
            </>
        );
    }

    if (suit === "p") {
        const layout = DOTS[rank];

        return (
            <>
                {layout?.points.map(([x, y], index) => (
                    <g key={`${x}-${y}`} className={dotColor(rank, index)}>
                        <circle cx={x} cy={y} r={layout.size} fill="none" stroke="currentColor" strokeWidth={rank === 1 ? 2.4 : 1.6} />
                        <circle cx={x} cy={y} r={layout.size * (rank === 1 ? 0.55 : 0.42)} fill="currentColor" />
                    </g>
                ))}
            </>
        );
    }

    if (suit === "s") {
        const layout = STICKS[rank];
        const width = rank === 1 ? 8 : 5;

        return (
            <>
                {layout?.points.map(([x, y], index) => (
                    <g key={`${x}-${y}`} className={stickColor(rank, index)}>
                        <rect x={x - width / 2} y={y - layout.size / 2} width={width} height={layout.size} rx={2.2} fill="currentColor" />
                        <rect x={x - width / 2} y={y - 0.6} width={width} height={1.2} className="tile-body" />
                    </g>
                ))}
            </>
        );
    }

    if (suit === "z") {
        if (rank === 5) {
            return <rect className="blue" x="10" y="9" width="24" height="38" rx="3" fill="none" stroke="currentColor" strokeWidth="2.6" />;
        }

        const glyph = rank <= 4 ? WIND_GLYPHS[rank - 1] : rank === 6 ? "發" : "中";
        const color = rank <= 4 ? "ink" : rank === 6 ? "green" : "red";

        return (
            <text className={`glyph ${color}`} x="22" y="39" fontSize="32">
                {glyph}
            </text>
        );
    }

    const glyph = (suit === "F" ? FLOWER_GLYPHS : SEASON_GLYPHS)[rank - 1];

    return (
        <>
            <text className={`glyph ${suit === "F" ? "red" : "blue"}`} x="22" y="33" fontSize="26">
                {glyph}
            </text>
            <text className="glyph ink" x="22" y="49" fontSize="11">
                {rank}
            </text>
        </>
    );
}

/** One tile face, drawn at the size its container gives it. */
export function TileFace({ tile }: { tile: Tile }) {
    return (
        <svg className="tile-face" viewBox="0 0 44 60" role="img" aria-label={tileName(tile)}>
            <rect className="tile-edge" y="4" width="44" height="56" rx="5" />
            <rect className="tile-body" width="44" height="56" rx="5" />
            <Glyph tile={tile} />
        </svg>
    );
}

/** A concealed tile seen from the other side of the table. */
export function TileBack({ side }: { side?: boolean }) {
    return <span className={side ? "tile-back tile-back-side" : "tile-back"} aria-hidden="true" />;
}
