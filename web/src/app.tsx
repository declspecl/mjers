import { HandView } from "./components/hand-view";
import { loadEngine, type Mjers } from "./engine/engine";
import { useEffect, useState } from "react";

export default function App() {
    const [mjers, setMjers] = useState<Mjers>();
    const [error, setError] = useState<string>();

    useEffect(() => {
        loadEngine().then(setMjers, (error_: unknown) => setError(String(error_)));
    }, []);

    return (
        <main>
            <h1>mjers</h1>
            {error === undefined ? mjers === undefined ? <p>Loading engine…</p> : <HandView mjers={mjers} seed={42n} /> : <p>{error}</p>}
        </main>
    );
}
