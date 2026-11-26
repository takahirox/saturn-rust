# Issue #254 CRT display filters

The player starts with **Clean**, the original pixelated Canvas output. The
toolbar selector works during playback and pause, including in Debug. Its
selection lasts for the page session and survives guest resets. The presets
approximate analog bandwidth and the appearance of CRT displays. Sega lists
November 22, 1994 as the Saturn's Japanese launch date, and the original manual
shows its stereo audio/video connection. The history informed the preset choices.
The coefficients are visual approximations without physical calibration.
[Sega hardware overview](https://www.sega.jp/history/hard/segasaturn/),
[Sega Saturn instruction manual](https://manuals.plus/m/0dbe18c007e91bcf970f948424e1b9fd202dbd7d7098f48d6e694ffe600151c2.pdf).

| Preset | Luma blend | Chroma blend | Maximum beam attenuation | Maximum mask attenuation |
| --- | ---: | ---: | ---: | ---: |
| CRT Composite | 44% | 72% | 13% | 2.8% |
| CRT S-Video | 19% | 30% | 12% | 3.0% |
| CRT RGB | 4% | 4% | 11% | 4.4% |

[The original MIT shader](../web/crt-display.js) makes one WebGL 2 draw with three
horizontal samples, clamped at image boundaries. It separates the luma change
from the chroma change in display RGB. High horizontal modes use a 1.5-dot
offset so the linear texture samples mix both phases of one-pixel dithering.
Vertical sampling retains the source row. The beam follows each source line
and fades in from 1.75 to 3.5 physical pixels per line. Double-density interlace
and 480-line modes suppress the beam. The shader uses the completed image of
every field with no field-parity modulation. A faint RGB triad starts above
three physical pixels per source dot and reaches its maximum at five. The
implementation uses local code and no third-party shader assets.

The focusable `#screen` still gets every original frame through Canvas 2D.
The separate WebGL canvas overlays it, follows its aspect ratio, and leaves
keyboard, pointer and debug access with `#screen`. The overlay sizes its
backing store for the viewport and device pixel ratio, subject to GPU limits and
an 8.3-megapixel budget. The page retains the latest transferred frame for paused
redraws. Clean creates no GL context or texture and uploads no frames. CRT
reuses one program and one source texture, reallocates the texture on source
size changes and skips texture uploads when redrawing a frame already on the GPU.
WebGL creation, compile and link failure leave the Canvas image visible.
Context restoration rebuilds the resources and redraws the available frame.
No presentation path changes the Worker protocol or its two-field credit bound.

## Browser and guest evidence

Run `npm run test:browser -- --group=crtDisplay` against `sh scripts/build-web.sh`
output. The [scenario](../scripts/crt-display.mjs) uses original sharp edges,
RGB bars, a checkerboard, grayscale levels, flat fields and small bitmap text.
The production shader runs through WebGL and the test reads its output with a
one-code-value tolerance for repeat draws. It samples edges, picture corners,
flat regions, glyph strokes and attenuation at 320×224, 352×240, 320×448,
640×224 and 704×480, at integer and fractional scales. At 1× and 2× in both
high-resolution modes, checkerboard contrast must be lowest for Composite,
followed by S-Video and then RGB. The original visual patterns carry the MIT
license.

At 1×, the Composite/S-Video/RGB grayscale edge spread was 35/15/3 code
values and red-to-green bleed was 61/26/4. The checkerboard contrast was
24/120/176 with 192 in the original pattern. The RGB bar centers retained
224 in the primary channel at 1× and at least 213 at 4×. Sampled flat gray at 4×
varied by 9–11 code values vertically on 224-line output and by zero on
448-line output. Repeat draws matched byte for byte on the tested renderer.

With the revised offsets, pinned Chromium produced 108/156/184 code values
of checkerboard contrast for Composite/S-Video/RGB at 1× in both
high-resolution modes. At 2× it produced 53/77/92 at 640×224 and 54/78/92
at 704×480. ANGLE Metal and SwiftShader agreed on these samples. The original
shader measured 192 for all three presets at 640×224 at 1× and failed the new
assertion on both renderers.

The byte comparison uses the original [Hardware Lab](../homebrew/hardware-lab/README.md)
ISO and [TV mode/INTBACK guest](../tests/fixtures/display-modes.hex). For each of
Clean, the three CRT presets and a switching trace, it compares 44 accepted
Worker frames and Canvas readbacks with SHA-256, 32 Hardware Lab PCM packets
with actual nonzero samples, exact peripheral clocks and Worker inspection
state, and guest-generated digital/analog INTBACK records. The guest selects
progressive, single-density and double-density modes while using the same
original code and deterministic controller values in each run. Existing native
and browser guest assertions remain in the suite.

The live player test uses Star Collector for switching while playing and paused,
composited picture checks, Worklet output, resize, DPR changes, fullscreen and
focus, Play/Debug, settings pause, reset, backup RAM and WebGL loss/restoration.
Separate runs inject WebGL context creation failure, shader compilation failure
and shader link failure and verify visible Canvas playback. The scenario checks
that requests are loopback GETs for public runtime assets. All guest files are
selected locally. The scenario passed on both ANGLE Metal and Chromium's
SwiftShader implementation. The Linux test runner can use SwiftShader.
[Chromium SwiftShader documentation](https://chromium.googlesource.com/chromium/src/+/main/docs/gpu/swiftshader.md).

## Paired performance

The comparison used the normal player at `55c16f4e` and the current player with
Clean and each CRT preset. All variants loaded the same optimized Wasm through
the same pinned Chromium 145.0.7632.6, used the original Hardware Lab mixed
disc, enabled real AudioWorklet processing, and ran the normal paced Worker
scheduler. The test host was an Apple M4 Max on macOS 27.2.1 with ANGLE Metal,
a 1280×720 browser viewport and DPR 1. The CRT backing store was 778×583 for
the 320×224 guest frames. The browser routes audio to its headless output.

Each variant used a fresh browser with a 120-field warmup and measured the
next 240 completed presentations. Five sequential rounds rotated the order
so each variant appeared in each position once. Rates and gaps use completed
presentation callbacks after 2D and optional GL submission, together with
field counts, transfer sizes and Worklet feedback. The p95 and p99 columns
are medians of the per-run percentiles, in milliseconds.

| Player | Median fields/s | Range | Gap p95 | Gap p99 | Submission p95 | Audio shortages / drops |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Main baseline | 59.83 | 59.80–59.85 | 18.40 | 19.20 | 0.20 | 0 / 0 |
| Clean | 59.83 | 59.82–59.85 | 18.30 | 19.00 | 0.10 | 0 / 0 |
| CRT Composite | 59.82 | 59.79–59.84 | 18.30 | 19.00 | 0.10 | 0 / 0 |
| CRT S-Video | 59.83 | 59.78–59.84 | 18.30 | 18.90 | 0.10 | 0 / 0 |
| CRT RGB | 59.82 | 59.78–59.83 | 18.40 | 19.20 | 0.10 | 0 / 0 |

Every window had 240 Worker frames and 240 Canvas writes, with at most two
frames awaiting acknowledgment. All variants transferred 68,812,800 frame
bytes per window from the Worker. Each CRT preset added exactly 240 draws and
240 texture uploads for those same 68,812,800 bytes, with zero steady-state
texture allocations. The baseline and Clean had zero WebGL draws and uploads.
The AudioWorklet reported 176,128–178,176 rendered stereo frames per window.
The mixed disc scheduled zero predictive Sound Worker jobs in every variant.

Clean throughput and gap tails stayed within baseline variation. CRT also
sustained the same paced workload with comparable gap tails on this host.
Submission p95 is main-thread time at Chromium timer precision. The measured
path ends at frame submission and Worklet processing. Panel scan-out, isolated
GPU time and capacity on lower-power devices were outside this comparison.

The browser harness uses the existing player entry points. The baseline web
assets can be extracted with `git archive 55c16f4e web`. For the baseline use
`--web=<extracted>/web --pkg=web/pkg` in place of the filter argument. Rotate
the baseline, Clean, Composite, S-Video and RGB across the five positions.

```sh
node scripts/benchmark-player.mjs --gpu --display-filter=composite \
  --disc=web/hardware-lab.cue --track=web/hardware-lab.iso \
  --track=web/hardware-lab-audio.bin --min-colors=5 --warmup=120 --fields=240 --runs=1
```

## Regression and merge gate

| Local check | Result |
| --- | --- |
| `cargo fmt --check`, `cargo build --locked` | Passed |
| `cargo test --locked --all-features` | 1,518 tests passed |
| Native and `wasm32-unknown-unknown` Clippy with `--all-features -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | All 106 fixture checks passed |
| `sh scripts/build-web.sh` | Passed with Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127. The profile-guided tail-call build emitted profile counter/hash warnings. |
| Runner, playback audio and pacing tests | 26 passed |
| Focused `crtDisplay` browser group | Passed with pinned Chromium on both ANGLE Metal and SwiftShader |
| Complete `npm run test:browser` | All 75 groups passed in pinned Chromium, with 32 shared and 43 scenarios. Coverage and tested-artifact verification passed. Suite wall time was 364.434 s. |

The optimized and compatible Wasm SHA-256 values used for the browser checks were
`74adfb8173242af4e65ab09a370566549788c5d6ded68d014deb852b1e42b864` and
`af5cd6d3e714ae32aaef8b861695f37c55512332a87142f1bef872fa676c517d`.

Exact-head GitHub PR CI is pending. It and the required review must pass
before merge. There is no required post-merge verification.
