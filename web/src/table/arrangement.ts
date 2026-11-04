import { TILE_HEIGHT, TILE_WIDTH } from "./geometry";

/**
 * How the table, your rack and the move buttons share whatever room the screen gives them.
 *
 * The table keeps its own 1000 unit square coordinates and is scaled as large as the layout allows. Everything else is placed in the
 * space left over: on a wide screen your hand runs along the bottom with the buttons and your melds in the margins beside the table; on a
 * tall one the buttons, your melds and your hand stack up under the table, and a narrow phone splits the hand over two rows so each tile
 * stays big enough to tap.
 */

/** Space around the edges and between bands, in CSS pixels. */
const PAD = 10;
/** The widest a tile in your hand gets, in CSS pixels. */
const MAX_HAND_TILE = 64;
/** Room the hand needs in tile widths: thirteen tiles, the one just drawn, and the gap before it. */
const ROW_TILES = 14.6;
const TWO_ROW_TILES = 7.6;
/** Each tile also gets a sliver of space beside it. */
const PITCH = 1.06;
/** On a wide screen the hand takes at most this share of the height. */
const WIDE_HAND_SHARE = 0.15;
/** Margins narrower than this cannot hold your melds; the table shrinks to make room. */
const MIN_MARGIN = 110;
/** Margins at least this wide also hold the move buttons; narrower ones leave the buttons over the felt. */
const BUTTON_MARGIN = 170;
/** The height of a row of buttons, and of the bar the corner controls sit in. */
const BUTTON_BAND = 56;
/** Margins at least this wide keep the corner controls clear of the table. */
const CORNER_MARGIN = 160;
const TOP_BAR = 54;
/** Tall screens are this much taller than wide before the layout stacks. */
const TALL_RATIO = 1.1;
/** A selected tile lifts by this share of its height. */
export const LIFT = 0.2;

/** A line of tiles in table units: its extent, the bottom edge the tiles stand on, their scale, and which end they gather at. */
export interface Line {
    left: number;
    right: number;
    bottom: number;
    scale: number;
    align: "start" | "center" | "end";
}

/** Where your own tiles go, in table units. */
export interface RackArea {
    /** Your hand. With two rows, `bottom` is the bottom of the lower one. */
    hand: Line & { rows: 1 | 2 };
    melds: Line;
    /** Flowers on their own line, or null to follow your melds on theirs. */
    flowers: Line | null;
}

export interface Point {
    x: number;
    y: number;
}

export interface Arrangement {
    mode: "wide" | "tall";
    /** CSS pixels per table unit. */
    scale: number;
    /** The table's top left corner and side, in CSS pixels from the top left of the play area. */
    table: Point & { size: number };
    rack: RackArea;
    /** The move buttons: the bottom edge they rest on, the x they gather at, and how wide they may grow. */
    actions: Point & { width: number; place: "margin" | "band" | "felt" };
    /** The sort button's top left corner. */
    sort: Point;
}

function tileHeight(width: number): number {
    return (width * TILE_HEIGHT) / TILE_WIDTH;
}

interface PixelLine {
    left: number;
    right: number;
    bottom: number;
    /** Tile width in pixels. */
    tile: number;
    align: Line["align"];
}

function toUnits(line: PixelLine, origin: Point, scale: number): Line {
    return {
        left: (line.left - origin.x) / scale,
        right: (line.right - origin.x) / scale,
        bottom: (line.bottom - origin.y) / scale,
        scale: line.tile / scale / TILE_WIDTH,
        align: line.align,
    };
}

function finish(
    mode: Arrangement["mode"],
    table: Point & { size: number },
    hand: PixelLine & { rows: 1 | 2 },
    melds: PixelLine,
    flowers: PixelLine | null,
    actions: Arrangement["actions"],
    sort: Point,
): Arrangement {
    const scale = table.size / 1000;

    return {
        mode,
        scale,
        table,
        rack: {
            hand: { ...toUnits(hand, table, scale), rows: hand.rows },
            melds: toUnits(melds, table, scale),
            flowers: flowers === null ? null : toUnits(flowers, table, scale),
        },
        actions,
        sort,
    };
}

/** Table above, your hand along the whole bottom edge, melds in the left margin and buttons in the right one when it is wide enough. */
function wide(width: number, height: number): Arrangement {
    // The hand never takes height the table had under the old fixed 1000 by 1160 stage, so no screen sees a smaller table than before.
    const keep = Math.min((width - 16) / 1000, (height - 16) / 1160) * 1000;
    const handHeight = Math.max(24, Math.min(height * WIDE_HAND_SHARE, height - PAD * 2.5 - keep));
    const tile = Math.min(MAX_HAND_TILE, (width - 2 * PAD) / (ROW_TILES * PITCH), (handHeight * TILE_WIDTH) / TILE_HEIGHT);
    const handBand = tileHeight(tile) + PAD * 1.5;
    const fit = (top: number) => Math.max(120, Math.min(height - handBand - top, width - 2 * MIN_MARGIN));
    // Margins too narrow for the corner controls push the table down below them instead.
    const top = (width - fit(PAD)) / 2 >= CORNER_MARGIN ? PAD : TOP_BAR;
    const size = fit(top);
    const table = { x: (width - size) / 2, y: Math.max(top, (height - handBand - size) / 2), size };
    const margin = table.x;
    const handBottom = height - PAD;
    const above = handBottom - tileHeight(tile) - PAD * 1.5;
    const sort = { x: PAD, y: above - 44 };
    const meldTile = Math.min(tile * 0.75, 38);
    const meldRight = margin - PAD;
    const melds: PixelLine = { left: PAD, right: meldRight, bottom: sort.y - PAD, tile: meldTile, align: "start" };
    const flowers: PixelLine = { ...melds, bottom: melds.bottom - tileHeight(meldTile) - PAD, tile: meldTile * 0.8 };
    const actions: Arrangement["actions"] =
        margin >= BUTTON_MARGIN
            ? { x: width - margin / 2, y: above, width: Math.min(260, margin - 2 * PAD), place: "margin" }
            : { x: width / 2, y: table.y + size * 0.985, width: size * 0.92, place: "felt" };

    return finish(
        "wide",
        table,
        { left: PAD, right: width - PAD, bottom: handBottom, tile, align: "center", rows: 1 },
        melds,
        flowers,
        actions,
        sort,
    );
}

/** Your melds are drawn at this share of a hand tile's size. */
const MELD_SHARE = 0.72;
const ROW_GAP = 0.14;

/** Everything under the table on a tall screen: buttons, your melds and your hand, with the space between them. */
function stackHeight(tile: number, rows: 1 | 2): number {
    const height = tileHeight(tile);

    return PAD * 2.6 + BUTTON_BAND + height * (rows + (rows - 1) * ROW_GAP + LIFT) + tileHeight(Math.min(tile * MELD_SHARE, 40));
}

/**
 * Corner controls on top, then the table across the full width, then from the bottom up: your hand, your melds and the move buttons.
 *
 * Width is what limits the table on a tall screen, so it always gets all of it; your hand takes the height that is left. With room to
 * spare the hand splits into two rows of bigger tiles; on a short screen, such as a phone with its browser bars showing, it keeps one.
 */
function tall(width: number, height: number): Arrangement {
    const room = width - 2 * PAD;
    const widthCap = (rows: 1 | 2) => Math.min(MAX_HAND_TILE, room / ((rows === 1 ? ROW_TILES : TWO_ROW_TILES) * PITCH));
    // The largest tile whose stack fits in this much height, treating melds as a fixed share of the hand.
    const heightCap = (space: number, rows: 1 | 2) =>
        ((space - PAD * 2.6 - BUTTON_BAND) / (rows + (rows - 1) * ROW_GAP + LIFT + MELD_SHARE)) * (TILE_WIDTH / TILE_HEIGHT);
    const top = TOP_BAR;
    const space = height - top - width;
    const [best, other] = ([2, 1] as const)
        .map((rows) => ({ rows, tile: Math.min(widthCap(rows), heightCap(space, rows)) }))
        .toSorted((a, b) => b.tile - a.tile);
    // A single row needs no more height than two, so prefer it when its tiles are as big.
    let { rows, tile } = other!.rows === 1 && other!.tile >= best!.tile - 0.5 ? other! : best!;
    let size = width;

    // Too short even for one row at full width: the table gives up just enough height for it.
    if (tile < widthCap(1)) {
        rows = 1;
        tile = widthCap(1);
        size = Math.max(120, height - top - stackHeight(tile, 1));
    }

    const meldTile = Math.min(tile * MELD_SHARE, 40);
    const handHeight = tileHeight(tile) * (rows + (rows - 1) * ROW_GAP);
    const handBottom = height - PAD;
    const meldBottom = handBottom - handHeight - tileHeight(tile) * LIFT - PAD * 0.6;
    const actionBottom = meldBottom - tileHeight(meldTile) - PAD;
    const above = actionBottom - BUTTON_BAND;
    const table = { x: (width - size) / 2, y: top + Math.max(0, (above - top - size) / 2), size };

    return finish(
        "tall",
        table,
        { left: PAD, right: width - PAD, bottom: handBottom, tile, align: "center", rows },
        { left: PAD, right: width - PAD, bottom: meldBottom, tile: meldTile, align: "start" },
        null,
        { x: width - PAD, y: actionBottom, width: room - 120, place: "band" },
        { x: PAD, y: actionBottom - 44 },
    );
}

/** Lays the play area out for a box of this many CSS pixels. */
export function arrange(width: number, height: number): Arrangement {
    const safeWidth = Math.max(240, width);
    const safeHeight = Math.max(240, height);

    return safeHeight / safeWidth >= TALL_RATIO ? tall(safeWidth, safeHeight) : wide(safeWidth, safeHeight);
}

/** A rack for code that lays tiles out before it knows the screen, such as the opening's final frame. The stage places it again. */
export const DEFAULT_RACK: RackArea = arrange(1000, 1400).rack;
