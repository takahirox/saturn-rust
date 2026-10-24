# Issue #53 validation

Related to #53 and #17. This addresses standard-TV high-resolution rotation
picture sampling; it remains explicitly partial under #17. No hardware-completion,
physical fetch-phase, genuine BIOS or commercial-game compatibility claim is made.
No post-merge verification is required. No GitHub state was changed.

## Contract and implementation

The [coordinate/transaction contract](vdp2-rotation-fetch.md#high-resolution-picture-coordinates-53)
links the exact 1997 Japanese/English §2.1, §§6.1/6.3/6.4, §8.1 table8.1,
manual correction history and STN-13/22/24/37 references inspected for this work.
The final manual and STN-24 agree on rotation picture resolution. Amended name
locations, window bit layouts, VDP1 combinations and high-resolution format
restrictions remain applicable; none establishes a physical rotation fetch phase.
No technical correction or weakening of Issue #53's acceptance was needed.

[The shared sampler](../src/vdp2/rotation.rs) supplies
`floor(output_x/2)` to matrix and coefficient horizontal equations only in
standard-TV HRESO=010/011. RBG0 A/B tile/bitmap and RBG1's B tile path use it in
scheduled address generation and debugger decoding. Parameter/layer windows keep
output X. Normal-background and VDP1 sampling, color/palette interpretation and
composition priorities remain independent. Image operands still acquire at each
output-dot edge: two main clocks in high resolution, four in normal. Pairing does
not introduce a fetch cache, shared retained operand or framebuffer resizer.
Coefficient bus timing, physical subaccess packing/latency, half-line interlace
and all unrelated hardware completion requirements remain outstanding under #17.

## Independent vectors and inherited-behavior negative control

[Pixel/address vectors](../src/vdp2/rotation_fetch_tests.rs) author a 512-dot
source row and palette directly, then check every output dot and acquisition
address over the 320→640 and 352→704 mode pairs. They cover four legal color
formats, one/two-word names, 8/16-dot characters, RBG0 A/B tile/bitmap and RBG1's
fixed banks. Further vectors check source/map/bitmap boundaries, signed negative
half-step and positive 1.5-step image increments, positive/negative half-step
coefficient indices, both RBG0 coefficient sets, and CRAM coefficients with the
RBG1-ignored MSB set. Expected source indices follow the authored equations;
no production address/color helpers or emulator captures supply expectations.

Parameter and layer windows end at output2, switching at output3 inside picture
pair1. A separate composition vector checks distinct per-dot NBG0 and VDP1
colors and changes their priorities, exposing accidental global X division.
Existing normal-resolution, RGB888-prohibition and dynamic high-resolution
retention tests retain their original regression purposes and expectations.

An actual inherited-behavior negative control temporarily replaced the single
coordinate mapping with `image_x = x`, restoring the inherited horizontal input
to all three matrix/coefficient uses while retaining the new tests. Running
`cargo test --locked high_resolution_ --lib` exited 101: four new regressions
failed (full spans, signed/fractional/coefficient progression, odd windows and
composition). The separate native guest command
`cargo test --locked --test rotation_high_resolution guest_high_resolution_rotation_full_images_mutations_and_cold_boots`
also exited 101: tile mode 8002, output `(1,0)` was `[16,0,0,255]`, expected
`[8,0,0,255]`. Thus even the second static output dot distinguishes the inherited
behavior. The mutation was restored before positive full-suite validation. The
later source/map-boundary vector additionally passes with the correction.

[Board vectors](../src/bus/vdp2_tests.rs) publish through both SH-2 adapters
before/equal/after the first paired dot's edge. At equal acquisition/publication,
dot0 remains red and dot1 becomes green two main clocks later, although both
address picture0. Chunks 1/3/17/10000 and repeated passive debugger previews give
identical traces/pixels; reset cancels the queued write and retained state.
Separate high-resolution vectors remove the bank at dot1, restore it, mutate
memory and preview: dot0 retains its original color and dot1 remains absent.
An entirely unscheduled dot also stays absent, and a fresh field reacquires new
memory without reusing the old field's operands.

## Original guest and production browser evidence

The original MIT [fixture builder](../tests/fixtures/build_rotation_high_resolution.py)
uses the repository assembler and adjacent [license](../tests/fixtures/LICENSE).
Four guests create tile, RGB555 bitmap, fixed-bank RBG1 and horizontal-coefficient
images through SH-2 instructions/MMIO. They issue and await SMPC CKCHG320/352
before setting up video. All names, image words, palettes, A/B matrices,
translations and Xp coefficients come from guest execution. Host code changes
only original ROM input operands, resets and advances the machine.

The source pattern is color/index `source_x + seed`; coefficients are `Xp[i]=i`
with delta_KAx=1 and the guest-supplied table start. The guest adds all 512 image
words, reads the first/last word and programmed parameter/control operands, then
writes its completion record. Literal `.tsv` defaults and expanded-ROM hashes
are reproduced without an emulator:

| Guest | Expanded 512 KiB ROM SHA-256 |
| --- | --- |
| tile | `d71e323703a4d4102eb24eca9563eafd6c4506db9db1c6afdfe53029a463cd2e` |
| bitmap | `2d294d54cde337762e14e247a0427dd1cd8496736cacf17a7820f700e44490f5` |
| RBG1 | `33b1cc98a5972f218eab8d978a5610e2a502d7a11ac9212f43adf518586398e3` |
| coefficient | `a1707185fe6df04c26c84db9dc2050f6bca905cf4b0fffb8138d13e6201746a5` |

[Native Machine tests](../tests/rotation_high_resolution.rs) check all 640×224
or 704×224 pixels and all eight guest records for baseline, changed seed plus
translation/table-start and RBG0 parameter-B selection, and omitted-enable cases,
with two cold boots each (48 runs). Another 16 native runs compare complete
records/images across execution chunks 1/3/17/10000.
[The browser extension](../scripts/rotation-high-resolution.mjs) runs the identical
fixture bytes for the same 48 image/control/cold-boot cases in production Wasm.
It checks actual Canvas dimensions, every framebuffer/Canvas pixel and every
guest record under `/saturn-rust/`. No host results or rendered pixels are injected.
The builder is registered in the existing Pages workflow and setup commands;
new diagnostic content does not change public media or runtime assets.

## Executed checks

Environment: macOS arm64, rustup stable Rust 1.99.0, wasm-bindgen 0.2.129,
Node 24.12.0, Playwright 1.58.2 with Google Chrome 156.0.8078.48.

- Passed `cargo fmt --check` and `cargo build --locked`.
- Passed full `cargo test --locked`: 967 tests (615 library and 352 integration),
  zero failures and zero ignored tests.
- Passed `cargo clippy --locked --all-targets -- -D warnings`.
- Passed `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`.
- Passed all 89 fixture/media `--check` commands registered in
  `.github/workflows/pages.yml`, including the four new ROMs and existing
  demonstration/source/media checks.
- Passed `sh scripts/build-web.sh` and `npm ci --ignore-scripts`.
- Passed complete extended `BROWSER_CHANNEL=chrome npm run test:browser`
  (Chrome 156.0.8078.48), exit 0, with no required skips. This includes the new
  48 high-resolution cases and existing normal rotation, reload, sprite rotation,
  video-write ordering, Star Collector, Prism Courier and Hardware Lab checks;
  actual Fullscreen entry/exit, Canvas and AudioWorklet paths also passed.
- Passed JavaScript syntax checks, local documentation links/anchors and
  `git diff --cached --check` (including new files).

All unmet #17 CPU/peripheral, multiprocessor/bus/reset/timing, SCU/DSP,
video/audio, CD/media/HLE, SMPC and integrated-validation requirements remain
mandatory. Historical Issues #11/#13/#15 remain closed; #17 remains open.
