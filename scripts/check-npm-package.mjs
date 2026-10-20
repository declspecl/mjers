import { execFileSync } from "node:child_process";
import { copyFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const consumer = await mkdtemp(join(tmpdir(), "mjers-npm-"));

function run(command, args, cwd = consumer) {
    return execFileSync(command, args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
}

try {
    const cache = join(consumer, "cache");
    const packOutput = run("npm", ["pack", "./npm", "--ignore-scripts", "--json", "--pack-destination", consumer, "--cache", cache], root);
    const [packed] = JSON.parse(packOutput);

    run("npm", ["install", join(consumer, packed.filename), "--offline", "--ignore-scripts", "--no-audit", "--no-fund", "--cache", cache]);

    await copyFile(new URL("../npm/tests/types.ts", import.meta.url), join(consumer, "consumer.mts"));
    run(process.execPath, [
        join(root, "npm/node_modules/typescript/bin/tsc"),
        "--strict",
        "--noEmit",
        "--target",
        "ES2022",
        "--module",
        "NodeNext",
        "--lib",
        "ES2022,DOM",
        "consumer.mts",
    ]);

    await copyFile(new URL("../npm/tests/engine.test.mjs", import.meta.url), join(consumer, "engine.test.mjs"));
    run(process.execPath, ["--test", "engine.test.mjs"]);

    console.log(`Packed package passed clean-install, runtime, and TypeScript checks (${packed.size} bytes).`);
} finally {
    await rm(consumer, { recursive: true, force: true });
}
