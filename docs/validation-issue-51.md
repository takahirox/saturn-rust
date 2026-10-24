# Issue #51 validation

Related to #51 and #17. The implementation and independently authored vectors
address the A-to-B startup interlock. This remains a partial #17 increment;
hardware completion, genuine BIOS use and broad commercial-game compatibility
are not established. No post-merge verification is required.

## Contract and implementation

The [contract](scu-dma-startup.md) links the inspected ST-210 §3 items1/8/12
and ST-097-R5 §§2.1/3.2/3.6 primary PDFs, their printed pages and applicable final
corrections. It distinguishes descriptor acquisition, pending startup and the
first data-read grant; records unspecified equal-edge/continuous-write boundary
conventions; and preserves descending DMA-level priority. No technical correction
to the source Issue was needed.

[Bus admission](../src/bus/transactions.rs) inspects both SH-2 CPU write queues
using physical addresses, including accepted buffered writes and operands waiting
for admission. Only a pending A-to-B data activation is made ineligible in
[SCU cycle preparation](../src/scu/transactions.rs).
[Channel state](../src/scu/dma.rs) retains qualification across source words,
B-bus halfwords, preemption and indirect descriptors, clearing it at completion
or cancellation. No delays, host-driven effects, blanket CPU priority, reset
domains or interrupts were added. Existing regressions keep their expectations.

## Ordering reproduction and negative controls

The authored oracle in
[`queued_cpu_ab_writes_publish_before_first_dma_source_read_all_routes_and_levels`](../src/bus/dma_startup_tests.rs)
is CPU words `AABB/CCDD`, followed by DMA words `1122/3344/5566/7788`.
A granted peer CPU RAM read holds the external bus while the write is accepted
internally. Seventy-two combinations cover master/slave, all three levels
individually, direct/indirect, software/hardware start, sound/VDP1/VDP2 memory.
Each publication is observed, FIFO progression is checked before publication,
DataEnd must report four consumed words, and DMA-end status must remain clear
until all DMA words have appeared. Expected values were authored before the
implementation was run and do not import production arbitration helpers.

Before changing the inherited arbiter, the new assertion failed: passive FIFO
preview was `33445566`, expected `11223344`, while the CPU write was still
unpublished (master, level0, direct). Thus copied bytes or final completion alone
cannot satisfy this regression.

A second negative control temporarily supplied `false` to the new startup check,
restoring inherited arbitration while retaining the same guest and test bytes.
The real mixed-disc guest failed with first bitmap word `11111111` instead of
`22223333`: DMA had completed and delivered its handler, then the finite CPU
stream overwrote the overlapping destination. Its source count was still four,
so the independently expected readback and sum detected the reversed order.
The mutation was restored; it is not part of the implementation.

The remaining seven native vectors cover finite write continuity and bounded
progress; writes queued after the first physical source grant; independent A-bus
CPU writes and unrelated routes; a two-descriptor activation; one retained
hardware retrigger; legal level2-over-level0 preemption with a retained source
halfword; PLL and system reset at pending/partial source/destination phases;
CPU-local cancellation without canceling SCU; and identical publication/FIFO
snapshots for advancement chunks 1/3/17/10000 with/without passive inspection.
PLL reset retains work RAM and CD FIFO consumption and resets video/sound LSIs;
SYSRES clears volatile data and cancels all unfinished work. These tests use the
actual existing reset paths and verify no stale read, write or end status follows.
Prohibited three-channel concurrency and level2 start during level1 are not used.

## Guest and media

The original [MIT](../tests/fixtures/LICENSE)
[builder](../tests/fixtures/build_dma_startup.py) reproduces the committed
[source](../tests/fixtures/dma-startup.s),
[handler](../tests/fixtures/dma-startup-handler.s) and mixed data/audio media.
ISO SHA-256:
`d04cf218087823c9a2d35c63b720c204f6e3bb14d575f10a2c71bf9b2749fd05`.
The fixture is included in CI's reproducibility checks.

The guest enables its instruction cache, acquires a runtime sector, arms Timer0
and performs 512 overlapping SH-2 bitmap stores. Real CD FIFO reads, two-word
B-bus DMA publications and the normal DMA-end handler execute. The guest computes
readback/sum, DataEnd count and completion records. Native and production Wasm
run normal, changed source/CPU operands and omitted-trigger variants twice from
cold boot. Enabled variants yield one handler entry, eight transferred bytes and
four consumed FIFO words. Omitted trigger yields no handler, no consumption and
the CPU pattern. Independently authored palette indices verify transferred video
pixels; the browser also compares real Canvas pixels under `/saturn-rust/`.
Host code changes only input media before boot and reads outputs.

## Checks on 2026-10-24

Stable Rust from rustup was selected for Cargo/rustc; wasm-bindgen CLI matches
pinned version 0.2.129. Commands ran in the assigned worktree. Logs were inspected
for failures, ignored coverage and final results.

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 958 tests, zero failed/ignored across 89 test/doc suites |
| `cargo test --locked --lib dma_startup_tests` | 8 passed |
| `cargo test --locked --test dma_startup` | Passed, six cold guest boots |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed on restored implementation and final tests |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/build-web.sh` | Passed, production release Wasm and original demos |
| All 88 CI fixture/media `--check` commands | Passed, including new mixed-disc source/media |
| `npm ci` | Passed using committed lockfile |
| `npm run test:browser` | Passed complete suite, Chromium 145.0.7632.6, including new guest/Canvas checks |
| `git diff --check` | Passed |

No required coverage was intentionally skipped. The inherited arbiter and disabled
startup predicate failures above are deliberate negative controls. They are not
remaining failures of the restored implementation. Existing buffered-write, TAS,
refresh, direct/indirect DMA, fringes, preemption, DSP, CD replay and original demo
checks remain part of the full suites.

## Remaining scope

The documented boundary conventions are emulator scheduling choices, not measured
hardware phase timing. Physical hang behavior, B-to-A writes and prohibited
concurrency receive no new guarantees. The fixture is original and self-contained;
no personal firmware, commercial media, hardware/human confirmation, live Pages
inspection or external closure investigation is required. Mandatory #17 work
across CPU/peripheral, multiprocessor/bus/reset, SCU/DSP, video/audio,
CD/media/HLE, SMPC and comprehensive integration remains open. Historical trackers
#11/#13/#15 remain administratively closed.
