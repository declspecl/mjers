import type { RoomLobbyProps, RoomSeat } from "./room-pages";
import "./rooms.css";
import { useId, useState } from "react";

const SEATS = [0, 1, 2, 3] as const;
const WINDS = ["East", "South", "West", "North"] as const;
const BOT_NAMES: Record<string, string> = { efficiency: "Efficiency bot", defensive: "Defensive bot", random: "Random bot" };

function occupant(slot: RoomSeat | undefined) {
    if (!slot || slot.kind === "open") {
        return "Waiting for a friend";
    }

    return slot.kind === "bot" ? (BOT_NAMES[slot.name ?? ""] ?? "Bot") : (slot.name ?? "Player");
}

function seatDetail(slot: RoomSeat | undefined, host: boolean) {
    if (host) {
        return "Host";
    }
    if (!slot || slot.kind === "open") {
        return "Open human seat";
    }

    return slot.kind === "human" ? "Human player" : "Bot player";
}

function presence(slot: RoomSeat | undefined) {
    if (!slot || slot.kind === "open") {
        return "Not joined yet";
    }
    if (slot.kind === "bot") {
        return "Ready";
    }

    return slot.connected ? "Connected" : "Offline";
}

function lobbyStatus(connected: boolean, openSeats: number) {
    if (!connected) {
        return "Connecting to your room...";
    }
    if (!openSeats) {
        return "All human seats are filled.";
    }

    return `Waiting for ${openSeats} more ${openSeats === 1 ? "person" : "people"}.`;
}

export function RoomLobby({ table, seats, seat, connected, busy, error, onStart, onLeave }: RoomLobbyProps) {
    const [copied, setCopied] = useState<string>();
    const [manualCopy, setManualCopy] = useState<string>();
    const id = useId();
    const host = seat === 0;
    const openSeats = SEATS.filter((index) => !seats.some((slot) => slot.seat === index && slot.kind !== "open")).length;
    const canStart = host && connected && !busy && openSeats === 0;

    async function copy(kind: "code" | "link") {
        const link = new URL("/", globalThis.location.origin);
        link.searchParams.set("room", table);
        const text = kind === "code" ? table : link.href;
        setCopied(undefined);

        try {
            if (!navigator.clipboard) {
                throw new Error("Clipboard unavailable");
            }

            await navigator.clipboard.writeText(text);
            setManualCopy(undefined);
            setCopied(kind === "code" ? "Room code copied." : "Room link copied.");
        } catch {
            setManualCopy(text);
        }
    }

    return (
        <section className="room-screen" aria-label="Room lobby">
            <div className="room-lobby">
                <header className="room-lobby-heading">
                    <div>
                        <h1>Your table</h1>
                        <p>Invite your friends, then settle in for a hand.</p>
                    </div>
                    <button type="button" disabled={busy} onClick={onLeave}>
                        Back to rooms
                    </button>
                </header>
                <div className="room-invite">
                    <div>
                        <span>Room code</span>
                        <strong className="room-code">{table}</strong>
                    </div>
                    <div className="room-share-actions">
                        <button type="button" onClick={() => void copy("code")}>
                            Copy code
                        </button>
                        <button type="button" onClick={() => void copy("link")}>
                            Copy link
                        </button>
                    </div>
                </div>
                <div className="room-copy-feedback" role="status" aria-live="polite">
                    {copied}
                </div>
                {manualCopy && (
                    <label className="room-manual-copy" htmlFor={`${id}-copy`}>
                        Select and copy this text. Automatic copying is unavailable.
                        <input
                            id={`${id}-copy`}
                            readOnly
                            value={manualCopy}
                            onFocus={(event) => event.currentTarget.select()}
                            onClick={(event) => event.currentTarget.select()}
                        />
                    </label>
                )}
                <ol className="room-seats" aria-label="The four seats">
                    {SEATS.map((index) => {
                        const slot = seats.find((slot) => slot.seat === index);
                        const human = slot?.kind === "human";

                        return (
                            <li key={index} className="room-seat" data-kind={slot?.kind ?? "open"} data-self={index === seat}>
                                <span className="room-seat-wind">{WINDS[index]}</span>
                                <strong>
                                    {occupant(slot)}
                                    {index === seat && " (you)"}
                                </strong>
                                <span className="room-seat-detail">{seatDetail(slot, index === 0)}</span>
                                <span className="room-seat-presence" data-online={human && slot.connected}>
                                    {presence(slot)}
                                </span>
                            </li>
                        );
                    })}
                </ol>
                <footer className="room-lobby-footer">
                    <div aria-live="polite">
                        <p>{lobbyStatus(connected, openSeats)}</p>
                        <small>
                            {host
                                ? "You choose when the first hand starts."
                                : `Waiting for ${seats.find((slot) => slot.seat === 0)?.name ?? "the host"} to start the hand.`}
                        </small>
                    </div>
                    {host && (
                        <button className="primary" type="button" disabled={!canStart} onClick={onStart}>
                            {busy ? "Starting hand..." : "Start hand"}
                        </button>
                    )}
                </footer>
                {error && (
                    <p className="room-error" role="alert">
                        {error}
                    </p>
                )}
            </div>
        </section>
    );
}
