import type { Action, Advice, Bonus, Hand, HandResult, Meld, Observation, Seat, SeatUnits, Tile, WinSource } from "@mjers/mjers/types";

/** Wire types mirroring `backend/table/src/protocol.rs`. */

export type BotPreset = "efficiency" | "random" | "defensive";
export type SeatKind = "open" | "human" | "bot";

export interface SeatView {
    seat: Seat;
    name: string | null;
    kind: SeatKind;
    connected: boolean;
    autoplay: boolean;
}

export type ErrorCode =
    | "illegal_action"
    | "stale"
    | "not_your_turn"
    | "table_full"
    | "bad_token"
    | "bad_password"
    | "invalid_room"
    | "not_host"
    | "not_ready"
    | "hand_in_progress"
    | "internal"
    | "rate_limited";

export interface SettlementView {
    win: SeatUnits;
    kongs: SeatUnits;
    total: SeatUnits;
}

export type PublicEvent =
    | { event: "bonus"; seat: Seat; bonus: Bonus }
    | { event: "draw"; seat: Seat }
    | { event: "discard"; seat: Seat; tile: Tile }
    | { event: "claim"; seat: Seat; from: Seat; meld: Meld }
    | { event: "kong"; seat: Seat; meld: Meld }
    | { event: "win"; seat: Seat; source: WinSource; tile: Tile }
    | { event: "exhausted" };

export interface StateMessage {
    type: "state";
    table: string;
    version: number;
    seats: [SeatView, SeatView, SeatView, SeatView];
    human_players: number;
    password_protected: boolean;
    scores: SeatUnits;
    hand_number: number;
    steps: number;
    observation: Observation | null;
}

export type ServerMessage =
    | StateMessage
    | { type: "events"; version: number; hand_number: number; events: PublicEvent[] }
    | {
          type: "hand_over";
          version: number;
          hand_number: number;
          result: HandResult;
          settlement: SettlementView;
          revealed: [Hand, Hand, Hand, Hand];
      }
    | { type: "hint"; version: number; hand_number: number; advice: Advice }
    | { type: "error"; code: ErrorCode; message: string }
    | { type: "pong" };

export type ClientMessage =
    | { type: "sync" }
    | { type: "ping" }
    | { type: "start_hand" }
    | { type: "act"; action: Action; hand_number: number; steps_seen: number }
    | { type: "hint"; samples: 0 }
    | { type: "autoplay"; enabled: boolean; hand_number: number; steps_seen: number }
    | { type: "next_hand"; hand_number: number };

export interface CreateRequest {
    name: string;
    human_players: number;
    password?: string;
    bot?: BotPreset;
}

export interface JoinRequest {
    name: string;
    password?: string;
}

export interface SeatGrant {
    table: string;
    seat: Seat;
    seat_token: string;
    join_path: string;
}

/** What the room screens show. */
export interface RoomInfo {
    code: string;
    mySeat: Seat;
    seats: [SeatView, SeatView, SeatView, SeatView];
    humanPlayers: number;
    passwordProtected: boolean;
    scores: SeatUnits;
    handNumber: number;
}

export type ConnectionStatus = "connecting" | "syncing" | "connected" | "reconnecting" | "disconnected" | "replaced";

export interface RoomError {
    code: ErrorCode | "network" | "http";
    message: string;
    status?: number;
}
