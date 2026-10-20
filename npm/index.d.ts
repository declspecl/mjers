/// <reference lib="esnext.disposable" />
export type * from "./types.js";

import type * as Bindings from "./mjers_wasm.js";

export interface MjersModuleOptions {
    /** Supply bytes or a compiled module instead of loading the adjacent file. */
    wasmBinary?: BufferSource | WebAssembly.Module;
    /** Resolve the WASM asset, e.g. from a CDN. The prefix is an absolute URL. */
    locateFile?: (path: string, prefix: string) => string | URL;
}

export type MjersModule = Pick<typeof Bindings, "Game" | "Bot" | "analyze" | "ruleset">;

/** Lazy, shared initialization. Options apply to the first successful call. */
export declare function createMjers(options?: MjersModuleOptions): Promise<MjersModule>;
export default createMjers;
