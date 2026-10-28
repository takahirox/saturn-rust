# Issue #65: raster-sampled VDP1 blank erase

Related to #65. Related to #17. This is partial hardware-completion progress.
No push, PR, merge or Issue comment is part of this node. No post-merge
verification, personal firmware, human testing/approval or live Pages inspection
is required. Issue #17 remains open.

## Specification and implementation

The [timing contract](vdp1-vbe-timing.md) contains the independently derived event
table, exact primary sections, legal/prohibited windows and capacity arithmetic.
ST-013-R3 printed p.35 and the later chapter4 update list agree on delayed VBE;
TVM remains immediate. §4.1 protects the first-to-second-HBlank interval, while
§4.2 specifies persistent VBE, manual erase/change and blank-end interruption.
§4.4 tables4.4–4.5 specify aggregate capacity, without the inherited per-raster
idle-cycle placement. ST-210 items3/30/31 reinforce word register accesses and
standard-TV raster counts; VDP1 chapter9 prohibits system-register DMA.

`Bus::advance_main_clocks` arms the decision at VBlank-in and delivers the first
HBlank-in to `Vdp1::hblank_in`. Exactly one decision consumes the pending flag.
Written TVMR/MODR, effective VBE, captured erase parameters and accepted erase
geometry/destination remain distinct. No register write launches or restarts
blank erase. Rotation's automatic blank erase remains independent of VBE.
At the sample, a retained job receives the full table capacity, distributed over
the remaining blank interval by integer word deadlines. This explicitly
documented emulator cadence preserves delayed publication and capacity without
pretending to establish physical erase timing. VBlank-out publishes the final
allowed opportunity, discards any suffix, then changes buffers/captures operands.
Reset discards pending samples, accepted work and register transactions.

The original `+201` first-word expectation is replaced with the new phase
convention's `first HBlank-in + 2` at NTSC 320×224. The independent 58,812-word
capacity, first untouched word, physical destinations, erase-capture mutation,
scanline display erase, drawing/status and IRQ assertions are preserved.

## Independent vectors and negative control

Before changing production code, the new
`legal_vbe_window_samples_last_completed_value_without_premature_publication`
vector ran against inherited immediate-start behavior:

```
cargo test --locked --lib legal_vbe_window_samples_last_completed_value_without_premature_publication
```

It failed with exit101: observed `4660` (`1234`), expected `61166` (`EEEE`),
while still before the sampling edge. Its independently authored expectations
are unchanged after the correction; it now passes. Rust formatting and module
placement changed, not this oracle. This is a failing negative control, not a
claim that the inherited implementation passed acceptance.

[Bus vectors](../src/bus/vbe_tests.rs) cover legal 0→1, 0→1→0 and 0→1→0→1,
written MODR versus unchanged pixels, both standard pixel depths, NTSC/PAL,
both PLLs and chunks1/3/17/10000. Literal table4.5 capacities cover 224/240 and
PAL256 display rasters; buffers saturating before the capacity limit are checked
in full. Unsaturated jobs explicitly distinguish the final word at OUT from the
preceding clock and the untouched suffix after cancellation. Real SH-2 replay
adapters on both processors place completed word transactions at -1/0/+1 around
sampling, distinguish early BSC acceptance from completion, and repeat with
passive debugger reads. Only -1 belongs to the legal window; equal/+1 are
explicitly deterministic emulator probes of prohibited accesses. Real PLL
conversion is independently checked at a fixed 700,000-reference-clock horizon.

[Device vectors](../src/vdp1/vbe_tests.rs) check one accepted job, no restart on
later HBlank edges, captured A despite pending B, persistent enable without
TVMR writes, subsequent post-OUT disable, both physical buffers, packed bytes
and automatic rotation erase. A separate prohibited-window test makes no
hardware-result claim. Simultaneous retained drawing crosses sampling and swap
without restarting its cursor, changing COPR/LOPR/CEF/BEF incorrectly or raising
SCU draw-end. SYSRES and [Machine cold selection](../src/reset_tests.rs) cover
pending samples and accepted jobs, with no stale writes or draw IRQ.

## Original guest, fixture corrections and browser registration

The [MIT builder](../tests/fixtures/build_vdp1_vbe.py) emits original SH-2 code
and a 512-KiB synthetic ROM covered by the adjacent license. It installs actual
SCU VBlank-in/out vectors in RAM and enters SLEEP/exception/RTE. The IN handler
writes TVMR/FBCR in the permitted window, while OUT advances the guest's own
state. Guest instructions initialize both buffers, capture A, write pending B,
enable erase/change, compute checksums plus eight edge readbacks for A and B,
clear VBE immediately after OUT, and prove that A remains unchanged in the next
field. It holds B for production display, reads status/IRQ, subtracts live FRT
samples and writes completion. The host supplies none of those results.

The independent checksum for an enabled 16×2 rectangle is
`208*0x8000 + 32*color` over a 40×6 region. The omitted-enable control is
`240*0x8000` with every sample unchanged. A is `801F`, x8..23/y1..2;
B is `83E0`, x16..31/y2..3. The mutation uses A=`FC00`, B=`FFFF`,
x8..23/y2..3. The disabled third record must equal A, rather than B or an
accumulation of both rectangles. Five IN and five OUT interrupts give exactly
ten expected exception entries (these are interrupts, not CPU faults). The FRT
blank interval must be 8200..8449 ticks, independently bounding 39×1708/8
plus finite handler observation skew; it is not an exact pin-phase claim.
Each variant repeats two cold boots in native Machine and rebuilt production
Wasm, with identical guest bytes, records and framebuffer RGBA. Browser tests
also read the real production Canvas after `advanceFrame`.

The existing erase-capture guest now enables VBE and requests erase/change
immediately after observing IN. It checks A immediately after that change,
requests the next change immediately after OUT, checks B, and disables immediately
after OUT before publishing B. The older progress guest likewise waits for IN
before enabling. These are §4.1/§4.2 programming corrections; all original
checksum, pixel, capacity, sampling and video-transaction goldens remain.
Direct device capture tests explicitly deliver the sample and use the corrected
first-word phase; their mutation/cancellation expectations remain unchanged.
Their deliberately broad internal-latch setups are not new claims about legal
VBE/FBCR combinations.

`scripts/check-fixtures.sh` requires the new builder's `--check`. The existing
`videoTransactions` browser scenario calls `scripts/vdp1-vbe.mjs`, so both the
full suite and required scenario shard cover it without changing the group
inventory or workflow dependency barrier. The three original demonstrations
remain in the required native/browser suites.

## Executed validation

Local checks on 2026-10-28; no hosted CI or deployment is claimed.

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 1,013 tests in 93 harnesses, zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | Passed: all 92 registered reproduction commands |
| `sh scripts/build-web.sh` | Passed: rebuilt release Wasm and production bindgen output |
| `BROWSER_CHANNEL=chrome npm run test:browser -- --group=videoTransactions` | Passed all assertions, including the new guest and existing Canvas regressions |
| `npm run test:browser` | Passed: 65 groups (30 shared + 35 scenarios), no required skips; artifact/report aggregation passed, exit0 |
| `node --test scripts/ci-runner.test.mjs` | Passed: 12 tests, zero failed/skipped |
| `git diff --check` | Passed |

Native build/test/Clippy used Homebrew Rust1.94.0. The first web attempt exposed
that compiler's missing Wasm target; rerunning with the already installed rustup
stable Rust1.99.0 and bindgen0.2.129 passed. Wasm Clippy used the same rustup
toolchain. Focused browser assertions used installed Google Chrome156.0.8078.49;
complete-suite validation uses Playwright1.58.2's bundled Chromium145.0.7632.6.
The complete parallel suite finished in152,610ms; shared completed in145,177ms
and scenarios in152,308ms, including cleanup and report generation. The tested
web-directory digest (the runner's path/size/content SHA-256) was
`0ca32279c74e547ce842ce836d6bc6f5f8c0243642166ec2a0249afc2bac64c5`.

Two system-Chrome full-suite attempts passed all 65 assertion groups but stalled
in browser shutdown before report aggregation. The first was terminated during
server cleanup and failed with an unsettled-await worker exit13; the second was
cancelled through the runner's normal signal path. Neither is reported as a
successful complete suite. A temporary cleanup experiment did not fix that
shutdown and was reverted. The installed bundled Chromium closed normally in a
separate launch/close check, so the full command was rerun using that matching
browser with every original assertion and report/digest barrier retained. No
skip, timeout suppression or successful-exit override was added.

The runner's fixture-inventory assertion was updated from91 to92 and explicitly
requires `build_vdp1_vbe.py --check`. Its initial stale-count failure was fixed;
all twelve runner regressions then passed. No allowance reset, purchase or
model/provider switch was used.

Artifact SHA-256 (expanded ROMs are reproduced by the builders):

| Artifact | SHA-256 |
| --- | --- |
| `vdp1-vbe.s` | `f1faf4bf380ae9a151cc43aa16821f4694ddb53ea0fb305feb6f9328577136e7` |
| `vdp1-vbe.hex` | `492ca1d495a21eb609715595a5f4a839cccbd625c99b6728a82906e1a508802b` |
| Expanded VBE ROM | `76a0e467b7f71a7381cbfeaf76d870c0683c5176056fee0c75e2488c9560d3bc` |
| Corrected `vdp1-erase-latch.hex` | `cdea635ec4b57cc185ba6ce23f6082f070e8d3223c033a304ae16d8ebaa84601` |
| Expanded capture ROM | `e0cd1a6254b741938bca5033ec298291b31256a5dc65af916c9a29f7f17a99c0` |
| Corrected `vdp1-progress.hex` | `27d0a670292357716b83d6a0577b66e9264058410b1565bff1ecb34700b21388` |
| Expanded progress ROM | `6c996888cc9ac8dfc3cede4aae3865f9d48868b6b4b440e9f1d6a809e72b0ccb` |
| Unchanged progress `.tsv` | `6f330c5e8886897cc627eaf1a1c87a733cbbd35a8a5f96de0634cc0dc977228a` |

## Remaining scope

This does not establish physical erase cadence, complete display-erase
microsequencing, unrelated PTMR/rasterizer behavior or VDP2 half-line interlace.
CPU/peripheral, multiprocessor/bus/reset, SCU/DSP, video/audio, CD/media/HLE,
SMPC and comprehensive integration acceptance remain mandatory under #17.
Genuine BIOS and commercial-game compatibility remain unverified. No required
post-merge verification remains.
