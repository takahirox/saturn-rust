# Issue #77 validation: clock-change standby and NMI

Related to #77 and #17. The [stage table and specification reconciliation](clock-standby.md)
define firmware versus hardware behavior, one completion NMI pulse, conservative
WDT wake selection, DMA cancellation/drain, retained Work RAM and the remaining
unverified electrical behavior. This checkpoint does not establish #17 hardware
completion, genuine BIOS operation or broad commercial-game compatibility.

All guest instructions, media and expectations are original MIT fixtures.
No proprietary firmware, copied firmware service, host-written result mailbox,
human testing/approval, live Pages inspection or external Issue-closure check
was used. No GitHub state is changed by this node.

## Implementation evidence

`Machine` delivers clock-completion NMI through the same input/controller and
scheduled exception mechanisms as NMIREQ. It preserves the master's outstanding
transaction and gates instruction and exception issue throughout standby/wake.
`CpuBus::enter_sleep` cancels unfinished on-chip DMA phases when standby initializes
its controls. WDT counter initialization follows table 14.3.

The original HLE routine masks IRQs, installs a temporary vector, disables cache,
selects a stopped WDT, issues CKCHG, sets SBYCR and executes SLEEP. Its RTE recovery
restores caller vector/SBYCR/cache/SR and stack balance, preserves R8–R14 and PR,
re-enables its boot capture service, restores IMS from the retained software
shadow and enables hot reset through RESENAB. It does not clear NMIF or restart
application DMA. Post-switch video and audio setup executes in the caller.

Native boundary tests cover NTSC/PAL, both directions, no master retirement/new
transactions while stopped, fixed-clock progress, saved PC/SR, handler execution
and RTE, all WDT choices, pending master reads/writes, internal/external DMA
phases, retained RTC/RAM/CD/backup and cold boot before/after the wake input.
Existing SCU startup and slave cancellation vectors remain required.

The original clock ROM and HLE disc compare independently authored register
records, guest saved-PC/SR results, DMA inhibition and explicit restart. Native
and production browser Wasm run identical bytes and expectations, both TV
standards and clock directions, repeated cold boots, changed operands and omitted
standby/restart controls. Browser assertions check actual production Canvas and
constant nonzero PCM (4096 in both channels). Native chunks 1/3/17/10000 and
passive inspection on/off produce identical lifecycle records.

## Inherited-main failure

A separate disposable worktree at `7a2f68c23bb39a64b8f3ddd4efe90a1205e7d8dd`
received only the new clock test and its fixtures. Running
`cargo test --locked --test clock clock_records_are_chunk_and_inspection_invariant`
failed the literal lifecycle comparison. The first records contained FRT compare
bytes `12/34`, SCI BRR `17`, DMAOR `0`, transferred data `13579BDF` in the
supposedly inhibited slot and mask `BFFF`; the authored expectations are
`FF/FF`, `FF`, `2`, zero and retained mask `B7FF`. Thus completed clock readback
alone cannot satisfy the new oracle. Golden values were not obtained from an
emulator run.

## Preserved and corrected older evidence

- The low-level PLL-reset test retains its useful running-FRC/cache assertion;
  it does not execute firmware SLEEP. Standby initialization is independently
  established by executing guests and table 14.3 vectors.
- HLE dual-CPU master exceptions increase from 5 to 7 for the two documented
  post-clock NMIs. TIER=81 is an explicit firmware recovery write, preserving
  that guest's boot communication contract. Its raster worker waits for a
  complete active interval before unmasking; arbitrary clock-service phase
  previously made the second handler straddle HBlank end. Both same-HBlank
  entry assertions remain. The two-boot SYSRES budget increases from 16 to
  22 million reference clocks to include four new WDT wake intervals, while
  retaining the completed second-boot and backup/power-clear assertions.
- Raw display/rotation/multiplier diagnostics now install vector 11 and an
  original RTE handler. Rotation expects one NMI; multiplier expects one only
  when its optional CKCHG path executes. Their picture, instruction-timing,
  dual-CPU, mutation and repeated-boot oracles are retained. The slave-raster
  board fixture likewise installs vector 11 without relaxing slave checks.

## Required checks

No publication workflow, inventory, assertion or gate is disabled for this change.
The full browser run passed all 67 groups (37 scenarios and 30 shared), including
Star Collector, Prism Courier and Hardware Lab, at `/saturn-rust/` in 203 seconds.
CI-runner tests passed 12/12 with no skips. The complete native suite passed
1,092 tests with none ignored on Homebrew Rust 1.94; the documented rustup stable
1.99 toolchain also passed the same 1,092 tests and native build/Clippy checks. Node was 26.12.1, Playwright 1.58.2 and
wasm-bindgen 0.2.129. Web compilation used rustup stable with its installed Wasm
target and the matching CLI.

Initial full-suite failures exposed the missing older-fixture NMI handlers,
required capture recovery and phase-dependent HBlank setup described above.
These were corrected and the full suites rerun. The first web command selected
the host Homebrew compiler without its Wasm target; selecting the documented
rustup PATH resolved that environment mismatch. No required check was skipped.

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed on both native toolchains |
| `cargo test --locked` | 1,092 passed, zero ignored on both native toolchains |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed on both native toolchains |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed on rustup stable |
| `sh scripts/check-fixtures.sh` | Complete registry passed, including rebuilt original guests and HLE |
| `sh scripts/build-web.sh` | Passed with the documented toolchain PATH |
| `node --test scripts/ci-runner.test.mjs` | 12 passed, zero skipped |
| `npm run test:browser` | Complete 67-group aggregate passed; both required shards |
| New lifecycle oracle on inherited main | Failed for the intended missing behavior |
| JS syntax, documentation relative links and `git diff --check` | Passed |

Required post-merge verification: none.
