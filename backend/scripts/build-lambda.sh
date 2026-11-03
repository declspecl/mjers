#!/usr/bin/env sh
set -eu
cd "$(dirname "$0")/../.."
uvx --from cargo-lambda==1.9.2 cargo-lambda lambda build --locked --release --arm64 --output-format zip --bin mjers-lambda -p mjers-lambda
