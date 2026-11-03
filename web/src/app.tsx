import { Result } from "./components/result";
import { SidePanel } from "./components/side-panel";
import { Table } from "./components/table";
import { YourSide } from "./components/your-side";
import { loadEngine, type Mjers } from "./engine/engine";
import { useHand } from "./engine/use-hand";
import { useEffect, useState } from "react";

const DEALER = 0;

function startingSeed(): bigint {
    const given = new URLSearchParams(globalThis.location.search).get("seed");

    return given !== null && /^\d+$/.test(given) ? BigInt(given) : BigInt(Math.floor(Math.random() * 1_000_000));
}

function Hand({ mjers }: { mjers: Mjers }) {
    const [seed, setSeed] = useState(startingSeed);
    const { view, advice, error, act, hint } = useHand(mjers, seed, DEALER);

    if (view === undefined) {
        return <p className="loading">Dealing the hand.</p>;
    }

    const source = view.result?.win?.source;
    const winningTile = source !== undefined && source !== "SelfDraw" ? view.observation.discards.at(-1)?.tile : undefined;
    const key = `${seed}-${view.observation.discards.length}-${view.observation.hand.concealed}-${view.observation.decision.kind}`;

    return (
        <div className="screen">
            <div className="play">
                <Table view={view} dealer={DEALER} />
                <YourSide key={key} view={view} advice={advice} dealer={DEALER} onAct={act} onHint={hint} />
            </div>
            <div className="column">
                {view.over && view.result && view.settlement && (
                    <Result
                        result={view.result}
                        settlement={view.settlement}
                        winnerHand={view.winnerHand}
                        winningTile={winningTile}
                        onNext={() => setSeed((current) => current + 1n)}
                    />
                )}
                <SidePanel view={view} advice={advice} error={error} />
            </div>
        </div>
    );
}

export default function App() {
    const [mjers, setMjers] = useState<Mjers>();
    const [error, setError] = useState<string>();

    useEffect(() => {
        loadEngine().then(setMjers, (error_: unknown) => setError(String(error_)));
    }, []);

    return (
        <main>
            <header>
                <h1>mjers</h1>
                <p>Family rules. You sit at the bottom against three bots.</p>
            </header>
            {error !== undefined && <p className="problem">The engine did not load: {error}</p>}
            {error === undefined && mjers === undefined && <p className="loading">Loading the engine.</p>}
            {mjers !== undefined && <Hand mjers={mjers} />}
        </main>
    );
}
