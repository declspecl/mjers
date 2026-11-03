import { Play } from "./components/play";
import { loadEngine, type Mjers } from "./engine/engine";
import { useSettings } from "./engine/settings";
import { useTable } from "./engine/use-table";
import { Outlet, useNavigate, useSearch } from "@tanstack/react-router";
import { useEffect, useState } from "react";

function Practice({ mjers, onLeave, seed }: { mjers: Mjers; onLeave: () => void; seed?: string }) {
    const { settings, update } = useSettings();
    const { match, ...hand } = useTable(mjers, settings, seed);

    return (
        <Play
            {...hand}
            settings={settings}
            update={update}
            grading
            totals={match.totals}
            overlay={
                <button type="button" className="leave-practice" onClick={onLeave}>
                    Leave practice
                </button>
            }
        />
    );
}

export default function App() {
    return (
        <main>
            <Outlet />
        </main>
    );
}

export function PracticePage() {
    const { seed } = useSearch({ from: "/practice" });
    const navigate = useNavigate();
    const [mjers, setMjers] = useState<Mjers>();
    const [error, setError] = useState<string>();

    useEffect(() => {
        let active = true;
        loadEngine().then(
            (engine) => {
                if (active) {
                    setMjers(engine);
                }
            },
            (error_: unknown) => {
                if (active) {
                    setError(String(error_));
                }
            },
        );

        return () => {
            active = false;
        };
    }, []);

    if (error !== undefined) {
        return <p className="problem">The engine did not load: {error}. Reload the page to try again.</p>;
    }

    if (mjers === undefined) {
        return <p className="loading">Loading the engine.</p>;
    }

    return <Practice key={seed ?? "random"} mjers={mjers} seed={seed} onLeave={() => void navigate({ to: "/", search: {} })} />;
}
