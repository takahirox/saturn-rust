# Issue #25: VDP1 erase data and window capture

Related to #25. Related to #17. This is a partial hardware-completion increment;
Issue #17 remains open. No personal firmware, physical console, human approval,
live Pages inspection or investigation of historical tracker closure is required.

## Specification and implementation

The primary contract is the VDP1 User's Manual ST-013-R3:

- [Chapter 4 register update timing](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_10.htm): EWDR, EWLR and EWRR become effective at framebuffer switching.
- [§4.2 framebuffer control](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_12.htm): automatic changes, manual erase, manual change and VBlank erase/change sequences.
- [§4.4 erase settings](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_14.htm): sixteen-byte X units, inclusive lower Y, 16-bit words or alternating high/low bytes, per-raster overhead and finite erase time.
- [§4.9 MODR](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_19.htm): readback reports written register settings, which can differ from internal values.

[Vdp1](../src/vdp1.rs) now retains `erase` for written settings and
`erase_latched` for the three words captured together by an actual framebuffer
change in `vblank_out`. `erase_area` consumes the captured copy for both normal
and VBlank erase. A manual erase request without change cannot capture new
settings. Repeated held fields retain the old capture. The existing accepted
`EraseArea`/`Erasing` owns its color, rectangle, geometry and physical buffer
until completion or cancellation. VBlank-OUT cancels unfinished work before
switching buffers. Device/system reset reconstructs both copies and discards
pending transactions/jobs. Tests explicitly initialize every configuration;
zero-initialized emulator storage is not used as a hardware power-on oracle.

MODR's expression is unchanged; its misleading comment now describes written
settings. Immediate PTMR=1, existing drawing-control capture, PTMR=2 start,
ENDR, EDSR/COPR/LOPR and SCU interrupt/DMA paths are unchanged.

The [Bus scheduler](../src/bus.rs) already processes raster edges before physical
external data completion at equal clocks. The new tests preserve that order:
a register write completed one clock before a change is included; an equal-edge
or one-clock-later write waits for a later change. ST-013 specifies the capture
boundary but not an electrical subphase. This order is an explicit emulator
convention, not a claim about measured pin timing. VBE sampling phase, half-line
timing, additional display modes and rasterizer changes are outside this increment.

## Independent vectors

[erase_tests.rs](../src/vdp1/erase_tests.rs) defines A = `1234/0201/0602`,
B = `5678/0402/0803`, C = `9ABC/0004/0204`. From §4.4, these are byte columns
16..48 on rows 1..2, 32..64 on rows 2..3, and 0..16 on row 4 respectively.
The partially overlapping areas distinguish color, extent and buffer identity.
The literal byte oracle does not call production geometry/address helpers.
It covers automatic/manual changes, manual erase followed by change, multiple
pending writes, held fields, mutations during accepted display/VBlank erase,
16-bit and both supported 8-bit geometries, packed high/low bytes and canaries.
Separate tests check cancellation after exactly one pixel, no later A writes,
subsequent captured B, and MODR's written settings while drawing/erase latches
still hold earlier values.

[Bus vectors](../src/bus/vdp1_tests.rs),
`both_sh2_adapters_capture_only_physically_completed_erase_writes_at_field_change`,
exercise each erase register via both actual replay adapters. All 144 combinations
of processor, register, -1/0/+1 completion, chunk 1/3/17/10000 and debugger
inspection pass. They assert the eight-clock external grant, the pending physical
write one clock before completion despite prior CPU buffer acceptance, physical
completion, literal pixels, the other physical framebuffer, MODR, EDSR and no
spurious draw interrupt. A separate eight-case reset vector starts captured A,
writes pending B, accepts but does not physically finish C, resets, initializes D,
and checks its literal pixels/canaries and absence of stale writes/interrupts.

The existing [progress vectors](../src/vdp1/progress_tests.rs) retain scanline
following, 200-clock overhead, 58,812-word partial budget and VBlank-OUT
cancellation assertions, plus drawing/status/stop/IRQ regressions. Their setup now
writes the intended erase parameters **before** the framebuffer change. The
[older progress guest builder](../tests/fixtures/build_video_transactions.py)
likewise moves EWDR/EWLR/EWRR writes before its publishing change, then enables
VBE. Under chapter 4, writes after that change could not legitimately define
that erase. Its `vdp1-progress.tsv` and all substantive assertions are unchanged;
only the source/setup and reproducibly generated ROM change.

## Original guest and independent expectations

The [MIT builder](../tests/fixtures/build_vdp1_erase_latch.py) reproduces the
[SH-2 source](../tests/fixtures/vdp1-erase-latch.s) and
[512-KiB ROM encoding](../tests/fixtures/vdp1-erase-latch.hex), covered by the
adjacent [MIT license](../tests/fixtures/LICENSE). Guest instructions initialize
both physical framebuffer regions to `8000`, set A, poll TVSTAT across an actual
framebuffer change, write pending B, let a VBlank erase use A, then request a
subsequent change to capture B. The guest reads the A result through the CPU draw
aperture, waits for B's erase on the other physical buffer, and reads B. It sums
all 240 pixels in each region, records eight independent edge samples, MODR,
EDSR, COPR and masked SCU draw status, then publishes B and parks with erase off.
Host code only loads the ROM, advances execution and inspects results.

A is red `801F`, x8..23/y1..2; B is green `83E0`, x16..31/y2..3.
Each checksum is literally `208 * 0x8000 + 32 * color`, with untouched `8000` canaries.
The mutated variant uses blue A `FC00`, white B `FFFF`, and B x8..23/y2..3.
An omitted-B-write control retains A's red rectangle on both buffers. Every
variant repeats two cold boots in [native Machine](../tests/video_transactions.rs)
and [production browser Wasm](../scripts/video-transactions.mjs), using identical
ROM bytes and operand mutations. Both check guest records and literal framebuffer
RGBA for a 40-by-6 area. Browser assertions also read the actual production
Canvas after `advanceFrame`; host code does not issue VDP1 setup or draw output.

Artifact SHA-256:

| Artifact | SHA-256 |
| --- | --- |
| `vdp1-erase-latch.s` | `4996e701ac64900746b44e0bab168e4b7ef5eec7626f331cec8af42bfd120b55` |
| `vdp1-erase-latch.hex` | `f3d76edc704ea67beb52395b1a86f2e3d83e13dacbfb3aab238bf2389de2bde2` |
| Expanded guest BIOS | `038a9217e309dda42b09977754e213eb87c22a9f0ea6f6ac212bb2daba4d7adc` |
| Corrected `vdp1-progress.hex` | `c62ba6355eb344a8c4381142ddd3012ff5fff42c2f18010d3c243862e4f97ca0` |
| Expanded progress BIOS | `30e75e2872651275421b093131c235332fae45673940c71b300c15bf7a0b54bf` |

## Negative controls

Before changing production behavior, the command
`cargo test --locked vblank_erase_uses_captured_a_until_the_next_change_captures_b --lib`
ran the newly authored test against the inherited VDP1 implementation from
`d5a9861cd0c2243686dc31f1f43d41d5a91cb3af` (unchanged in upstream node
`d12adbd63ee61616bd0ae6918303719dc59f1bf3`). It **failed**, exit 101:
at byte16/row1 the observed byte was 238 (`EE`), expected 18 (`12`).
A's rectangle was left untouched because VBlank consumed pending B. This is
expected failing negative-control evidence, separate from passing fixed tests.
The same regression passes with captured settings. Goldens were authored from
the manual, never recorded from corrected execution.

The original guest was also run with the exact inherited `src/vdp1.rs`
temporarily substituted from commit `d5a9861cd0c2243686dc31f1f43d41d5a91cb3af`:
`cargo test --locked --test video_transactions guest_erase_register_capture_survives_pending_writes_and_cold_boots -- --nocapture`.
It **failed**, exit 101: A's guest checksum was 7,896,064 (B/green), expected
7,865,312 (A/red), and its edge samples contained B's color/rectangle.
B's records already matched, isolating the premature capture defect.
A `try/finally` restored the fixed file byte-for-byte before further validation
(SHA-256 `6846c1cb047f3803d85f67d926f26eda563d67a26d8e186753b57213e380341a`).
The same guest test then passed with the correction (without `--nocapture`). The negative run is not
included in the passing complete-suite count.

## Validation results

Executed in this worktree on 2026-10-23:

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| Complete `cargo test --locked` | **799 passed**: 502 unit + 297 integration; zero failures/ignored/filtered; doc tests passed (zero defined) |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/build-web.sh` | Passed; production release Wasm and static assets built |
| All fixture/media builders in [CI's reproduction step](../.github/workflows/pages.yml), each with `--check` | **75/75 passed**, including `python3 tests/fixtures/build_vdp1_erase_latch.py --check` and the corrected video builder |
| `npm ci --ignore-scripts` | Passed; three packages installed, zero reported vulnerabilities |
| Complete `npm run test:browser` under `/saturn-rust/` | **Passed**, Chromium 145.0.7632.6, no required skips; includes the new erase guest, records, literal framebuffer and Canvas assertions |
| `git diff --check` | Passed |

Native commands used Homebrew Rust 1.94.0. Wasm commands used the installed
rustup stable Rust 1.99.0 toolchain and `wasm-bindgen 0.2.129`, by prefixing PATH
with `/Users/takahiro/.rustup/toolchains/stable-aarch64-apple-darwin/bin` and
`/Users/takahiro/.cargo/bin`. The first web-build attempt used Homebrew Rust,
which lacked its Wasm `core` target and failed with E0463. Selecting the already
installed rustup toolchain/target corrected the environment; the final full web
build and Wasm Clippy passed. No required check was waived.

The complete browser run preserved Star Collector completion/restart, Prism
Courier collision/win/restart, Hardware Lab diagnostic/pixel restart, production
WebAudio/AudioWorklet, both controllers, backup RAM import/export/retention,
offline operation and privacy assertions. Its final `Browser smoke passed`
message and successful exit followed every new and existing assertion.

The reproduction run executed each `python3 ... --check` line from the existing
CI file with `subprocess.run(command.split(), check=True)` and completed all 75.
The new builder is also listed in [setup instructions](setup.md).

## Remaining scope

This fixes the erase-parameter capture lifecycle and its current automatic/manual
integration. CPU/peripheral semantics, multiprocessor/bus/reset completeness,
SCU/DSP, further video/audio, CD/media/HLE, SMPC and comprehensive hardware
acceptance remain outstanding under #17. A green checkpoint or intermediate
merge cannot establish hardware completion or broad commercial-game compatibility.
Historical #11/#13/#15 remain closed. Required post-merge verification: none.
