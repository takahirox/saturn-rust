#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
python3 homebrew/star-collector/build.py
cp homebrew/star-collector/LICENSE web/star-collector-LICENSE.txt
python3 homebrew/prism-courier/build.py
cp homebrew/prism-courier/LICENSE web/prism-courier-LICENSE.txt
python3 src/hle/build.py --check
python3 homebrew/hardware-lab/build.py
cp homebrew/hardware-lab/LICENSE web/hardware-lab-LICENSE.txt
expected="wasm-bindgen 0.2.129"
if ! command -v wasm-bindgen >/dev/null 2>&1 || [ "$(wasm-bindgen --version)" != "$expected" ]; then
  echo "Install the matching CLI: cargo install wasm-bindgen-cli --version 0.2.129 --locked" >&2
  exit 1
fi
cargo build --locked --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir web/pkg --no-typescript target/wasm32-unknown-unknown/release/saturn_rust.wasm
