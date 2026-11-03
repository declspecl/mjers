import type { Seat } from "@mjers/mjers/types";

export type RoomBot = "efficiency" | "defensive" | "random";

export interface CreateRoomInput {
    name: string;
    human_players: number;
    password?: string;
    bot: RoomBot;
}

export interface JoinRoomInput {
    name: string;
    password?: string;
}

export interface SavedRoom {
    table: string;
    name?: string;
}

export interface RoomSeat {
    seat: Seat;
    name: string | null;
    kind: "open" | "human" | "bot";
    connected: boolean;
    autoplay: boolean;
}

export interface RoomEntryProps {
    initialCode?: string;
    savedRooms?: SavedRoom[];
    busy: boolean;
    error?: string;
    storageWarning?: string;
    onPractice: () => void;
    onCreate: (input: CreateRoomInput) => void;
    onJoin: (code: string, input: JoinRoomInput) => void;
    onResume: (table: string) => void;
}

export interface RoomLobbyProps {
    table: string;
    seats: RoomSeat[];
    seat: Seat;
    connected: boolean;
    busy: boolean;
    error?: string;
    onStart: () => void;
    onLeave: () => void;
}

export type RoomConnectionStatus = "connecting" | "syncing" | "connected" | "reconnecting" | "disconnected" | "replaced";

export interface RoomStatusProps {
    status: RoomConnectionStatus;
    waitingFor?: string[];
    autoplay: boolean;
    canControl: boolean;
    error?: string;
    onAutoplay: (enabled: boolean) => void;
    onReconnect: () => void;
    onLeave: () => void;
}
