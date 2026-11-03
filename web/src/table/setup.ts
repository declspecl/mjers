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

/** Tiles wash over the whole felt, inside this half-width of the square table and this radius at its corners. */
const TABLE_REACH = 440;
const CORNER_REACH = 540;
/** The pile the tiles are pushed into before the washing starts. */
const HEAP_RADIUS = 280;
/** Tiles are treated as discs this wide when they knock into each other. */
const TILE_RADIUS = 20;
/** A flat hand with its fingers spread covers this much; the forearm behind it plows a band this wide. */
const PALM_RADIUS = 105;
const ARM_RADIUS = 42;
/** Each hand sweeps a loop this big across its player's half of the table, once every this many milliseconds. */
const HAND_LOOP = 210;
const HAND_PERIOD = 900;
const WASH_MS = 4500;
/** The pace the swirl drags the whole mass counter-clockwise toward, in radians a second; friction keeps tiles well short of it. */
const SWIRL = 10;
const FRAME_MS = 50;
const SUBSTEPS = 5;

/** The same angle as `rotation`, moved by whole turns to lie within half a turn of `target`. */
function nearestTurn(rotation: number, target: number): number {
    return rotation - Math.round((rotation - target) / 360) * 360;
}

interface Body {
    x: number;
    y: number;
    vx: number;
    vy: number;
    rotation: number;
    /** Degrees of turn per unit of distance moved; tiles knocked about turn as they slide. */
    twist: number;
}

/** Keeps a tile on the felt, bouncing it off the rim. */
function contain(body: Body) {
    for (const axis of ["x", "y"] as const) {
        const speed = axis === "x" ? "vx" : "vy";

        if (Math.abs(body[axis]) > TABLE_REACH) {
            body[axis] = Math.sign(body[axis]) * TABLE_REACH;
            body[speed] = -body[speed] * 0.5;
        }
    }

    const distance = Math.hypot(body.x, body.y);

    if (distance > CORNER_REACH) {
        body.x *= CORNER_REACH / distance;
        body.y *= CORNER_REACH / distance;
    }
}

/** Pushes overlapping tiles apart, the way solid tiles knock each other aside. */
function separate(bodies: Body[]) {
    const reach = TILE_RADIUS * 2;

    for (let pass = 0; pass < 2; pass++) {
        for (let i = 0; i < bodies.length; i++) {
            const a = bodies[i]!;

            for (let j = i + 1; j < bodies.length; j++) {
                const b = bodies[j]!;
                const dx = b.x - a.x;
                const dy = b.y - a.y;

                if (Math.abs(dx) >= reach || Math.abs(dy) >= reach) {
                    continue;
                }

                const gap = Math.hypot(dx, dy);

                if (gap >= reach || gap === 0) {
                    continue;
                }

                const shove = (reach - gap) / gap / 2;

                a.x -= dx * shove;
                a.y -= dy * shove;
                b.x += dx * shove;
                b.y += dy * shove;
                // Knocks pass some speed between the two.
                const vx = (a.vx - b.vx) * 0.15;
                const vy = (a.vy - b.vy) * 0.15;

                a.vx -= vx;
                a.vy -= vy;
                b.vx += vx;
                b.vy += vy;
            }
        }

        for (const body of bodies) {
            contain(body);
        }
    }
}

/** The tiles pushed face down into one loose layer in the middle of the table. Positions are from the table centre. */
function heapBodies(state: { random: number }): Body[] {
    const bodies = Array.from({ length: TILE_COUNT }, () => {
        const angle = nextRandom(state) * Math.PI * 2;
        const distance = Math.sqrt(nextRandom(state)) * HEAP_RADIUS;

        return {
            x: Math.cos(angle) * distance,
            y: Math.sin(angle) * distance,
            vx: 0,
            vy: 0,
            rotation: nextRandom(state) * 360,
            twist: (nextRandom(state) - 0.5) * 1.4,
        };
    });

    for (let settle = 0; settle < 6; settle++) {
        separate(bodies);
    }

    return bodies;
}

function posesOf(bodies: Body[]): Pose[] {
    return bodies.map((body, id) => ({
        x: CENTER + body.x,
        y: CENTER + body.y,
        rotation: body.rotation,
        scale: 1,
        face: "down" as const,
        z: 100 + id,
    }));
}

/**
 * Washing the tiles: all four players lean in and sweep both hands, palms flat and arms behind them, in huge counter-clockwise
 * loops across their half of the table. Each hand carries a big armful of tiles with it, each arm plows the tiles in its way,
 * and together they swirl the whole mass round and round the table. Tiles are solid and knock each other aside. A small
 * simulation runs the motion, sampled into short linear frames.
 */
function washFrames(bodies: Body[], state: { random: number }): Frame[] {
    const hands = ([0, 1, 2, 3] as Seat[]).flatMap((seat) =>
        [-1, 1].map((side) => ({
            seat,
            side,
            // A player's two hands sweep half a loop apart, so one is always pushing while the other comes back round.
            phase: seat * 1.3 + (side > 0 ? Math.PI : 0) + nextRandom(state) * 0.5,
            period: HAND_PERIOD * (0.9 + nextRandom(state) * 0.2),
            shoulder: toStage(seat, side * 150, 600),
        })),
    );
    const frames: Frame[] = [];
    const dt = FRAME_MS / SUBSTEPS;

    for (let time = FRAME_MS; time <= WASH_MS; time += FRAME_MS) {
        for (let sub = 1; sub <= SUBSTEPS; sub++) {
            const now = time - FRAME_MS + sub * dt;
            // Hands get going quickly and lift off at the end.
            const effort = Math.min(1, now / 300, (WASH_MS - now) / 400);
            const palms = hands.map((hand) => {
                // Counter-clockwise on screen, where y points down, means the angle runs backwards.
                const angle = hand.phase - (now / hand.period) * Math.PI * 2;
                const speed = ((Math.PI * 2 * HAND_LOOP) / hand.period) * dt * effort;
                const centre = toStage(hand.seat, hand.side * 110, 210);

                const x = centre.x - CENTER + Math.cos(angle) * HAND_LOOP;
                const y = centre.y - CENTER + Math.sin(angle) * HAND_LOOP;
                const reach = Math.hypot(x, y) || 1;
                // Players bear down on the sweep that carries tiles counter-clockwise round the table, along (y, -x), and
                // lighten their hands coming back.
                const along = Math.sin(angle) * (y / reach) + Math.cos(angle) * (x / reach);

                return {
                    x,
                    y,
                    vx: Math.sin(angle) * speed,
                    vy: -Math.cos(angle) * speed,
                    press: 0.2 + 0.8 * Math.max(0, along),
                    sx: hand.shoulder.x - CENTER,
                    sy: hand.shoulder.y - CENTER,
                };
            });

            for (const body of bodies) {
                // The swirl: counter-clockwise round the centre is along (y, -x).
                const distance = Math.hypot(body.x, body.y);
                const swirl = SWIRL * (dt / 1000) * effort * Math.min(1, distance / 180);
                // Players keep raking tiles off the rim back into the middle.
                const rake = distance > 300 ? ((distance - 300) / distance) * 0.004 * effort : 0;

                body.vx += (body.y * swirl - body.vx) * 0.1 - body.x * rake;
                body.vy += (-body.x * swirl - body.vy) * 0.1 - body.y * rake;

                for (const palm of palms) {
                    const gap = Math.hypot(body.x - palm.x, body.y - palm.y);

                    if (gap < PALM_RADIUS) {
                        // Friction under the palm drags the tile along with the hand, hardest under its middle.
                        const grip = (0.35 + 0.35 * (1 - gap / PALM_RADIUS)) * palm.press;

                        body.vx += (palm.vx - body.vx) * grip;
                        body.vy += (palm.vy - body.vy) * grip;
                        continue;
                    }

                    // The forearm, from the shoulder at the table edge to the wrist, shoves tiles along with it.
                    const ax = palm.x - palm.sx;
                    const ay = palm.y - palm.sy;
                    const along = Math.max(0, Math.min(1, ((body.x - palm.sx) * ax + (body.y - palm.sy) * ay) / (ax * ax + ay * ay)));
                    const nx = body.x - (palm.sx + ax * along);
                    const ny = body.y - (palm.sy + ay * along);
                    const off = Math.hypot(nx, ny);

                    // An arm only plows while its hand bears down; coming back, it lifts over the tiles.
                    if (palm.press > 0.5 && off < ARM_RADIUS && off > 0) {
                        const push = (ARM_RADIUS - off) / off;

                        body.x += nx * push;
                        body.y += ny * push;
                        body.vx += (palm.vx * along - body.vx) * 0.3;
                        body.vy += (palm.vy * along - body.vy) * 0.3;
                    }
                }

                body.vx *= 0.9;
                body.vy *= 0.9;
                body.x += body.vx;
                body.y += body.vy;
                body.rotation += Math.hypot(body.vx, body.vy) * body.twist;
            }

            separate(bodies);
        }

        frames.push({ poses: posesOf(bodies), duration: FRAME_MS, hold: FRAME_MS, linear: true });
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
    const bodies = heapBodies(random);
    const frames: Frame[] = [{ poses: posesOf(bodies), duration: 380, hold: 420 }, ...washFrames(bodies, random)];

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
