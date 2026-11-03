import { DEFAULT_RACK, LIFT, type Line, type RackArea } from "./arrangement";
import { CENTER, type Face, flowerWallPose, lying, type Pose, TILE_HEIGHT, TILE_WIDTH, wallPose } from "./geometry";
import { SEATS, type TableModel, TILE_COUNT } from "./model";
import type { Seat, Tile } from "@mjers/mjers/types";

/** How far a seat's hand sits from the middle of the table, along its edge. */
const HAND_DISTANCE = 448;
/** The longest a seat's row of tiles may run before it reaches the corner walls. */
const ROW_ROOM = 680;

/** Where a tile waits before anyone takes it: in its wall, or in the flower wall once it has been turned down. */
export function restingPose(model: TableModel, id: number): Pose {
    const slot = model.flowerSlots[id] ?? -1;

    return slot === -1 ? wallPose(id) : flowerWallPose(model.setter, slot);
}

interface RowItem {
    id: number;
    width: number;
    scale: number;
    face: Face;
    gapBefore: number;
    lift?: number;
}

/** Lays a row of tiles out from left to right, centred on 0, shrinking it if it would not fit. */
function rowPositions(items: RowItem[], available: number): { x: number; fit: number; width: number }[] {
    const total = items.reduce((sum, item) => sum + item.width + item.gapBefore, 0);
    const fit = total > available ? available / total : 1;
    let cursor = (-total * fit) / 2;

    return items.map((item) => {
        cursor += item.gapBefore * fit;

        const x = cursor + (item.width * fit) / 2;

        cursor += item.width * fit;

        return { x, fit, width: total * fit };
    });
}

function meldItems(model: RackContents, seat: Seat, scale: number, pitch: number, meldGap: number, firstGap: number): RowItem[] {
    const items: RowItem[] = [];

    for (const [meldIndex, entry] of model.melds[seat]!.entries()) {
        const concealed = "kong" in entry.meld && entry.meld.kong[1] === "concealed";

        for (const [index, id] of entry.ids.entries()) {
            const hidden = concealed && (index === 0 || index === entry.ids.length - 1);

            items.push({
                id,
                width: pitch,
                scale,
                face: hidden ? "down" : "up",
                gapBefore: index === 0 ? (meldIndex === 0 ? firstGap : meldGap) : 0,
            });
        }
    }

    return items;
}

function bonusItems(model: RackContents, seat: Seat, scale: number, pitch: number, firstGap: number): RowItem[] {
    return model.bonus[seat]!.map((id, index) => ({ id, width: pitch, scale, face: "up" as const, gapBefore: index === 0 ? firstGap : 0 }));
}

/** What the rack reads from the table: the melds and flowers each seat has shown. */
export type RackContents = Pick<TableModel, "melds" | "bonus">;

export interface RackOptions {
    /** Your hand in the order to show it, which differs from the model while you drag a tile. */
    order: number[];
    drawn: number | null;
    selected: number | null;
    /** Where your tiles go; the stage fits it to the screen. */
    area?: RackArea;
}

export interface RackSlot {
    id: number;
    x: number;
    /** The centre of the slot's row, ignoring any lift. */
    y: number;
    row: number;
    width: number;
}

/** Places a line of tiles along a line, gathered at its start, centre or end. */
function placeLine(items: RowItem[], line: Line, poses: Map<number, Pose>, z: number) {
    if (items.length === 0) {
        return;
    }

    const positions = rowPositions(items, line.right - line.left);
    const width = positions[0]!.width;
    const centre =
        line.align === "start" ? line.left + width / 2 : line.align === "end" ? line.right - width / 2 : (line.left + line.right) / 2;

    for (const [index, item] of items.entries()) {
        const { x, fit } = positions[index]!;
        const scale = item.scale * fit;

        poses.set(item.id, {
            x: centre + x,
            y: line.bottom - (TILE_HEIGHT * scale) / 2,
            rotation: 0,
            scale,
            face: item.face,
            z: z + index,
        });
    }
}

/** Your rack: hand and the tile you just drew along your edge of the screen, then your melds and flowers on their own line. */
export function rackLayout(model: RackContents, options: RackOptions): { poses: Map<number, Pose>; slots: RackSlot[] } {
    const area = options.area ?? DEFAULT_RACK;
    const { hand } = area;
    const pitch = TILE_WIDTH * hand.scale * 1.06;
    const height = TILE_HEIGHT * hand.scale;
    const drawnGap = pitch * 0.6;
    const split = hand.rows === 2 ? Math.ceil(options.order.length / 2) : options.order.length;
    const rows = [options.order.slice(0, split), options.order.slice(split)].filter((row) => row.length > 0);

    if (rows.length === 0) {
        rows.push([]);
    }

    const rowWidths = rows.map(
        (row, index) => row.length * pitch + (index === rows.length - 1 && options.drawn !== null ? drawnGap + pitch : 0),
    );
    const widest = Math.max(...rowWidths);
    const fit = Math.min(1, (hand.right - hand.left) / widest);
    const scale = hand.scale * fit;
    const step = pitch * fit;
    const rowGap = height * fit * 0.14;
    const left = (hand.left + hand.right) / 2 - (widest * fit) / 2;
    const poses = new Map<number, Pose>();
    const slots: RackSlot[] = [];
    let z = 5000;

    const place = (id: number, x: number, row: number) => {
        const y = hand.bottom - (rows.length - 1 - row) * (height * fit + rowGap) - (height * fit) / 2;
        const lift = id === options.selected ? height * fit * LIFT : 0;

        poses.set(id, { x, y: y - lift, rotation: 0, scale, face: "up", z: z++ });
        slots.push({ id, x, y, row, width: step });
    };

    for (const [row, ids] of rows.entries()) {
        for (const [index, id] of ids.entries()) {
            place(id, left + (index + 0.5) * step, row);
        }
    }

    if (options.drawn !== null) {
        const row = rows.length - 1;

        place(options.drawn, left + rows[row]!.length * step + drawnGap * fit + step / 2, row);
    }

    const meldScale = area.melds.scale;
    const meldPitch = TILE_WIDTH * meldScale * 1.03;
    const melds = meldItems(model, 0, meldScale, meldPitch, meldPitch * 0.35, 0);

    if (area.flowers === null) {
        const flowerScale = meldScale * 0.8;

        placeLine(
            [...melds, ...bonusItems(model, 0, flowerScale, TILE_WIDTH * flowerScale * 1.03, melds.length > 0 ? meldPitch * 0.6 : 0)],
            area.melds,
            poses,
            z,
        );
    } else {
        const flowerScale = area.flowers.scale;

        // With no melds yet, flowers take the meld line so they sit close to your hand.
        const line = melds.length > 0 ? area.flowers : { ...area.melds, scale: flowerScale };

        placeLine(melds, area.melds, poses, z);
        placeLine(bonusItems(model, 0, flowerScale, TILE_WIDTH * flowerScale * 1.03, 0), line, poses, z + 40);
    }

    return { poses, slots };
}

/** Where a dragged tile would land in your hand: the row nearest the pointer, then every tile in it to the pointer's left. */
export function dropIndex(slots: RackSlot[], hand: number[], id: number, point: { x: number; y: number }): number {
    const rowCentres = new Map(slots.map((slot) => [slot.row, slot.y]));
    let row = 0;
    let nearest = Infinity;

    for (const [index, y] of rowCentres) {
        if (Math.abs(y - point.y) < nearest) {
            nearest = Math.abs(y - point.y);
            row = index;
        }
    }

    const slotOf = new Map(slots.map((slot) => [slot.id, slot]));

    return hand.filter((other) => {
        const slot = other === id ? undefined : slotOf.get(other);

        return slot !== undefined && (slot.row < row || (slot.row === row && slot.x < point.x));
    }).length;
}

/** A bot's side: standing tiles, then what it has shown. At the end of the hand the tiles are laid face up. */
function seatRow(model: TableModel, seat: Seat, poses: Map<number, Pose>) {
    const face: Face = model.revealed ? "up" : "standing";
    const drawn = model.drawn[seat];
    const items: RowItem[] = [
        ...model.hands[seat]!.map((id) => ({ id, width: TILE_WIDTH + 1, scale: 1, face, gapBefore: 0 })),
        ...(drawn === null || drawn === undefined ? [] : [{ id: drawn, width: TILE_WIDTH + 1, scale: 1, face, gapBefore: 10 }]),
        ...meldItems(model, seat, 0.84, TILE_WIDTH * 0.84 + 1, 8, 26),
        ...bonusItems(model, seat, 0.72, TILE_WIDTH * 0.72 + 1, model.melds[seat]!.length > 0 ? 10 : 26),
    ];
    const positions = rowPositions(items, ROW_ROOM);

    for (const [index, item] of items.entries()) {
        const { x, fit } = positions[index]!;
        const pose = lying(seat, x, HAND_DISTANCE + (item.face === "standing" ? 4 : 0), item.face, item.scale * fit);

        poses.set(item.id, { ...pose, z: 4000 + index });
    }
}

/** Where every tile sits for a table state. */
export function layout(model: TableModel, options: RackOptions): Pose[] {
    const poses = new Map<number, Pose>();

    for (const [index, tile] of model.pond.entries()) {
        poses.set(tile.id, { x: CENTER + tile.x, y: CENTER + tile.y, rotation: tile.rotation, scale: 1, face: "up", z: 500 + index });
    }

    for (const seat of SEATS) {
        if (seat !== 0) {
            seatRow(model, seat, poses);
        }
    }

    for (const [id, pose] of rackLayout(model, options).poses) {
        poses.set(id, pose);
    }

    return Array.from({ length: TILE_COUNT }, (_, id) => poses.get(id) ?? restingPose(model, id));
}

export function faceOf(model: TableModel, id: number, pose: Pose): Tile | null {
    return pose.face === "up" ? (model.faces[id] ?? null) : null;
}
