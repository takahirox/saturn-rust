# Issue #101 validation: BIOS-only browser startup

## Scope

User-provided BIOS mode can start after a local raw 524288-byte BIOS has been
fully read and accepted, with no disc selected. HLE disc mode still requires a
valid disc. Both use the existing Rust/Wasm reset and CPU/frame scheduler paths;
BIOS mode never falls back to HLE. No emulator hardware behavior was changed.

Start, Pause/Resume, Restart, Debug Reset, Step and frame advance work without
media. Resetting/sleeping machine states and actual execution faults are reported
without asking for a missing disc. Restart remains available to recover a valid
boot input after an execution fault. Rejected local selections remain blocked
through Reset and mode changes until replaced or cleared, and pending reads still
block execution. Mode switches retain completed selections and cancel pending
reads. File replacement stops playback and releases input.

Setup, placeholder and status text describe optional media in BIOS mode and
required media in HLE disc mode. The existing layout, developer fixture, local
privacy policy, explicit audio gesture and backup RAM controls are retained.

This completes UI boot enablement only. Authentic Sega BIOS menus, CD-player
behavior, retail-game compatibility, optical-drive timing and remaining hardware
conformance are unverified or incomplete. Related to #17; that tracker remains
open. No proprietary firmware, commercial disc, live Pages inspection or human
approval is a prerequisite. Required post-merge verification: none.

## Original synthetic firmware and regression oracles

[`bios-only.hex`](../tests/fixtures/bios-only.hex) is an original, manually authored
SH-2 diagnostic under the adjacent [MIT license](../tests/fixtures/LICENSE).
Its explicit big-endian offset/word representation expands into a zero-filled
524288-byte ROM with SHA-256
`6f5bf3ce87e998b43cdaebc4067b6db8a13d08d156fe8f390412080800dbb1b1`.
The reset PC/SP are `0x100` / `0x06100000`. Six instructions load a counter address,
initialize R0, then increment/store R0 at `0x06001000` in a BRA/NOP loop. The native
and browser tests independently expand the same bytes. Neither test writes guest
RAM, CPU registers, pixels or a firmware menu on behalf of the guest.

- The new `tests/boot.rs` test advances real raster frames through reset, checks
  retired instructions and increasing guest-written RAM, asserts no CD medium is
  attached, and repeats a cold boot while checking backup RAM retention.
- The existing `ui-bios-and-exceptions` browser group runs the production Wasm
  and Canvas UI. With an empty disc picker, it checks BIOS eligibility during
  hardware reset, Start-driven guest instructions/RAM/frame progress, stationary
  state during Pause, Resume progress, synchronous Restart clearing counters
  before playback continues, and Debug Reset/Step/frame functionality. A SLEEP
  guest with no wake source still permits Pause; Resume remains disabled and
  reports the real sleeping state until a reset makes execution possible.
- A second existing original MIT [`external-startup.hex`](../tests/fixtures/external-startup.hex)
  firmware reaches its own `0xE001` no-media result through the UI Start action.
  The existing native `external_startup` no-media control checks the same result.
  This catches an implicitly attached substitute medium. A deliberately faulting
  synthetic BIOS checks honest unsupported-register diagnostics and disabled
  execution controls.
- Negative UI cases cover missing and wrong-sized BIOS, rejected local reads,
  pending-read cancellation, HLE without media, and invalid optional media that
  stays blocked through Reset/mode changes. Existing BIOS+disc, CUE/BIN/CHD/ISO,
  retained-media, focus/visibility, backup RAM, audio and debug checks remain.

The expanded UI coverage stays in the existing browser group; no worker, timeout,
retry, fixture-builder registry or CI/publication gate changed. The new fixture is
literal authored data, with no generated source or additional reproduction tool.
Broad hardware conformance remains in existing native/guest tests.

## Executed validation

Executed locally on macOS arm64 with stable Rust 1.99.0, Node 26.12.1,
Playwright 1.58.2, Chromium 145.0.7632.6 and wasm-bindgen 0.2.129.

- `cargo fmt --check`, `cargo build --locked` and focused `cargo test --locked
  --test boot` pass (6 focused tests).
- Complete `cargo test --locked`: 1,167 tests pass, no failures or ignored tests,
  across 103 unit/integration/doc-test harnesses. Summed harness duration is
  483.07 s; test compilation is 44.63 s.
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` pass.
- `sh scripts/check-fixtures.sh`: all 103 registered reproduction commands pass.
- `sh scripts/build-web.sh`: production Wasm and web bindings build successfully.
- `node --test scripts/ci-runner.test.mjs`: all 13 checks pass, including expected
  deliberate-failure propagation and publication-gate assertions.
- Focused production Chromium `playUi` validation passes. Desktop 1440 px and
  mobile 390 px BIOS-only ready-state screenshots were visually inspected;
  automated responsive checks also cover 1024, 768 and 320 px widths.
- Complete `npm run test:browser`: all 74 groups pass (32 shared in 394.601 s,
  42 scenarios in 342.041 s; total wall time 394.854 s). The normal two-worker
  invocation verifies complete group coverage and the exact tested-site digest.
  The expanded existing `ui-bios-and-exceptions` group takes 8.451 s. These
  concurrent local timings are not hosted-CI performance claims.
- JavaScript syntax, relative Markdown link targets and staged Git whitespace
  checks pass.

The first focused UI attempt identified that Reset replaced a local-read failure
with the missing-BIOS error. Reset now preserves the selection-specific error;
the corrected focused run passes. The initial Chromium launch lacked the pinned
headless shell. Its official Playwright download completed but Node 26 extraction
stalled; the downloaded archive passed a full ZIP CRC check and was extracted
with its original file permissions. All browser validation uses that pinned shell.

A final control probe found that a guest sleeping without a wake source disabled
Pause even while playback was active. Pause now remains available for active
sessions; Resume still requires an executable machine. The in-progress browser
run was stopped and the complete suite restarted with this correction and its
regression case, rather than treating earlier partial group results as completion.

Hosted GitHub Actions was not invoked: this node is prohibited from pushing or
opening a PR. Local validation uses the normal required native/Wasm/browser and
runner commands, without claiming hosted CI or live deployment results.
