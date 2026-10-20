import { execFileSync } from "node:child_process";
import { resolve } from "node:path";

const root = new URL("../", import.meta.url);
const buildOptions = { cwd: root, stdio: "inherit" };

execFileSync("cargo", ["build", "--locked", "--release", "-p", "mjers-wasm", "--target", "wasm32-unknown-unknown"], buildOptions);

const metadata = execFileSync("cargo", ["metadata", "--locked", "--no-deps", "--format-version", "1"], { cwd: root, encoding: "utf8" });
const { target_directory: targetDirectory } = JSON.parse(metadata);

execFileSync(
    "wasm-bindgen",
    [
        resolve(targetDirectory, "wasm32-unknown-unknown/release/mjers_wasm.wasm"),
        "--target",
        "web",
        "--out-dir",
        "npm",
        "--out-name",
        "mjers_wasm",
        "--typescript",
    ],
    buildOptions,
);
