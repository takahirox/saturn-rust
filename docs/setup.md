# Build, run and test

Use a Rust stable toolchain managed by rustup, its `wasm32-unknown-unknown`
target, and Python 3.10+ for building the homebrew disc and static serving. Node.js 18.17+ and npm are needed only for
the automated browser check. There is no application backend or JavaScript bundle
step. Run commands from the repository root.

## Install build tools

```sh
rustup toolchain install stable --profile minimal --component rustfmt --component clippy
rustup target add --toolchain stable wasm32-unknown-unknown
export PATH="$(rustup run stable rustc --print sysroot)/bin:$HOME/.cargo/bin:$PATH"
cargo install wasm-bindgen-cli --version 0.2.129 --locked
```

The PATH line selects rustup's compiler and Cargo together, including on systems
that also have Homebrew Rust. The CLI must match the crate version pinned in
`Cargo.toml`. Cargo and npm lockfiles are committed; no emulator-core dependency
is used. Initial tool/dependency installation needs network access.

## Native checks

```sh
cargo fmt --check
cargo build --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

The test profile uses optimization level 1 with debug assertions and overflow
checks explicitly enabled. Hardware guests still execute every emulated clock;
optimization keeps the complete native integration suite practical in CI.

The Rust library is browser-independent. Tests cover instructions/flags/delay
slots, synthetic BIOS vectors, RAM/VRAM/MMIO, digital-pad selection, VDP2 pixels,
ISO parsing/errors and actual guest gameplay/completion/restart. The shared
original synthetic ROM also exercises guest trap/illegal/slot-illegal
handlers, RTE and bounded SLEEP in native and Wasm execution. An additional original
synthetic BIOS tests address errors from data, branch slots and odd fetches,
guest-written exception traces, RTE recovery and reset of a pending error.
The original dual-SH-2 ROM additionally tests independent register/stack state,
shared RAM, timed SSHON/SSHOFF, bidirectional FRT capture interrupts, SLEEP wake,
slave restart and NMIREQ in both native and browser Wasm execution.
An additional original SH7604 guest computes signed division, copies RAM through
on-chip DMA, handles DMA/WDT IRQs and a DMA address error, polls serial completion,
proves stale-cache/purge behavior, triggers watchdog manual reset and handles
misaligned exception stacking. It executes in native and browser Wasm tests.
An additional original SCU guest executes DSP product/sum programs, direct and
indirect DMA, timer IRQs, and reset/fault checks natively and in browser Wasm.
The interrupt/latch ROM additionally exercises SH-2 signed DIV1 sequences,
ICR auto/external vectors, SCU DMA acknowledgement and both TH/PAD/VDP2 latch
inputs. The sound family ROM checks twenty-four complete fault frames and partial
transfers, including MOVEM terminal reads and deferred instruction fetch, plus
self-modifying instruction prefetch. UNLK's A7 alias now has a source-authored
family case and frame-literal mutation, retaining all other family/fault records.
Independent native vectors cover every UNLK address register, flags, user/supervisor
stacks, first/second-word faults and separate four-clock sound-bus sampling.
Both ROMs run in native and Wasm.
The sound reset builder also reproduces two original guests: SH-2 changes the
sound PC vector after SNDON completes, and a traced 68k RESET records its saved
SR/PC and retained operand from the guest trace handler. The video transaction
builder additionally reproduces command-word retention and local-memory
ownership guests; FRT distinguishes framebuffer contention from independent
VRAM access during untextured drawing. Native and browser checks include
operand changes, missing/reordered guest-write controls and cold resets.
An original HLE service disc also executes vector/callback/mask/semaphore/priority
calls, nested IRQs, both CPU views and NMI/SYSRES retention. Native and browser
Wasm compare authored records and changed guest data/priority operands.
No proprietary files are required. See [Issue #17 coverage](hardware-conformance.md),
[current validation](validation-issue-17.md) and
[earlier SH-2 validation](validation-issue-11.md). The committed sample can be rebuilt and
verified without an
external cross compiler or SDK:

```sh
python3 homebrew/star-collector/build.py
sh scripts/check-fixtures.sh
```

This is the complete 91-command CI reproducibility registry, including all three
homebrew discs, HLE code, hardware guests and standard-size media. Individual
`python3 ... --check` commands in that script remain usable for focused checks.

## Build and serve the browser client

```sh
sh scripts/build-web.sh
cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings
python3 -m http.server 8000 --bind 127.0.0.1 --directory web
```

Open <http://127.0.0.1:8000>. The build regenerates the original licensed ISO and
its public license, then creates
`target/wasm32-unknown-unknown/release/saturn_rust.wasm` and the generated
`web/pkg/saturn_rust.js` / `saturn_rust_bg.wasm`. Generated Wasm bindings are ignored
by Git; the reproducible ISO and its license are committed. Rebuild after changing
Rust or guest source. Opening `index.html` as a `file:` URL will not work. The client needs browser support for ES modules, Wasm and the
`wasm-unsafe-eval` CSP directive; the tested browser is recorded in
[validation results](validation-issue-9.md).

1. The default **Play** view centers the native 320×224 display, scaled at 10:7
   with pixelated rendering. Click **Play Star Collector** to fetch the packaged
   MIT-licensed ISO and boot it through BIOS-less HLE. Initial guest bitmap
   clearing takes a few frames. Click the screen for keyboard focus; arrows move
   the cyan player, yellow stars are collected one at a time. Five gold blocks and a green banner
   mean success. Press **Enter / Start** to restart inside the guest, including
   after completion. Star Collector has no soundtrack. **Enable audio** starts
   WebAudio for guests using the supported direct PCM path; its status reports
   blocked, muted, paused and underflow states explicitly.
2. Use the adjacent **Pause/Resume**, **Restart** and **Fullscreen** controls.
   Restart cold-boots the selected disc and starts playback when settings are
   closed. Fullscreen targets the player shell; exit with its button or the
   browser's Escape action. Unavailable/denied requests show an explanation and
   leave the normal player usable. Focus returns to the Fullscreen button on exit.
   Losing display/window focus releases held buttons. Hiding the browser tab pauses.
3. Choose **Debug** for CPU R0–R15 and special registers, retired instructions,
   pending delay target, exception count/last vector, machine status, **Step**,
   **Run**, **Reset** and **Advance
   frame**. Frame advancement requires a ready, paused disc. Switching Play/Debug
   pauses and releases input without resetting, changing boot mode, discarding
   selected files or clearing errors. Returning to Play requires **Resume**.
   Invisible register tables are not refreshed during Play.
4. Open **Load ISO / BIOS & boot settings** with click or Enter/Space on its
   disclosure. Opening pauses and releases input; closing it does not reset or
   resume. Even if a pending sample download finishes after you dismiss settings
   or return from Debug, playback stays paused. Choose **BIOS-less HLE disc** and
   a local authorized ISO to use the same loader without fetching the sample.
   Local images load paused; close settings and use Resume to play.
   Startup supports Saturn IP area groups and ISO9660 first-read files,
   including file-unit/gap interleaving and extended-attribute offsets. Select a
   cooked ISO alone (18–450,000 sectors, at most 921,600,000 bytes), or one CUE
   and all its BINARY data/audio track files together (at most 1,058,400,000
   data/audio bytes plus a 1 MiB CUE). These are application resource bounds,
   separate from hardware recording limits. Large selections require enough
   browser memory for the local input buffer and the retained Wasm medium.
   Reset cancels pending reads; restart reuses the retained medium without a
   second upload. Bootstrap reads only metadata and the selected executable.
   See the [standard-size validation record](validation-issue-33.md). Cooked and raw Mode1,
   raw Mode2, audio, shared/separate files and track gaps/indices are supported.
   Runtime CD commands retain and read the actual medium; CD-DA passes through
   SCSP. Compressed images and generic game compatibility remain unsupported. Malformed or unsupported media
   produce errors and stop the old machine.
5. To inspect the preserved debugger fixture, select **HLE bootstrap prototype**
   in settings and choose **Debug**. Click Step to see `R0=5`, then
   Reset and Run the default 6 instructions. Expect `R0=4`, `R1=7`,
   `PC=0x0600000E`, and 6 retired instructions.
6. Step again to see a pending branch, and again to execute its NOP delay slot.
   Further bounded runs stay in that loop. This is a test/homebrew startup path,
   not BIOS service emulation.
7. In settings, choose **User-provided BIOS**. Missing input shows a size error
   and disables stepping. Select a legally obtained **raw 524288-byte** BIOS.
   Incorrect sizes are rejected; correct size validates the layout, not authenticity or region.
   Native Sega BIOS boot is incomplete; BIOS mode never switches to HLE.
   PC and R15 are loaded from the first two big-endian vector words. Step or Run
   may stop on unimplemented bus/device behavior with its PC. Illegal opcodes
   dispatch guest exception vectors; failing entry is reported explicitly.
8. **Backup RAM** in settings imports/exports a packed raw 32768-byte file. Import
   pauses playback and replaces all backup bytes without resetting CPU/media.
   Export your existing backup first if you need it. Backup survives Reset,
   Restart, mode changes and rejected boots within this tab; export before closing
   or reloading, then import to restore in a later session. No automatic browser
   persistence or network transfer occurs. This is not a save state or BIOS
   backup-file service; Star Collector does not use backup RAM. Guest programs
   can also read a second direct digital pad: I/K/J/L directions, U/O/P for A/B/C,
   Right Shift for Start. Both pads release with focus loss and pause.
9. Debug Reset restarts the selected mode. Changing mode or file performs a cold
   reset; switching modes discards the previous file selection. Run accepts a whole
   count from 1 to 10000 interleaved machine turns. Each turn attempts a master
   step and, when enabled, a slave step; exception entry and sleeping turns may
   retire no instructions. SLEEP keeps controls enabled when an implemented
   peripheral or the slave can wake the master. CPU/device faults stop the entire
   machine until reset.

Only static files, including the original MIT-licensed sample ISO, are fetched.
Local BIOS/disc bytes are read via `File.arrayBuffer()` and
copied into this tab's Wasm memory, with no upload, browser storage, analytics,
CDN or external runtime assets. They are discarded from application state on mode
change or tab close (this is not a secure memory-erasure guarantee). The fixture
works with network access disabled after the static assets have loaded; offline
reload is not supported because there is no service worker.

## Automated browser smoke test

After building `web/pkg`:

```sh
npm ci --ignore-scripts
npx playwright install chromium
node --test scripts/ci-runner.test.mjs
npm run test:browser
```

On a machine with Google Chrome already installed, skip the browser download and
use `BROWSER_CHANNEL=chrome npm run test:browser`. Linux hosts may need Playwright's
documented browser system dependencies. The script starts and stops its own
loopback static server on a free port and runs the real UI/Wasm in a headless
browser mounted at `/saturn-rust/`, matching the GitHub project page. It rejects
unexpected files or links in `web/` before upload and checks nonempty runtime
assets, HTML/CSS/JS/Wasm responses and MIME types, stylesheet loading and Wasm
initialization. It checks fixture results, controls, invalid run counts,
missing/invalid BIOS, synthetic reset vectors, visible execution faults,
reset/mode transitions, offline execution and the absence of requests carrying
BIOS/disc data. It boots the actual ISO through Wasm, drives two distinct
keyboard sequences, verifies guest-written position/score/completion/update
counters and Canvas pixel differences, completes and restarts, rejects malformed
local discs, checks pending-file/mode races and loads a local ISO offline. It uses
only original licensed guest content and no Sega firmware. It also checks the
Play-first state, keyboard view navigation and native settings disclosure,
CPU/media/diagnostic preservation, pause and held-key release, hidden register
updates, loading/view races, Play Restart and Debug frame advancement. At
the hardware boundary it also executes an original backup/port-2 guest as a
synthetic ROM and temporary local ISO, checks local backup import/export bytes,
reset retention, rejected/read-failing imports and stale-read cancellation,
second-pad input release and fresh-tab isolation. It also executes the original
dual-SH-2 ROM, checking independent CPU registers/stacks, shared guest computation,
FRT IRQ/capture results, two slave starts/stops, master NMI and a visible sticky
slave exception-entry fault. At
1440×900, 768×1024 and 390×844 it asserts native canvas dimensions, rendered 10:7
ratio, pixelated scaling, no document horizontal overflow and reachable controls
in Play, Debug and expanded settings (including a selected BIOS).

The test exercises actual Fullscreen API entry/exit and focus restoration when
the headless browser allows it, and reports the path taken. It always checks
unavailable feedback and, when the API is enabled, a deterministic rejected
request. Headless Chromium cannot establish OS fullscreen/Escape behavior or
support in Safari, Firefox or mobile browsers; responsive tests resize a desktop
browser and do not emulate touch hardware. Keyboard, not virtual touch controls,
is required. See [Issue #9 validation](validation-issue-9.md) for actual browsers
and results. Nonzero exit means failure.

## GitHub Pages

The [Browser Pages workflow](../.github/workflows/pages.yml) uses stable Rust,
the committed Cargo/npm lockfiles, matching `wasm-bindgen-cli 0.2.129`, and
`scripts/build-web.sh`. The independent `native` job checks every registered
fixture and runs formatting, build, tests and native Clippy. The test profile
uses optimization level 3 while retaining debug assertions and overflow checks.
The independent `wasm` job builds Wasm once, runs Wasm Clippy and retains the immutable
`web-build` artifact. Its bindgen CLI cache has one exact version/OS/architecture
key, no fallback keys, and the existing build always verifies CLI version 0.2.129.
A cold cache performs the original locked install. Two `browser` matrix jobs
download that same artifact:
`shared` runs the original dependent UI trace (30 groups), and `scenarios` runs
34 modules in fresh browser contexts, one at a time. Both jobs use Chromium and
all the original assertions. Runner regression tests execute once in `shared`.
The retained `build` status depends on native, Wasm and **both** browser jobs.
Its always-run first step explicitly fails on any failed, skipped or cancelled
dependency, so a skipped aggregate cannot appear as a green required check.
It then rejects missing/duplicate/failed groups or shard reports, stale revision IDs,
and a SHA-256 digest that does not match every tested artifact byte and path.
On main pushes only, `site` waits for all those jobs and uploads that same
artifact; `deploy` still checks current main. PRs cannot publish. Main runs are
serialized and superseded PR validation can be canceled.

The direct `npm run test:browser` command runs **all 64 original groups** with
at most two independent Node/Chromium process trees. Each owns its loopback
server, page state, media and backup RAM. The scenario shard creates a fresh
context for each module and closes it before the next; the shared UI trace
retains its original order and page lifecycle. No callbacks run concurrently on
a shared page. CI uses separate Ubuntu 24.04 runners for the two browser shards;
at most three validation runners overlap (native plus two browsers). Each shard
has a 20-minute failure timeout (60 minutes for raw transport), with no retries
or performance pass threshold.

```sh
# Complete parallel suite (default), with coverage/artifact aggregation:
npm run test:browser
# Complete original single-process order, useful for parity and fallback:
npm run test:browser -- --serial
# Reproduce one independent module, including all of its assertions:
npm run test:browser -- --group=scspMvol
# Shared UI groups have ordering dependencies; reproduce the whole trace:
npm run test:browser -- --shard=shared
# Run the independent modules, each in a fresh context:
npm run test:browser -- --shard=scenarios
# Original lossless/raw protocol fallback, also running the complete suite:
BROWSER_TRANSPORT=raw npm run test:browser -- --serial
# Repeat both complete modes with raw logs, CPU time and sampled tree RSS:
python3 scripts/measure-browser.py target/ci-profile/comparison 3
python3 scripts/ci-time.py native-tests cargo test --locked
```

`BROWSER_TRANSPORT=compact` remains the default lossless numeric-array transport.
Neither mode changes emulation, guest clocks, operands, expected pixels/PCM or
fixture data. `scripts/browser-groups.json` is the unchanged original inventory;
`scripts/browser-shards.json` maps every original group to exactly one shard.
Unknown selections fail. Single-group selection is available for the 34
independent modules; dependent UI groups deliberately stay in `shared`.

Timings are always printed, including on failure. `BROWSER_GROUP_START` identifies
the active group even if a worker crashes or times out; `BROWSER_TIMING` reports
group segments, `BROWSER_MANIFEST` aggregates them, and `BROWSER_TOTAL` reports
shard duration and required group count. Replicated startup is marked `setup`
in the scenario worker and is excluded from coverage, but its assertions still
run and its duration is included. `BROWSER_SUITE` measures the executed parallel
wall time. `BROWSER_OPERATIONS` separates page initialization, polling, assertion
methods and evaluate wall/browser time; these inclusive measurements must not be
summed as CPU percentages. `BROWSER_WASM` measures guest stepping, frames, reset,
media and frame/audio reads on each instrumented page. `CI_TIMING` records stage
commands and `NATIVE_TIMING` records each native harness's duration.

To reproduce the CI report barrier locally:

```sh
npm run test:browser -- --shard=shared --report=target/browser-results/shared.json
npm run test:browser -- --shard=scenarios --report=target/browser-results/scenarios.json
node scripts/browser-suite.mjs --verify target/browser-results
```

Reports are written only after successful coverage validation and cleanup, and
bind the run to its Git revision and exact `web/` content. A previous report is
removed before a worker starts. No rebuild or asset modification is allowed
between validation and aggregation. When running native fixture checks and
`build-web.sh` concurrently, use separate source checkouts as CI does: both
commands access generated homebrew media, so different Cargo target directories
alone do not isolate them. The default full command uses a new temporary
report directory so stale local results cannot satisfy it. Worker exit codes,
rejections, signals, timeouts and absent reports all fail the full command.
The Pages dependency barrier also fails closed on required job cancellation.
See [performance evidence](ci-performance.md) for actual measurements, resource
cost, remaining bottlenecks and the unexecuted hosted verification requirement.

One-time administrator setup may be required: **Settings → Pages → Build and
deployment → Source: GitHub Actions**, with Actions and deployments permitted.
The workflow does not enable Pages itself. The expected URL is
<https://takahirox.github.io/saturn-rust/>; it is not live until Pages is enabled
and the first successful deployment completes. On failure, inspect **Actions →
Browser Pages** and the `github-pages` environment, correct the failing build/test
or repository setting, then push a fix or rerun the latest `main` workflow.
The existing relative asset URLs and `wasm-unsafe-eval` CSP work at the project
prefix; no custom domain or backend is required.

Issue #17 requires no human post-merge verification. Mandatory checks run before
merge using native/Wasm tests and the headless browser against the same static
assets under `/saturn-rust/`. An optional automated deployed-site smoke check may
report publication defects after merge; it is not an approval/closure gate.
Local results do not prove a production deployment succeeded. Updating a draft
implementation PR does not publish or verify a new live revision.

The hosted build supports Star Collector and the documented synthetic diagnostic
subsets, including SCU DSP/DMA/interrupts, MC68EC000/direct SCSP PCM, SMPC
RTC/INTBACK and NTSC raster timing. The original game has no soundtrack; a
bounded AudioWorklet path plays emulated PCM when explicitly enabled. External BIOS mode retains local/private reset-vector stepping but does
not promise a Sega BIOS splash screen or game boot. No proprietary BIOS/security
code or commercial disc is distributed. See [architecture](architecture.md) and
[game license/provenance](homebrew-provenance.md).

The original sound and SMPC fixture builders use the existing original SH-2
assembler plus literal independently authored 68k encodings, with the adjacent
MIT license. Their native/browser tests assert computed stereo samples, 68k timer
IRQ/RTE, RTC/SMEM status, timed INTBACK/SCU IRQ and digital/analog packets. The
browser suite enables the real AudioWorklet via a click and checks nonzero render
instrumentation, drain/underflow, mute and suspended-context feedback. Synthetic
Gamepads exercise two independent ports and focus release. These fixtures are
diagnostics and do not replace the two required original demonstration discs.

**Play Prism Courier** starts the new original MIT-licensed VDP1+VDP2+SCSP disc.
Steer the cyan prism through the red barrier's gap to the green dock; collision
turns the banner red, delivery turns it green, and Enter/Start restarts. Enable
audio for the separately executing 68k's computed stereo tone. The source,
license, reproducible SHA-256 and guest contract are in its
[README](../homebrew/prism-courier/README.md). The native and browser suites now
test this disc's pixels, VDP1 IRQ, input, loss/win/restart, 68k computation and
nonzero stereo; the browser also verifies real Canvas and AudioWorklet delivery.
Mutating its original texture changes native and Wasm pixels. This is the first
required new demonstration; the hardware exerciser disc remains required.

When the display has focus, the first two connected Gamepads map standard buttons
0/1/2/3/4/5/6/7/9 to A/B/C/X/Y/Z/L/R/Start and buttons 12–15 to directions.
Standard-mapped Gamepads with at least two axes expose a 3D-pad ID16 report:
X/Y are bounded to the circular stick range, and buttons 7/6 supply independent
right/left analog trigger levels. The emulated controller applies the documented
trigger hysteresis. Other mappings retain the ID15 X/Y/Z joystick report. Axis
levels map from [-1,1] to [0,255]; missing Z is zero. Focus loss/pause releases
buttons, centers X/Y and sets triggers/Z to zero. Keyboard controls remain independent.


The original BSC and four-layer tile fixtures also execute in native and browser
Wasm checks. They cover BCR1 processor identification, refresh interval IRQ/RTE,
module stops, NBG0–3 pattern names/characters and actual Canvas output. The
builders above verify the committed source/hex; license and reproducible ROM
hashes are recorded in the [conformance matrix](hardware-conformance.md#additional-original-review-fixtures).

The original advanced-video and direct-controller fixtures also execute in native
and browser Wasm checks. They cover guest-created RBG0 matrix/coefficient data,
line scrolling, line-window composition and changed matrix pixels on the actual
Canvas, plus both ports' clock-delayed analog handshake/ID/data and changed axis
input. Their reproducibility commands are listed above; licenses, hashes,
implementation and limitations are in the [conformance matrix](hardware-conformance.md#original-advanced-video-and-controller-fixtures).


Hardware Lab is the original mixed-track diagnostic in
[homebrew/hardware-lab](../homebrew/hardware-lab/README.md). Select the three
`web/hardware-lab` CUE/ISO/audio files together in local disc settings. It computes
slave SH-2, SCU DSP and runtime CD/FIFO/DMA/IRQ results, starts the independent
68k sound guest, uploads/executes its SCSP effects program, mixes CD-DA through
the effects path, displays the advanced VDP2 scene and
increments backup RAM on controller-driven restart. Native and browser checks
assert those results, exact wet stereo/changed-coefficient results, changed-sector
checksums and changed-audio buffers; the
browser exercises the actual Canvas/AudioWorklet path and missing-file rejection.
Full SCSP synthesis/DSP conformance and the other mandatory Issue #17 criteria
are still required. CI also verifies the original independent HLE
slave reset trampoline; external BIOS mode never installs that trampoline.


The original slot-synthesis and dual-rotation fixtures also run natively and in
browser Wasm. The 68k programs FM, TL and amplitude/pitch LFO, with independent
stereo expectations, guest-operand mutations, internal noise/zero sources and
production AudioWorklet delivery. The SH-2 RBG1 guest writes VRAM-B0 characters,
VRAM-B1 pattern names and both matrices; tests assert dual-layer pixels, changed
parameter-B pixels and actual Canvas output. Both fixtures are original MIT
content, reproducible using the builders listed above. Timed envelopes, full
video timing and the other mandatory umbrella requirements remain pending in the
[conformance inventory](hardware-conformance.md).

The original gradation and two-way-cache fixtures run in native and browser Wasm
checks. The gradation guest writes both bitmaps, palettes and CCCTL through MMIO;
tests compare top/second-screen filtering, changed controls, repeated reset and
actual Canvas pixels. The cache guest executes from P1, verifies RAM ways0/1
cannot satisfy cache tags or receive cache-hit writes, and mutates guest source
operands. The builders listed above reproduce both original MIT-licensed ROMs;
[the conformance entries](hardware-conformance.md#gradation-and-cache-refillram-implementation)
record primary specifications, hashes, evidence and remaining deviations.

The original PCM read and PCM/EG/noise arbitration guests run in native and browser
Wasm checks. The 68k changes actual waveform RAM between the two retained
interpolation operands; the SH-2 separately times internal DMA with 32 PCM voices,
maximum attenuation and noise. The browser verifies the arithmetic midpoints,
FRT bounds, mutations, repeated boots, omitted-KEYON silence and production
AudioWorklet output. Reproduce both with the PCM builder listed above; source,
license and primary-specification evidence are linked in the
[PCM conformance entry](hardware-conformance.md#separately-retained-pcm-operands-and-sound-memory-ownership).

The SCSP clock builder also reproduces the original repeated-effect-store guest.
Its 68k-uploaded DSP program overwrites positive, negative and zero effect
register values and computes separate left/right samples. Native and browser
checks include three operand pairs, an omitted-store control, cold resets and
production AudioWorklet delivery. The [effect-store conformance entry](hardware-conformance.md#scsp-effect-register-store-semantics)
links the primary rule, source, license, hash and deadline vectors for all sixteen
registers.

The original MVOL fixture builder also reproduces mixed data/audio media outside
`web/`. Its SH-2/68k guest and executing DSP run natively and in production browser
Wasm, with independent stereo expectations for isolated and combined sources,
intermediate global gains, mute/resume continuity and AudioWorklet delivery.
See the [numerical contract](scsp-mvol.md) and [validation record](validation-issue-29.md).


The original CPU sound-memory fixture also runs natively and in production Wasm
under `/saturn-rust/`. A real SH-2 DMAC fills sound RAM subject to an independently
bounded 512-cycle DRAM budget; a separately executing 68k reads the shared data,
changes PCM waveform words and runs a half-gain DSP. Tests assert guest records,
three operand pairs, omitted DMA start, repeat boots and actual AudioWorklet
rendering/underflow/blocked feedback. See the
[transaction contract](scsp-cpu-memory.md) and [validation](validation-issue-41.md).


The original video-write guest runs the same bytes on both SH-2s and their
on-chip DMACs in native Machine tests and production browser Wasm. It computes
readback/FRT records and performs a timed CRAM longword whose separately
published words render a two-dot black interval between old blue and new
red/green. Changed operands, omitted writes, repeated cold boots, native
advancement chunks and actual Canvas pixels are checked. Reproduce it with
`python3 tests/fixtures/build_video_writes.py --check`; see the
[transaction contract](sh2-video-writes.md) and [validation](validation-issue-43.md).


The original slave-raster diagnostic starts the slave with SMPC, installs normal
IRL autovectors and computes separate HBlank/VBlank records through actual
SLEEP/exception/RTE execution. Native and production Wasm checks include re-entry
within a blank, changed arithmetic, SR masking, omitted startup and repeated cold
boots. The fixture reproduction command above is registered in CI; see
[Issue #47 validation](validation-issue-47.md).

The original DMA-startup mixed-disc diagnostic runs natively and in production
Wasm under `/saturn-rust/`. Its SH-2 performs finite overlapping bitmap writes
while Timer0 requests CD-FIFO-to-video DMA, computes source/readback records and
executes the DMA-end handler. Tests include changed operands, omitted trigger,
repeated cold boots and actual Canvas output. Reproduce its source and media with
`python3 tests/fixtures/build_dma_startup.py --check`; see the
[startup contract](scu-dma-startup.md) and [validation](validation-issue-51.md).

The original ordinary MOV store guest runs on both SH-2s in native Machine and
production browser Wasm. It measures warmed alignment/dependency kernels, checks
readback and exchanges a shared-data acknowledgement through guest stores/loads.
Reproduce its source/bytes/hashes with
`python3 tests/fixtures/build_sh2_store.py --check`; see the
[store stage contract](sh2-store-pipeline.md) and
[Issue #63 validation](validation-issue-63.md).
