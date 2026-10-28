# Issue #71 validation

Related to #71 and #17. Validation date: 2026-10-28. No GitHub write, push,
PR, merge, live Pages inspection, personal firmware or commercial media was
used. #17 remains open; this is one interrupt-input increment.

## Contract and change

[Primary references, register-text reconciliation and independent oracles](m68k-interrupt-input.md)
cover MC68000UM §§3.5/6.3.2 and Sega ST-077-R2 printed pp.95–100 against the
later §4.2.12 text. No technical correction to the task is required. Existing
SCIEB/MCIEB zero-disable and SCILV register addressing agree with the later
Sega text and register diagrams. Electrical synchronization and equal-edge
ordering remain explicit emulator conventions.

The scheduler propagates output changes while work is in flight. A frozen
boundary consumes only its captured request; merging replay preserves live
level/edge input without repeating completed bus operations or adding an IRQ
queue. SNDOFF/SNDON and cold reset cancel the abandoned lifetime.

## Reproduction and inherited-code rejection

The original MIT fixture's expected ordering was authored before the production
change in [the builder](../tests/fixtures/build_m68k_irq.py). Its generated
[SH-2 source](../tests/fixtures/m68k-irq.s), [bytes](../tests/fixtures/m68k-irq.hex),
[license](../tests/fixtures/LICENSE) and expanded-ROM SHA-256 are reproducible:

`acd9a4d91d683f7b031df7165e1bbd7359e119d98436e7c758ca35e90c438754`

The initial fixture setup was corrected to wait for the 68k's initialization
marker before raising the first request. With that valid setup, before any
production change, `cargo test --locked --test m68k_irq` reached the first
handler, completed DIVU to `00010003`, and failed on exception count 1 versus 2.
This was a runtime interrupt loss, not a compile or setup failure.

The final fixture/tests were then copied into an isolated temporary directory.
Only production `src/m68k.rs`, `src/sound.rs` and
`src/sound/transactions.rs` were restored from inherited commit `3144c937`
(the same contents at the upstream checkpoint). The final native oracle compiled
and both tests failed: exception count 1 versus 2, and an empty second handler
record versus SR=2700/PC=322/D0=00010003/count=2. The first record was correct.
The following store at 322 published `00010003`, independently proving completed
computation and continued execution after the lost interrupt.

The same inherited production source was built for Wasm with wasm-bindgen 0.2.129.
`npm run test:browser -- --group=m68kReset` reached the new guest assertion
through the real `/saturn-rust/` page and failed for the same empty second
record and unexpected store `00010003`. The assigned worktree and its fixed Wasm
assets were unchanged by these isolated negative controls.

Fixed native observations of completed SCIPD writes, in reference clocks:

| Output | Reference clock | 68k PC | Retired instructions | D0 | Exceptions |
| --- | --- | --- | --- | --- | --- |
| First assertion7 | 60389 | 21E | 6 | 7 | 0 |
| Clear0 | 60814 | 320 | 14 | 7 | 1 |
| Reassert7 | 60896 | 320 | 14 | 7 | 1 |

The 82-reference-clock low interval and both changes occur while the same DIVU
is in flight at I=7. Both requests stay asserted through acknowledgement.
The second handler's saved PC=322/SR=2700 and D0=00010003 prove retirement before
entry. Changing 7 to 9 produces `00010004`. Omitting the clear or reassertion gives
one entry and a completed following store, with no host-authored handler results.

## Checks

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 1062 tests across 95 harnesses, plus doctests; zero failures/ignored tests |
| `cargo test --locked --test m68k_irq` | 2 passed; all four guest variants, chunks 1/3/17/10000, inspection on/off, two cold boots |
| `cargo test --locked --lib sound::transactions::irq_tests` | 8 passed, including actual RAM stalls, separate IACK/stack cycles and exactly-once MIDI reads; strengthened chunk/inspection and serial-before-CPU equal-deadline probes passed |
| `cargo test --locked --lib sndoff_completion_cancels` | Passed; actual delayed SMPC SNDOFF cancels rearmed input before DIVU retirement, SNDON cannot revive it |
| Native `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| Wasm `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | All 95 registered commands passed |
| `sh scripts/build-web.sh` | Passed with wasm-bindgen 0.2.129 and the installed Rustup Wasm toolchain |
| `npm run test:browser -- --group=m68kReset` | Passed, including new IRQ guest and existing reset/trace/retirement guests |
| `npm run test:browser` | Passed all 66 groups: shared 30 and independent 36; aggregate 203506 ms, Chromium 145.0.7632.6 |
| `git diff --check`, new JavaScript syntax, local documentation links | Passed: staged/unstaged whitespace, JavaScript syntax and new local links |

Homebrew Rust 1.94.0 handled native checks. The first web-build attempt used
Homebrew Rust, which lacked Wasm `core`; rerunning with the already-installed
Rustup stable 1.99.0 toolchain and wasm32 target passed. No toolchain installation, usage reset, allowance purchase or model/provider
switch was used.
`npm ci --ignore-scripts` installed the locked browser dependencies (zero audit
vulnerabilities). Production web assets contain no diagnostic ROM.

The complete suite preserved existing SCSP reset, CPU RESET-output, trace and
retirement, held maskable delivery, Star Collector, Prism Courier, Hardware Lab,
WebAudio, input, backup and local-file/privacy regressions. After final review added serial sampling before CPU issue at a shared deadline,
the required native/Wasm checks and full browser suite were repeated. Guest bytes
and browser assertions stayed identical.

## Remaining scope

Full CPU/peripheral conformance, data-dependent division timing, general prefetch
microsequences, physical IPL synchronization, MCIEB readback versus the write-only diagram,
bus/cache/reset/arbitration,
SCU/DSP, video/audio, CD/media/HLE, SMPC and comprehensive integration criteria
remain mandatory under #17. Existing broad conformance rows remain Pending.
Genuine BIOS execution and broad commercial-game compatibility are unverified.
No post-merge verification is required for #71.
