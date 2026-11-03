import type { RoomConnectionStatus, RoomStatusProps } from "./room-pages";
import "./rooms.css";

const STATUS: Record<RoomConnectionStatus, string> = {
    connecting: "Connecting to the room...",
    syncing: "Getting the latest hand...",
    connected: "Connected",
    reconnecting: "Reconnecting to the room...",
    disconnected: "Connection lost. Reconnect to continue.",
    replaced: "Your seat is open in another tab or device.",
};

export function RoomStatus({ status, waitingFor = [], autoplay, canControl, error, onAutoplay, onReconnect, onLeave }: RoomStatusProps) {
    const waiting = new Intl.ListFormat("en", { style: "long", type: "conjunction" }).format(waitingFor);
    const canReconnect = status === "disconnected" || status === "replaced";

    return (
        <section className="room-status" aria-label="Room connection and controls" data-connection={status}>
            <div className="room-status-message" role="status" aria-live="polite">
                <p>{STATUS[status]}</p>
                {status === "connected" && waiting && <small>Waiting for {waiting}.</small>}
                {autoplay && <small>A bot is playing your seat. You can take it back.</small>}
            </div>
            <div className="room-status-actions">
                {canReconnect && (
                    <button type="button" onClick={onReconnect}>
                        {status === "replaced" ? "Play in this tab" : "Reconnect"}
                    </button>
                )}
                <button type="button" disabled={!canControl} aria-pressed={autoplay} onClick={() => onAutoplay(!autoplay)}>
                    {autoplay ? "Play my seat" : "Let a bot play my seat"}
                </button>
                <button type="button" onClick={onLeave}>
                    Back to rooms
                </button>
            </div>
            {error && (
                <p className="room-error" role="alert">
                    {error}
                </p>
            )}
        </section>
    );
}
