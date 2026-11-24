#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
expected_optimizer="wasm-opt version 127"
optimizer_version="$(wasm-opt --version 2>/dev/null || true)"
case "$optimizer_version" in
  "$expected_optimizer"|"$expected_optimizer ("*")") ;;
  *)
    echo "Install Binaryen 127 and add its bin directory to PATH; see docs/setup.md." >&2
    exit 1
    ;;
esac
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
# Keep hot CPU/bus helpers visible to LLVM and use SIMD for the four cache tags.
# The separate compatible output keeps the existing instruction requirements.
rustflags="${RUSTFLAGS--C llvm-args=--inline-threshold=1000 -C target-feature=+simd128}"
target_wasm="${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/saturn_rust.wasm"
build_package() {
  flags="$1"
  output="$2"
  shift 2
  RUSTFLAGS="$flags" cargo rustc --locked --release --target wasm32-unknown-unknown --lib --features sound-worker -- "$@"
  wasm-bindgen --target web --out-dir "$output" --no-typescript "$target_wasm"
}

optimize_package() (
  input="$1/saturn_rust_bg.wasm"
  optimized_wasm="$(mktemp "$1/.saturn-opt-XXXXXX")"
  trap 'rm -f "$optimized_wasm"' EXIT HUP INT TERM
  shift
  wasm-opt "$input" -O4 \
    --enable-sign-ext --enable-mutable-globals \
    --enable-nontrapping-float-to-int --enable-bulk-memory \
    --enable-reference-types --enable-multivalue --enable-simd "$@" -o "$optimized_wasm"
  chmod 644 "$optimized_wasm"
  mv "$optimized_wasm" "$input"
)

build_package "$rustflags" web/pkg
optimize_package web/pkg
tail_package="$(mktemp -d)"
trap 'rm -rf "$tail_package"' EXIT HUP INT TERM
# These branch counts come from an original, locally authored Saturn guest.
# Keep the regular optimized build on toolchains with a different LLVM format.
if [ "${RUSTFLAGS+x}" != x ] && [ "$(rustc --version)" = 'rustc 1.100.0 (0ef1bc386 2026-11-09)' ]; then
  profile="$tail_package/saturn-wasm.profdata"
  python3 -c 'import gzip, shutil, sys
with gzip.open(sys.argv[1], "rb") as source, open(sys.argv[2], "wb") as destination:
    shutil.copyfileobj(source, destination)' scripts/saturn-wasm.profdata.gz "$profile"
  build_package "$rustflags -C target-feature=+tail-call" "$tail_package" \
    "-Cprofile-use=$profile" -Cllvm-args=-disable-vp
else
  build_package "$rustflags -C target-feature=+tail-call" "$tail_package"
fi
optimize_package "$tail_package" --enable-tail-call
python3 scripts/select-web-wasm.py web/pkg "$tail_package"
