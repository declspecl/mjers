let runtimePromise;

async function initialize(options) {
    const bindings = await import("./mjers_wasm.js");
    let source = options.wasmBinary;
    if (source === undefined) {
        const url = options.locateFile
            ? new URL(options.locateFile("mjers_wasm_bg.wasm", new URL(".", import.meta.url).href), import.meta.url)
            : new URL("./mjers_wasm_bg.wasm", import.meta.url);
        if (url.protocol === "file:" && typeof process !== "undefined" && process.versions?.node) {
            const { readFile } = await import("node:fs/promises");
            source = await readFile(url);
        } else {
            source = url;
        }
    }
    await bindings.default({ module_or_path: source });
    return Object.freeze({
        Game: bindings.Game,
        Bot: bindings.Bot,
        analyze: bindings.analyze,
        ruleset: bindings.ruleset,
    });
}

/** Initialize once, on demand. Concurrent callers share the same runtime.
 * Options from the first successful call select the WASM binary.
 */
export function createMjers(options = {}) {
    if (!runtimePromise) {
        runtimePromise = initialize(options).catch((error) => {
            runtimePromise = undefined;
            throw error;
        });
    }
    return runtimePromise;
}

export default createMjers;
