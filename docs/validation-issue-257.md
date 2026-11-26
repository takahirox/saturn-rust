# Issue #257 player and touch controller validation

The desktop and touch player share the same Canvas, Worker and media state. The
sibling references inform the compact header, media slots, grouped playback
controls and display-centered fullscreen. Saturn keeps its existing tab-local
media policy. [SNES HTML](https://github.com/takahirox/snes-rust/blob/main/web/index.html),
[SNES CSS](https://github.com/takahirox/snes-rust/blob/main/web/style.css),
[WonderSwan HTML](https://github.com/takahirox/wonderswan-rust/blob/main/web/index.html),
[WonderSwan CSS](https://github.com/takahirox/wonderswan-rust/blob/main/web/src/style.css)

## Browser evidence

The expanded `ui-bios-and-exceptions` group loads the production page with
original MIT firmware and disc fixtures. It exercises the actual keyboard and
touch file-picker actions, visible selection and error states, Play/Pause/Resume
and Reset, Help focus return, and the existing CPU/backup interfaces. BIOS/disc
selection and completion order checks cover cancelled/rejected reads, pending
BIOS blocking, explicit clear/reselection, and a disc handoff while a newer BIOS
reset supersedes an earlier reset. Existing ISO, CHD, CUE/BINARY, drag/drop,
offline and large-media guests exercise the underlying format boundaries. The
tests assert same-origin static GET requests and empty browser storage.

The table lists the tested Chromium CSS viewports.

| Pointer | CSS viewport | DPR | Presentation |
| --- | --- | --- | --- |
| Fine | 1280×720, 1440×900 | 1 | Media beside the screen, touch controller hidden |
| Coarse | 320×568, 390×844 | 3 | Screen above the controller, media below the toolbar |
| Coarse | 768×1024 | 3 | Tablet portrait |
| Coarse | 568×320, 844×390 | 3 | Direction and face clusters flank the screen |
| Fine | CRT player and differential contexts | 2 | Live/paused CRT redraw, fullscreen and WebGL fallback |

Tests check aspect ratio, exact screen/toolbar ordering, the maximum image that
fits the fullscreen well, exit focus, no horizontal overflow, and disjoint
touch targets of at least 44×44 CSS pixels with hit testing. Phone portrait
and landscape fullscreen include all thirteen controls. The tests check
rotation from 320×568 to 568×320 inline and in fullscreen. They also hold a
button while rotating inside fullscreen and verify release through the guest.
Landscape viewports under 560 CSS pixels use the stacked controller layout.
Browser fullscreen requires support for the element Fullscreen API. Very short
windows can scroll to reach all controls.

The touch checks drive Chromium CDP input through the production pointer
handlers. The MIT digital-input guest must complete two fresh SMPC INTBACK
reports for each assertion. Tests cover all thirteen buttons, diagonals,
sliding outside and back, independent release across three pointers, cancel,
lost capture, and keyboard/gamepad union. The 3D-pad guest report also checks
full digital shoulder travel overlays without affecting physical trigger
hysteresis. Separate checks retain both physical controller ports and keyboard
mapping tests in HLE and external BIOS mode. Focus loss, hiding, Pause, Help,
Advanced, Debug, Reset, media replacement, fullscreen and rotation release
touch state. An explicit Resume with Advanced or Debug still open accepts
fresh touch input, verified through the guest's SMPC report.

| Source | Directions | A/B/C | X/Y/Z | L/R | Start |
| --- | --- | --- | --- | --- | --- |
| Player 1 touch | D-pad and diagonals | Lower row | Upper row | Shoulder buttons | Start |
| Player 1 keyboard | Arrow keys | Z/X/C | A/S/D | Q/W | Enter |
| Player 2 keyboard | I/K/J/L (up/down/left/right) | U/O/P | — | — | Right Shift |
| Standard gamepad, either port | 12/13/14/15 | 0/1/2 | 3/4/5 | 7/6 | 9 |

The Prism Courier guest provides real stereo PCM for the production AudioWorklet
test. The first Play gesture reaches AudioContext activation with user
activation set. A deliberately rejected resume leaves the guest playing and
allows Enable audio to recover. Analyser taps before and after the output gain
confirm a nonzero source and proportional output at 0, 25, 60 and 100 percent.
Keyboard mute/unmute restores 37 percent, Pause clears pending playback, and
Reset retains volume while starting a new playback epoch. Existing pitch,
FIFO/backpressure and exact PCM tests remain in the suite.
Chromium accessibility tree checks cover media and player feedback while the
live regions are empty, and verify media and audio messages as they update.

## Clean playback comparison

The baseline frontend is from `194eb5c`. Both frontends use the same production
Wasm artifacts, original Prism Courier ISO, Clean filter and normal paced
playback. On macOS arm64 with Chromium 145.0.7632.6, each fresh browser warmed up
for 90 fields and measured two consecutive 120-field windows. Runs alternated
in the order baseline, player, player, baseline. Other local browser validation
was running on the same host.

| Frontend | Combined fields/s by run | Presentation gap p95 | Presentation gap p99 | Audio frames dropped |
| --- | --- | --- | --- | --- |
| Baseline | 59.853, 59.841 | 18.0–18.2 ms | 18.3–19.3 ms | 0 |
| Player | 59.828, 59.818 | 17.9–18.6 ms | 18.6–20.6 ms | 0 |

All 960 measured fields were presented. Both versions peaked at two pending
frames and a Worklet PCM peak of 0.25. Frame submission p95 was 0.1–0.2 ms for the
baseline and 0.1 ms for the player. The slowest player throughput was within
0.06% of the fastest baseline run. Gap p95 ranges overlapped, and the largest
observed p99 increased by 1.3 ms. This comparison uses normal pacing on a
shared host.

Reproduce with the existing benchmark runner, pointing `--web` to either
frontend and `--pkg` to the same production build for both.

```sh
node scripts/benchmark-player.mjs --web=web --pkg=web/pkg \
  --disc=web/prism-courier.iso --display-filter=clean --warmup=90 --fields=120 --runs=2
```

## Local validation and merge gate

- Rust 1.100.0 formatting, locked native build, all-feature tests and
  all-target/all-feature Clippy passed. The native test run passed 1,519 tests
  in 112 harnesses, with zero ignored.
- All 106 registered fixture reproduction commands passed.
- The production compatible and accelerated Wasm build passed with
  wasm-bindgen 0.2.129 and Binaryen 127. All-feature Wasm Clippy passed.
- The runner, audio and pacing test command passed 26 tests, including real
  Chromium worker failure propagation and artifact verification.
- The complete `npm run test:browser` run passed in 403.525 seconds with
  Chromium 145.0.7632.6 and Playwright 1.58.2. All 75 groups passed (32 shared,
  43 scenarios). The runner verified full coverage and matching tested-site
  digests across both shards.
- The GitWeave node produces a local commit. Applicable GitHub checks and
  repository review are still required on the exact PR head before merge.
  No post-merge verification is required.

The controller covers Player 1 digital input. The browser evidence uses
emulated devices and a software audio graph. Firmware menus and commercial
game compatibility remain bounded by the emulator’s documented
[hardware conformance](hardware-conformance.md).
