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

/** Tiles spread over this much of the table while mixing; it stays clear of where the walls will stand. */
const PILE_RADIUS = 290;
const SHUFFLE_STEPS = 36;
const SHUFFLE_STEP_MS = 140;
/** How far a hand reaches, and how hard it shoves the tiles under it. */
const HAND_REACH = 85;
const HAND_SHOVE = 26;

/** The same angle as `rotation`, moved by whole turns to lie within half a turn of `target`. */
function nearestTurn(rotation: number, target: number): number {
    return rotation - Math.round((rotation - target) / 360) * 360;
}

function spread(state: { random: number }, radius: number): { x: number; y: number } {
    const angle = nextRandom(state) * Math.PI * 2;
    const distance = Math.sqrt(nextRandom(state)) * radius;

    return { x: Math.cos(angle) * distance, y: Math.sin(angle) * distance };
}

/** The tiles pushed face down and strewn all over the middle of the table. */
function heap(state: { random: number }): Pose[] {
    return Array.from({ length: TILE_COUNT }, (_, id) => {
        const point = spread(state, PILE_RADIUS * 0.9);

        return {
            x: CENTER + point.x,
            y: CENTER + point.y,
            rotation: nextRandom(state) * 360,
            scale: 1,
            face: "down" as const,
            z: 100 + id,
        };
    });
}

/**
 * Four players scrambling with both hands. Every tile keeps its own momentum and spin. Each step all eight hands slap down
 * somewhere in front of their player and shove the tiles under them, mostly counter-clockwise round the table but every shove
 * a little different. So the pile churns chaotically while drifting counter-clockwise overall. Each frame is one short
 * linear step, so the motion runs on without stopping.
 */
function shuffleFrames(start: Pose[], state: { random: number }): Frame[] {
    const tiles = start.map((pose) => ({
        pose: { ...pose },
        vx: 0,
        vy: 0,
        spin: (nextRandom(state) - 0.5) * 20,
    }));
    const frames: Frame[] = [];
    const seats: Seat[] = [0, 1, 2, 3];

    for (let step = 1; step <= SHUFFLE_STEPS; step++) {
        // Hands get going out of the push and ease off at the end.
        const effort = Math.min(1, step / 4, (SHUFFLE_STEPS + 1 - step) / 6);
        const hands = seats.flatMap((seat) =>
            [-1, 1].map((side) => {
                // Anywhere within reach in front of the player, that hand on its own side.
                const at = toStage(seat, side * (30 + nextRandom(state) * 150), 40 + nextRandom(state) * 230);
                const x = at.x - CENTER;
                const y = at.y - CENTER;
                const distance = Math.hypot(x, y) || 1;
                // Counter-clockwise on screen (y points down), turned up to 50 degrees either way, and usually scooping inward
                // a little, the way hands gather the tiles back in.
                const swerve = (nextRandom(state) - 0.5) * 1.75;
                const scoop = -0.2 + nextRandom(state) * 0.9;
                const tx = y / distance - (x / distance) * scoop;
                const ty = -x / distance - (y / distance) * scoop;
                const length = Math.hypot(tx, ty);

                return {
                    x,
                    y,
                    dx: (tx * Math.cos(swerve) - ty * Math.sin(swerve)) / length,
                    dy: (tx * Math.sin(swerve) + ty * Math.cos(swerve)) / length,
                    force: HAND_SHOVE * (0.6 + nextRandom(state) * 0.8),
                };
            }),
        );

        for (const tile of tiles) {
            const { pose } = tile;
            let x = pose.x - CENTER;
            let y = pose.y - CENTER;

            for (const hand of hands) {
                const gap = Math.hypot(x - hand.x, y - hand.y);

                if (gap < HAND_REACH) {
                    const push = (1 - gap / HAND_REACH) * hand.force * effort;

                    tile.vx += hand.dx * push + (nextRandom(state) - 0.5) * push;
                    tile.vy += hand.dy * push + (nextRandom(state) - 0.5) * push;
                    tile.spin += (nextRandom(state) - 0.5) * push * 5;
                }
            }

            const distance = Math.hypot(x, y) || 1;

            // Tiles shoved past the edge of the pile get swept back in.
            if (distance > PILE_RADIUS * 0.8) {
                tile.vx -= (x / distance) * (distance - PILE_RADIUS * 0.8) * 0.3;
                tile.vy -= (y / distance) * (distance - PILE_RADIUS * 0.8) * 0.3;
            }

            tile.vx = tile.vx * 0.7 + (nextRandom(state) - 0.5) * 4 * effort;
            tile.vy = tile.vy * 0.7 + (nextRandom(state) - 0.5) * 4 * effort;
            tile.spin *= 0.75;

            const speed = Math.hypot(tile.vx, tile.vy);

            if (speed > 70) {
                tile.vx *= 70 / speed;
                tile.vy *= 70 / speed;
            }

            x += tile.vx;
            y += tile.vy;
            pose.x = CENTER + x;
            pose.y = CENTER + y;
            pose.rotation += tile.spin;
        }

        // Tiles pass over and under each other as they are shoved about.
        if (step % 6 === 0) {
            for (const tile of tiles) {
                tile.pose.z = 100 + Math.floor(nextRandom(state) * TILE_COUNT);
            }
        }

        frames.push({ poses: tiles.map((tile) => ({ ...tile.pose })), duration: SHUFFLE_STEP_MS, hold: SHUFFLE_STEP_MS, linear: true });
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
