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

## Remaining limits

The multiplier is clocked within the existing architectural instruction-boundary
interpreter and scheduled bus model. It does not implement general speculative
pipeline completion or all future IF latencies. Full SH-2 pipeline/exception
speculation, complete Saturn hardware and board timing, all other unmet #17
requirements and commercial-game compatibility remain separate required work.
Green checks and this checkpoint do not establish hardware completion. No
personal firmware, commercial media, physical console, human approval or live
Pages inspection is required. No required post-merge verification is added.
