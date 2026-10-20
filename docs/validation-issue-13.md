# Issue #13 checkpoint validation

**Result: partial implementation; Issue #13 must remain open.** This checkpoint
does not meet the comprehensive acceptance criteria and must not become a final
resolution or trigger an issue-close node. Related to #13. No push, PR, merge,
comment, issue-state change or deployment was performed.

## Inputs and scope

Read the live [Issue #13](https://github.com/takahirox/saturn-rust/issues/13),
AGENTS.md, development flow, review guidelines and Issue template. Upstream main
was inspected with `git ls-remote origin refs/heads/main` and was
`80cb1509c96aea110f559ed41b200516c6d007c1`. The assigned worktree began at
`640290a5dba4ce1d39eefeb0fa7bd0e67b8e9422`; the readiness input provided no technical
diagnosis. All deliverable files remain in the assigned worktree.

Implemented independent 32 KiB battery-backed byte storage, atomic packed
import/export, retention across resets/failed boot selections, and independent
SMPC direct digital port 2. Browser controls expose explicit local backup
replacement/download, release both pads on focus/pause/navigation and reject
stale asynchronous imports. No storage, upload, third-party guest/media, Sega
firmware, SDK or emulator core was added.

Specifications consulted outside the worktree: SCU User's Manual ST-097-R5,
§1.2 figs.1.3/1.5 (pp.4/6, backup window); SMPC User's Manual ST-169-R1, §1.2
(pp.7–9, independent port registers), §2.3 SYSRES (p.29, battery RAM retention),
§3.4 table 3.36 (p.97, pad protocol). See the [architecture](architecture.md) and
[conformance matrix](hardware-conformance.md) for primary-source links and
explicitly pending bus-lane/timing behavior. No manuals are redistributed.

## Executed checks (2026-10-20)

The stable rustup toolchain was selected consistently for compiler and Cargo,
using the PATH command in [setup](setup.md). Existing matching wasm-bindgen
0.2.129 and wasm32-unknown-unknown target were available. Browser dependencies
were installed with `npm ci --ignore-scripts` and `npx playwright install chromium`.

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 59 native tests; no ignored tests |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `sh scripts/build-web.sh` | Passed; reproducible original ISO, release Wasm and bindings |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `python3 homebrew/star-collector/build.py --check` | Passed; 131072-byte ISO unchanged |
| `npm run test:browser` | Passed with actual Chromium 145.0.7632.6 and Wasm at `/saturn-rust/` |
| `git diff --check` and staged whitespace/link/table checks | Passed |

The first native guest-fixture run exposed an incorrect store-register encoding
in the original new test ROM. Correcting it to store R0 made the expected
guest-written diagnostic match; subsequent full checks passed. No emulator
special case was added to accommodate the fixture.

## New integration evidence

The MIT [shared SH-2 guest](../tests/fixtures/backup-pad.hex), whose instruction
words and comments are the reproducible source, runs through actual CPU decoding
and bus/MMIO access in [native tests](../tests/backup.rs) and
[browser smoke](../scripts/browser-smoke.mjs). It increments the first backup
byte with MOV.B, configures DDR2/IOSEL2/EXLE2/PDR2, waits 32 guest iterations and
stores active-low direction reports in work RAM. After 83 retired instructions
it is in a polling branch loop; the host only supplies electrical button levels.
The same program is put into the existing original ISO container in memory and
executed through its real HLE first-read/AIP path (three extra instructions).
This is a diagnostic disc-boot check, not either required new demo game.

Native assertions cover every backed byte and both window endpoints, independent
work RAM, P1 aliases, rejected unsupported lanes/widths, exact-size atomic import,
all boot modes including rejected images, separate-machine isolation, guest
increments over resets, exported-byte restoration and both pads' independent
nibble selection/direction/IOSEL/EXLE controls.

Chromium assertions cover the same guest ROM and locally loaded diagnostic ISO,
keyboard port-2 directions and release on view change/window blur/settings/pause,
offline import, exact downloaded filename/content, invalid sizes, local-read
failure, older-read supersession, cancellation on mode change, unchanged CPU on
import and a fresh tab with no retained backup. Network assertions still allow
only same-origin static GETs, with no file-bearing requests. Existing Star
Collector scoring, completion, restart, pixels/input, responsive layouts,
Play/Debug behavior, synthetic exception ROM and error/race checks also pass.

Backup is deliberately raw/unformatted. Even byte lanes, word/long accesses,
additional mirrors, BIOS filesystem/services and complete reset/timing behavior
remain unsupported. No automatic browser persistence is claimed; users must
export before closing/reloading and explicitly import later. The two direct pads
do not implement INTBACK, RTC, analog input or general SMPC command sequencing.

## Mandatory completion gates still unmet

Every overarching acceptance item remains pending; passing subset tests cannot
waive these requirements. The matrix retains an explicit Pending status per
instruction/device row. It still groups some unimplemented register families;
the exhaustive register/output-mode inventory itself needs further expansion.

| Issue #13 gate | Remaining work |
| --- | --- |
| CPU semantics / on-chip resources / multiprocessor | Architectural address errors, SH7604 resources/cache, slave SH-2, entire MC68EC000 and integrated guest execution |
| Hardware time / memory / reset / backup | Event scheduler and contention, complete base map/mirrors/open bus, reset domains and remaining backup contracts |
| SCU / DMA / DSP | Interrupt controller, transfer modes/channels and executing DSP |
| VDP1 / VDP2 composition | VDP1, remaining VDP2 layers/effects/rotation/window/color math and independent pixel fixtures |
| SCSP / sound CPU / DSP / CD-DA | Entire subsystem, deterministic guest-driven stereo and audible browser output |
| Runtime CD / media / HLE services | CD command/status/IRQ/sector/TOC/filters, BIN/CUE/raw/mixed tracks and general BIOS services |
| Genuine local BIOS milestone | No BIOS supplied or executed; no initialization/menu/boot evidence. Request minimum lawful local verification once the missing hardware makes it meaningful; never commit/transmit firmware. |
| SMPC / inputs / system behavior | Commands, INTBACK, RTC, analog packets, timings/interrupts and power/reset controls |
| Two additional licensed demos | Neither the playable multi-device game nor the hardware showcase is implemented. Star Collector remains the sole bundled game. |
| Exhaustive conformance / provenance | Many mandatory subsystems absent; full register/mode inventory and guest native/Wasm evidence outstanding. Third-party images remain uncleared and link-only. |
| Repository checks for comprehensive implementation | All current-tree checks pass, but do not exercise missing hardware/demos; their future implementations require corresponding checks. |
| Automation closure prevention | External GitWeave merge-only issue-close behavior is documented and still uncorrected; repository instructions alone do not enforce the external gate. |

The [candidate audit](homebrew-provenance.md) pins reviewed upstream revisions,
licenses, build recipes, asset/dependency uncertainties and reuse decisions.
No third-party full redistribution chain was established. The two original-demo
fallback is mandatory and remains unfulfilled.

## Automation and handoff

The [closure comment on #11](https://github.com/takahirox/saturn-rust/issues/11#issuecomment-6292152071)
was fetched directly. It reports an explicit GitWeave instruction to close on
an upstream merge, while acknowledging unmet hardware acceptance. This repository
has only the Pages workflow and no source/configuration for that close node.
It cannot be corrected or verified within this artifact boundary. AGENTS.md and
the matrix now identify #13 as authoritative and prohibit treating partial
checkpoints/merges as completion. The downstream workflow must bypass any
merge-only closure until full acceptance review; **do not close #13 or reopen #11**.

Continue implementation against the pending matrix, preserving the original
independent-core/manual-source and media-provenance constraints. Do not mistake
this scoped checkpoint for comprehensive completion. Never redeem usage-reset
tickets, buy allowance or change models/providers to bypass limits. No usage
limit was encountered or reset here.

## Required post-merge verification — pending

After eventual final comprehensive merge and Pages deployment, record the exact
deployed SHA/workflow run and verify the public Play/Debug app, both new lawful
discs, guest graphics/audio/input/CD and local-file/error behavior. No deployment
or public-site result is claimed by these local tests.
