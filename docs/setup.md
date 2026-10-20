# Build, run and test

Use a Rust stable toolchain managed by rustup, its `wasm32-unknown-unknown`
target, and Python 3 for static serving. Node.js 18+ and npm are needed only for
the automated browser check. There is no application backend or JavaScript bundle
step. Run commands from the repository root.

## Install build tools

```sh
rustup toolchain install stable --profile minimal --component rustfmt --component clippy
rustup target add --toolchain stable wasm32-unknown-unknown
export PATH="$(rustup run stable rustc --print sysroot)/bin:$HOME/.cargo/bin:$PATH"
cargo install wasm-bindgen-cli --version 0.2.129 --locked
```

The PATH line selects rustup's compiler and Cargo together, including on systems
that also have Homebrew Rust. The CLI must match the crate version pinned in
`Cargo.toml`. Cargo and npm lockfiles are committed; no emulator-core dependency
is used. Initial tool/dependency installation needs network access.

## Native checks

```sh
cargo fmt --check
cargo build --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

The Rust library is browser-independent. Its tests use hand-authored instructions
and synthetic reset vectors; no proprietary files are required.

## Build and serve the browser client

```sh
sh scripts/build-web.sh
cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings
python3 -m http.server 8000 --bind 127.0.0.1 --directory web
```

Open <http://127.0.0.1:8000>. The build creates
`target/wasm32-unknown-unknown/release/saturn_rust.wasm` and the generated
`web/pkg/saturn_rust.js` / `saturn_rust_bg.wasm`. Generated files are ignored by
Git; rebuild them after changing Rust. Opening `index.html` as a `file:` URL will
not work. The client needs browser support for ES modules, Wasm and the
`wasm-unsafe-eval` CSP directive; the tested browser is recorded in
[validation results](validation-issue-3.md).

1. Leave **HLE bootstrap prototype** selected. Click Step to see `R0=5`, then
   Reset and Run the default 6 instructions. Expect `R0=4`, `R1=7`,
   `PC=0x0600000E`, and 6 retired instructions.
2. Step again to see a pending branch, and again to execute its NOP delay slot.
   Further bounded runs stay in that loop. This is a test/homebrew startup path,
   not BIOS service emulation.
3. Choose **User-provided BIOS**. Missing input shows a size error and disables
   stepping. Select a legally obtained **raw 524288-byte** BIOS. Incorrect sizes
   are rejected; correct size validates the layout, not authenticity or region.
   PC and R15 are loaded from the first two big-endian vector words. Step or Run
   may stop immediately with the unsupported opcode and its PC.
4. Reset restarts the selected mode. Changing mode or file performs a cold reset;
   switching modes discards the previous file selection. Run accepts a whole
   count from 1 to 10000. Faults remain stopped until reset.

Only static files are fetched. BIOS bytes are read via `File.arrayBuffer()` and
copied into this tab's Wasm memory, with no upload, browser storage, analytics,
CDN or external runtime assets. They are discarded from application state on mode
change or tab close (this is not a secure memory-erasure guarantee). The fixture
works with network access disabled after the static assets have loaded; offline
reload is not supported because there is no service worker.

## Automated browser smoke test

After building `web/pkg`:

```sh
npm ci --ignore-scripts
npx playwright install chromium
npm run test:browser
```

On a machine with Google Chrome already installed, skip the browser download and
use `BROWSER_CHANNEL=chrome npm run test:browser`. Linux hosts may need Playwright's
documented browser system dependencies. The script starts and stops its own
loopback static server on a free port and runs the real UI/Wasm in a headless
browser. It checks fixture results, controls, invalid run counts, missing/invalid
BIOS, synthetic reset vectors, visible execution faults, reset/mode transitions,
offline execution and the absence of requests carrying BIOS data. It uses no
Sega firmware. Nonzero exit means failure.

Use only BIOS/game data you are authorized to use. Never commit, redistribute or
upload proprietary firmware or disc images to this repository. Disc loading is
not implemented. See [provenance and roadmap](architecture.md#sources-and-provenance).
