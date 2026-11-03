import { TileFace } from "../components/tile";
import type { RoomEntryProps } from "./room-pages";
import { roomFormSchema, type RoomFormValues } from "./room-schema";
import "./rooms.css";
import { zodResolver } from "@hookform/resolvers/zod";
import { useId, useState } from "react";
import { useForm, useWatch } from "react-hook-form";

function seatDescription(humans: number) {
    if (humans === 4) {
        return "Four humans, no bots.";
    }

    const people = humans === 1 ? "human" : "humans";
    const bots = humans === 3 ? "bot" : "bots";

    return `${humans} ${people} and ${4 - humans} ${bots}. You count as one human.`;
}

/** Room setup only: the caller owns requests, saved identity and navigation. */
export function RoomEntry({
    initialCode,
    savedRooms = [],
    busy,
    error,
    storageWarning,
    onPractice,
    onCreate,
    onJoin,
    onResume,
}: RoomEntryProps) {
    const [mode, setMode] = useState<"choose" | "create" | "join">(initialCode ? "join" : "choose");
    const form = useForm<RoomFormValues>({
        resolver: zodResolver(roomFormSchema),
        defaultValues: {
            mode: initialCode ? "join" : "create",
            name: savedRooms[0]?.name ?? "",
            code: initialCode?.trim().toUpperCase() ?? "",
            password: "",
            human_players: 2,
            bot: "efficiency",
        },
    });
    const humans = useWatch({ control: form.control, name: "human_players" });
    const { errors } = form.formState;
    const id = useId();
    const problem =
        errors.name?.message ??
        errors.code?.message ??
        errors.password?.message ??
        errors.human_players?.message ??
        errors.bot?.message ??
        error;
    const actionLabel = mode === "create" ? "Create room" : "Join room";
    const pendingLabel = mode === "create" ? "Creating room..." : "Joining room...";

    function changeMode(next: "choose" | "create" | "join") {
        setMode(next);
        form.resetField("password");
        form.clearErrors();
        if (next !== "choose") {
            form.setValue("mode", next);
        }
    }

    function submit(values: RoomFormValues) {
        if (busy) {
            return;
        }
        const credentials = { name: values.name, ...(values.password ? { password: values.password } : {}) };
        if (values.mode === "create") {
            onCreate({ ...credentials, human_players: values.human_players, bot: values.bot });
        } else {
            onJoin(values.code.trim().toUpperCase(), credentials);
        }
    }

    return (
        <section className="room-screen" aria-label="Play mahjong">
            <div className="room-entry">
                <header className="room-welcome">
                    <div className="room-tiles" aria-hidden="true">
                        <TileFace tile="1m" />
                        <TileFace tile="1s" />
                        <TileFace tile="1p" />
                    </div>
                    <h1>Mahjong</h1>
                    <p>A table for family and friends.</p>
                    <p className="room-welcome-note">Four seats. Your people, with bots to fill the gaps.</p>
                </header>
                <div className="room-form-panel">
                    {mode === "choose" ? (
                        <>
                            <h2>Play a hand</h2>
                            <div className="room-choices">
                                <button className="primary" type="button" disabled={busy} onClick={() => changeMode("create")}>
                                    Create a room
                                </button>
                                <button type="button" disabled={busy} onClick={() => changeMode("join")}>
                                    Join a room
                                </button>
                                <button className="room-practice" type="button" disabled={busy} onClick={onPractice}>
                                    Practice with bots
                                </button>
                            </div>
                        </>
                    ) : (
                        <>
                            <button className="room-back" type="button" disabled={busy} onClick={() => changeMode("choose")}>
                                Back
                            </button>
                            <h2>{mode === "create" ? "Create a room" : "Join a room"}</h2>
                            <form
                                noValidate
                                onSubmit={form.handleSubmit(submit)}
                                aria-busy={busy}
                                aria-describedby={problem ? `${id}-error` : undefined}
                            >
                                <fieldset className="room-fields" disabled={busy}>
                                    <legend className="room-sr-only">{mode === "create" ? "Room settings" : "Join details"}</legend>
                                    {mode === "join" && (
                                        <label htmlFor={`${id}-code`}>
                                            Room code
                                            <input
                                                id={`${id}-code`}
                                                {...form.register("code")}
                                                aria-invalid={Boolean(errors.code)}
                                                autoComplete="off"
                                                autoCapitalize="characters"
                                                spellCheck={false}
                                                minLength={6}
                                                maxLength={6}
                                                required
                                                aria-describedby={`${id}-code-help`}
                                            />
                                            <small id={`${id}-code-help`}>The six-character code from your host.</small>
                                        </label>
                                    )}
                                    <label htmlFor={`${id}-name`}>
                                        Your name
                                        <input
                                            id={`${id}-name`}
                                            {...form.register("name")}
                                            aria-invalid={Boolean(errors.name)}
                                            autoComplete="nickname"
                                            maxLength={40}
                                            required
                                        />
                                    </label>
                                    {mode === "create" && (
                                        <>
                                            <label htmlFor={`${id}-humans`}>
                                                Human players
                                                <select id={`${id}-humans`} {...form.register("human_players", { valueAsNumber: true })}>
                                                    {[1, 2, 3, 4].map((count) => (
                                                        <option key={count} value={count}>
                                                            {count}
                                                        </option>
                                                    ))}
                                                </select>
                                                <small aria-live="polite">{seatDescription(humans)}</small>
                                            </label>
                                            {humans < 4 && (
                                                <label htmlFor={`${id}-bot`}>
                                                    Bot style
                                                    <select id={`${id}-bot`} {...form.register("bot")}>
                                                        <option value="efficiency">Efficiency</option>
                                                        <option value="defensive">Defensive</option>
                                                        <option value="random">Random</option>
                                                    </select>
                                                </label>
                                            )}
                                        </>
                                    )}
                                    <label htmlFor={`${id}-password`}>
                                        <span>
                                            Room password <span className="room-optional">(optional)</span>
                                        </span>
                                        <input
                                            id={`${id}-password`}
                                            type="password"
                                            {...form.register("password")}
                                            aria-invalid={Boolean(errors.password)}
                                            autoComplete={mode === "create" ? "new-password" : "current-password"}
                                            maxLength={128}
                                            aria-describedby={`${id}-password-help`}
                                        />
                                        <small id={`${id}-password-help`}>
                                            {mode === "create"
                                                ? "Leave blank to let anyone with the invite join."
                                                : "Enter it if your host set one."}
                                        </small>
                                    </label>
                                    <button type="submit" className="primary">
                                        {busy ? pendingLabel : actionLabel}
                                    </button>
                                </fieldset>
                            </form>
                        </>
                    )}
                    {problem && (
                        <p className="room-error" id={`${id}-error`} role="alert">
                            {problem}
                        </p>
                    )}
                    {storageWarning && (
                        <p className="room-note" role="status">
                            {storageWarning}
                        </p>
                    )}
                    {savedRooms.length > 0 && (
                        <section className="room-saved" aria-label="Your saved rooms">
                            <h3>Return to your room</h3>
                            <ul>
                                {savedRooms.map((room) => (
                                    <li key={room.table}>
                                        <span>
                                            <strong>{room.table}</strong>
                                            {room.name && <small>Playing as {room.name}</small>}
                                        </span>
                                        <button type="button" disabled={busy} onClick={() => onResume(room.table)}>
                                            Resume room
                                        </button>
                                    </li>
                                ))}
                            </ul>
                        </section>
                    )}
                </div>
            </div>
        </section>
    );
}
