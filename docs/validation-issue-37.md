# Validation: clocked SH-2 B-bus memory reads

Related to #37 and #17. Worktree baseline `a8fb5be714f303127cb9984be636bd7b87e0f977`
contains the inherited implementation inspected at `bfe51271b33f75821258a250d665fcdaf6f90f29`.
Validation date: 2026-10-24. No remote writes or post-merge verification.

## Contract and implementation

The [transaction table](sh2-b-bus-reads.md) cites ST-210 §3 item32, ST-097-R5
§§2.1/3.2, SCSP ST-077-R2 §§3.1–3.2 and VDP2 §3.3. It distinguishes specified
granularity/ownership from deterministic phase timing and records SCSP's software
byte restriction. The [adapter](../src/bus.rs) aligns the bridge unit, requests
two halfwords and extracts operand lanes. The [scheduler](../src/bus/transactions.rs)
retains each completed word and emits a CPU-facing longword observation only
after its second completion. It never supplies this result from `Bus::read::<4>`.

## Independent native evidence

[bridge_tests.rs](../src/bus/bridge_tests.rs) contains literal byte results and
explicit event addresses/clocks; production timing helpers do not provide its
oracle. It covers sound RAM, VDP1 VRAM/draw framebuffer, VDP2 VRAM/CRAM, framebuffer,
VRAM and CRAM aliases, P0/P1, all byte lanes/word lanes/longwords and both CPUs.
Cache refills issue the existing four-longword wrap order with two halfwords per
longword; cache hits preserve private data after backing-memory changes. Alignment
errors acquire nothing, and unpopulated sound addresses remain pulled-up data.

Mutation immediately before clock 8, after clock 8 and after clock 16 proves retained
first-word and separately sampled second-word data; longword output is literally
`11227788`, never a single backing snapshot. Narrow operands remain pending after
clock 8 and occupy the second phase before a competing CPU or SCU channel. Both
competitors subsequently finish, and SCU destination data/IRQ establish progress.
External UBC address vectors exclude both early first-word and extra second-word
observations, while the local CPU UBC still compares its original byte operand.

[Sound vectors](../src/bus/sound_tests.rs) enumerate DSP MRD release 15, PCM second
read release 20, internal DMA release 24, and refresh releases 600/610, preserving
the high word while the low word waits. The refresh boundaries are derived from
26,874,100 reference clocks/s and 128×44,100 memory cycles/s, with refresh slots
124/126. [VDP1 vectors](../src/bus/vdp1_tests.rs) complete both physical phases:
draw framebuffer ownership blocks dot publication, while independent VRAM does
not stop untextured drawing. VDP2 tests cover display grants, denied grants until
blanking, and grants revoked between words. Results agree with advancement chunks
1/3/17/10000 and intervening debugger reads.

Power/manual watchdog resets are asserted while the request is queued behind a
peer, while the first word is active, after its completion, and during the second
word. Restarted reads return fresh `55667788`; stale journal results cannot enter
the restarted CPU. The historical slow-video manual-reset test retains its peer
peripheral check and original first boundary 1282, then drains the additional
eight-clock word. This change follows ST-210 item32 plus the existing SH7604
manual-reset operand-drain rule; the newly granted phase uses reset BSC timing.

## Executed negative controls

All three controls were actually compiled/run against temporary source mutations;
the original source was restored in a `finally` block. None of these mutations
is part of the implementation.

| Control | Exact selected test | Actual failure |
| --- | --- | --- |
| Disable the new bridge branch, retaining inherited atomic BSC-width reads | `bridge_longword_retains_first_word_until_second_completion` | Exit 101, at clock 8: actual `Ok(287454020)` (`11223344`), expected `Err(Pending)` |
| Disable the bridge and cap inherited physical width at 2, preserving operand-sized narrow reads | `bridge_narrow_operand_occupies_both_phases_before_competing_cpu_and_scu` | Exit 101, at clock 8: actual `Ok(0)`, expected `Err(Pending)` |
| Run the original guest with the inherited atomic/operand-sized path | `guest_bridge_reads_both_cpus_self_initialize_mutate_and_measure_frt` | Exit 101: replaced-read 20 versus inherited-read 39 FRT ticks violates the independently authored difference exceeding 20 ticks |

The cap in the second control prevents atomic longwords from masking the distinct
narrow-read occupancy defect. The restored production path passes both boundary tests and the guest.

## Original guest and browser evidence

[build_b_bus_reads.py](../tests/fixtures/build_b_bus_reads.py) reproduces the
[MIT-licensed](../tests/fixtures/LICENSE) assembly, ROM bytes and authored golden
records. Expanded ROM SHA-256:
`d7838e209c8b493aad57b4e572f1f3aba8263dd786bc5c9704923d4363ae2473`.
The guest initializes source halfwords itself, reads each sound/video endpoint,
and records computed lanes/longwords and FRT differences in ordinary work RAM.
It starts the slave through an actual guest SMPC command; both CPUs execute the
same diagnostic. The host only loads the ROM, advances execution and reads records.

[Native](../tests/b_bus_reads.rs) and [Chromium Wasm](../scripts/b-bus-reads.mjs)
use identical ROM bytes and independent expectations for `A1B2:C3D4` and the
changed source `1122:3344`. Each variant is cold-booted twice. Eighty timed
`MOV.W @R4,R0` instructions are replaced with `MOV R12,R0` for the control,
leaving the arithmetic/read-result diagnostic intact. Sixteen bridge reads
alone require 32 FRT ticks; the checked interval is 32–127 including instruction
and peripheral overhead. The replacement interval is 10–63 and must be over 20
ticks shorter at every endpoint/CPU. These are independently authored bounds,
not emulator-derived golden timings; zero timing fields in the TSV are markers.
The browser uses rebuilt production Wasm mounted strictly at `/saturn-rust/`.

## Required checks

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 869 tests, including 555 library tests; zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| All registered fixture/media reproduction commands in `.github/workflows/pages.yml` | Passed: all 81, including the new builder and large media |
| `sh scripts/build-web.sh` | Passed; production Wasm rebuilt with wasm-bindgen 0.2.129 |
| Complete `npm run test:browser` | Passed in headless Chromium 145.0.7632.6 under `/saturn-rust/`, without required skips |
| Documentation link targets and `git diff --check` | Passed |

Native build/tests/Clippy used Homebrew Rust 1.94.0; Wasm build/Clippy used the
installed rustup stable Rust 1.99.0 toolchain and its Wasm standard library. The
initial web-build attempts exposed a missing CLI on PATH and a Homebrew/rustup
target mismatch. Both were resolved by selecting the installed toolchain/CLI;
the final build and browser run completed successfully. Node was 24.12.0.

The full suite retains CD FIFO replay, VDP2 clear-on-read, CPU write strobes,
alignment/sign extension, cache/coherence and bounded exception tests. Native and
browser Star Collector, Prism Courier and Hardware Lab checks passed, including
their gameplay/restart, video/audio and computed diagnostic behavior. The browser
also passed the real 552,960,000-byte ISO/CUE selections and mutation controls.
No required check remains skipped or pending. Required post-merge verification:
none. This is scoped read-bridge evidence, not completion of the #17 inventory.

## Remaining scope

This is a partial #17 hardware increment. The scoped read implementation does not
establish complete board pin timing or parallel-bridge arbitration. MMIO, A-bus
conversion, CPU writes and on-chip DMAC conversion retain their existing behavior
and require separate device-specific contracts. Comprehensive CPU/peripheral,
board arbitration/reset, SCU/DSP, video/audio, CD/media/HLE, SMPC and integration
acceptance remain mandatory. No commercial compatibility or physical hardware
equivalence is claimed. No personal firmware, commercial media, human approval,
live Pages inspection or external Issue closure investigation is required.
