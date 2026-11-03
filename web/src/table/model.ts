import { expandCounts, meldTiles } from "../engine/tiles";
import { pilePoint, STACKS_PER_WALL, WALL_CENTRES, WALL_RING } from "./geometry";
import { nextRandom, randomInt, shuffle } from "./random";
import type { Hand, Meld, Observation, Seat, Tile } from "@mjers/mjers/types";

/**
 * The physical table: where each of the 144 tiles is. The engine only knows counts, so this follows every tile from its place in
 * the wall to a hand, the discard pile, or a meld. A tile's id is the wall slot it was built into.
 */

export const TILE_COUNT = 144;
export const STACKS = 72;
/** The flower wall is padded back to this many stacks once it runs low. */
const FLOWER_STACKS = 8;
/** Below this many tiles the flower wall counts as low. */
const FLOWER_LOW = 4;
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
    /** The seat that broke the wall; the flower wall lies beside its right-corner wall. */
    setter: Seat;
    /** Tiles still in the live wall, in the order they will be drawn. */
    live: number[];
    /** Tiles in the flower wall, in the order replacements are drawn. */
    flowerWall: number[];
    /** Each tile's place in the flower wall (even places are stack tops), or -1. Kept after a tile is drawn until it lands. */
    flowerSlots: number[];
    flowerNext: number;
    /** Tiles the engine has taken from the live end and from the flower wall. */
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
function frontId(breakStack: number, n: number): number {
    return ((breakStack + Math.floor(n / 2)) % STACKS) * 2 + (n % 2);
}

/** The tile m places back from the break, which runs toward the front of the broken wall. */
function backId(breakStack: number, m: number): number {
    return ((((breakStack - 1 - Math.floor(m / 2)) % STACKS) + STACKS) % STACKS) * 2 + (m % 2);
}

export interface WallBreak {
    setter: Seat;
    /** Stacks counted in from the front of the setter's left wall; they become the flower wall. */
    counted: number;
    /** The ring index of the first live stack, just after the counted ones. */
    breakStack: number;
}

/**
 * Where the wall breaks. The dealer's roll counts seats counter-clockwise starting from the dealer as one, so a 5 comes back to
 * the dealer. That seat counts that many stacks in from the front (the corner end) of the wall at its left corner and turns them
 * down beside its right-corner wall as the flower wall. The live wall starts just after them.
 */
export function wallBreak(dealer: Seat, total: number): WallBreak {
    const setter = ((dealer + total - 1) % 4) as Seat;
    const leftWall = ((setter + 3) % 4) as Seat;
    const ring = WALL_RING.indexOf(leftWall);

    return { setter, counted: total, breakStack: (ring * STACKS_PER_WALL + total) % STACKS };
}

function toFlowerWall(model: TableModel, id: number) {
    model.flowerWall.push(id);
    model.flowerSlots[id] = model.flowerNext++;
}

/**
 * Tops the flower wall back up to eight stacks from the back of the live wall. This is only how it looks: the engine keeps the last
 * sixteen tiles for replacements however they are laid out.
 */
function padFlowerWall(model: TableModel) {
    if (model.flowerWall.length === 0) {
        model.flowerNext = 0;
    }

    model.flowerNext += model.flowerNext % 2;

    while (model.flowerWall.length < FLOWER_STACKS * 2 - 1 && model.live.length >= 2) {
        const bottom = model.live.pop()!;
        const top = model.live.pop()!;

        toFlowerWall(model, top);
        toFlowerWall(model, bottom);
    }
}

function drawFront(model: TableModel): number | undefined {
    return model.live.shift() ?? model.flowerWall.pop();
}

function drawBack(model: TableModel): number | undefined {
    if (model.flowerWall.length < FLOWER_LOW) {
        padFlowerWall(model);
    }

    return model.flowerWall.shift() ?? model.live.pop();
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
export function createModel(observation: Observation, wall: WallBreak, seed: number): { model: TableModel; plan: DealPlan } {
    const dealer = observation.dealer;
    const order = seatOrder(dealer);
    const dealt: number[][] = [[], [], [], []];
    const model: TableModel = {
        dealer,
        setter: wall.setter,
        live: Array.from({ length: TILE_COUNT - wall.counted * 2 }, (_, n) => frontId(wall.breakStack, n)),
        flowerWall: [],
        flowerSlots: Array.from({ length: TILE_COUNT }, () => -1),
        flowerNext: 0,
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

    for (let m = 0; m < wall.counted * 2; m++) {
        toFlowerWall(model, backId(wall.breakStack, m));
    }

    for (const take of DEAL_ROUNDS) {
        for (const seat of order) {
            for (let index = 0; index < take; index++) {
                dealt[seat]!.push(drawFront(model)!);
            }
        }
    }

    dealt[dealer]!.push(drawFront(model)!);

    const flowers: number[][] = [[], [], [], []];
    const replacements: number[][] = [[], [], [], []];

    for (const seat of order) {
        const player = observation.players[seat];
        const count = player.bonus.length;
        const picked = shuffle(model, dealt[seat]!).slice(0, count);

        flowers[seat] = picked;
        replacements[seat] = Array.from({ length: count }, () => drawBack(model)!);
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

        const flowers = model.flowerWall.map((tile) => pilePoint(model.setter, model.flowerSlots[tile]!));

        if ([...WALL_CENTRES, ...flowers].some((stack) => Math.hypot(stack.x - x, stack.y - y) < WALL_CLEARANCE)) {
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
        ...Array.from({ length: Math.max(0, front - model.front) }, () => ({ id: drawFront(model)!, source: "front" as const })),
        ...Array.from({ length: Math.max(0, back - model.back) }, () => ({ id: drawBack(model)!, source: "back" as const })),
    ];

    model.front = Math.max(model.front, front);
    model.back = Math.max(model.back, back);

    if (drawnIds.length > 0) {
        const pendingBonus = model.pending.filter((step) => step.bonus).length;
        const known = (seat: Seat) => model.bonus[seat]!.length + (seat === 0 ? pendingBonus : 0);
        const growth = SEATS.map((seat) => {
            const player = observation.players[seat];

            return player.concealed_count - heldCount(model, seat) + player.bonus.length - known(seat);
        });
        const drawers = SEATS.filter((seat) => growth[seat]! > 0);

        if (drawers.length <= 1) {
            const drawer = SEATS.reduce((best, seat) => (growth[seat]! > growth[best]! ? seat : best), observation.to_act[0] ?? SEATS[0]!);

            placeDraws(model, observation, drawer, drawnIds, known(drawer));
        } else {
            // Several seats drew since the last update (a server update can carry many moves): share the drawn tiles out.
            const queue = [...drawnIds];

            for (const seat of drawers) {
                placeDraws(model, observation, seat, queue.splice(0, growth[seat]!), known(seat));
            }

            if (queue.length > 0) {
                placeDraws(model, observation, drawers.at(-1)!, queue, known(drawers.at(-1)!));
            }
        }
    }

    return model;
}

function placeDraws(
    model: TableModel,
    observation: Observation,
    drawer: Seat,
    drawnIds: { id: number; source: "front" | "back" }[],
    known: number,
) {
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

/** Whether the table shows what the observation says: hand sizes, melds, flowers and discards. */
export function matches(model: TableModel, observation: Observation): boolean {
    const pendingBonus = model.pending.filter((step) => step.bonus).length;

    return (
        model.discardIds.length === observation.discards.length &&
        model.pond.length === observation.discards.filter((discard) => !discard.claimed).length &&
        SEATS.every((seat) => {
            const player = observation.players[seat];

            return (
                heldCount(model, seat) === player.concealed_count &&
                model.melds[seat]!.length === player.melds.length &&
                model.bonus[seat]!.length + (seat === 0 ? pendingBonus : 0) === player.bonus.length
            );
        })
    );
}

/**
 * Lays out a table mid-hand straight from an observation, with no history: used when joining or reconnecting part way through a
 * hand, or when an update skipped more than the table can follow. Face-down tiles come from the wall in draw order.
 */
export function restoreModel(observation: Observation, wall: WallBreak, seed: number): TableModel {
    const model: TableModel = {
        dealer: observation.dealer,
        setter: wall.setter,
        live: Array.from({ length: TILE_COUNT - wall.counted * 2 }, (_, n) => frontId(wall.breakStack, n)),
        flowerWall: [],
        flowerSlots: Array.from({ length: TILE_COUNT }, () => -1),
        flowerNext: 0,
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

    for (let m = 0; m < wall.counted * 2; m++) {
        toFlowerWall(model, backId(wall.breakStack, m));
    }

    const pool: number[] = [];

    for (let n = 0; n < model.front; n++) {
        const id = drawFront(model);

        if (id !== undefined) {
            pool.push(id);
        }
    }

    for (let n = 0; n < model.back; n++) {
        const id = drawBack(model);

        if (id !== undefined) {
            pool.push(id);
        }
    }

    const next = (face: Tile | null): number | undefined => {
        const id = pool.shift();

        if (id !== undefined) {
            model.faces[id] = face;
        }

        return id;
    };

    for (const seat of SEATS) {
        const player = observation.players[seat];

        model.bonus[seat] = player.bonus.map((bonus) => next(bonus)).filter((id): id is number => id !== undefined);
        model.melds[seat] = player.melds.map((meld) => ({
            meld,
            ids: meldTiles(meld)
                .map((tile) => next(tile))
                .filter((id): id is number => id !== undefined),
        }));
    }

    for (const discard of observation.discards) {
        model.claimed.push(discard.claimed);

        if (discard.claimed) {
            model.discardIds.push(-1);
            continue;
        }

        const id = next(discard.tile);

        if (id === undefined) {
            model.discardIds.push(-1);
            continue;
        }

        model.discardIds.push(id);
        model.pond.push(toss(model, id, discard.seat));
    }

    const mine = expandCounts(observation.hand.concealed);
    const drawn = observation.decision.kind === "turn" && observation.to_act.includes(0) ? observation.decision.drawn : null;
    const drawnIndex = drawn === null ? -1 : mine.lastIndexOf(drawn);

    if (drawnIndex !== -1) {
        mine.splice(drawnIndex, 1);
    }

    for (const face of mine) {
        const id = next(face);

        if (id !== undefined) {
            model.hands[0]!.push(id);
        }
    }

    if (drawnIndex !== -1) {
        model.drawn[0] = next(drawn) ?? null;
    }

    model.hands[0] = sortedHand(model);

    for (const seat of SEATS.slice(1)) {
        for (let index = 0; index < observation.players[seat].concealed_count; index++) {
            const id = next(null);

            if (id !== undefined) {
                model.hands[seat]!.push(id);
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

/** How you lay out a sorted hand: characters, then bamboo, then dots, then honors. */
const SUIT_ORDER: Record<string, number> = { m: 0, s: 1, p: 2, z: 3 };

function sortKey(tile: Tile | null | undefined): number {
    if (!tile) {
        return Number.MAX_SAFE_INTEGER;
    }

    return (SUIT_ORDER[tile.slice(-1)] ?? 4) * 10 + Number(tile.slice(0, -1));
}

/** Your hand in sorted order. The tile you just drew stays apart at the right until you place it. */
export function sortedHand(model: TableModel): number[] {
    return model.hands[0]!.toSorted((a, b) => sortKey(model.faces[a]) - sortKey(model.faces[b]) || a - b);
}

export function isHandSorted(model: TableModel): boolean {
    const sorted = sortedHand(model);

    return model.hands[0]!.every((id, index) => id === sorted[index]);
}

/** Sorts your hand, keeping the tile you just drew apart. */
export function sortHand(previous: TableModel): TableModel {
    if (isHandSorted(previous)) {
        return previous;
    }

    const model = clone(previous);

    model.hands[0] = sortedHand(previous);

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
