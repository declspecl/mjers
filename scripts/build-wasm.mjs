import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));

function run(command, args) {
    const result = spawnSync(command, args, { cwd: root, stdio: "inherit" });
    if (result.error) throw result.error;
    if (result.status !== 0) process.exit(result.status ?? 1);
}

// Match mjers-wasm/Cargo.toml; bindgen's CLI and library must agree.
const version = spawnSync("wasm-bindgen", ["--version"], { encoding: "utf8" });
if (version.status !== 0 || version.stdout.trim() !== "wasm-bindgen 0.2.125") {
    throw new Error("Install the matching CLI: cargo install wasm-bindgen-cli --version 0.2.125 --locked");
}

run("cargo", ["build", "--locked", "--release", "-p", "mjers-wasm", "--target", "wasm32-unknown-unknown"]);
const metadata = spawnSync("cargo", ["metadata", "--locked", "--no-deps", "--format-version", "1"], { cwd: root, encoding: "utf8" });
if (metadata.status !== 0) throw new Error(metadata.stderr || "cargo metadata failed");
const target = JSON.parse(metadata.stdout).target_directory;
run("wasm-bindgen", [
    resolve(target, "wasm32-unknown-unknown/release/mjers_wasm.wasm"),
    "--target", "web",
    "--out-dir", "npm",
    "--out-name", "mjers_wasm",
    "--typescript",
]);
