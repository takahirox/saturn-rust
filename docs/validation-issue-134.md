# Issue #134 input validation

Player 1 now exposes all thirteen Saturn digital controls through physical
`KeyboardEvent.code` bindings: Arrows for directions, Z/X/C for A/B/C, A/S/D
for X/Y/Z, Q/W for L/R and Enter for Start. Shift, Caps Lock and changed
characters/layout do not alter these positions. Host Ctrl/Alt/Meta shortcuts
do not acquire buttons; keyup still releases a button if modifiers change.
Repeated keydown events neither duplicate input nor reacquire input cleared
by a focus/pause transition. Focused arrow repeats remain default-prevented.

Player 2's existing keyboard mapping remains intentionally partial: I/K/J/L
for up/down/left/right, U/O/P for A/B/C and Right Shift for Start. Its X/Y/Z/L/R
require a gamepad. No Player 1 binding overlaps these keys. The visible input
hint, Advanced help, README and setup guide document the controls and limits.

The existing full Standard Gamepad button table and analog/3D-pad axes,
circular bound and physical trigger hysteresis are preserved. Keyboard L/R
also supply full-travel trigger levels when an attached Standard Gamepad selects
the 3D-pad report, so these keys remain guest-visible alongside analog input.
Their report overlay is separate from the physical trigger levels and latches;
keyboard release restores the physical travel and its independently retained
hysteresis state.
Key transitions refresh that combined input immediately. The first two connected
gamepads take free Saturn ports; a connected pad retains its port when another
disconnects. This fixes the prior filtering behavior that shifted P2 onto P1
when P1 disconnected. Digital buttons 6/7 retain their existing R/L masks;
the setup guide's reversed digital shoulder description is corrected.

## Guest evidence

The original MIT-licensed [digital-input.hex](../tests/fixtures/digital-input.hex)
is reproduced by the existing
[`build_smpc.py --check`](../tests/fixtures/build_smpc.py) registry entry.
Its expanded 524288-byte ROM SHA-256 is
`7819b62a4ba5c0423ab1b781387471b041e09537e9baed92706ae4fc17320a8b`.
The original one-shot `smpc.hex` remains byte-identical. The new position-independent
SH-2 guest repeatedly issues peripheral-only INTBACK, waits for SF completion,
copies the actual SMPC output bytes to `0x06008020` and increments a sequence
at `0x06008044`. The adjacent fixture LICENSE applies. No guest RAM is written
by the browser input adapter or assertions.

The native test in [smpc.rs](../tests/smpc.rs) checks fresh neutral, all-buttons
and released packets on both ports without resetting between transitions.
[digital-input.mjs](../scripts/digital-input.mjs), invoked by the existing
Play UI group, runs the same guest payload through both external-BIOS reset
and an HLE-compatible original local ISO container. Production event handlers,
Gamepad polling and Wasm setters remain active. Test-only setter instrumentation
records masks and forwards every call to the real emulator. Each observation
requires two fresh completed guest reports, so an earlier in-flight INTBACK
cannot satisfy a release assertion.

Chromium checks independent literal active-low packet expectations for:

- Every P1 physical key and every Standard Gamepad digital button individually
  on both ports, with a fresh release report after each press. Both the digital
  ID02 and realistic Standard Gamepad ID16 paths are exercised.
- All P1 keys held simultaneously, all existing P2 bindings individually and
  together, and independent port release without collisions.
- Shifted characters, changed layout characters, repeat, modifier-changing keyup,
  host shortcuts and unfocused input.
- Keyboard/gamepad union, overlapping A presses, release of either source and
  simultaneous two-port input, including keyboard shoulders with 3D-pad triggers.
- Guest-visible physical trigger hysteresis at levels 140, 146, 100 and 85,
  independently of the adapter's ordinary greater-than-0.5 button bit.
- Q/W release with inactive and active physical triggers in the 86–145 hold
  band, physical activation/release while those keys are held, and 3D-pad blur
  release. Both boot modes check fresh INTBACK bytes and reported trigger travel.
- Disconnect, a sparse browser slot, neutral reconnect and retention of P2's
  port while P1 is absent.
- Canvas/window blur, visibility hiding, pause, settings, view changes, restart
  and boot-mode changes with both keyboard and gamepad input asserted. A repeat
  after focus returns cannot restore the released keyboard state.

Existing native/browser SMPC status/IRQ, ID15/ID16 analog/3D-pad and direct
handshake guests remain in the complete suites, together with HLE gameplay,
pad-2, Play/Debug, audio, BIOS-only, ISO/CUE/BIN/CHD, backup and local-file
security/privacy regressions. No new browser group or fixture command is added:
the inventory remains 74 groups and 106 fixture commands. Broad UI-boundary
coverage is kept in Chromium; the small native addition verifies the reusable
guest's continuing press/release behavior.

## Executed checks

Executed locally on macOS arm64 with rustup stable Rust 1.99.0, Node 26.12.1,
Playwright 1.58.2, pinned Chromium 145.0.7632.6, wasm-bindgen 0.2.129 and
Binaryen 127:

- `cargo fmt --check` and `cargo build --locked` pass. An initial formatting
  finding in the new Rust assertion was corrected with rustfmt.
- `cargo test --locked`: **1,337 passed**, no failures or ignored tests,
  across 111 harnesses including doc tests.
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` pass.
- `sh scripts/check-fixtures.sh`: all 106 reproduction commands pass,
  including the new polling guest in the existing SMPC builder check.
- `sh scripts/build-web.sh` passes, including the required production optimizer.
- Focused production Chromium digital-input checks pass in both boot modes.
- `node --test scripts/ci-runner.test.mjs`: **13 passed**, no failures/skips
  (11.544 seconds), including deliberate assertion failures and publication gates.
- Complete `npm run test:browser`: **all 74 groups pass**, shared 33/33 in
  224.523 seconds and scenarios 41/41 in 185.294 seconds; aggregate two-worker
  wall time 224.795 seconds. The expanded Play UI group took 37.954 seconds.
  Complete coverage and exact tested-site identity verification pass.
- JavaScript syntax checks, fixture hashes and Git whitespace checks pass.

These are local concurrent timings, not hosted-CI performance measurements.
Expanded 3D-pad coverage initially failed for keyboard Q/L: its digital mask
arrived, but the ID16 report replaced that bit with a neutral analog trigger.
The initial frontend fix supplied the keyboard's full-travel trigger and repolled
on key transitions. Review then found that this changed the physical hysteresis
latch, leaving a shoulder stuck after keyup at travel 100. The correction passes
physical travel unchanged and applies digital shoulders only when generating a
report. It retains the existing trigger thresholds and full-travel keyboard
reports, without letting either source alter the other's latch.
The first browser launch found no installed pinned executable. Playwright's
download then stalled after transfer under Node 26; the official pinned Chrome
and headless-shell archives passed complete ZIP CRC checks and were extracted
locally. Validation used the pinned version throughout, without substituting a
different browser or weakening assertions.

## Review correction checks

Revalidated the shoulder-latch correction locally with the same tool versions:

- Formatting, native build and complete native tests pass, as do native
  all-target and Wasm Clippy with warnings denied.
- Fixture reproducibility (106 commands), optimized production Wasm build and
  JavaScript syntax checks pass.
- All 13 CI-runner checks pass, without skips.
- The complete production Chromium suite passes all 74 groups: shared 33/33
  in 225.562 seconds, scenarios 41/41 in 182.400 seconds, and aggregate
  two-worker wall time 225.833 seconds. The expanded Play UI group passes
  in 41.151 seconds, including both boot modes and 3D-pad blur release.
- A negative control temporarily restored the original frontend against the
  rebuilt Wasm. The new `bios KeyQ inactive released` guest assertion failed
  as expected: L remained pressed after keyup. The corrected frontend was
  restored before the complete suite ran.

The first build attempt selected Homebrew Rust without its Wasm target; the
documented rustup-stable PATH resolved it. A standalone `playUi` group selection
was rejected because this group belongs to the shared trace; the complete suite
above supplies its validation. These setup/selection failures did not relax
any assertions or required checks.

## Automatic boot selection integration

Integrated main `efac3c8169e313ef9d3c75dbb3844f93e64ec6de` (PR #136)
to resolve PR #137's conflicts. Input admission now uses the automatic boot
selection's validity checks while preserving the complete physical keyboard
mapping, stable gamepad ports and independent keyboard/physical trigger latches.
The controls help retains every binding and describes automatic restart selection.

The digital-input browser guest no longer references the removed `#mode` selector.
It loads the original firmware through the BIOS picker for external reset and
clears that picker for HLE, asserting the automatically selected method. Its
held-input boot-change checks exercise both adding and clearing firmware, with
fresh guest INTBACK reports proving release on both ports. Unfocused-key and
canvas-blur checks now focus the disc picker.

Revalidated locally with the tool versions recorded above:

- Formatting, native build and complete native tests pass: **1,337 passed**,
  no failures or ignored tests, across 111 harnesses including doc tests.
  The focused native SMPC guest suite also passes all four tests.
- Native all-target and Wasm Clippy pass with warnings denied.
- All 106 fixture checks and the optimized production Wasm build pass.
- All 13 CI-runner checks pass, without skips.
- The complete production Chromium suite passes all 74 groups: shared 33/33
  in 227.281 seconds, scenarios 41/41 in 182.791 seconds, aggregate two-worker
  wall time 227.543 seconds. The expanded Play UI group passes in 42.859 seconds.
  Complete coverage and exact tested-site identity verification pass.
- Changed JavaScript syntax checks and Git whitespace checks pass.

These are local validation results. This node does not push or merge PR #137;
successful hosted CI for the newly published head remains required before approval.

## Remaining pre-merge gate

Exact-head hosted PR CI must pass after the correction is published to PR #137
and before merge. This GitWeave correction node does not push or merge, and
claims no new remote CI result. Physical-controller testing
is optional and was not performed. No personal firmware, stock BIOS/menu
compatibility claim, live Pages inspection or human approval gate is required
for this input change. Required post-merge verification: none.
