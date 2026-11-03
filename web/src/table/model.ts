import { expandCounts, meldTiles } from "../engine/tiles";
import { STACKS_PER_WALL, WALL_CENTRES, WALL_RING } from "./geometry";
import { nextRandom, randomInt, shuffle } from "./random";
import type { Hand, Meld, Observation, Seat, Tile } from "@mjers/mjers/types";

/**
 * The physical table: where each of the 144 tiles is. The engine only knows counts, so this follows every tile from its place in
 * the wall to a hand, the discard pile, or a meld. A tile's id is the wall slot it was built into.
 */

export const TILE_COUNT = 144;
export const STACKS = 72;
/** Replacement draws come from this many tiles at the tail of the wall. */
export const DEAD_TILES = 16;
export const SEATS: readonly Seat[] = [0, 1, 2, 3];
/** Tiles each seat takes per round of the deal, matching the engine. */
const DEAL_ROUNDS = [4, 4, 4, 1] as const;

export interface MeldModel {
    meld: Meld;
    ids: number[];
}

/** A discard lying in the middle of the table, in board coordinates from the centre. */
export interface PondTile {
    id: number;
    x: number;
    y: number;
    rotation: number;
    seat: Seat;
}

/** A tile the engine already drew for you that is still in the wall until you take it. */
export interface DrawStep {
    id: number;
    source: "front" | "back";
    bonus: boolean;
}

export interface TableModel {
    dealer: Seat;
    /** The ring index of the first live stack, just after the break. */
    breakStack: number;
    /** Tiles taken from the live end and from the tail. */
    front: number;
    back: number;
    /** Faces you can see. Null for tiles that are face down to you. */
    faces: (Tile | null)[];
    hands: number[][];
    /** The tile each seat just drew, held apart from the hand. */
    drawn: (number | null)[];
    melds: MeldModel[][];
    bonus: number[][];
    pond: PondTile[];
    /** The tile id of each engine discard, by index. */
    discardIds: number[];
    claimed: boolean[];
    pending: DrawStep[];
    revealed: boolean;
    random: number;
}

export interface DealPlan {
    /** Tile ids each seat was dealt, in the order it took them. */
    dealt: number[][];
    /** Which of those were flowers or seasons, set aside after the deal. */
    flowers: number[][];
    /** The replacements each seat drew from the tail for its flowers. */
    replacements: number[][];
    order: Seat[];
}

/** Tiles that have left the wall and are visibly somewhere else. Tiles waiting for you to draw them still count as wall. */
export function placedIds(model: TableModel): Set<number> {
    return new Set([
        ...model.hands.flat(),
        ...model.drawn.filter((id): id is number => id !== null),
        ...model.melds.flat().flatMap((meld) => meld.ids),
        ...model.bonus.flat(),
        ...model.pond.map((tile) => tile.id),
    ]);
}

/** The tile at the n-th draw from the live end: top of a stack, then the one underneath. */
export function frontId(breakStack: number, n: number): number {
    return ((breakStack + Math.floor(n / 2)) % STACKS) * 2 + (n % 2);
}

/** The tile at the m-th draw from the tail, which runs back from the break. */
export function backId(breakStack: number, m: number): number {
    return ((((breakStack - 1 - Math.floor(m / 2)) % STACKS) + STACKS) % STACKS) * 2 + (m % 2);
}

/**
 * Where the live wall starts. The dealer's roll counts seats counter-clockwise starting from the dealer as one, so a 5 comes back
 * to the dealer. That seat breaks its left wall: it counts that many stacks in from the front (the corner end) of the wall at its
 * left corner, and the live wall starts just after them.
 */
export function breakStackFor(dealer: Seat, total: number): number {
    const setter = (dealer + total - 1) % 4;
    const leftWall = ((setter + 3) % 4) as Seat;
    const ring = WALL_RING.indexOf(leftWall);

    return (ring * STACKS_PER_WALL + total) % STACKS;
}

export function seatOrder(dealer: Seat): Seat[] {
    return SEATS.map((offset) => ((dealer + offset) % 4) as Seat);
}

function kongCount(melds: Meld[]): number {
    return melds.filter((meld) => "kong" in meld).length;
}

/** Tiles drawn from the tail so far: one per flower or season and one per kong. */
function backCount(observation: Observation): number {
    return observation.players.reduce((sum, player) => sum + player.bonus.length + kongCount(player.melds), 0);
}

function frontCount(observation: Observation): number {
    return TILE_COUNT - observation.wall_remaining - backCount(observation);
}

/** The table right after the deal, with every flower already replaced. */
export function createModel(observation: Observation, breakStack: number, seed: number): { model: TableModel; plan: DealPlan } {
    const dealer = observation.dealer;
    const order = seatOrder(dealer);
    const dealt: number[][] = [[], [], [], []];
    let n = 0;

    for (const take of DEAL_ROUNDS) {
        for (const seat of order) {
            for (let index = 0; index < take; index++) {
                dealt[seat]!.push(frontId(breakStack, n++));
            }
        }
    }

    dealt[dealer]!.push(frontId(breakStack, n++));

    const model: TableModel = {
        dealer,
        breakStack,
        front: frontCount(observation),
        back: backCount(observation),
        faces: Array.from({ length: TILE_COUNT }, () => null),
        hands: [[], [], [], []],
        drawn: [null, null, null, null],
        melds: [[], [], [], []],
        bonus: [[], [], [], []],
        pond: [],
        discardIds: [],
        claimed: [],
        pending: [],
        revealed: false,
        random: Math.trunc(seed),
    };
    const flowers: number[][] = [[], [], [], []];
    const replacements: number[][] = [[], [], [], []];
    let m = 0;

    for (const seat of order) {
        const player = observation.players[seat];
        const count = player.bonus.length;
        const picked = shuffle(model, dealt[seat]!).slice(0, count);

        flowers[seat] = picked;
        replacements[seat] = Array.from({ length: count }, () => backId(breakStack, m++));
        model.bonus[seat] = picked;
        for (const [index, id] of picked.entries()) {
            model.faces[id] = player.bonus[index] ?? null;
        }
        model.hands[seat] = [...dealt[seat]!.filter((id) => !picked.includes(id)), ...replacements[seat]!];
    }

    const tiles = shuffle(model, expandCounts(observation.hand.concealed));

    for (const [index, id] of model.hands[0]!.entries()) {
        model.faces[id] = tiles[index] ?? null;
    }

    return { model, plan: { dealt, flowers, replacements, order } };
}

function clone(model: TableModel): TableModel {
    return structuredClone(model);
}

function removeFrom(list: number[], id: number): boolean {
    const index = list.indexOf(id);

    if (index === -1) {
        return false;
    }

    list.splice(index, 1);

    return true;
}

/** Takes a tile out of a seat's concealed tiles. Your tiles are matched by face; a bot's face-down tiles are picked at random. */
function take(model: TableModel, seat: Seat, tile: Tile | null, preferred?: number): number | undefined {
    const hand = model.hands[seat]!;
    const drawn = model.drawn[seat];

    if (preferred !== undefined && (drawn === preferred || hand.includes(preferred))) {
        if (drawn === preferred) {
            model.drawn[seat] = null;
        } else {
            removeFrom(hand, preferred);
        }

        return preferred;
    }

    if (seat === 0) {
        if (drawn !== null && drawn !== undefined && model.faces[drawn] === tile) {
            model.drawn[0] = null;

            return drawn;
        }

        const index = hand.findLastIndex((id) => model.faces[id] === tile);

        if (index !== -1) {
            return hand.splice(index, 1)[0];
        }
    }

    const pool = drawn === null || drawn === undefined ? hand : [...hand, drawn];

    if (pool.length === 0) {
        return undefined;
    }

    const id = pool[randomInt(model, pool.length)]!;

    if (id === drawn) {
        model.drawn[seat] = null;
    } else {
        removeFrom(hand, id);
    }

    return id;
}

/** Puts a held drawn tile into the hand: yours goes to the right end for you to arrange, a bot's anywhere. */
function settleDrawn(model: TableModel, seat: Seat) {
    const drawn = model.drawn[seat];

    if (drawn === null || drawn === undefined) {
        return;
    }

    const hand = model.hands[seat]!;

    hand.splice(seat === 0 ? hand.length : randomInt(model, hand.length + 1), 0, drawn);
    model.drawn[seat] = null;
}

/** Discards land in the open middle of the table, clear of the walls. */
const WALL_CLEARANCE = 42;
const POND_RADIUS = 200;
const POND_SPACING = 38;

/**
 * Where a discard lands: tossed into the shared pile in the middle, a little toward the thrower and turned roughly to face them.
 * Nothing lines them up, so everyone's discards mix the way they do on a real table.
 */
function toss(model: TableModel, id: number, seat: Seat): PondTile {
    const angle = (-seat * Math.PI) / 2;
    let best: PondTile | undefined;
    let bestGap = -1;

    for (let attempt = 0; attempt < 40; attempt++) {
        const spin = nextRandom(model) * Math.PI * 2;
        const reach = Math.sqrt(nextRandom(model)) * POND_RADIUS;
        const localX = Math.cos(spin) * reach;
        const localY = Math.sin(spin) * reach * 0.9 + 25;
        const x = localX * Math.cos(angle) - localY * Math.sin(angle);
        const y = localX * Math.sin(angle) + localY * Math.cos(angle);

        if (WALL_CENTRES.some((stack) => Math.hypot(stack.x - x, stack.y - y) < WALL_CLEARANCE)) {
            continue;
        }

        const gap = Math.min(Infinity, ...model.pond.map((tile) => Math.hypot(tile.x - x, tile.y - y)));
        const rotation = seat * -90 + (nextRandom(model) * 2 - 1) * 35;
        const candidate = { id, x, y, rotation, seat };

        if (gap >= POND_SPACING) {
            return candidate;
        }

        if (gap > bestGap) {
            best = candidate;
            bestGap = gap;
        }
    }

    return best ?? { id, x: 0, y: 0, rotation: seat * -90, seat };
}

export interface MoveContext {
    /** The tile you picked to discard, so the same physical tile leaves your hand. */
    discardId?: number;
}

function heldCount(model: TableModel, seat: Seat): number {
    const pending = seat === 0 ? model.pending.filter((step) => !step.bonus).length : 0;

    return model.hands[seat]!.length + (model.drawn[seat] === null ? 0 : 1) + pending;
}

/** Faces of your tiles the table has not yet placed: what the engine says you hold, minus the tiles you can already see. */
function unplacedFaces(model: TableModel, hand: Hand): Tile[] {
    const remaining = expandCounts(hand.concealed);
    const known = [
        ...model.hands[0]!,
        ...(model.drawn[0] === null ? [] : [model.drawn[0]!]),
        ...model.pending.filter((step) => !step.bonus).map((step) => step.id),
    ];

    for (const id of known) {
        const face = model.faces[id];
        const index = face === null || face === undefined ? -1 : remaining.indexOf(face);

        if (index !== -1) {
            remaining.splice(index, 1);
        }
    }

    return remaining;
}

/** Follows the tiles through one engine move: discards, claims, kongs and draws. */
export function advance(previous: TableModel, observation: Observation, context: MoveContext = {}): TableModel {
    const model = clone(previous);

    for (let index = model.discardIds.length; index < observation.discards.length; index++) {
        const discard = observation.discards[index]!;
        const seat = discard.seat;
        const preferred =
            seat === 0 ? context.discardId : discard.drawn && model.drawn[seat] !== null ? (model.drawn[seat] ?? undefined) : undefined;
        const id = take(model, seat, seat === 0 ? discard.tile : null, preferred);

        settleDrawn(model, seat);

        if (id === undefined) {
            continue;
        }

        model.faces[id] = discard.tile;
        model.pond.push(toss(model, id, seat));
        model.discardIds.push(id);
        model.claimed.push(false);
    }

    for (const seat of SEATS) {
        const now = observation.players[seat].melds;
        const held = model.melds[seat]!;

        for (const [index, entry] of held.entries()) {
            const current = now[index];

            if (current && "kong" in current && "pong" in entry.meld) {
                const id = take(model, seat, current.kong[0]);

                if (id !== undefined) {
                    model.faces[id] = current.kong[0];
                    entry.ids.push(id);
                }

                entry.meld = current;
            }
        }

        for (const meld of now.slice(held.length)) {
            const tiles = meldTiles(meld);
            const ids: number[] = [];
            const concealed = "kong" in meld && meld.kong[1] === "concealed";
            const claimedIndex = concealed
                ? -1
                : observation.discards.findLastIndex((discard, index) => discard.claimed && !model.claimed[index]);

            if (claimedIndex !== -1) {
                const id = model.discardIds[claimedIndex]!;
                const tile = observation.discards[claimedIndex]!.tile;

                model.claimed[claimedIndex] = true;
                model.pond = model.pond.filter((entry) => entry.id !== id);
                tiles.splice(tiles.indexOf(tile), 1);
                ids.push(id);
            }

            for (const tile of tiles) {
                const id = take(model, seat, tile);

                if (id !== undefined) {
                    model.faces[id] = tile;
                    ids.push(id);
                }
            }

            ids.sort((a, b) => (model.faces[a] ?? "").localeCompare(model.faces[b] ?? ""));
            held.push({ meld, ids });
        }
    }

    const front = frontCount(observation);
    const back = backCount(observation);
    const drawnIds = [
        ...Array.from({ length: Math.max(0, front - model.front) }, (_, k) => ({
            id: frontId(model.breakStack, model.front + k),
            source: "front" as const,
        })),
        ...Array.from({ length: Math.max(0, back - model.back) }, (_, k) => ({
            id: backId(model.breakStack, model.back + k),
            source: "back" as const,
        })),
    ];

    model.front = Math.max(model.front, front);
    model.back = Math.max(model.back, back);

    if (drawnIds.length > 0) {
        const pendingBonus = model.pending.filter((step) => step.bonus).length;
        const growth = SEATS.map((seat) => {
            const player = observation.players[seat];
            const known = model.bonus[seat]!.length + (seat === 0 ? pendingBonus : 0);

            return player.concealed_count - heldCount(model, seat) + player.bonus.length - known;
        });
        const drawer = SEATS.reduce((best, seat) => (growth[seat]! > growth[best]! ? seat : best), observation.to_act[0] ?? SEATS[0]!);
        const known = model.bonus[drawer]!.length + (drawer === 0 ? pendingBonus : 0);
        const newBonus = observation.players[drawer].bonus.slice(known);
        const steps: DrawStep[] = drawnIds.map((entry, index) => ({ ...entry, bonus: index < newBonus.length }));

        for (const [index, step] of steps.entries()) {
            if (step.bonus) {
                model.faces[step.id] = newBonus[index] ?? null;
            }
        }

        if (drawer === 0) {
            const faces = unplacedFaces(model, observation.hand);
            const kept = steps.filter((step) => !step.bonus);
            const latest = observation.decision.kind === "turn" ? observation.decision.drawn : null;

            if (latest !== null && kept.length > 0) {
                const index = faces.lastIndexOf(latest);

                if (index !== -1) {
                    faces.splice(index, 1);
                    faces.push(latest);
                }
            }

            for (const [index, step] of kept.entries()) {
                model.faces[step.id] = faces[faces.length - kept.length + index] ?? null;
            }
            model.pending.push(...steps);
        } else {
            for (const step of steps) {
                if (step.bonus) {
                    model.bonus[drawer]!.push(step.id);
                } else if (model.drawn[drawer] === null) {
                    model.drawn[drawer] = step.id;
                } else {
                    model.hands[drawer]!.push(step.id);
                }
            }
        }
    }

    return model;
}

/** Takes the next tile the engine drew for you out of the wall: a flower goes beside your melds, anything else into your hand. */
export function takeDraw(previous: TableModel): TableModel {
    const [step, ...rest] = previous.pending;

    if (!step) {
        return previous;
    }

    const model = clone(previous);

    model.pending = rest;

    if (step.bonus) {
        model.bonus[0]!.push(step.id);
    } else if (model.drawn[0] === null) {
        model.drawn[0] = step.id;
    } else {
        model.hands[0]!.push(step.id);
    }

    return model;
}

/** Moves one of your tiles to a new place in your hand. The drawn tile joins the hand wherever you put it. */
export function arrange(previous: TableModel, id: number, index: number): TableModel {
    const model = clone(previous);
    const hand = model.hands[0]!;

    if (model.drawn[0] === id) {
        model.drawn[0] = null;
    } else if (!removeFrom(hand, id)) {
        return previous;
    }

    hand.splice(Math.max(0, Math.min(index, hand.length)), 0, id);

    return model;
}

/** Lays every hand face up at the end. The winner's discard joins their hand, and the bots' tiles are sorted for reading. */
export function reveal(previous: TableModel, hands: Hand[], winner: Seat | null, fromDiscard: boolean): TableModel {
    const model = clone(previous);

    for (const step of model.pending.splice(0)) {
        if (step.bonus) {
            model.bonus[0]!.push(step.id);
        } else {
            model.hands[0]!.push(step.id);
        }
    }

    if (winner !== null && fromDiscard) {
        const id = model.discardIds.at(-1);

        if (id !== undefined && model.pond.some((tile) => tile.id === id)) {
            settleDrawn(model, winner);
            model.pond = model.pond.filter((tile) => tile.id !== id);
            model.drawn[winner] = id;
        }
    }

    for (const seat of SEATS) {
        const hand = hands[seat];

        if (seat === 0 || !hand) {
            continue;
        }

        const faces = expandCounts(hand.concealed);
        const drawn = model.drawn[seat];

        if (drawn !== null && drawn !== undefined) {
            const face = model.faces[drawn];
            const index = face === null || face === undefined ? -1 : faces.indexOf(face);

            if (index === -1) {
                model.faces[drawn] = faces.pop() ?? null;
            } else {
                faces.splice(index, 1);
            }
        }

        for (const [index, id] of model.hands[seat]!.entries()) {
            model.faces[id] = faces[index] ?? null;
        }
    }

    model.revealed = true;

    return model;
}
