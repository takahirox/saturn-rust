# Automatic browser boot selection — Issue #135

The public setup now has three steps: Select disc, Add BIOS (optional), and
Start playing. There is no mode picker or hidden fixture option. A read-only
boot method reports the automatic decision. Clear BIOS and Clear disc provide
explicit selection removal, including cancellation of outstanding local reads.

Ordinary Play derives its method from BIOS selection state: an accepted 512 KiB
BIOS selects real firmware startup, an absent BIOS selects disc HLE, and a
selected but pending, rejected or cancelled BIOS blocks execution. A replacement
stops the old guest before awaiting local reads. Per-picker generations prevent
stale success or failure from changing newer selections. Reset cancellation stays
blocked until explicit replacement or clearing. Disc validation/mounting is
independent of this choice, with validated media retained in Wasm across BIOS
changes. The Debug fixture is explicit, clears both selections, supports CPU
Reset/Run, and leaves Play Start/Restart disabled.

No emulator boot semantics or keyboard mapping were changed. The firmware path
never installs HLE or automatically falls back after failure. BIOS size checks
establish neither authenticity nor compatibility. Stock menus and commercial
games remain outside the evidence here. Files remain local to the tab.

## Acceptance evidence

| Issue criterion | Evidence |
| --- | --- |
| Public UI | `play-ui.mjs` checks that `#mode` is absent, there are exactly three setup steps, and controls fit 1440, 1024, 768, 390 and 320 px layouts. Desktop/mobile local screenshots were inspected. |
| Automatic derivation | Production Wasm/Chromium checks cover empty, disc-only HLE, BIOS-only and BIOS-plus-disc without mode interactions. Original BIOS-only guest counters and hardware records execute through Start/Restart; `external-startup.mjs` checks firmware CD/FIFO execution and Canvas for both file-selection orders. |
| BIOS fail closed | Wrong metadata size, short returned bytes, rejected reads, pending reads and Reset cancellation block Start/Restart/Step/frame with and without retained media. Firmware device faults also remain genuine errors. Clearing or replacing the selection recovers explicitly. |
| In-place changes and races | BIOS replacement/clear stops playback, retains disc and rejects stale successful/rejected reads. A disc completion cannot erase a BIOS error. Disc clear/replacement retains BIOS bytes; stale disc reads cannot restore cleared or superseded media. Existing overlapping BIOS/disc tests exercise both selection and completion orders. |
| Debug fixture | Explicit fixture loading, original Run 6 registers, Reset, disabled Play Start/Restart, view transitions, and stale BIOS/disc completions are covered. Selecting a normal file exits the fixture path. |
| Preserved behavior | Existing native/Wasm/browser media, both pads/gamepads, backup, audio, fullscreen, pause/restart, focus release, errors, offline and local-only request assertions remain in the required groups. ISO, CHD and CUE retain media when BIOS is added/cleared. Issue #134 keyboard expansion is unchanged. |
| Automated browser coverage | Existing manual-mode drivers now select/clear actual files or explicitly load the Debug fixture. New transition tests control only File read timing/failure; all machine resets/execution use production Wasm and original redistributable fixtures. |
| Documentation and CI | README, setup, media, architecture and current BIOS browser instructions are updated. Local commands below pass. Exact-head hosted PR CI and normal review remain required before merge. |

## Executed validation

Local macOS arm64 validation used rustup stable, wasm-bindgen 0.2.129,
Binaryen 127 and Playwright's pinned Chromium 145.0.7632.6:

- `cargo fmt --check`
- `cargo build --locked`
- `cargo test --locked`
- `cargo clippy --locked --all-targets -- -D warnings`
- `sh scripts/check-fixtures.sh` (complete reproducibility registry)
- `sh scripts/build-web.sh` (production optimized Wasm)
- `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`
- `node --test scripts/ci-runner.test.mjs` — 13 passed, no failures/skips.
- Complete `npm run test:browser` — 74 required groups with the normal two-worker
  coverage and tested-artifact verification: 33 shared groups in 196.229 s,
  41 scenario groups in 185.004 s, aggregate wall time 196.463 s. These local
  timings include concurrent validation and are not hosted-CI performance claims.
- `npm run test:browser -- --serial` — all 74 groups passed in 374.058 s,
  verifying the migrated shared-page media-selection drivers as well.
- JavaScript syntax, changed documentation file links and Git whitespace checks pass.

The initial runner invocation started before the Wasm build finished and timed
out waiting for initialization; after the build, all 13 runner tests passed.
Focused Play checks passed while expanding cancellation and replacement coverage.

Hosted PR CI was not invoked: this worktree task authorizes no push or PR creation.
Local evidence does not replace exact-head hosted checks or required review.
No private BIOS, physical hardware, live Pages inspection or human testing was
required. Required post-merge verification: none.
