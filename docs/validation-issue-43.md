# Issue #43 validation: separate video-memory write words

Related to #43 and #17. Baseline `580a451e042591acdf2ce1a49b538a440a09b35b`
was checked out through GitWeave checkpoint `57478dd51a26882569efb32442a0d01892bffea5`.
Validation date: 2026-10-24. No push, PR, merge, Issue mutation or post-merge
verification is part of this work.

## Contract and implementation

The [transaction contract](sh2-video-writes.md#primary-contract) links the primary
SCU ST-097-R5 §3.2 pp. 43–45/figs3.6–3.8, ST-210 §3 items 03/32, SH7604 §7.11.2
pp. 210–211 and VDP1/VDP2 ownership rules. The actual primary PDFs were inspected,
including the DMA context of the SCU diagrams and the VDP1 register restriction.
The [literal transaction examples](sh2-video-writes.md#independently-derived-examples)
and [guest arithmetic/pixel oracle](sh2-video-writes.md#original-guest-oracle)
distinguish specified granularity from deterministic phase envelopes, ascending
address order and equal-edge display-before-publication conventions. No measured
CPU-write waveform or physical-console equivalence is asserted.

[CpuBus::buffered_external_write](../src/bus/transactions.rs) retains each accepted
payload and emits at most a word per scoped video-memory phase. The original BSC
configuration is retained. The per-transfer original-longword marker preserves
one CPU-facing external UBC observation after the second word, including manual
reset drains. CPU-local and DMAC UBC still observe their requested access once at
acceptance. Byte/word strobes and narrow BSC transfers retain existing behavior.
The [adapter](../src/bus.rs) performs the existing alignment/endian/cache handling;
shared endpoint decoding includes existing VRAM/framebuffer/CRAM and P0/P1 aliases.
The inherited MMIO path is unchanged, including its existing VDP1 longword
compatibility, which is not hardware-conforming under ST-210 item 03. Memory
conversion introduces no additional register permissions or adjacent MMIO writes.
Read bridging, SCU/DSP, debugger behavior and #41 sound allocation are preserved.

## Independent native vectors

[video_write_tests.rs](../src/bus/video_write_tests.rs) contains twelve tests with
literal addresses, payloads, ordering and deadlines. No production width/grant/
transfer helper supplies expected outcomes. Coverage includes:

- Every scoped memory endpoint and its existing mirrors, both processors, P0/P1,
  byte lanes, word lanes, longwords, supported little endian and BSC byte/word
  configurations, alignment rejection and exclusion of adjacent MMIO.
- Initial `AABBCCDD`, first publication `1122CCDD` at 8, final `11223344` at 16;
  changing a completed high word to `7788` retains `77883344` after the pending
  low word. Retired-CPU/source changes cannot replace an accepted payload.
- Both processors' DMACs, both channels, byte/word/longword and 16-byte units;
  explicit word deadlines 17/25/33/41/49/57/65/73 and final-unit TE/IRQ at 58,
  before the last two publications. The first acceptance adds its existing
  internal state; later queued stores can begin an external grant as the prior
  buffer empties while acceptance notification completes on the next clock.
  This convention does not add a fictitious idle state between device words.
- VDP1 drawing ownership across both word envelopes, with continued untextured
  drawing during independent VRAM writes. VDP2 grants revoked after the first
  word delay the second until 1282; a new write remains backpressured. Independent
  on-chip work progresses; a dependent external read drains both pending words.
- Before/equal/after display acquisition at 32: first word is new in every case,
  second word is new only when completed before 32. Exact retained display traces,
  memory results and pixels agree across chunks 1/3/17/10000 and passive previews.
- CPU and DMAC queued/accepted/partial writes through real watchdog manual/power
  resets; canceled queued stores never appear, manual reset drains only the
  accepted longword, completed words do not replay after mutation, and peer
  peripheral state survives. System reset cancels CPU/DMAC work at queued,
  accepted and partial stages and clears volatile memory.
- CPU/DMAC UBC requested size and once-only acceptance, plus peer external UBC
  seeing one longword after the second publication and no second-word address
  event. Existing SCU final-destination/IRQ and sound allocation/reset tests
  remain part of the complete suite.

## Original guest and actual pixels

The original [MIT](../tests/fixtures/LICENSE)
[builder](../tests/fixtures/build_video_writes.py) reproduces the
[assembly](../tests/fixtures/video-writes.s) and [ROM](../tests/fixtures/video-writes.hex).
Expanded 512-KiB SHA-256:
`3c885b12e0b8a8a5f810e65a5f8f9b6bf56875349fb73be26ac5781256354803`.
The guest starts the slave using SMPC. Each CPU executes sixteen longword stores
and a 16-byte on-chip DMA unit at each of VDP1 VRAM/draw framebuffer and VDP2
VRAM/CRAM. It computes readbacks and FRT differences in ordinary work RAM.
The hosts supply only initial ROM bytes/operand mutations, execution advancement
and passive observation, never running results or pixels.

[Native Machine tests](../tests/video_writes.rs) and the registered
[production Wasm/Canvas check](../scripts/video-writes.mjs) execute identical bytes,
with original/changed payloads, omitted stores/DMA start, and two cold boots per
variant. Sixteen video longwords require at least 256 clocks, or 32 FRT/8 ticks;
the omitted-store interval must be more than 24 ticks shorter. Actual normal CPU
intervals were 61–62 ticks and omitted intervals 28–29. Normal 16-byte DMA intervals
were 41 ticks; omitted starts 19–20. All readback/control expectations passed.
Native execution chunks 1/3/17/10000 produce identical records and complete pixels.

The guest times a CRAM mode2 longword on line 16. The high word removes old blue;
the low word later supplies new red or green. Eight clocks between publications
at four clocks per dot independently require exactly two intermediate black dots.
Actual dots 184/185 are black between blue and red/green. Their position is an
observed result, not a golden constant used by tests. Omitting the store leaves
the entire row blue. Native pixels and the actual production Canvas agree under
`/saturn-rust/`, including changed payloads and repeated cold boots.

## Executed inherited-path negative controls

Only the video bridge predicate was temporarily disabled, restoring inherited
BSC-width atomic writes while leaving sound allocation, reads, guest bytes and
independent assertions intact. Sources were saved/restored byte-for-byte in a
`finally` block. A separate static-site copy held the negative production Wasm;
positive browser assets were never replaced by the negative control.

| Check | Actual inherited-path failure |
| --- | --- |
| `cargo test --locked --lib video_longword_publishes_retained_words_at_eight_and_sixteen_without_replay -- --nocapture` | Exit 101 at clock 8: actual `11223344`, expected partial `1122CCDD` |
| `cargo test --locked --test video_writes guest_video_writes_both -- --nocapture` | Exit 101: zero black dots instead of independently required two |
| Rebuilt release Wasm + `node scripts/video-writes-probe.mjs target/video-writes-negative-site` | Exit 1: actual Canvas/guest pixel check reports `0 !== 2` |

Negative Wasm SHA-256:
`254e13ab65c06ab27b3011f63c2b5266ae78182ab7ed1ddbed5c09fd5ca74341`.
Production sources were restored, rebuilt, and the positive native vectors and
focused browser probe passed. Reproduce the positive probe after building with
`node scripts/video-writes-probe.mjs`; an optional argument selects a static-site
directory. The complete required browser command remains `npm run test:browser`.

## Historical timing assertion correction

The first full native run exposed Hardware Lab's immediate host EFREG1 preview:
its guest record was correct (`0900`), but the live register was temporarily zero.
ST-077 fig5.1/ST-228 Reference2 and existing
[DSP deadline vectors](../src/sound/dsp/clock_tests.rs) establish clearing at sample
start followed by the actual instruction 7 effect store. Extra video word phases
move scene completion relative to that existing sound clock. The old assertion
incorrectly required a held result at an arbitrary host sampling instant.

[Hardware Lab's test](../tests/hardware_lab.rs) now observes the same real register
for 128 bounded machine turns, permits only the specified clear value or exact
coefficient-dependent result, and requires that exact result to appear. The
original guest result, all three coefficient variants, exact wet/dry stereo
vectors, disc bytes and rendering/gameplay/restart assertions are unchanged.
The focused four-test Hardware Lab suite passed after this correction; its guest
and audio expectations and device timing are unchanged.

A later complete native/browser run exposed the original rotation-image guest's
late-store phase. Serialized longword parameter-table setup changes when its
polling loop begins. ST-058-R2 §2.5 tables2.3/2.4 specify invalid normal HCNT bit0;
the existing model exposes a four-main-clock counter bucket, not a unique clock
edge. The [builder](../tests/fixtures/build_rotation_fetch.py) adds one cached NOP
between the poll exit and word store, retaining the warm 16-byte instruction
block and the blanking grant. This restores publication at/after first-dot
acquisition and before the next dot. All four native rotation tests pass with
**unchanged** H=844/sum/completion records, every original pixel assertion,
source mutations, early and omitted writes, cold boots and chunk comparisons.
No rotation timing, grant policy or image expectation was changed. The
[conformance matrix](hardware-conformance.md#scheduled-rotation-image-acquisition)
records regenerated hashes; the old #39 validation is explicitly historical.

## Required checks

Rust checks use installed rustup stable Rust 1.99.0 (`b940084d7`); Node 24.12.0,
Playwright 1.58.2, Chromium 145.0.7632.6 and matching wasm-bindgen 0.2.129.
Early focused checks also used the existing Homebrew Rust 1.94.0. No allowance resets, purchases or provider/model
switches occurred.

| Check | Actual result |
| --- | --- |
| `cargo fmt --check`, `cargo build --locked` | Passed |
| `cargo test --locked --no-fail-fast` | Passed: 912 tests (589 library, 323 integration), zero failed/ignored |
| `cargo test --locked --lib video_write_tests` | Passed: 12 vectors, including final reset/TE refinements |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| All registered fixture/media reproduction checks from `.github/workflows/pages.yml` | All 84 passed, including new guest and large media |
| `sh scripts/build-web.sh` | Passed, restored production module rebuilt |
| `npm ci --ignore-scripts` | Passed, zero reported vulnerabilities |
| `node scripts/video-writes-probe.mjs` | Passed |
| Complete extended `npm run test:browser` | Passed in Chromium 145.0.7632.6 under `/saturn-rust/`, no required skips |
| JavaScript syntax and Git whitespace | Passed |

All mandatory checks passed. The full native run includes Star Collector, Prism
Courier, Hardware Lab, SCU/DSP destination/IRQ and #41 sound-memory/reset evidence.
The complete browser run also preserves those demonstrations, actual AudioWorklet,
large-media, backup, input, offline and privacy assertions. No required check
remains pending or skipped.

Production Wasm SHA-256:
`856a73d3e9375ecbcd0e044bd205ccce4a683e5a1649de11ee5e57077526e4a9`.

## Remaining hardware scope

This is a partial #17 increment. Remaining CPU/peripheral semantics,
multiprocessor and board timing/reset, SCU/DSP, video/audio, CD/media/HLE, SMPC
and comprehensive integration acceptance remain mandatory. #17 stays open;
commercial-game compatibility and full hardware completion remain unverified.
No personal firmware, commercial media, physical console, human testing/approval,
live Pages inspection or external Issue closure investigation is required.
Required post-merge verification: none.
