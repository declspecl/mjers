import { Play } from "./components/play";
import { seatName } from "./engine/names";
import { useSettings } from "./engine/settings";
import { createRoom, joinRoom, RoomRequestError } from "./remote/api";
import { forgetSeat, normalizeCode, type SavedSeat, savedSeat, savedSeats, seatStore, saveSeat, storageAvailable } from "./remote/identity";
import type { BotPreset, SeatGrant } from "./remote/protocol";
import { rotateSeats } from "./remote/rotate";
import { useRemoteTable } from "./remote/use-remote-table";
import { RoomEntry } from "./rooms/room-entry";
import { RoomLobby } from "./rooms/room-lobby";
import { RoomStatus } from "./rooms/room-status";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { useCallback, useState } from "react";
import { useStore } from "zustand";

function requestError(error: unknown): string {
    if (!(error instanceof RoomRequestError)) {
        return "Something went wrong. Try again.";
    }

    switch (error.room.code) {
        case "bad_password": {
            return "That password is wrong. Check it with the host and try again.";
        }
        case "table_full": {
            return "This room has no open seats. Ask the host to create a room with more players.";
        }
        case "hand_in_progress": {
            return "This room has already started, so it cannot be joined. If you hold a seat, open the room on the site where you joined it: seats saved on mjers.pages.dev and mjers.gavindhondt.com are separate.";
        }
        case "invalid_room": {
            return error.room.status === 404
                ? "No room has that code. Check the invite code; rooms expire after a while."
                : error.room.message;
        }
        default: {
            return error.room.message;
        }
    }
}

/** Multiplayer: choose practice or a room, then the lobby and the table for that room. */
export function Room() {
    const { room: initialCode } = useSearch({ from: "/" });
    const navigate = useNavigate();
    const identities = useStore(seatStore, (state) => state.seats);
    const seat = initialCode === undefined ? undefined : identities[initialCode];
    const showRoom = useCallback(
        (code: string | undefined) => {
            void navigate({ to: "/", search: code === undefined ? {} : { room: code } });
        },
        [navigate],
    );
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string>();
    const [storageWarning, setStorageWarning] = useState(() =>
        storageAvailable() ? undefined : "This browser is not saving site data, so reloading the page will lose your seat.",
    );

    const enter = useCallback(
        (grant: SeatGrant, name: string) => {
            if (!saveSeat(grant, name)) {
                setStorageWarning("Your seat could not be saved in this browser, so keep this tab open: reloading will lose it.");
            }

            const code = normalizeCode(grant.table);

            showRoom(code);
        },
        [showRoom],
    );

    const request = useCallback(
        async (send: () => Promise<SeatGrant>, name: string) => {
            if (busy) {
                return;
            }

            setBusy(true);
            setError(undefined);

            try {
                enter(await send(), name);
            } catch (error_) {
                setError(requestError(error_));
            } finally {
                setBusy(false);
            }
        },
        [busy, enter],
    );

    const onCreate = useCallback(
        (input: { name: string; human_players: number; password?: string; bot: BotPreset }) => {
            void request(() => createRoom(input), input.name);
        },
        [request],
    );

    const onJoin = useCallback(
        (code: string, input: { name: string; password?: string }) => {
            const normalized = normalizeCode(code);
            const saved = savedSeat(normalized);

            // Joining again would take a second seat; restore the one this browser already holds.
            if (saved) {
                showRoom(normalized);

                return;
            }

            void request(() => joinRoom(normalized, input), input.name);
        },
        [request, showRoom],
    );

    const onResume = useCallback(
        (table: string) => {
            const saved = savedSeat(table);

            if (saved) {
                setError(undefined);
                showRoom(saved.table);
            } else {
                setError("That seat is no longer saved in this browser. Join the room again with its invite code.");
            }
        },
        [showRoom],
    );

    const onLeave = useCallback(
        (forget: boolean) => {
            if (forget && seat) {
                forgetSeat(seat.table);
            }

            showRoom(undefined);
        },
        [seat, showRoom],
    );

    if (seat) {
        return <RoomSession key={`${seat.table}:${seat.token}`} seat={seat} onLeave={onLeave} />;
    }

    return (
        <RoomEntry
            key={initialCode ?? "entry"}
            initialCode={initialCode}
            savedRooms={savedSeats().map((saved) => ({ table: saved.table, name: saved.name }))}
            busy={busy}
            error={error}
            storageWarning={storageWarning}
            onPractice={() => {
                void navigate({ to: "/practice", search: {} });
            }}
            onCreate={onCreate}
            onJoin={onJoin}
            onResume={onResume}
        />
    );
}

function RoomSession({ seat, onLeave }: { seat: SavedSeat; onLeave: (forget: boolean) => void }) {
    const { settings, update } = useSettings();
    const remote = useRemoteTable(seat, settings);
    const { status, room, table } = remote;
    const connected = status === "connected";
    const leave = () => onLeave(status === "disconnected");
    const mine = room?.seats[seat.seat];
    const autoplay = mine?.autoplay ?? false;

    const roomStatus = (
        <RoomStatus
            status={status}
            waitingFor={
                table && table.phase === "play" && !table.humanToAct
                    ? table.observation.to_act.filter((view) => view !== 0).map((view) => seatName(view))
                    : undefined
            }
            autoplay={autoplay}
            canControl={connected && table !== undefined && table.phase !== "over"}
            onAutoplay={remote.setAutoplay}
            onReconnect={remote.reconnect}
            onLeave={leave}
            error={table ? undefined : remote.error}
        />
    );

    if (!room) {
        return (
            <div className="room-connecting">
                <p className="loading">Connecting to room {seat.table}.</p>
                {roomStatus}
            </div>
        );
    }

    if (!table) {
        return (
            <>
                <RoomLobby
                    table={room.code}
                    seats={room.seats}
                    seat={seat.seat}
                    connected={connected}
                    busy={false}
                    error={remote.error}
                    onStart={remote.start}
                    onLeave={leave}
                />
                {!connected && roomStatus}
            </>
        );
    }

    const host = room.seats[0]?.name ?? "the host";

    return (
        <Play
            settings={settings}
            update={update}
            table={table}
            advice={remote.advice}
            error={remote.error}
            graded={remote.graded}
            gradingFailure={remote.gradingFailure}
            grading
            totals={rotateSeats(room.scores, seat.seat)}
            act={remote.act}
            draw={remote.draw}
            move={remote.move}
            sort={remote.sort}
            hint={remote.hint}
            finishOpening={remote.finishOpening}
            nextHand={seat.seat === 0 ? remote.nextHand : undefined}
            waiting={`Waiting for ${host} to deal the next hand.`}
            overlay={roomStatus}
        />
    );
}
