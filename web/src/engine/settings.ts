import { useCallback, useState } from "react";

export type ViewMode = "immersive" | "analytic";

export interface Settings {
    /** Immersive shows only the table; analytic adds the status, hints and the move log. */
    view: ViewMode;
    /** Grade each of your moves against the engine once you make it. */
    feedback: boolean;
    /** Play the mixing, wall building, dice and deal before each hand. */
    opening: boolean;
}

const KEY = "mjers.settings";
const DEFAULTS: Settings = { view: "immersive", feedback: true, opening: true };

function load(): Settings {
    try {
        const stored = globalThis.localStorage?.getItem(KEY);

        return stored ? { ...DEFAULTS, ...(JSON.parse(stored) as Partial<Settings>) } : DEFAULTS;
    } catch {
        return DEFAULTS;
    }
}

export function useSettings() {
    const [settings, setSettings] = useState<Settings>(load);

    const update = useCallback((change: Partial<Settings>) => {
        setSettings((current) => {
            const next = { ...current, ...change };

            try {
                globalThis.localStorage?.setItem(KEY, JSON.stringify(next));
            } catch {
                // Settings still apply for this visit when storage is unavailable.
            }

            return next;
        });
    }, []);

    return { settings, update };
}

export function prefersReducedMotion(): boolean {
    return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}
