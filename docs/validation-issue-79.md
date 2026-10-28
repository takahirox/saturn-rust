# Issue #79 validation: mosaic density and rotation sampling

Related to #79 and #17. [The coordinate contract](vdp2-mosaic.md) was recorded
before the implementation change and links the primary Sega specifications.
It distinguishes output rows, field lines and source-picture rows; later Sega
§4.11 resolves the earlier PDF's misplaced interlace-independent note. The note
belongs to horizontal sizes; the explicit vertical table remains interlaced.
No hardware requirement or acceptance condition was removed. No Issue/PR writes,
push, merge, personal firmware, human confirmation or live Pages check occurred.

## Implementation and independent oracles

- [Normal address selection](../src/vdp2.rs): `normal_sample` converts selected
  double-density output Y to a field line before upper-left grouping and
  scroll/zoom, covering tile/bitmap and every applicable color format.
- [Line-table reads](../src/vdp2/tables.rs): selected normal image rows index
  scroll operands; other screens and window/back/line-color Y remain independent.
  Existing per-screen vertical-cell-scroll exclusion is preserved.
- [Rotation](../src/vdp2/rotation.rs): `rotation_sample` uses mosaic-selected X,
  retains horizontal-only processing and passes density selection through both
  coefficient and transform starts. `read_rotation_line` retains both progressions
  from the same line-table capture and RPRCTL request. Sprites use the original
  starts. One request consumes once at the existing event; inspection consumes none.
- [Native coordinates/addresses/pixels](../src/vdp2/mosaic_tests.rs): all sixteen
  size encodings on each axis, boundaries, both fields, NBG0–3, bitmap/tile paths,
  applicable formats, enable isolation, scroll/zoom, RBG0 A/B and RBG1, retained
  acquired words, legal grants, line-scroll indexing and field-boundary changes.
  The separate `mosaic_retention_tests` in rotation.rs cover start/KAst/line/dot
  coefficients, ignored vertical sizes, requests, field starts and reset.

The original MIT [reproducible builder](../tests/fixtures/build_mosaic.py) produces
three guests with adjacent `.s`, `.hex` and `.sha256` files under the existing
[MIT license](../tests/fixtures/LICENSE). Guests write 64 distinct source rows
using `color = 3*row + column + seed`, their own palette, names, rotation matrices
and VDP2 controls. They compute write checksums and record actual register reads
and opposite TVSTAT field observations before publishing completion. Expected
checksums are independently derived arithmetic sums. Changed source and size
controls run separately as well as together; omitted enable leaves display on.

[Native Machine](../tests/mosaic.rs) and [production browser Wasm](../scripts/mosaic.mjs)
execute the same ROM bytes, single and double density, minimum and larger sizes,
A/B, RBG1 and repeated cold boots. Both compare full literal framebuffer pixels
on consecutive fields; browser also checks every actual Canvas pixel and the
original ROM hashes. Normal NBG1 remains independently double-density while
NBG0 uses mosaic. Native chunk sizes 1/3/17/10000 with inspection off/on produce
identical guest records and images. Only ROM operand words change before reset;
no host injects running guest state, patterns, progress or pixels. Framebuffer
height remains 448 in double density and 224 in single density. Bitmap wrapping
at source row 256 is included in the independently authored expectations.

## Inherited-density mutation failure

A reversible mutation restored the inherited normal mapping by making
`mosaic_y` retain output Y, and restored rotation density by supplying the
unmodified starts/coefficients while preserving horizontal quantization.
The source files were restored byte-for-byte in a `finally` block after testing.

- `cargo test --locked --test mosaic mosaic_guest_fields_pixels_controls_and_repeated_cold_boots -- --nocapture`
  failed at normal TVMD=80C0, seed=1, MZCTL=1, pixel (0,1): inherited
  `[33,0,0,255]`, expected `[8,0,0,255]` (source row 1 instead of row 0).
- `cargo test --locked --lib rotation_sizes_parameters_fields_and_independent_rbg1_density_have_literal_pixels -- --nocapture`
  failed its independent address oracle: inherited 131104 (`20020`), expected
  131088 (`20010`). The image sampled row 2 instead of field-line row 1.

Further guest rows enforce source progression: output row 4 of selected minimum
mosaic must read source row 2, not source row 4. Copying an adjacent output row
cannot satisfy the oracle. Restored-source checks pass as recorded below.

## Required checks

All required checks passed after the changes. Commands use the documented rustup stable
compiler/Cargo PATH (Rust 1.99.0), installed Wasm target and matching
wasm-bindgen 0.2.129 CLI. Node is 26.12.1 and Playwright 1.58.2. Browser
runs the complete registered aggregate, including Star Collector, Prism Courier
and Hardware Lab; no scenario or assertion is skipped.

| Check | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed, 1,102 tests, zero failures or ignored tests |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | Passed, 98 registered commands including original mosaic guests |
| `sh scripts/build-web.sh` | Passed |
| `node --test scripts/ci-runner.test.mjs` | Passed, 12/12, zero skips; inventory includes 68 browser groups |
| `npm run test:browser` | Passed, complete 68-group aggregate: 38 scenarios and 30 shared groups, zero skips |
| Inherited-density mutations | Both independent oracles failed for the intended missing mapping |
| Relative links, JavaScript syntax and Git whitespace | Passed |

The full browser aggregate completed in 228 seconds (scenarios 228 seconds,
shared 199 seconds); its expanded mosaic scenario passed in 36 seconds.
Star Collector, Prism Courier and Hardware Lab passed their existing checks.

The fixture's first setup incorrectly left the back-screen pointer at source
VRAM; giving it a dedicated zero word restored the intended black background
without relaxing the pixel oracle. Independent expectations include documented
256-row bitmap wrapping. No inherited regression expectation was weakened. The first complete native run
passed 1,101 tests; after adding the final applicable-color-decoder vector, the
complete suite was rerun and passed all 1,102. The first native Clippy run
flagged two ambiguous-precedence expressions in new test setup; explicit
parentheses fixed both, and warnings-denied Clippy passed.

## Remaining umbrella scope

This functional coordinate correction does not complete #17 or demonstrate
commercial compatibility. CPU/peripheral coverage; multiprocessor/cache/bus/reset/
DMA/interrupt interactions; SCU/DSP; video/audio/CD timing; BIOS/HLE/SMPC; and
integrated acceptance remain open. Physical mosaic/table/character acquisition
strobes, active-line switching phases and half-line raster timing remain outside
this increment. Historical horizontal-only mosaic subset claims are distinguished
from current evidence in [the conformance matrix](hardware-conformance.md).
Do not reopen administratively closed #11/#13/#15. Required post-merge
verification: none.
