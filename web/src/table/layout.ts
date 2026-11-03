import { CENTER, type Face, flowerWallPose, lying, type Pose, STAGE_WIDTH, TILE_HEIGHT, TILE_WIDTH, wallPose } from "./geometry";
import { SEATS, type TableModel, TILE_COUNT } from "./model";
import type { Seat, Tile } from "@mjers/mjers/types";

/** How far a seat's hand sits from the middle of the table, along its edge. */
const HAND_DISTANCE = 448;
/** The longest a seat's row of tiles may run before it reaches the corner walls. */
const ROW_ROOM = 680;
const RACK_BASELINE = 1150;
const RACK_SCALE = 1.95;

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
function rowPositions(items: RowItem[], available: number): { x: number; fit: number }[] {
    const total = items.reduce((sum, item) => sum + item.width + item.gapBefore, 0);
    const fit = total > available ? available / total : 1;
    let cursor = (-total * fit) / 2;

    return items.map((item) => {
        cursor += item.gapBefore * fit;

        const x = cursor + (item.width * fit) / 2;

        cursor += item.width * fit;

        return { x, fit };
    });
}

function meldItems(model: TableModel, seat: Seat, scale: number, pitch: number, meldGap: number, firstGap: number): RowItem[] {
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

function bonusItems(model: TableModel, seat: Seat, scale: number, pitch: number, firstGap: number): RowItem[] {
    return model.bonus[seat]!.map((id, index) => ({ id, width: pitch, scale, face: "up" as const, gapBefore: index === 0 ? firstGap : 0 }));
}

export interface RackOptions {
    /** Your hand in the order to show it, which differs from the model while you drag a tile. */
    order: number[];
    drawn: number | null;
    selected: number | null;
}

export interface RackSlot {
    id: number;
    x: number;
    width: number;
}

/** Your rack below the table: hand, the tile you just drew, then your melds and flowers to the right. */
export function rackLayout(model: TableModel, options: RackOptions): { poses: Map<number, Pose>; slots: RackSlot[] } {
    const handWidth = TILE_WIDTH * RACK_SCALE + 2;
    const face: Face = "up";
    const items: RowItem[] = [
        ...options.order.map((id) => ({
            id,
            width: handWidth,
            scale: RACK_SCALE,
            face,
            gapBefore: 0,
            lift: id === options.selected ? 18 : 0,
        })),
        ...(options.drawn === null
            ? []
            : [
                  {
                      id: options.drawn,
                      width: handWidth,
                      scale: RACK_SCALE,
                      face,
                      gapBefore: 22,
                      lift: options.drawn === options.selected ? 18 : 0,
                  },
              ]),
        ...meldItems(model, 0, 1.3, TILE_WIDTH * 1.3 + 1, 12, 40),
        ...bonusItems(model, 0, 1, TILE_WIDTH + 1, model.melds[0]!.length > 0 ? 16 : 40),
    ];
    const positions = rowPositions(items, STAGE_WIDTH - 24);
    const poses = new Map<number, Pose>();
    const slots: RackSlot[] = [];

    for (const [index, item] of items.entries()) {
        const { x, fit } = positions[index]!;
        const scale = item.scale * fit;
        const y = RACK_BASELINE - (TILE_HEIGHT * scale) / 2 - (item.lift ?? 0);

        poses.set(item.id, { x: CENTER + x, y, rotation: 0, scale, face: item.face, z: 5000 + index });

        if (item.scale === RACK_SCALE) {
            slots.push({ id: item.id, x: CENTER + x, width: item.width * fit });
        }
    }

    return { poses, slots };
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
