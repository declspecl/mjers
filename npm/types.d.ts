/** Suited tiles use m/p/s; honors 1z–7z (E S W N White Green Red). */
export type Tile = string;
/** Grouped notation, e.g. "123m55p7z". */
export type TileCounts = string;
/** Flowers 1F–4F, seasons 1S–4S. */
export type Bonus = string;
export type Seat = 0 | 1 | 2 | 3;
export type KongKind = "exposed" | "concealed";
export type Meld = { chow: Tile } | { pong: Tile } | { kong: [Tile, KongKind] };
export type Action =
    | { discard: Tile }
    | "self_draw_win"
    | { concealed_kong: Tile }
    | { upgrade_kong: Tile }
    | "pass"
    | { chow: Tile }
    | "pong"
    | "kong"
    | "win";

export interface Payouts {
    discard_win: number;
    discard_win_discarder: number;
    self_draw_win: number;
    per_bonus: number;
    exposed_kong: number;
    concealed_kong: number;
    kongs_paid_without_winner: boolean;
}
export interface Ruleset {
    name: string;
    flower_wall_stacks: number;
    allow_rob_kong: boolean;
    payouts: Payouts;
}
export interface Hand {
    concealed: TileCounts;
    melds: Meld[];
    bonus: Bonus[];
}
export interface Player {
    seat: Seat;
    concealed_count: number;
    melds: Meld[];
    bonus: Bonus[];
}
export interface Discard {
    seat: Seat;
    tile: Tile;
    claimed: boolean;
}
export type Decision =
    | { kind: "turn"; after_claim: boolean; drawn: Tile | null }
    | { kind: "claim"; tile: Tile; from: Seat; offer: "discard" | "kong_upgrade" }
    | { kind: "none" };

/** Only this seat's concealed hand is included. Players are in seat order. */
export interface Observation {
    seat: Seat;
    dealer: Seat;
    rules: Ruleset;
    hand: Hand;
    players: [Player, Player, Player, Player];
    discards: Discard[];
    public_tiles: TileCounts;
    wall_remaining: number;
    live_wall: number;
    to_act: Seat[];
    legal_actions: Action[];
    decision: Decision;
}
export type WinSource = "SelfDraw" | { Discard: Seat };
export interface HandResult {
    win: { winner: Seat; source: WinSource; bonus_tiles: number } | null;
    kongs: { owner: Seat; kind: KongKind }[];
}
export type SeatUnits = [number, number, number, number];
export interface Settlement {
    win: SeatUnits;
    kongs: SeatUnits;
    total: SeatUnits;
}
export interface AdviceOptions {
    /** Playouts per candidate. Default 200; use 0 for instant ranking. */
    samples?: number;
    /** Number of ranked actions to simulate. Default 6. */
    candidates?: number;
    /** Unsigned 64-bit bigint. Default 0n. */
    seed?: bigint;
    /** Bot preset for playouts. Default "efficiency". */
    rollout?: string;
}
export interface Assessment {
    action: Action;
    shanten: number;
    live_improving: number;
    danger: number;
    immediate_units: number;
    reasons: string[];
}
export interface Estimate {
    action: Action;
    samples: number;
    ev: number;
    ev_stderr: number;
    win_rate: number;
    deal_in_rate: number;
    difference: number;
    difference_stderr: number;
}
export interface Advice {
    recommended: Action;
    ranked: Assessment[];
    estimates: Estimate[];
}
export interface Improvement {
    tile: Tile;
    live: number;
}
export interface Ukeire {
    shanten: number;
    live_improving: number;
    improvements: Improvement[];
}
export interface DiscardOption extends Ukeire {
    tile: Tile;
}
export interface Analysis extends Ukeire {
    /** Non-winning 14-tile hand: sorted by shanten, then live tiles. */
    discards: DiscardOption[];
}
