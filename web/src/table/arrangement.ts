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
/** Below this width a single row of tiles is too small to tap, so a tall screen splits the hand in two. */
const MIN_HAND_TILE = 34;
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
    const tile = Math.min(MAX_HAND_TILE, (width - 2 * PAD) / (ROW_TILES * PITCH), (height * WIDE_HAND_SHARE * TILE_WIDTH) / TILE_HEIGHT);
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

/** Corner controls on top, then the table, then from the bottom up: your hand, your melds and the move buttons. */
function tall(width: number, height: number): Arrangement {
    const room = width - 2 * PAD;
    const rows: 1 | 2 = room / (ROW_TILES * PITCH) >= MIN_HAND_TILE ? 1 : 2;
    const tile = Math.min(MAX_HAND_TILE, room / ((rows === 1 ? ROW_TILES : TWO_ROW_TILES) * PITCH));
    const rowGap = tileHeight(tile) * 0.14;
    const handHeight = rows * tileHeight(tile) + (rows - 1) * rowGap;
    const meldTile = Math.min(tile * 0.72, 40);
    const handBottom = height - PAD;
    const meldBottom = handBottom - handHeight - tileHeight(tile) * LIFT - PAD * 0.6;
    const actionBottom = meldBottom - tileHeight(meldTile) - PAD;
    const above = actionBottom - BUTTON_BAND;
    // The corner controls get a bar of their own unless that would cost the table more than a sliver.
    const top = above - PAD - TOP_BAR >= room * 0.88 ? TOP_BAR : PAD;
    const size = Math.max(120, Math.min(room, above - top - PAD));
    const table = { x: (width - size) / 2, y: top + (above - top - size) / 2, size };

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
