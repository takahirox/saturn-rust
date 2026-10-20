#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
expected="wasm-bindgen 0.2.129"
if ! command -v wasm-bindgen >/dev/null 2>&1 || [ "$(wasm-bindgen --version)" != "$expected" ]; then
  echo "Install the matching CLI: cargo install wasm-bindgen-cli --version 0.2.129 --locked" >&2
  exit 1
fi
cargo build --locked --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir web/pkg --no-typescript target/wasm32-unknown-unknown/release/saturn_rust.wasm
