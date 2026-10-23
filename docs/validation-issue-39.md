# Rotation image acquisition validation

Related to #39 and #17. #17 remains open; Issues #11/#13/#15 remain
administratively closed. No required post-merge verification. Genuine BIOS and
commercial-game compatibility remain independently unconfirmed.

## Contract and implementation

The [transaction contract](vdp2-rotation-fetch.md) was written before the
implementation and image expectations. It links exact primary sections and
reconciles Sega bulletins #13/#14/#33/#37/#SOA-6/#SOA-11. The later §4.5 color
control tables also affirm RGB888's high-resolution prohibition. No acceptance
criterion was weakened and no external Issue was changed.

[Raster](../src/timing.rs) supplies a separate rotation event at each output
dot's start, using four main clocks in normal resolution or two in high
resolution. [Bus](../src/bus.rs) splits advancement at these deadlines and orders
line-start latches before acquisition before simultaneous physical completion.
Unlike normal acquisition, rotation does not prime the following line. It never
consumes RPRCTL while previewing future addresses. Existing #35 reload tests and
the rotation/sprite guests remain unchanged.

[Acquisition](../src/vdp2/fetch.rs) owns one complete name and aligned image
operand per dot and screen. [Address decoding](../src/vdp2/rotation.rs) is shared
with snapshot rendering, while production consumption uses retained base,
source-pixel, palette, name flags, color format and coefficient-line selection.
It cannot fill an image miss from current VRAM. Every output dot reacquires even
an identical address. RBG1 has independent raw-word storage despite sharing register/composition id0
with NBG0; changing display modes cannot overwrite an acquired normal operand.
Normal and rotation operands share bounded line storage;
field/line tags prevent reuse of stale line-zero operands. New lines and reset
discard pending data. Snapshot decoding leaves hardware requests and traces
unchanged. Palette and composition controls remain live.

These are deterministic phase/port-packing assumptions, not measured physical
rotation latency. Separate coefficient-table acquisition, other table strobes,
complete board arbitration and all remaining #17 requirements remain outside
this increment.

## Independent vectors and guest evidence

[Device vectors](../src/vdp2/rotation_fetch_tests.rs) use literal physical
addresses, names, words and pixels for all five formats, one/two-word names,
8/16-dot characters, quarter-turn vertical-row sampling, backwards tile crossing
and address revisits. They cover
RBG0 tile/bitmap, RBG1 fixed routing, split/unsplit sides, ineligible banks,
normal coexistence and NBG0/RBG1 display-switch isolation, high-resolution
deadlines and restrictions, and stale
line-zero cancellation. Destroying image RAM and changing address registers
after acquisition preserves already acquired dots; an unacquired dot is black.
[Raster vectors](../src/timing.rs) independently enumerate all four standard
horizontal modes, dot widths, active/blank boundaries and exclusive exclusion.

[Board vectors](../src/bus/vdp2_tests.rs) grant physical writes in blanking and
complete them before, at and after line1's literal clock1708 deadline. Both SH-2
adapters and both on-chip DMACs retain existing buffered acceptance/IRQ ordering.
All three SCU channels check final-write publication before completion IRQ; a
second beat granted before display proceeds, while a later beat waits for
HBlank. A rotation-owned A port does not prevent B progress. Complete traces and
pixels agree across chunks1/3/17/10000, with/without debugger snapshots; reset
cancels pending writes and image operands.

The [original MIT builder](../tests/fixtures/build_rotation_fetch.py), its
[tile source](../tests/fixtures/rotation-fetch-tile.s),
[bitmap source](../tests/fixtures/rotation-fetch-bitmap.s) and
[RBG1 source](../tests/fixtures/rotation-fetch-rbg1.s) use the adjacent
[fixture license](../tests/fixtures/LICENSE). Each SH-2 guest builds its own
parameter tables, names, images and palette, configures RBG0 or dual RBG0/RBG1,
enables its cache, waits for a field and line15, polls the real H counter and
performs a legal blanking-granted image write. The poll/store block is explicitly
aligned to avoid a cold instruction miss moving the store into active display.
The guest records H, reads the published word, computes initial+published with
SH-2 ADD, and writes its completion marker. Host code supplies no device setup,
results or rendered pixels.

The default tile/RBG1 write completes at line16 clock0; bitmap completes at
clock1. Both follow the first acquisition and precede the next. Literal image
expectations are old red at (0,16), new green at x1..319 of that line, red on
preceding lines, and green afterward. Changed initial word3333 makes old pixels
blue; threshold700 moves the write earlier and makes all of line16 green;
omitting the store leaves every pixel unchanged. Guest instruction alignment
and delay operands were investigated with temporary bounded publication probes;
expected image values follow the primary-derived retained-operand contract and
authored palette/coordinates, never captured emulator frame goldens.

[Native Machine tests](../tests/rotation_fetch.rs) assert every framebuffer pixel
and computed records through two cold boots of every variant/control, and
compare results across chunks1/3/17/10000. The
[browser extension](../scripts/rotation-fetch.mjs), registered in the complete
suite, executes identical ROM bytes in rebuilt production Wasm under
`/saturn-rust/`. It checks all framebuffer and actual production Canvas pixels
for the same 36 cold-boot/control combinations. Builders reproduce source, hex,
literal records and expanded-ROM SHA-256 files; the
[conformance matrix](hardware-conformance.md#scheduled-rotation-image-acquisition)
records all three hashes.

## Executed inherited-path negative controls

Temporarily restored the rotation bypass in `fetch_word` for consumption and
disabled `scheduled_rotation_sample`, leaving the inherited `display_word`
serializer and live rotation address decoder in use during rendering. Acquisition
still runs, but the old render path ignores it. No production fault switch or
negative implementation is committed. Source was restored in a `finally` block.

- `CARGO_TARGET_DIR=target/rotation-negative cargo test --locked --test rotation_fetch`
  failed all four tests. Each of the three screen-specific guests failed at
  (0,16): actual green `[0,255,0,255]`, expected retained red `[255,0,0,255]`.
- Built that same temporary source with
  `cargo build --locked --release --target wasm32-unknown-unknown` in the isolated
  target directory and the matching wasm-bindgen0.2.129 CLI. Mounted a copy of
  production web assets under `/saturn-rust/`, running `rotationFetch` separately
  for tile, bitmap and RBG1 in Chromium. All three failed at (0,16) with the same green-versus-retained-red mismatch;
  direct Canvas reads independently confirmed the wrong green pixel. The isolated
  Chromium checker exited successfully only after rejecting every inherited path.

The first isolated Wasm attempt used the host Homebrew compiler, which lacked
its Wasm standard library. Repeating with the already installed stable toolchain
PATH from `docs/setup.md` built successfully. No allowance reset, purchase,
model/provider switch, firmware or external media was used.

## Actual final checks

Executed on 2026-10-24 using the installed stable Rust toolchain, Wasm target
and wasm-bindgen0.2.129. The complete production browser run also passed.

| Command/check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 884 tests across 83 targets, including 566 unit tests; zero failures/ignored tests |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| Every `python3 … --check` registered in `.github/workflows/pages.yml` | Passed all 81 inherited builders/media checks, plus the newly registered rotation builder (82 total) |
| `sh scripts/build-web.sh` | Passed; rebuilt final production Wasm and all original demo media |
| Complete extended `npm run test:browser` | Passed in Chromium145.0.7632.6 under `/saturn-rust/`; no required skips |
| Expanded ROM hashes, focused relative links, `git diff --check` | Passed |

The historical normal-fetch, reload, sprite rotation, coefficient, window,
composition, CPU/DMA/bus and all other native/browser regressions pass. Star
Collector, Prism Courier and Hardware Lab retain their gameplay, restart,
graphics, audio and diagnostic checks; their media reproduce unchanged. The
complete browser run also passed real Canvas/AudioWorklet delivery, 552,960,000-byte
media, CD-DA controls and the guest player. Earlier complete
browser runs were superseded during final review by the final-artifact run after
gating the extra dot deadlines on rotation enable and isolating RBG1 storage; those interrupted runs are
not counted as completed validation. No required check is intentionally skipped.
