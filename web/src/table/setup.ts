import { CENTER, lying, type Pose, STACKS_PER_WALL, toStage, wallPose, WALL_RING } from "./geometry";
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
    /** Linear frames chain into one continuous motion, as the shuffle does. */
    linear?: boolean;
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

const ROW_PITCH = 40;
const ROW_START = 258;

/** Opening pace: every step of the opening runs this much slower than its base timing. */
const PACE = 1.4;

function ms(base: number): number {
    return Math.round(base * PACE);
}

const PILE_RADIUS = 235;
const SHUFFLE_STEPS = 34;
const SHUFFLE_STEP_MS = 150;
/** The whole pile turns this far counter-clockwise each step (screen y points down, so counter-clockwise is negative). */
const PILE_TURN = -0.11;
/** Each hand circles this far each step, also counter-clockwise. */
const HAND_TURN = -0.62;
const HAND_CIRCLE = 62;
const HAND_REACH = 78;

/** The same angle as `rotation`, moved by whole turns to lie within half a turn of `target`. */
function nearestTurn(rotation: number, target: number): number {
    return rotation - Math.round((rotation - target) / 360) * 360;
}

/** The tiles pushed face down into one heap in the middle of the table. */
function heap(state: { random: number }): Pose[] {
    return Array.from({ length: TILE_COUNT }, (_, id) => {
        const angle = nextRandom(state) * Math.PI * 2;
        const distance = Math.sqrt(nextRandom(state)) * PILE_RADIUS;

        return {
            x: CENTER + Math.cos(angle) * distance,
            y: CENTER + Math.sin(angle) * distance,
            rotation: nextRandom(state) * 360,
            scale: 1,
            face: "down" as const,
            z: 100 + id,
        };
    });
}

/**
 * Four players mixing with both hands. Each hand circles counter-clockwise over the heap in front of its player and drags the
 * tiles under it along, while the heap as a whole turns counter-clockwise, so the tiles spiral round the table from hand to hand.
 * Every frame is one short linear step of that motion.
 */
function shuffleFrames(start: Pose[], state: { random: number }): Frame[] {
    const tiles = start.map((pose) => ({ ...pose }));
    const hands = ([0, 1, 2, 3] as Seat[]).flatMap((seat) =>
        [-1, 1].map((side) => ({
            anchor: toStage(seat, side * 92, 150),
            phase: seat * (Math.PI / 2) + (side > 0 ? Math.PI : 0) + nextRandom(state) * 0.6,
        })),
    );
    const frames: Frame[] = [];

    for (let step = 1; step <= SHUFFLE_STEPS; step++) {
        // Hands speed up out of the push and ease off at the end.
        const effort = Math.min(1, step / 5, (SHUFFLE_STEPS + 1 - step) / 5);
        const turn = PILE_TURN * effort;
        const at = hands.map(({ anchor, phase }) => {
            const angle = phase + step * HAND_TURN;
            const previous = angle - HAND_TURN * effort;

            return {
                x: anchor.x + Math.cos(angle) * HAND_CIRCLE,
                y: anchor.y + Math.sin(angle) * HAND_CIRCLE,
                dx: (Math.cos(angle) - Math.cos(previous)) * HAND_CIRCLE,
                dy: (Math.sin(angle) - Math.sin(previous)) * HAND_CIRCLE,
            };
        });

        for (const tile of tiles) {
            let x = tile.x - CENTER;
            let y = tile.y - CENTER;
            const cos = Math.cos(turn);
            const sin = Math.sin(turn);

            [x, y] = [x * cos - y * sin, x * sin + y * cos];
            tile.rotation += (turn * 180) / Math.PI;

            for (const hand of at) {
                const gap = Math.hypot(CENTER + x - hand.x, CENTER + y - hand.y);
                const grip = Math.exp(-(gap * gap) / (2 * HAND_REACH * HAND_REACH));

                x += hand.dx * grip * 1.1;
                y += hand.dy * grip * 1.1;
                tile.rotation += (HAND_TURN * 180 * grip * effort) / Math.PI;
            }

            x += (nextRandom(state) - 0.5) * 6;
            y += (nextRandom(state) - 0.5) * 6;

            const distance = Math.hypot(x, y);

            if (distance > PILE_RADIUS) {
                const pull = (PILE_RADIUS + (distance - PILE_RADIUS) * 0.3) / distance;

                x *= pull;
                y *= pull;
            }

            tile.x = CENTER + x;
            tile.y = CENTER + y;
        }

        frames.push({ poses: tiles.map((tile) => ({ ...tile })), duration: SHUFFLE_STEP_MS, hold: SHUFFLE_STEP_MS, linear: true });
    }

    return frames;
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

        return ms((row * 9 + Math.round(4 - x / 35)) * 14);
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
export function setupFrames(model: TableModel, plan: DealPlan, roll: number[], final: Pose[], random: { random: number }): Frame[] {
    const pushed = heap(random);
    const frames: Frame[] = [{ poses: pushed, duration: ms(560), hold: ms(600) }, ...shuffleFrames(pushed, random)];

    const built = Array.from({ length: TILE_COUNT }, (_, id) => wallPose(id));
    const lined = rows();
    const mixed = frames.at(-1)!.poses;

    frames.push(
        // The spiral leaves tiles turned many times over; drop whole turns, which looks the same, so none spins back on the way out.
        { poses: mixed.map((pose, id) => ({ ...pose, rotation: nearestTurn(pose.rotation, lined[id]!.rotation) })), duration: 0, hold: 40 },
        { poses: lined, duration: ms(240), delays: rowDelays(), hold: ms(36 * 14 + 320) },
        { poses: stacked(), duration: ms(260), hold: ms(340) },
        { poses: built, duration: ms(360), hold: ms(420) },
    );

    // The dealer throws two dice. Counting the dealer as one, the total goes counter-clockwise round the seats to the setter, who
    // then counts that many stacks into the wall.
    const total = roll.reduce((sum, value) => sum + value, 0);
    const dice = (rolling: boolean): DiceView => ({ values: roll, rolling, roller: model.dealer });

    frames.push(
        { poses: built, duration: 0, hold: ms(400), dealer: model.dealer },
        { poses: built, duration: 0, hold: ms(800), dice: dice(true), dealer: model.dealer },
        { poses: built, duration: 0, hold: ms(900), dice: dice(false), dealer: model.dealer },
    );

    // The setter counts stacks from the front of their left wall and turns them down beside their right wall as the flower wall.
    const turnedDown = Array.from({ length: TILE_COUNT }, (_, id) => restingPose(model, id));

    frames.push({
        poses: turnedDown,
        duration: ms(520),
        delays: turnedDown.map((pose, id) =>
            pose === built[id] || model.flowerSlots[id] === -1 ? 0 : ms((model.flowerSlots[id] ?? 0) * 18),
        ),
        hold: ms(520 + total * 36),
        dice: dice(false),
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
            duration: ms(230),
            hold: ms(250),
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

    frames.push({ poses: partial(model, hands, none, true), duration: ms(320), hold: ms(520), dealer: model.dealer });

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
            duration: ms(320),
            hold: ms(520),
            dealer: model.dealer,
        });
    }

    frames.push({ poses: final, duration: ms(300), hold: ms(320), dealer: model.dealer });

    return frames;
}
