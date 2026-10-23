# One-shot rotation parameter request validation

Related to #35 and #17. This increment implements RPRCTL request consumption.
Issue #17 remains incomplete; Issues #11/#13/#15 remain administratively closed.
Commercial-game compatibility remains unverified. No required post-merge checks.

## Primary contract and implementation

[Sega ST-058-R2](https://antime.kapsi.fi/sega/files/ST-058-R2-060194.pdf),
§6.3 printed pp.157–158, and the
[later HTML manual's RPRCTL description and final paragraph](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p06_31.htm)
specify six independent requests: A Xst/Yst/KAst at bits 0/1/2 and B at bits 8/9/10.
Each request clears with the next rotation-parameter read; repeated reloads
require repeated writes. The register is write-only. The
[start-coordinate equations](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p06_30.htm)
retain independent origins and accumulate current vertical deltas between reads.
§6.4 governs coefficient addressing; §6.3 printed p.159 links A starts/deltas to
rotated sprite framebuffer readout without its matrix or coefficients.

[`read_rotation_line`](../src/vdp2/rotation.rs) clears only the six request bits
at the existing raster parameter latch, including field initialization and the
initial scanout fallback. Both rotation backgrounds and sprites use that latch.
Writes, debugger rendering and additional dot sampling of an already latched
line leave later requests pending. Signed accumulator widths, first-line
initialization, field parity and reset behavior are preserved. Reserved-bit
validation remains in place. General register-read semantics are outside scope;
no guest relies on RPRCTL readback.

[`Bus::advance_main_clocks`](../src/bus.rs) preserves raster events before
external transaction completion at equal clock edges. Thus a request completing
one clock before a line read affects that line; equal-edge and one-clock-later
completions affect the following line. This is an emulator ordering convention,
not measured electrical timing or a claim of full rotation VRAM fetch timing.

## Independent regression evidence

[Rotation unit tests](../src/vdp2/rotation.rs) cover the literal
10 → 100 → 101 Xst sequence, initial fallback, all 64 A/B request combinations
(including each individual bit), several consecutive lines, changed table
operands, different A/B deltas and subsequent delta changes, rearming after
consumption, repeated current-line rendering, signed X/Y and unsigned KAst
wrap, first-line requests, both double-interlace field parities and reset.
Expected starts use authored integer sequences rather than production helpers.
The older independence test's explicit host clear was removed.

The scheduled-bus test uses both real SH-2 replay adapters, buffered writes,
physical completion before/at/after the line boundary, chunks 1/3/17/10000, with
and without intervening debugger inspection. It asserts A/B starts and RGB
pixels. The adapter's physical write duration is probed away from a raster edge
so acceptance cannot be confused with publication. Reset cases queue a request
or accept it before reset, then initialize a fresh running origin and change its
table; a stale write would visibly reload the changed origin and fail.

The new [original MIT builder](../tests/fixtures/build_rotation_reload.py),
[SH-2 A source](../tests/fixtures/rotation-reload-0.s),
[B source](../tests/fixtures/rotation-reload-1.s),
[ROM bytes](../tests/fixtures/rotation-reload-0.hex) and
[literal records](../tests/fixtures/rotation-reload-0.tsv) use the
[fixture license](../tests/fixtures/LICENSE). Each guest constructs a RGB555
gradient, coefficient table and rotation table using SH-2 stores. Parameter A
and B are displayed through RBG0's respective parameter modes. The guest polls
real beam counters and performs these operations:

- Row 64: change Xst/Yst/KAst table operands without a request; running origins
  continue. The first-field X delta comes from ROM 100 and is mutated from 1 to 2.
- Row 127: request all three starts once, affecting row 128. Row 128 changes all
  table operands again without another request; row 129 continues accumulation.
- Row 159: change vertical X and KAst deltas without reloading either start.
- Rows 191/192/193: reissue requests with different X/Y/KAst operands, taking
  effect on rows 192/193/194; row 195 resumes accumulation.
- VBlank: read back six actual table operands, compute three sums with SH-2 ADD,
  and write a completion record. RPRCTL is never read.

Short mode3 coefficients supply X translation of `KAst index modulo 4`, making
coefficient address progression independently visible. The guest computes the
gradient with arithmetic/stores; the host supplies neither results nor pixels.
[Native integration](../tests/rotation_reload.rs) checks all 71,680 pixels per
execution and all ten guest records, for both parameter variants and cold boots
with deltas 1/1/2. The [browser extension](../scripts/rotation-reload.mjs),
registered in the [complete smoke suite](../scripts/browser-smoke.mjs), runs the
same bytes through production Wasm under `/saturn-rust/`, checking every
framebuffer and actual production Canvas pixel plus all guest records.

The historical [sprite guest](../tests/fixtures/build_sprite_rotation.py)
incorrectly expected sticky requests. Its corrected sequence reloads X=128 on
row 128 (then127 on129), and X=0/Y=128 with vertical deltas 1/1 on row 192.
[Native](../tests/sprite_rotation.rs) and browser expectations now independently
map the four guest-drawn polygons through those progressing coordinates. Both
8/16-bit paths, all native pixels, source-corner records, horizontal-delta ROM
mutation, no-request table writes, cold boots and historical-field checks remain.
Browser boundary samples add rows 129/130/193/194. No assertions were removed to
accommodate the fix. This correction follows §6.3 pp.157–159 and the HTML final
paragraph above; no correction to Issue #35's technical contract was necessary.

## Actual negative controls

Before implementing consumption, the new sequence and corrected guest tests
failed against the unchanged rotation implementation from `859cc4d`. After
finalizing the tests, the literal sequence and guest negative controls were
repeated by temporarily removing only the two consuming byte masks, restoring
the unchanged production behavior; the fixed source was restored afterward:

```text
cargo test --locked --lib single_x_request
FAILED: single_x_request_10_100_101_including_initial_scanout_fallback
line2: actual A Xst=6553600 (100 << 16)
expected A Xst=6619136 (101 << 16)

cargo test --locked --test sprite_rotation
FAILED: (0,129) changed=false
actual [0,255,0,255]; expected [255,0,0,255]
```

Both returned exit101. The first detects a repeated reload without any table
mutation or host clear; the second detects it through guest-driven pixels.
Both pass with consumption enabled. These were actual failures, not inferred
negative-control outcomes.

## Reproduction and check results

```sh
python3 tests/fixtures/build_rotation_reload.py --check
python3 tests/fixtures/build_sprite_rotation.py --check
cargo test --locked --lib vdp2::rotation
cargo test --locked --test rotation_reload --test sprite_rotation
cargo fmt --check
cargo build --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings
sh scripts/build-web.sh
npm run test:browser
```

Validation on 2026-10-24:

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Pass |
| `cargo build --locked` | Pass |
| `cargo test --locked` | **858 passed**, 0 failed, 0 ignored, including all three demonstration regressions |
| Native Clippy, all targets, warnings denied | Pass |
| Wasm Clippy, warnings denied | Pass |
| All registered fixture/media reproduction commands in [CI](../.github/workflows/pages.yml) | **80 commands passed**, including the new builder and large-media checks |
| `sh scripts/build-web.sh` | Pass, using the documented rustup toolchain |
| Complete extended `npm run test:browser` under `/saturn-rust/` | Pass in Chromium **145.0.7632.6**, including extended rotation records/framebuffer/Canvas checks and Star Collector, Prism Courier and Hardware Lab |
| Documentation links, recorded hashes and Git whitespace | Pass |

An initial web-build attempt selected the Homebrew compiler without its Wasm
standard library; selecting the installed rustup compiler and Cargo together as
in [setup](setup.md) resolved this environment mismatch. No required checks were
skipped. The final native run took 222.69 seconds; actual negative controls are
recorded separately above and are not included in the passing test count.

## Fixture hashes

SHA-256 of committed text artifacts; builders print each source/vector hash too:

| Artifact | SHA-256 |
| --- | --- |
| `rotation-reload-0.hex` | `285e6978f9b492e8e6e3d98b75a9a0fe1df6079c1f0c3e7b4855059ad18bfd5c` |
| `rotation-reload-1.hex` | `745a1cbf0dccbf128511864cd099d5a13fd36703fedf158bc267e2ceba270ac0` |
| Both `rotation-reload-*.tsv` | `0480eec1f3650982f8b1fb52e1b22f26093b020e8a5e83b627fe9d777f3c1114` |
| `sprite-rotation16.hex` | `77d5f055037cb4ad6f90ae967497eed54178e8fe577fdec1c3aa710b0013e599` |
| `sprite-rotation8.hex` | `553b8c1a1c3571bcf5f5a45894b405f8fbd32341cc0b6b68369f8b2f6b36da4f` |
| `sprite-rotation16.tsv` (unchanged) | `81e30f69e8093bc349c452dbf75c6b2ab7504ab389b8369537a32a8033522748` |
| `sprite-rotation8.tsv` (unchanged) | `946db30774561bdcea9ddd6c93766c7dafd21cfc8b9542e459efd49c77e94b25` |

Star Collector, Prism Courier and Hardware Lab media are unchanged and remain
required regressions. Full rotation VRAM acquisition timing, general VDP2
register-read behavior, remaining CPU/peripheral, multiprocessor/bus/reset,
SCU/DSP, video/audio, CD/media/HLE, SMPC and comprehensive integration requirements
of #17 remain outstanding. This increment does not establish hardware completion.
