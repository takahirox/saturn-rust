# Issue #19: scheduled normal-background acquisition

Related to #19. Related to #17. This is a video hardware increment, not full
hardware completion or demonstrated commercial-game compatibility.

## Contract and implementation

Primary contract: Sega ST-058-R2 §3.3, figure 3.2, tables 3.2–3.4 and figure 3.5.
The [original manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-058-R2-060194.pdf)
and archived sections on [cycle order](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p03_30.htm),
[image access quantities and dependencies](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p03_31.htm)
and [vertical-cell ordering](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p03_32.htm)
are the sources for the authored schedules, not emulator-generated pixel goldens.

- [Raster](../src/timing.rs) supplies display deadlines in T0–T7 order, or
  T0–T3 in four-timing modes. Slot widths use the existing CPU VRAM phase:
  four main clocks, or two in exclusive-monitor modes.
- [Bus](../src/bus.rs) splits advancement at those deadlines. On an equal edge,
  raster/line-start effects precede display acquisition; display acquisition
  precedes CPU, on-chip DMAC or SCU transaction completion. Transactions retain
  their existing programmed grants, split-bank restrictions and completion costs.
- [Acquisition](../src/vdp2/fetch.rs) has independent layer/path cursors and
  serializers for names, characters/bitmaps and vertical-cell data. Each bank
  executes at most one normal command at an edge. Names acquire a complete
  one/two-word entry; character and cell transfers acquire two physical words.
  Bank selection, reduction quantities, table dependencies and rotation-owned
  exclusions come from [access eligibility](../src/vdp2/access.rs).
- Address generation is shared with the renderer, without window, priority or
  palette gating. A bounded eight-dot packet retains its addresses, words and
  coordinate/name interpretation. Scroll-register changes preserve acquired
  packets and restart unfinished address generation. Palette and composition
  controls remain live during scanout. Normal rendering cannot fill a missing
  packet or read missing VRAM; debugger snapshots decode live memory separately.
- Rotation retains its previous rendering and first-use serializers. No rotation
  acquisition timing claim is added. Blanking, new lines and cold reset discard
  unfinished normal requests. Prefetch across a field boundary uses the target
  field's parity, including double-density line zero.

## Explicit phase convention

The manual establishes access order, quantities and restrictions; it does not
establish the complete physical fetch-to-display delay used here. The emulator
primes the following line during the final **two cycles** of horizontal blank,
then continues the same eight-dot packet stream during active display. The lead
is 64 main clocks in normal mode, 32 in high resolution and 16 in exclusive
monitor modes. This is a deterministic emulator convention, not measured console
latency. Existing exclusive-monitor coordinate/output multiplexing is retained.
Normal address generation previews the upcoming line tables when priming begins;
composition/rotation line-start latches retain their existing event.

For a 1708-clock normal line, next-line T0 is at 1644, T7 at 1672 and the next
T0 at 1676. This phase is shared by all advancement sizes. The initial reset line
has no fabricated negative-time prefetch; its partial startup output is not a
promise about physical power-on pixels. The raster-prefix test explicitly checks
this unprimed line: a VRAM clear at clock768 cannot invalidate the four 8bpp dots
already acquired at that T0. Its earlier immediate-clear expectation was corrected
using §3.3 table 3.3; palette and color-offset prefix assertions remain intact.

## Independent tests and original guest

[Device vectors](../src/vdp2/fetch.rs) cover every supported tile color depth on
NBG0–3, NBG0 bitmap formats, eight/four timings, one/two-word names, exact address
and data traces, mutation after name acquisition, independent vertical-cell
paths with before/after mutations, split/unsplit banks, scrolling, enlargement,
1/2 and 1/4 reduction, subsequent line refetch, disabled-layer release and
rotation exclusions. The access vectors also correct and test the manual's
NBG0/1 T0 exception and the separate high-resolution 2×2-cell restriction row.

[Board vectors](../src/bus/vdp2_tests.rs) exercise both real SH-2 transaction
adapters, both on-chip DMACs and all three SCU DMA channels. Literal schedules
check publication before/at/after T0, retained words, independent-bank progress,
DMAC acceptance/IRQ before buffered publication, SCU completion/IRQ after the
last physical beat, reset cancellation, and identical complete line acquisition
traces/pixels for chunks **1, 3, 17 and 10000**. Repeated debugger snapshots do not
change those results. [Native Machine tests](../tests/vdp2_fetch.rs) also compare
actual guest records and completed frames across those execution batch sizes.

The new original [SH-2 source](../tests/fixtures/vdp2-cycle-slots.s) and
[synthetic BIOS](../tests/fixtures/vdp2-cycle-slots.hex) are generated by the
[existing builder](../tests/fixtures/build_vdp2_fetch.py), under the adjacent
[MIT license](../tests/fixtures/LICENSE). The guest initializes VRAM, CRAM,
cycle patterns and zoom, enables its instruction cache, polls real counters,
executes an actual timed VRAM store, and records counters and VRAM readback.
It parks during active display; production frame advancement completes and
presents that same field. Host code supplies media and inspects results only.

Native and [production Wasm/Canvas checks](../scripts/video-transactions.mjs)
run source-word mutations (red/blue), a write before acquisition, a write after
acquisition but before line16 consumes the word, an omitted-write control,
T0/T7 cycle-slot mutations and repeated cold resets. Literal whole-row pixels
check old line15, retained/new line16 and refetched line17/223. The old
word-retention guest and its substantive assertions remain unchanged.

Expanded 512 KiB ROM SHA-256 values:

| Fixture | SHA-256 |
| --- | --- |
| New cycle slots | `126676ce4a03e2216730c8496d8132109a6615a85ba8836c9bd7961f4c8c3623` |
| Existing fetch | `121eb0283aa2e6f0b677bc5a04755efd5e6df2f6b4810cc99d752e0b5ccd727b` |
| Existing word retention | `242a1e1f3e832525a12463da39937ba47a5f814f0e5d66a77e862f0edf4e9c69` |

The identical new guest/test was executed in a separate worktree of inherited
revision `927ad71f238f8ff173f4ac0c3c45ecc4ed71d3da`. It **fails** on line16,
pixel0: inherited first-use acquisition shows green, while the independently
specified scheduled result is retained red. This negative control is deliberate,
not a passing validation run. No baseline production files were modified.

## Validation results

Validation executed in this worktree on 2026-10-23:

- `cargo fmt --check`, `cargo build --locked` and
  `cargo clippy --locked --all-targets -- -D warnings`: passed with native
  Homebrew Rust 1.94.0.
- Complete `cargo test --locked`: **763 passed** (480 unit, 283 integration),
  zero failed/ignored/filtered, plus passing doc tests. The repository's optimized
  test profile retains debug assertions and overflow checks.
- All **72** CI fixture/media reproduction commands: passed. The existing fetch
  builder checks the new source/hex too; no fixture or media check was omitted.
- `sh scripts/build-web.sh` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`: passed
  with installed rustup stable Rust 1.99.0 and wasm-bindgen 0.2.129. The initial
  build attempts found a missing matching CLI and a Homebrew Rust path without
  the Wasm target; installing the matching CLI and selecting the installed rustup
  toolchain resolved both.
- `npm ci --ignore-scripts`, JavaScript syntax, relative documentation-file links
  and Git whitespace checks: passed.
- Complete `npm run test:browser` on final rebuilt production Wasm: passed
  on Chromium **145.0.7632.6**, Node **24.12.0**, under `/saturn-rust/`, with no
  required skips. The new timing guest passes framebuffer and actual Canvas
  assertions; the suite also passes all existing diagnostics, Star Collector
  completion/restart, Prism Courier, Hardware Lab, rotation/window/composition,
  audio/AudioWorklet, both pads, backup retention and browser privacy checks.

An earlier complete browser run passed before the final packet-metadata/scroll
refinement. That run is not substituted for the final rebuild. The first broad
native/browser runs exposed next-field parity in priming; that implementation
bug was fixed, and the complete 60-mode native test passed without changing its
fixture or assertions. Earlier failing runs are not reported as passes.
Historical records elsewhere remain historical.

## Remaining scope

The horizontal phase convention and startup policy above remain explicit model
assumptions. Invalid programming still has no invented hardware pixel guarantee;
existing validation errors and grant eligibility remain enforced. Rotation-specific
acquisition timing, half-line/counter behavior, other video boundaries, remaining
CPU/SH7604/68k semantics and timing, board/reset/cache arbitration, SCU/DSP, sound,
CD/media/HLE, SMPC and comprehensive integrated acceptance remain mandatory under
#17. No campaign policy or tracker state changes. Required post-merge verification:
none.
