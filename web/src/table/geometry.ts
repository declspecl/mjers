import type { Seat } from "@mjers/mjers/types";

/** Stage coordinates: a 1000 unit square table with your rack below it. Everything scales together. */
export const STAGE_WIDTH = 1000;
export const STAGE_HEIGHT = 1160;
export const CENTER = 500;
export const TILE_WIDTH = 34;
export const TILE_HEIGHT = 45;
const STACK_PITCH = 35;
/**
 * Walls run from the corners inward like a pinwheel. In a seat's frame its wall starts in the corner on its right and slants across
 * in front of it, ending well short of the centre so the middle stays open for the discards.
 */
const WALL_START = { x: 445, y: 445 };
const WALL_AIM = { x: -99, y: 191 };
const WALL_LENGTH = Math.hypot(WALL_AIM.x - WALL_START.x, WALL_AIM.y - WALL_START.y);
const WALL_STEP = { x: (WALL_AIM.x - WALL_START.x) / WALL_LENGTH, y: (WALL_AIM.y - WALL_START.y) / WALL_LENGTH };
/** Turns a tile so it lies across its wall. */
const WALL_TURN = (Math.atan2(WALL_STEP.y, WALL_STEP.x) * 180) / Math.PI - 180;
/** A stacked tile sits this far up the screen from the one beneath it. */
const LAYER_LIFT = 7;

export type Face = "up" | "down" | "standing";

export interface Pose {
    x: number;
    y: number;
    rotation: number;
    scale: number;
    face: Face;
    z: number;
}

/** The rotation that turns a seat's own view (its hand toward +y, its right toward +x) into the stage. */
export function seatAngle(seat: Seat): number {
    return [0, -90, 180, 90][seat]!;
}

export function toStage(seat: Seat, x: number, y: number): { x: number; y: number } {
    const radians = (seatAngle(seat) * Math.PI) / 180;

    return {
        x: CENTER + x * Math.cos(radians) - y * Math.sin(radians),
        y: CENTER + x * Math.sin(radians) + y * Math.cos(radians),
    };
}

/** A tile lying on the table in a seat's frame. */
export function lying(seat: Seat, x: number, y: number, face: Face, scale = 1, extraRotation = 0): Pose {
    const point = toStage(seat, x, y);

    return { ...point, rotation: seatAngle(seat) + extraRotation, scale, face, z: 1000 + Math.round(point.y) };
}

/** A tile in its built wall. Even ids are the top of a stack, and stack 0 of each wall sits in its corner. */
export function wallPose(id: number, shift = 0): Pose {
    const stack = stackOf(id);
    const seat = WALL_RING[Math.floor(stack / STACKS_PER_WALL)]!;
    const position = stack % STACKS_PER_WALL;
    const top = id % 2 === 0;
    const along = (position + 0.5) * STACK_PITCH - shift;
    const point = toStage(seat, WALL_START.x + WALL_STEP.x * along, WALL_START.y + WALL_STEP.y * along);

    return {
        x: point.x,
        y: point.y - (top ? LAYER_LIFT : 0),
        rotation: seatAngle(seat) + WALL_TURN,
        scale: 1,
        face: "down",
        z: (top ? 3000 : 2000) + Math.round(point.y),
    };
}

export const STACKS_PER_WALL = 18;
/** The walls in the order the live wall is drawn from, which runs clockwise: yours, then left, across and right. Each seat's wall
 * sits in the corner on its right. */
export const WALL_RING: readonly Seat[] = [0, 3, 2, 1];

export function stackOf(id: number): number {
    return Math.floor(id / 2);
}

/** The centres of every wall stack, so discards can land clear of the walls. */
export const WALL_CENTRES: { x: number; y: number }[] = Array.from({ length: 72 }, (_, stack) => {
    const pose = wallPose(stack * 2 + 1);

    return { x: pose.x - CENTER, y: pose.y - CENTER };
});
