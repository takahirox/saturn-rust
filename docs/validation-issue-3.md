# Issue #3 validation

Validated on 2026-10-20 on macOS/aarch64. The live Issue was read before
implementation. GitHub `main` was still `aa248ef5bcfda10fa3f903d9ab7d3ed85933e896`
with contribution/process files only; those files were preserved and their
obsolete tooling descriptions updated.

## Results

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed; native library |
| `cargo test --locked` | Passed: 17 tests (9 CPU, 5 boot/machine, 3 bus); no ignored tests |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `sh scripts/build-web.sh` | Passed; release `wasm32-unknown-unknown` artifact and static JS/Wasm bindings generated |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `node --check web/app.js` and `node --check scripts/browser-smoke.mjs` | Passed |
| `sh -n scripts/build-web.sh` | Passed |
| `npm ci --ignore-scripts` | Passed; dependency audit reported no vulnerabilities |
| `BROWSER_CHANNEL=chrome npm run test:browser` | Passed in headless Google Chrome 155.0.8059.85 with Playwright 1.58.2 |
| Markdown relative links/anchors, HTML structure and source review | Checked; no proprietary assets or imported emulator code |
| `git diff --cached --check` | Passed |

Final native and Wasm checks used rustup Rust 1.99.0, wasm-bindgen crate/CLI
0.2.129, Node 24.12.0 and Python 3.13.13. Native build/tests/Clippy also passed
with Homebrew Rust 1.94.0. Generated binaries and npm dependencies are ignored;
the checkpoint contains their reproducible sources and lockfiles.

CPU checks cover each implemented opcode, immediate sign extension, wrapping
arithmetic, unchanged SR, signed branch displacement boundaries, exactly one
delay slot, RTS return, illegal/unsupported slots, sticky diagnostics and fetch
faults. Bus checks cover byte/word/long ordering, both RAM banks, ROM boundaries
and write rejection, unmapped regions, alignment and failed-write atomicity.
Boot checks cover BIOS lengths, PC/SP reset vectors, prototype startup, cold
reset/mode transitions, fault clearing and bounded execution.

The browser check exercises real UI controls and Wasm: default six-instruction
fixture result (`R0=4`, `R1=7`, `PC=0x0600000E`), Step/Reset/Run, invalid counts,
missing/incorrect BIOS size, synthetic external reset vectors, unsupported-opcode
diagnostic, mode changes and asynchronous file-read cancellation. It also checks
invalid file replacement during a pending read, module-load error reporting,
offline execution after asset load, and requests restricted to static same-origin
GETs without query strings or bodies. No real Sega BIOS/game data was used.
The fixture trace was compared against the manual calculation in
[architecture notes](architecture.md#hle-bootstrap-prototype-and-manual-trace).

## Resolved check failures and limits

Initial Wasm checks failed because Homebrew Rust was selected while the Wasm
target belonged to rustup. Selecting the rustup toolchain's bin directory via
the documented PATH command resolved this; the final checks passed. An initial
browser attempt before assets were built timed out; the built client passed.
The added initialization-failure test initially used Playwright's disabled check
on a fieldset; it now checks the Reset button affected by that fieldset and passes.
Installing the pinned upstream wasm-bindgen CLI succeeded with warnings about
its own locked transitive dependencies (a yanked version and future Rust
compatibility); project build/lint checks emitted no warnings.

Additional browsers, reference-emulator comparison and throughput measurements
were optional and were not run. Real BIOS boot completion, games, hardware timing
and peripherals are outside this Issue; synthetic external BIOS input verifies
the required loading/reset/stepping path without proprietary data. No required
post-merge verification remains.
