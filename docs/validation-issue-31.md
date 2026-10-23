# Issue #31 validation: clocked SH-2 multiplier

Related to #31 and #17. No Issue closure, push, PR, merge, GitHub comment or
post-merge verification is part of this worktree checkpoint. #17 remains open.

## Specification and implementation

The [primary-derived contract](sh2-multiplier.md) gives the producer/consumer
matrix, progressively separated instructions, baseline EX intervals, access
stages, exact figure/page references, excluded SH-1 diagrams and the diagrams
that explicitly include IF/MA contention. It was written before production
changes and before using implementation output as evidence. No acceptance
criterion was removed or weakened.

`src/cpu/multiplier.rs` retains a deadline in elapsed SH-2 clocks. Each `Cpu` has
its own multiplier; reset constructors start with no older work. The short and
long tails last two and four clocks after the final MA. Consumers distinguish a
single MA from a second MA; MAC can fetch its first operand before waiting.
MACH/MACL transfers in either direction and CLRMAC use the same port. Arithmetic,
saturation and flags remain in the existing instruction implementation.

`Bus::main_clocks` advances with the selected PLL, including external and
on-chip waits. `ReplayMemory::sh2_clock` journals each observation, so retries
reuse the original sampled deadline and completed effects. Local waits reuse
the existing scheduled pipeline operation and do not acquire the external bus.
`Machine` CPU deadlines now remain in main clocks. The scheduler can issue a CPU
between two main ticks delivered within one reference tick, avoiding accumulated
per-instruction fast-PLL rounding. Public elapsed time and fixed devices retain
reference-clock units. Batched idle advancement remains available.

The independently derived no-contention MAC baseline is two clocks (fig7.23/34).
The production adapter adds the IF/MA split. Other corrected inherited vectors
are the four unaligned STS operations in the separated kernel (28 total states,
final FRT 4), and one unaligned LDS in the WB→MMIO scheduler test (one extra
clock before its unchanged WB interlock). Primary §§7.4.1–3 and figs7.88/90
establish these corrections. Original regression coverage remains present.

Implementation links: [CPU multiplier](../src/cpu/multiplier.rs),
[clocked bus and journal](../src/bus/transactions.rs),
[CPU scheduler](../src/lib.rs), [instruction vectors](../src/cpu/multiplier_tests.rs),
[scheduler/lifecycle vectors](../src/multiplier_scheduler_tests.rs),
[native guest](../tests/sh2_multiplier.rs), and
[browser guest checks](../scripts/sh2-multiplier.mjs).

## Native sequences and lifecycle

`src/cpu/multiplier_tests.rs` uses literal independent timing tables for every
scoped multiply/MAC and transfer class, adjacent and separated by 1/2/3 NOPs.
Separate signed/unsigned vectors assert high/low products, MACH preservation,
flags and register preservation, both memory directions and address updates.
Consecutive MAC.W/MAC.L sequences assert timing, saturation, accumulator values
and exactly two increments per operand. IF/MA vectors assert aligned/unaligned
splits and consecutive MAC overlap. Existing all-register STS-to-GPR WB checks
remain in `src/cpu/tests.rs`.

`src/multiplier_scheduler_tests.rs` checks suspended STS.L and LDS.L, unchanged
architectural state before their boundary, memory sampling after the wait,
exactly one address update/write, independent peer progress and slave-local DMA
completion. External reads, on-chip reads and a cold waited fetch advance the
older multiply. Direct reset-boundary tests cover both processors and both reset
kinds. Actual watchdog assertion during the wait, both watchdog selections,
actual SSHOFF/SSHON and a cold boot cancel stale journals/deadlines. The new
boot computes a different result through guest LDS/STS. IRQ and NMI tests check
saved PC/SR, unchanged MAC registers, handler transfers and RTE/delay-slot return.
Eight exception-entry clocks exhaust the maximum tail; no undocumented flush
rule is inferred.

## Original executable guest and controls

The original [MIT fixture](../tests/fixtures/LICENSE) has source/build artifacts
`build_sh2_multiplier.py`, `sh2-multiplier.s`, `.hex` and `.tsv`. It initializes
operands with guest instructions, starts the slave through SMPC, warms each
kernel, resets each CPU's own FRT divider, and stores measured FRC and computed
results in ordinary work RAM. Fifteen kernels cover signed/unsigned word and
long multiplication, MUL.L, adjacent/separated STS, omitted producers, CLRMAC,
LDS MACH/MACL, consecutive word/long MAC and separated MAC.L.

Both CPUs run identical bytes; the fast variant changes only a guest conditional
operand enabling an actual SMPC clock-selection command before starting the
slave. Native and production Wasm/browser use the same bytes and authored TSV.
Every variant repeats cold boots. No host write supplies a computed record.
Operand mutation changes 3×7 to 7×7 (21→49), spacing mutation inserts two NOPs
per word-multiply/STS pair (FRT 8→10), and omitted producer changes FRT 8→6 and
product 21→0. Browser execution mounts only production `web/` under
`/saturn-rust/` and uses rebuilt release Wasm.

An isolated disabled-interlock mutation replaced the two contention calculations
with zero, retaining arithmetic and fetch splits. Both commands failed with
exit 101 at their intended assertions, and the original source was restored:

- `cargo test --locked --lib multiplier_all_producer_consumer_families -- --nocapture`:
  adjacent `212E→212E`, second instruction actual 1 clock, expected 2.
- `cargo test --locked --test sh2_multiplier both_guest_cpus_measure_multiplier -- --nocapture`:
  first word-multiply/STS FRT actual 6, expected 8, while product remained 21;
  double-multiply/STS actual 5, expected 8, while product remained 21.

The restored twelve focused native tests passed after this negative control.

## Reproduction and validation

Validation ran on 2026-10-23 with rustup stable Rust 1.99.0, wasm-bindgen 0.2.129,
Node 24.12.0 and Playwright Chromium 145.0.7632.6. The default shell also provides
Homebrew Rust 1.94; final checks explicitly used the installed rustup stable
binaries.

| Required check | Observed result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 832 tests, 0 failed, 0 ignored; 79 result groups including doc tests |
| Native Clippy, all targets, warnings denied | Passed |
| Wasm Clippy, warnings denied | Passed |
| All 78 registered fixture/media reproduction checks | Passed; updated multiplier/timing artifacts rechecked after source comments |
| `sh scripts/build-web.sh` | Passed; final production rebuild passed |
| `npm ci --ignore-scripts` | Passed, 0 vulnerabilities reported |
| Final-artifact multiplier Playwright recheck | Passed; both PLLs, both CPUs, all mutations and repeated cold boots |
| Complete extended `npm run test:browser` | Passed under `/saturn-rust/`; `Browser smoke passed (145.0.7632.6)`, no required skips |
| Disabled-interlock negative controls | Both rejected at intended timing assertions |
| Documentation links and Git whitespace checks | Passed |

| Artifact | SHA-256 |
| --- | --- |
| Expanded 524288-byte BIOS | `508166c91e3d0d2356e5f1d3071af3f27bbb1b9df7dc95b2cfcd622f597f431a` |
| `sh2-multiplier.hex` | `f1954fa01ca472f9bddfe2a4bd90379b1a92891ce16a1817c4622d7806b5a115` |
| `sh2-multiplier.s` | `0a454a7c1be3b81961841357a61807c4d5ba7a1c459980c613a4b0a2521e737c` |
| `sh2-multiplier.tsv` | `12b963b67b221bf7af1029cdd1243bdaad9c338626a1a44662aec6c26cbdf7da` |

The complete browser run used production Wasm SHA-256
`84e0449eeef426f3fd7e6f31573d53ab2e9d556a1fd42827a803c4e8315561c9`.
The final rebuild after formatting has SHA-256 `ed3d1a6d43d24f4e303083e2f1557d6b4ac5f4884768207b9d35cea8f491862d`.
The final artifact was checked again with the same `sh2Multiplier` Playwright
module, static-only project prefix, both CPUs, both PLL selections, all three
mutations and repeated cold boots. It passed in Chromium 145.0.7632.6.
The first isolated harness attempt left the UI in HLE mode and timed out
waiting for a BIOS-reset status. Selecting BIOS mode and Debug, as the full
harness does, corrected that extra harness setup; the rerun passed.
Ignored build outputs remain reproducible through the existing script.

The complete suite retains Star Collector, Prism Courier and Hardware Lab,
address-error/reset, bus/DMA, sound, controller, backup, local-file and privacy
checks. All required checks and the final artifact recheck passed, with no
required skips or outstanding failures. No post-merge check is required.

## Review correction: zero-stall memory boundaries

Review found that `Multiplier::wait` skipped the EX/MA lead-in when the extra
multiplier stall was zero. This let a separated STS.L complete one clock before
the older multiplier's final mm boundary. The fix schedules memory access from
its MA boundary independently of extra instruction stalls, keeping the existing
instruction cost and overlap rules. Primary references are P §7.7.2,
figs7.63/7.71 (STS.L) and 7.73 (LDS.L).

Two new real-scheduler regressions in
[`src/multiplier_scheduler_tests.rs`](../src/multiplier_scheduler_tests.rs)
cover MACH and MACL stores and loads through on-chip DMAC SAR0. With DMULS.L
starting at clock 34 and its tail ending at 42, stores after two and three NOPs
must both complete at 42; four NOPs put completion at 43. The corresponding
instruction costs remain 3/1/2 clocks, including IF contention. Three-NOP loads
must sample at 42, observing a memory change made at 41. Tests also assert single
address updates across replay, retained arithmetic results and peer progress.

Before the fix, the store regression failed at its three-NOP early-store
assertion (SAR0 was already zero at 41), and the load regression observed the
old `0x12345678` rather than the boundary-time `0x87654321`. Both commands exited
101 at the intended assertions. After the fix all 14 focused multiplier tests
passed.

Revision checks on 2026-10-23 passed with rustup stable: `cargo fmt --check`,
`cargo build --locked`, `cargo test --locked` (834 tests, none failed or ignored),
native all-target and Wasm Clippy with warnings denied, all 78 registered
fixture/media reproduction checks, and `sh scripts/build-web.sh`.
The complete extended `npm run test:browser` passed under `/saturn-rust/` in
Chromium 145.0.7632.6 with no required skips, including both CPUs, both PLLs,
multiplier mutations and repeated cold boots. The tested production Wasm has
SHA-256 `833b9caa5451f6a5b8e5094898d887b0c75f9a1e72e7340ec897a9501a44ba7d`.

## Review correction: synchronous cache transfer boundaries

The previous correction subtracted one clock from every memory-transfer wait,
assuming the subsequent access would supply it. Cache-array accesses and cache
hits instead complete immediately in `CpuBus`, allowing LDS.L/STS.L effects one
clock before MA completion. P §7.7.2 figs7.63/7.71/7.73 require the same MA
completion boundary regardless of which adapter path supplies the memory data.

The CPU now supplies the transfer address and direction. The replay adapter
journals whether that access has zero latency or a scheduled transfer's minimum
one-clock latency, using the same tag/way lookup as cache reads. Only a scheduled
transfer subtracts that clock from the preceding wait. Existing MAC operand
overlap, arithmetic, execution-state costs and external/on-chip waits remain
covered by their original vectors and guest expectations.

Two additional scheduler regressions in
[`src/multiplier_scheduler_tests.rs`](../src/multiplier_scheduler_tests.rs)
exercise 30 cases: both MACH/MACL directions, 0–4 separating NOPs, cache-array
stores/loads and resident cache reads. Their independent boundaries remain 42
for 0–3 NOPs and 43 for four NOPs. They verify no early memory/register effect,
unchanged execution-state costs, loads seeing a value changed immediately before
completion, one address update, no repeated store/sample and peer progress.
The peer's second code line is warmed so the tests can inspect each exact clock
without skipping over the boundary during an unrelated fetch wait.

An isolated mutation forcing the observed latency back to at least one clock
restored the reviewed assumption. Both commands exited 101 because the affected
instruction completed at clock 41, before the independently required 42:

- `cargo test --locked --lib cache_array_multiplier_stores -- --nocapture`
- `cargo test --locked --lib cache_backed_multiplier_loads -- --nocapture`

The mutation was restored before final checks. All 16 focused multiplier unit/
scheduler tests and both original dual-CPU FRT guest tests then passed, including
both PLL selections, arithmetic/spacing/omitted-operation controls and cold boots.

Final revision checks on 2026-10-24 explicitly selected rustup stable Rust 1.99.0
in PATH: formatting, locked native build, all 836 native tests (none failed or
ignored), native all-target and Wasm Clippy with warnings denied, all 78
registered fixture/media reproduction checks and the production web build passed.
The first Wasm Clippy attempt selected the host's Homebrew compiler and failed
to find the Wasm target's `core`; selecting the already installed stable compiler
and target resolved that tooling mismatch. The complete native checks were also
rerun with that explicit compiler selection.

The complete extended `npm run test:browser` passed under `/saturn-rust/` in
Chromium 145.0.7632.6 with no required skips, including the original multiplier
guest, both CPUs/PLLs, mutations and repeated cold boots, all three demonstration
discs, audio, backup and privacy checks. The tested production Wasm has SHA-256
`bc3882bb1fd6687bd359f5ca06d4f010188480af32d4f017fb7a6848dbd80faf`.

## Review correction: preserve synchronous transfer overlap

The unconditional MA lead-in preserved memory-effect boundaries but prevented
the successor's EX from overlapping an aligned LDS.L/STS.L. Table7.2 and
P §7.7.6 figs7.89/7.91 give these instructions one baseline EX interval;
retirement at the later MA boundary must not add another issue interval.

The multiplier now retains the permitted successor EX time for synchronous
transfers whose MA ends later. The scheduler recovers that interval while
keeping its actual memory-completion clamp. A successor multiplier operation
uses the retained EX time. IF/MA splits already occupy the interval, a waited
fetch exhausts it, and exception entry clears it. The original transfer wait,
memory-effect boundaries and replay journal remain in place.

Two new scheduler regressions check actual clocks: all four MACH/MACL memory
transfer forms preserve their MA effects while the following NOP and MOV issue
at the independently required times. Both instruction alignments are checked,
including an explicit assertion that the unaligned IF/MA split still delays EX.
A separate cold-fetch control verifies that a waited successor fetch consumes
the overlap and has the same deadline as an independent NOP control.

Four new original guest kernels extend the reproducible MIT fixture to nineteen
kernels. Each executes sixteen aligned transfer/NOP pairs, taking 32 clocks
and measuring FRT floor((6+32)/8)=4. Both CPUs run the same kernels at both PLL
selections and across repeated cold boots in native and production-browser
checks. The existing arithmetic, spacing and omitted-operation mutations remain.

Before the fix, `cargo test --locked --lib synchronous_mac_memory_transfers
-- --nocapture` exited 101 at the successor-issue assertion. The expanded native
guest command, `cargo test --locked --test sh2_multiplier
both_guest_cpus_measure_multiplier -- --nocapture`, also exited 101: all four
new FRT records were 6 rather than the independently authored 4. These failures
demonstrate why `last_clocks()` assertions alone did not detect the regression.

Revision checks on 2026-10-24 explicitly used rustup stable: `cargo fmt --check`,
`cargo build --locked`, `cargo test --locked` (838 passed, zero failed or ignored,
79 result groups), native all-target and Wasm Clippy with warnings denied, and
all 78 registered fixture/media reproduction checks passed.

`sh scripts/build-web.sh` and the complete extended `npm run test:browser`
passed against the final production artifact under `/saturn-rust/` in Chromium
145.0.7632.6, with no required skips. This includes both CPUs/PLLs, all nineteen
multiplier kernels, arithmetic/spacing/omitted-operation mutations and cold
boots, the three demonstration discs, audio, backup and privacy checks. The
tested production Wasm has SHA-256
`2868ac374fdd9017e8c92386240456c637c58548eef9db111cd4a12634e432ef`.

The expanded 524288-byte BIOS has SHA-256
`c278a0ee08398cf591df6fa887c165145435d45d0fd4b675e18164389d78996d`.
Its reproducible `.hex`, `.s` and `.tsv` hashes are respectively
`4265b9187b07d50d3ce02a9cae0885affedb7138725cd26fd7197963d3adf5f5`,
`2ff6b59e6382e58477b2cc0d319050fc0ccec7c8653f9685ba6dcafbcd08782e` and
`6c09e4a3246554dc1256a311af77f7b00e0c095d963087de9512fb9cd7e5aa69`.

## Remaining limits

The multiplier is clocked within the existing architectural instruction-boundary
interpreter and scheduled bus model. It does not implement general speculative
pipeline completion or all future IF latencies. Full SH-2 pipeline/exception
speculation, complete Saturn hardware and board timing, all other unmet #17
requirements and commercial-game compatibility remain separate required work.
Green checks and this checkpoint do not establish hardware completion. No
personal firmware, commercial media, physical console, human approval or live
Pages inspection is required. No required post-merge verification is added.
