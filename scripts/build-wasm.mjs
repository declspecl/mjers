import { execFileSync } from "node:child_process";
import { resolve } from "node:path";

const root = new URL("../", import.meta.url);
const buildOptions = { cwd: root, stdio: "inherit" };

execFileSync("cargo", ["build", "--locked", "--release", "-p", "mjers-wasm", "--target", "wasm32-unknown-unknown"], buildOptions);
const { target_directory } = JSON.parse(execFileSync("cargo", ["metadata", "--locked", "--no-deps", "--format-version", "1"], { cwd: root, encoding: "utf8" }));
execFileSync("wasm-bindgen", [
    resolve(target_directory, "wasm32-unknown-unknown/release/mjers_wasm.wasm"),
    "--target", "web",
    "--out-dir", "npm",
    "--out-name", "mjers_wasm",
    "--typescript",
], buildOptions);
