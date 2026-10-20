import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

function isolated(code) {
    const source = `
        import assert from "node:assert/strict";
        import { readFile } from "node:fs/promises";
        import createMjers from "./index.js";
        ${code}
    `;

    const result = spawnSync(process.execPath, ["--input-type=module", "-e", source], {
        cwd: fileURLToPath(new URL("../", import.meta.url)),
        encoding: "utf8",
    });

    assert.equal(result.status, 0, result.stderr || result.stdout);
}

test("import is lazy; failed initialization can be retried with supplied bytes", () => {
    isolated(`
        const original = WebAssembly.instantiate;
        let calls = 0;

        WebAssembly.instantiate = (...args) => {
            calls++;

            return original(...args);
        };

        assert.equal(calls, 0);
        await assert.rejects(createMjers({ wasmBinary: new Uint8Array([0]) }));

        const bytes = await readFile("./mjers_wasm_bg.wasm");
        const [a, b] = await Promise.all([createMjers({ wasmBinary: bytes }), createMjers()]);

        assert.equal(a, b);
        assert.equal(a.analyze("123m456p78s11122z").shanten, 0);
    `);
});

test("accepts a precompiled WebAssembly module", () => {
    isolated(`
        const module = await WebAssembly.compile(await readFile("./mjers_wasm_bg.wasm"));
        const mjers = await createMjers({ wasmBinary: module });

        assert.equal(mjers.ruleset("family").name, "family");
    `);
});

test("browser/CDN loading uses fetch and supports a locateFile override", () => {
    isolated(`
        const bytes = await readFile("./mjers_wasm_bg.wasm");
        let fetched = 0;

        globalThis.fetch = async (url) => {
            assert.equal(String(url), "https://cdn.example.test/engine.wasm");
            fetched++;

            return new Response(bytes, { headers: { "Content-Type": "application/wasm" } });
        };

        const mjers = await createMjers({
            locateFile(path, prefix) {
                assert.equal(path, "mjers_wasm_bg.wasm");
                assert.ok(prefix.endsWith("/npm/"));

                return "https://cdn.example.test/engine.wasm";
            },
        });

        assert.equal(fetched, 1);

        const game = new mjers.Game(1n, 0);

        assert.deepEqual(game.toAct(), [0]);
        game.free();
    `);
});
