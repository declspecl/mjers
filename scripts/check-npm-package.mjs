import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const consumer = await mkdtemp(join(tmpdir(), "mjers-npm-"));

function run(command, args, cwd = consumer) {
    const result = spawnSync(command, args, { cwd, encoding: "utf8" });
    if (result.error) throw result.error;
    assert.equal(result.status, 0, result.stderr || result.stdout);
    return result.stdout;
}

try {
    const cache = join(consumer, "cache");
    const packed = JSON.parse(run("npm", [
        "pack", "./npm", "--ignore-scripts", "--json", "--pack-destination", consumer, "--cache", cache,
    ], root))[0];
    const files = packed.files.map((file) => file.path);
    assert.ok(files.includes("mjers_wasm_bg.wasm"));
    assert.ok(files.includes("mjers_wasm.d.ts"));
    run("npm", [
        "install", join(consumer, packed.filename), "--offline", "--ignore-scripts", "--no-audit", "--no-fund", "--cache", cache,
    ]);
    run(process.execPath, ["--input-type=module", "-e", `
        import assert from "node:assert/strict";
        import createMjers from "@mjers/mjers";
        import * as types from "@mjers/mjers/types";
        assert.deepEqual(Object.keys(types), []);
        const mjers = await createMjers();
        const game = new mjers.Game(42n, 0);
        const advice = game.advise(0, { samples: 0 });
        game.apply(0, advice.recommended);
        assert.equal(mjers.analyze("123m456p78s11122z").shanten, 0);
        game.free();
    `]);
    const types = await readFile(new URL("../npm/tests/types.ts", import.meta.url), "utf8");
    await writeFile(join(consumer, "consumer.mts"), types);
    run(process.execPath, [
        join(root, "npm/node_modules/typescript/bin/tsc"), "--strict", "--noEmit",
        "--target", "ES2022", "--module", "NodeNext", "--lib", "ES2022,DOM", "consumer.mts",
    ]);
    console.log(`Packed package passed clean-install, runtime, and TypeScript checks (${packed.size} bytes).`);
} finally {
    await rm(consumer, { recursive: true, force: true });
}
