# Issue #57 validation

Related to #57 and #17. This corrects horizontal source selection on existing
scaled/distorted VDP1 raster lines. Issue #17 remains open. No hardware completion,
physical bandwidth or commercial compatibility is claimed. No post-merge
verification is required. No GitHub state was changed.

## Contract and implementation

The [sampling contract](vdp1-sampling.md) provides exact sections, figure rows,
older/later primary reconciliation, ST-013-SP1 version applicability and STN-15
correction. The source Issue's examples are correct; no technical change to its
acceptance criteria was needed. Its optional permission to correct the Issue
was not exercised, consistently with this execution's prohibition on GitHub
writes/comments.

[`Vdp1::texture_coordinates`](../src/vdp1.rs) now selects reduced source-bin
centers and distributes enlarged repeats between source endpoints. HSS forms a
half-width parity source before rescaling; it does not round ordinary samples
to parity. This integer generalization matches every illustrated ratio. Command
flips retain original EOS parity; geometry reversal and pre-clipping traversal
reversal retain their separate meanings. Drawing/texture addressing consume the
same selection through retained clocked transactions. Destination geometry,
vertical interpolation, shading, publication, command processing, IRQ and reset
mechanisms remain intact. No mutable sampler state was added.

The manual illustrates ratios rather than specifying a universal accumulator
circuit. Unillustrated ratios use the documented inferred extension, tested at
additional legal widths. No physical hardware measurement is claimed. Full
source walking, skipped-texel end-code acquisition/bandwidth, vertical scaling
and polygon coverage remain #17 deviations; this change preserves the existing
sample-acquisition end-code processor and the explicit all-magnification HSS rule.

## Independent device and transaction checks

[Framebuffer tests](../tests/vdp1_sampling.rs) include all 24 literal figure rows
(across three HSS/EOS panels), EOS independence with HSS=0, both destination
orders and horizontal command flips for scaled and distorted commands (256
combinations). Additional literal vectors cover source widths 16, 24 and 504,
one-pixel output and 2× enlargement. Distorted rows with destination spans 3/4/5
and distinct source rows verify per-line selection and vertical flip.

All six texture formats use authored words and independent LUT/RGB values,
including nibble/byte boundaries at 8→3, 8→4 and 8→10. Further cases preserve
transparency, ECD/HSS behavior at reduced/unchanged/enlarged sizes, mesh, Gouraud,
pre-clipping reversal and user clipping without changing phase. The historical
`tests/video.rs` ordinary 8→4 expectation changes from `[0,2,4,7]` to Figure
6.5 row10's `[1,3,5,7]`; all parity/end-code/register assertions remain.

[Transaction tests](../src/vdp1/sampling_tests.rs) run 72 format/boundary/chunk
cases at chunks 1/3/17/10000. VRAM ownership stalls acquisition, framebuffer
ownership permits acquisition but blocks publication, and writes on either side
of the two-clock completion distinguish fresh and retained values. The first
and second reduced nibble samples share a retained physical word despite writes.
Passive register/VRAM/framebuffer/display inspection leaves progress unchanged.
A further 36 cancellation cases cover ENDR, PTMR restart and actual Bus system
reset during partial acquisition, retained acquisition and partial publication;
no stale write or repeated completion occurs, and fresh commands restart at the
correct sample. These are inherited transaction timing conventions, distinct
from the specification-established sample vectors. Existing table, progress,
effects, clipping, erase, interrupt and bus tests are retained.

## Original guest, controls and hashes

The extended [builder](../tests/fixtures/build_sprite_shrink.py),
[SH-2 source](../tests/fixtures/sprite-shrink.s),
[ROM encoding](../tests/fixtures/sprite-shrink.hex) and
[literal records](../tests/fixtures/sprite-shrink.tsv) retain the adjacent
[MIT license](../tests/fixtures/LICENSE). It writes eight distinct source colors
and scaled/distorted commands through guest instructions/MMIO, clears the target
framebuffer region, waits for framebuffer-control capture, polls draw completion,
reads 53 actual framebuffer samples, accumulates their checksum in SH-2 r9,
records MODR/checksum/completion and publishes a final ordinary 8→4 row. There
are 56 result longs at `0600D000`, plus a completion marker at `0600DFFC`.
No host-written output, personal firmware, commercial media or SDK is used.

The changed variant adds eight to each source red component; expected records
and the guest checksum change. The omitted variant replaces the first draw
with END: four sampled records become zero, and the checksum drops by their
literal sum. Both controls still execute the guest and final distinguishing row.
A retained HSS-clear control patches only the seven guest HSS mode operands.
Its independent literal rows verify that EOS has no effect with HSS clear,
including the newly distinguishing 8→3 ordinary row.
Every variant and the HSS-clear control execute two cold boots in [native Machine](../tests/sprite_shrink.rs)
and [production Wasm](../scripts/sprite-shrink.mjs) under `/saturn-rust/`.
The final framebuffer and actual production Canvas must contain red components
`[8,24,41,57]` (changed `[74,90,107,123]`) and a black trailing canary.
Hosts supply only ROM bytes and advance/inspect execution.

| Variant | Source SHA-256 | HEX SHA-256 | Expanded 512-KiB ROM SHA-256 |
| --- | --- | --- | --- |
| Original | `fa36afab93526f8fa502d46012146e154b521309b235956589d9a65cb08de782` | `8b2a2148c8eaede9f2b168aff01ee1615c77ed9d35be5072b7564313c778fb1d` | `c72d72e91d6ba641d484c8d29a868befadc898a955c28a8bfb8ab979da7692fd` |
| Changed texture | `2e4d68d81403077f50b85cbb3ad9520067d7a68af04575c543fc5ed655c3e86e` | `3427bbbdb8a72790703d6497d1ae80a1fcd30fa1c5962f99264810c965e35cdc` | `d42dad5cd523502438880b07de67ccd3c56d9f68dc2b53e966c75729c85a4c22` |
| Omitted first draw | `fd82d753822be316ca935acd77497f6cc413f53232b5f55c5d8463a099d39869` | `6b628a2886e67b0e885fc822bc3c8774363d16f131298a87d29000003e12fabf` | `220369d7a9986f83d028c0cfe87605b19ef7600683f7777bceff9af97ec3dd70` |

The retained HSS-clear control uses the original ROM with seven literal mode
operands changed; its expanded ROM SHA-256 is
`c8baa8e402e05e24b80cd627a00a5c2f5ef7193aa56a10ee9583e9b228a4559c`.

There is no additional disc/media artifact; the original synthetic BIOS holds
all guest bytes. The existing CI-registered builder reproduces all three variants.

## Inherited-implementation negative controls

Before changing production `src/vdp1.rs`, with the inherited sampler from
`cd887d6d86b8386ed7b0656773d920e4cd1de2cd` (unchanged by the upstream GitWeave
checkpoint commits), these newly authored assertions failed as expected:

- `cargo test --locked --test vdp1_sampling every_figure_6_5_row -- --nocapture`:
  exit 101, first scaled 3/8 row produced `[8000,8003,8007]`, expected
  `[8001,8004,8006]`.
- `cargo test --locked --test sprite_shrink`: exit 101. The first 4/8 guest
  records were `[8000,8002,8004,8007]`, expected `[8001,8003,8005,8007]`.
  The complete guest checksum was 1,736,879 instead of 1,736,894.
- Rebuilt the inherited production Wasm with `sh scripts/build-web.sh`, then
  ran the new `spriteShrink(page)` assertions using a temporary Playwright
  launcher serving exactly `web/` at `/saturn-rust/`. Node exited 1. Both cold
  boots exposed framebuffer **and actual Canvas** reds `[0,16,33,57]`, expected
  `[8,24,41,57]`, along with the same guest-record/checksum mismatches.

After the correction, the native tests and the identical production-Wasm guest
assertions pass for original/changed/omitted variants. The negative runs are
expected failures recorded separately from the passing final validation.

## Validation results

Executed in this worktree on 2026-10-24. Native tooling: Homebrew Rust 1.94.0.
Wasm tooling: installed rustup stable Rust 1.99.0, `wasm-bindgen 0.2.129`, selected
by prefixing PATH with the stable toolchain's bin and `/Users/takahiro/.cargo/bin`.

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | **988 passed**: 629 unit + 359 integration; zero failures, ignored or filtered tests; doc tests passed (zero defined) |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| All CI-registered fixture/media reproduction commands | **90/90 passed**, including all three sprite guest variants and large-media output |
| `sh scripts/build-web.sh` | Passed; rebuilt production release Wasm and runtime assets |
| `npm ci --ignore-scripts` | Passed; zero reported vulnerabilities |
| Complete extended `npm run test:browser` | **Passed**, Chromium 145.0.7632.6 under `/saturn-rust/`; all four sampling variants, actual Canvas and existing regressions; no required skips |
| `git diff --check` | Passed |

The complete suites retained Star Collector, Prism Courier, Hardware Lab and
existing hardware regressions without required skips. Final native and browser
runs were repeated after retaining the HSS-clear guest control and both passed. All remaining #17 CPU/
peripheral, multiprocessor/bus/reset timing, SCU/DSP, video/audio, CD/media/HLE,
SMPC and integrated acceptance requirements remain mandatory. This checkpoint
does not reopen historical trackers or close the hardware umbrella.
