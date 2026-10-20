# Issue #11 implementation checkpoint validation

This is **partial implementation**, not Issue completion. Issue #11 remains open.
The [conformance matrix](hardware-conformance.md) records all remaining built-in
hardware requirements and evidence limits. No push, PR, merge, comment or deployment
was performed in this node.

## Implemented at this checkpoint

- Independently expanded the master SH-2 reference interpreter to decode all 142
  documented architectural forms, including addressing modes, carry/overflow,
  comparisons, bit/shift operations, DIV1, multiply/MAC and system transfers.
- Added guest illegal/slot-illegal/TRAPA entry and RTE, standalone CPU IRQ/NMI
  entry with masking/deferral, bounded SLEEP, and separate exception diagnostics.
  Architectural address-error dispatch and device IRQ integration remain absent.
- Added an independently maintained encoding inventory and semantic edge vectors.
  All 65,536 encodings are checked: 53,752 valid and 11,784 illegal. This exhaustive
  decode check is not an exhaustive proof of every instruction's semantic behavior.
- Added one original MIT-licensed synthetic ROM shared by native and Chromium/Wasm
  tests. Its guest handlers manipulate the emulated exception stack and return
  through RTE. The CPU stores 35 at `06001000`, retires 19 instructions, enters
  three exceptions and sleeps at next-PC `0000011A`. Further bounded steps/frame
  advancement preserve that CPU state; reset clears exception state.
- Preserved Star Collector source/assets/ISO and guest-visible gameplay, scoring,
  finish/restart and input. No new complete game, audio path or native BIOS
  compatibility is claimed.

## Actual checks (2026-10-20)

Environment: macOS aarch64, rustup stable Rust/Cargo 1.99.0, installed
`wasm32-unknown-unknown` target, wasm-bindgen-cli 0.2.129, Node 24.12.0 and
Playwright 1.58.2 with headless Chromium 145.0.7632.6. The stable toolchain and
matching CLI were selected using the PATH recipe in [setup](setup.md); the host
also has a different Homebrew Cargo version, which was not used for validation.

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 52 tests (35 CPU unit, 17 integration), no skipped tests; zero doc tests |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `python3 homebrew/star-collector/build.py --check` | Passed: original 131,072-byte ISO reproduced |
| `sh scripts/build-web.sh` | Passed: release Wasm, matching generated bindings and original ISO |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `npm ci --ignore-scripts` | Passed |
| `npm run test:browser` | Passed in real headless Chromium at `/saturn-rust/` |
| `git diff --check` / staged whitespace check | Passed |

Browser evidence includes Play/Debug state and diagnostic preservation; three
responsive viewport sizes; keyboard focus/release; actual fullscreen entry/exit
and denied/unavailable paths; relative assets/MIME/Wasm; synthetic BIOS vectors
and bounded fault/reset; the shared exception ROM and sleeping CPU; malformed
local input and loading races; offline local media; no local-file transmission;
and actual Star Collector guest input/pixels, scoring/completion/restart.

The first browser attempts exposed a test-scroll edge after adding Debug rows:
Chromium's intersection calculation treated a select clipped by 0.42 CSS pixels
as fully visible, so `scrollIntoViewIfNeeded` did not move it. The harness now
explicitly centers each control before asserting its exact viewport bounds; the
bounds assertion remains intact. The corrected complete browser run passed.
The previous unsupported-opcode diagnostic fixture now uses an unaligned data
access, because illegal opcodes correctly enter guest exception handlers.

## Unperformed required work / verification

The missing systems in the matrix are not verified or accepted. In particular:
slave SH-2 and sound 68EC000, hardware-time scheduler/cache/on-chip devices, full
bus/backup RAM, SCU DMA/IRQ/DSP, VDP1 and remaining VDP2, SCSP/DSP/audio/CD-DA,
runtime CD and BIN/CUE, complete BIOS HLE/native boot, SMPC commands/RTC/second
port/analog, and the additional independent guest discs/DSP fixtures remain open.

No lawfully supplied real BIOS was available. Genuine BIOS initialization/menu or
BIOS-driven boot beyond reset vectors was not tested. The original synthetic ROM
proves guest exception/control flow only. No firmware, SDK, commercial media,
reference emulator core or external processor library was introduced. No
reference-emulator/physical hardware differential observations were performed.
Firefox, Safari, touch hardware, analog input, audio playback and commercial
compatibility were not tested or claimed.

## Required post-merge verification

Pending for the eventual full implementation: after its merge and successful
Pages deployment, verify the public site, record the deployed commit and workflow,
and exercise independent homebrew, video/audio, BIOS/local media, input and Debug.
Local checks here do not establish public deployment or satisfy that requirement.
