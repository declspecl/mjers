/** Production backend endpoints. Local development proxies `/tables` and `/ws` to the dev server (see vite.config.ts). */
const PRODUCTION_HTTP = "https://ovx79phvqa.execute-api.us-east-1.amazonaws.com";
const PRODUCTION_WS = "wss://21g3gv0b56.execute-api.us-east-1.amazonaws.com/prod";

export function httpBase(): string {
    return import.meta.env.VITE_MJERS_HTTP ?? (import.meta.env.DEV ? "" : PRODUCTION_HTTP);
}

export function socketUrl(table: string, token: string): string {
    const query = `table=${encodeURIComponent(table)}&token=${encodeURIComponent(token)}`;
    const configured = import.meta.env.VITE_MJERS_WS ?? (import.meta.env.DEV ? undefined : PRODUCTION_WS);

    if (configured !== undefined) {
        return `${configured}?${query}`;
    }

    const protocol = globalThis.location.protocol === "https:" ? "wss:" : "ws:";

    return `${protocol}//${globalThis.location.host}/ws?${query}`;
}
