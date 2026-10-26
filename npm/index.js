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
        Network: bindings.Network,
        analyze: bindings.analyze,
        ruleset: bindings.ruleset,
        loadNetwork: (source) => loadNetwork(bindings, source),
    });
}

/** Build a network from the bytes of a suit-resnet-v2 safetensors checkpoint. */
function loadNetwork(bindings, source) {
    const bytes = ArrayBuffer.isView(source) ? new Uint8Array(source.buffer, source.byteOffset, source.byteLength) : new Uint8Array(source);
    const length = Number(new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getBigUint64(0, true));
    const header = JSON.parse(new TextDecoder().decode(bytes.subarray(8, 8 + length)));
    const metadata = JSON.parse(header.__metadata__?.mjers ?? "null");

    if (metadata?.architecture !== "suit-resnet-v2") {
        throw new Error("only suit-resnet-v2 checkpoints run in the browser");
    }

    const version = metadata.dataset.encoding_version;
    const planes = bindings.encodingPlanes(version);

    if (planes === undefined) {
        throw new Error(`checkpoint uses observation encoding ${version}`);
    }

    const weights = new bindings.NetworkWeights();

    try {
        for (const [name, tensor] of Object.entries(header)) {
            if (name === "__metadata__") {
                continue;
            }

            if (tensor.dtype !== "F32") {
                throw new Error(`${name} is not float32`);
            }

            const [begin, end] = tensor.data_offsets;
            // Copy so the floats are aligned; safetensors and Float32Array are little-endian here.
            const data = new Float32Array(bytes.slice(8 + length + begin, 8 + length + end).buffer);

            weights.add(name, Uint32Array.from(tensor.shape), data);
        }

        const network = new bindings.Network(weights);

        if (network.planes !== planes) {
            network.free();
            throw new Error(`checkpoint's stem does not read encoding ${version} observations`);
        }

        return network;
    } finally {
        weights.free();
    }
}

/**
 * Initialize once, on demand. Concurrent callers share the same runtime.
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
