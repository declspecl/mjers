import { lying, type Pose, STACKS_PER_WALL, wallPose, WALL_RING } from "./geometry";
import { layout, restingPose } from "./layout";
import { type DealPlan, type TableModel, TILE_COUNT } from "./model";
import { nextRandom } from "./random";
import type { Seat } from "@mjers/mjers/types";

/** One step of the opening: where every tile goes, how long the move takes, and what the dice show. */
export interface Frame {
    poses: Pose[];
    /** Move time in milliseconds. */
    duration: number;
    /** Per-tile start delays, for rows that build one tile after another. */
    delays?: number[];
    /** Time from the start of this frame to the next one. */
    hold: number;
    dice?: DiceView;
    /** The seat the dealer marker sits at, once it is known. */
    dealer?: Seat;
}

export interface DiceView {
    values: number[];
    rolling: boolean;
    /** Who threw them; they land on the felt in front of that seat. */
    roller: Seat;
}

export interface Rolls {
    /** The first hand's roll that picks the dealer, counted counter-clockwise from you. */
    dealer?: number[];
    /** The dealer's roll that picks where the wall breaks. */
    wall: number[];
}

const ROW_PITCH = 40;
const ROW_START = 258;

function scatter(state: { random: number }, radius: number): Pose[] {
    return Array.from({ length: TILE_COUNT }, (_, id) => {
        const angle = nextRandom(state) * Math.PI * 2;
        const distance = Math.sqrt(nextRandom(state)) * radius;
        const x = 500 + Math.cos(angle) * distance;
        const y = 500 + Math.sin(angle) * distance;

        return { x, y, rotation: nextRandom(state) * 360, scale: 1, face: "down" as const, z: 100 + id };
    });
}

/**
 * Each side lays four rows of nine. The first two rows stack into the outer half of the wall in the corner on that side's right
 * (stacks 0 to 8); the last two stack into the inner half of the wall in the corner on its left (stacks 9 to 17), so each corner
 * wall joins halves from the two sides that meet there.
 */
function rowSlot(id: number): { seat: Seat; row: number; x: number } {
    const stack = Math.floor(id / 2);
    const owner = WALL_RING[Math.floor(stack / STACKS_PER_WALL)]!;
    const position = stack % STACKS_PER_WALL;
    const layer = id % 2;

    if (position < 9) {
        return { seat: owner, row: layer, x: (4 - position) * 35 };
    }

    return { seat: ((owner + 1) % 4) as Seat, row: 2 + layer, x: -(4 - (position - 9)) * 35 };
}

function rows(): Pose[] {
    return Array.from({ length: TILE_COUNT }, (_, id) => {
        const { seat, row, x } = rowSlot(id);

        return lying(seat, x, ROW_START + row * ROW_PITCH, "down");
    });
}

function rowDelays(): number[] {
    return Array.from({ length: TILE_COUNT }, (_, id) => {
        const { row, x } = rowSlot(id);

        return (row * 9 + Math.round(4 - x / 35)) * 14;
    });
}

/** Rows one and two stack into one half wall, rows three and four into the other. */
function stacked(): Pose[] {
    return Array.from({ length: TILE_COUNT }, (_, id) => {
        const { seat, row, x } = rowSlot(id);
        const half = Math.floor(row / 2);
        const top = row % 2 === 0;
        const pose = lying(seat, x, ROW_START + ROW_PITCH / 2 + half * ROW_PITCH * 2, "down");

        return { ...pose, y: pose.y - (top ? 7 : 0), z: (top ? 3000 : 2000) + Math.round(pose.y) };
    });
}

/** The table with some hands dealt so far: tiles not yet taken stay in the wall. */
function partial(model: TableModel, hands: number[][], bonus: number[][], faceUp: boolean): Pose[] {
    const view: TableModel = {
        ...model,
        hands,
        bonus,
        back: 0,
        drawn: [null, null, null, null],
        melds: [[], [], [], []],
        pond: [],
        pending: [],
        revealed: false,
    };
    const poses = layout(view, { order: hands[0]!, drawn: null, selected: null });

    if (!faceUp) {
        for (const id of hands[0]!) {
            poses[id] = { ...poses[id]!, face: "down" };
        }
    }

    return poses;
}

/** The whole opening: mixing, building the walls, the dice, the deal, and replacing flowers. */
export function setupFrames(model: TableModel, plan: DealPlan, rolls: Rolls, final: Pose[], random: { random: number }): Frame[] {
    const frames: Frame[] = [{ poses: scatter(random, 250), duration: 520, hold: 560 }];

    for (let pass = 0; pass < 3; pass++) {
        frames.push({
            poses: scatter(random, 235),
            duration: 380,
            hold: 400,
            delays: Array.from({ length: TILE_COUNT }, () => nextRandom(random) * 90),
        });
    }

    const built = Array.from({ length: TILE_COUNT }, (_, id) => wallPose(id));

    frames.push(
        { poses: rows(), duration: 240, delays: rowDelays(), hold: 36 * 14 + 320 },
        { poses: stacked(), duration: 260, hold: 340 },
        { poses: built, duration: 360, hold: 420 },
    );

    if (rolls.dealer) {
        frames.push(
            { poses: built, duration: 0, hold: 700, dice: { values: rolls.dealer, rolling: true, roller: 0 } },
            { poses: built, duration: 0, hold: 900, dice: { values: rolls.dealer, rolling: false, roller: 0 }, dealer: model.dealer },
        );
    }

    frames.push(
        { poses: built, duration: 0, hold: 700, dice: { values: rolls.wall, rolling: true, roller: model.dealer }, dealer: model.dealer },
        { poses: built, duration: 0, hold: 700, dice: { values: rolls.wall, rolling: false, roller: model.dealer }, dealer: model.dealer },
    );

    // The setter counts stacks from the front of their left wall and turns them down beside their right wall as the flower wall.
    const turnedDown = Array.from({ length: TILE_COUNT }, (_, id) => restingPose(model, id));

    frames.push({
        poses: turnedDown,
        duration: 520,
        delays: turnedDown.map((pose, id) => (pose === built[id] || model.flowerSlots[id] === -1 ? 0 : (model.flowerSlots[id] ?? 0) * 18)),
        hold: 520 + rolls.wall.reduce((sum, value) => sum + value, 0) * 36,
        dice: { values: rolls.wall, rolling: false, roller: model.dealer },
        dealer: model.dealer,
    });

    const hands: number[][] = [[], [], [], []];
    const none: number[][] = [[], [], [], []];
    const step = (seat: Seat, ids: number[]) => {
        hands[seat]!.push(...ids);
        frames.push({
            poses: partial(
                model,
                hands.map((hand) => [...hand]),
                none,
                false,
            ),
            duration: 230,
            hold: 250,
            dealer: model.dealer,
        });
    };

    for (let round = 0; round < 3; round++) {
        for (const seat of plan.order) {
            step(seat, plan.dealt[seat]!.slice(round * 4, round * 4 + 4));
        }
    }

    const dealer = plan.order[0]!;

    step(dealer, [plan.dealt[dealer]![12]!, plan.dealt[dealer]![13]!]);

    for (const seat of plan.order.slice(1)) {
        step(seat, [plan.dealt[seat]![12]!]);
    }

    frames.push({ poses: partial(model, hands, none, true), duration: 320, hold: 520, dealer: model.dealer });

    const bonus: number[][] = [[], [], [], []];

    for (const seat of plan.order) {
        if (plan.flowers[seat]!.length === 0) {
            continue;
        }

        hands[seat] = [...hands[seat]!.filter((id) => !plan.flowers[seat]!.includes(id)), ...plan.replacements[seat]!];
        bonus[seat] = plan.flowers[seat]!;
        frames.push({
            poses: partial(
                model,
                hands.map((hand) => [...hand]),
                bonus.map((tiles) => [...tiles]),
                true,
            ),
            duration: 320,
            hold: 520,
            dealer: model.dealer,
        });
    }

    frames.push({ poses: final, duration: 300, hold: 320, dealer: model.dealer });

    return frames;
}
