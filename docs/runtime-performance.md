# Runtime performance investigation

Related to #17. BIOS initialization correctness and real-time throughput are
separate requirements. This local follow-up does not close the hardware tracker.

## Problem and expected outcome

At upstream `e69250e`, a release native run of 60 fields of authorized local
firmware takes 15.66 seconds, excluding firmware loading and presentation. The
goal is a measured reduction in host work with identical guest clocks, effects,
interrupt ordering and results. Firmware is optional and remains private.

## Context and reproduction

A Chrome CPU profile of 30 fields attributes about 13% of sampled self time to
the SCSP event-deadline calculation, and about 6.5% each to DMA preparation and
CPU internal-bus allocation. Samples identify candidates, not exact accounting.
The production build already uses Cargo's release profile. Native process
sampling was unavailable under the host's process-inspection permissions;
browser DevTools profiling provided the measurements instead.

## Acceptance criteria

- Preserve deadlines, guest work and observation/width/reset semantics; do not
  change emulated delays or substitute host-computed completion.
- Optimize measured repeated work, retaining pending and active transfer paths.
- Compare baseline and optimized production builds on identical original guests
  and, optionally, authorized local firmware, including guest-state equality.
- Run native, Wasm and complete browser checks. Report actual speedup and remaining
  limits; a performance improvement is not a claim of real-time compatibility.

## Optional validation

Local private BIOS measurements and startup screenshots may supplement original
guest evidence. Never publish firmware, private paths, dumps or screenshots.

## Required post-merge verification

None. This remains a partial hardware/performance increment, related to #17.

## First optimization pass

- Compute the next idle SCSP slot arithmetically, retaining all distributed
  OP/LFO edges and the distinct final slots 127/128. An exhaustive literal
  schedule test covers two sample periods and consumed/unconsumed edges.
- Skip DMAC preparation only when no transfer is enabled, no unit is accepted,
  no error awaits publication, and no grant or transfer journal needs service.
  Accepted DMA still finishes through the ordinary path after DREQ/DME changes.
- Skip CPU internal-bus arbitration only when neither an internal operand nor
  buffered-write acceptance requests it. Pending transfers retain arbitration.
- Skip ordinary internal interrupt arbitration at priority zero, after checking
  the independent UBC source; return immediately for an empty SCU request set.
- Reuse the board's already established on-chip event bound. Standalone on-chip
  advancement still splits arbitrary chunks; debug builds assert the bound.

No instruction, transfer, clock, interrupt, sample or raster edge is skipped.
This first pass kept the interpreter/replay design, presentation loop and thread model unchanged.

## Reproduce measurements

Native (original source-authored cache/RAM loop by default):

```sh
cargo run --locked --release --example runtime_benchmark -- --frames 60 --runs 3
```

Production Wasm (after `sh scripts/build-web.sh`):

```sh
BROWSER_CHANNEL=chrome node scripts/benchmark-runtime.mjs --frames 60 --runs 3
```

To compare builds, save the baseline's `saturn_rust.js` and
`saturn_rust_bg.wasm` together outside `web/`, then add `--baseline DIRECTORY`.
The harness alternates baseline/optimized runs in one browser, warms each module
for three fields, cold-resets each measured run, and requires equal selected
guest results: clocks, retired instructions, exceptions, all master registers,
a guest-written RAM counter and the complete framebuffer's FNV-1a checksum.
Native emits the same state fields and checks repeated cold-run equality.
These snapshots supplement functional regressions, not prove complete state
equivalence. No wall-clock threshold is a correctness test or CI gate.

Both tools optionally accept `--bios FILE` for an authorized raw local BIOS.
Loading/reset is outside the measured interval. Native `--draw` includes
framebuffer production/copying on every field. Browser `--draw` now also includes
Canvas `putImageData`; the historical measurements below predate this addition. An
ending-frame checksum is always computed outside that interval. The browser
tool's optional `--profile OUTPUT` saves a local DevTools CPU profile. Its server
binds only to loopback and serves only the selected public Wasm runtime files;
firmware is never an HTTP resource and no external request is allowed.

The browser harness accepts `--skip FIELD` and optional `--buttons FILE` to
measure initialization or a later menu rather than only cold startup. Button
events are ordinary controller levels, written as increasing field numbers and
hexadecimal masks. Skipped fields run normally outside the measured interval.
Each sample reports average throughput, the 95th-percentile field time and the
slowest field. Optional `--continuous` keeps each runtime alive across runs,
measuring successive windows after a single warm-up/skip. Baseline/candidate
snapshots are then checked separately at each matching end field; this measures
steady operation, not repeated cold resets. Baseline/candidate comparisons also check sound CPU registers,
PC and retired instructions. A source-authored GBR wait loop is available in
`examples/runtime-wait.hex`; no firmware is required for scheduler tests.

## First-pass measurements

Measured locally on 2026-11-08, comparing upstream `e69250e` against the same
source plus these performance changes. The existing workspace retains an older
checkout and unpublished initialization work; an isolated latest-main copy was
used for the upstream comparison without replacing that work. Wasm uses rustup
stable Rust 1.99.0, wasm-bindgen 0.2.129 and Chrome 157.0.8103.38 via Playwright
1.58.2. Each row is the median of three cold runs after module warmup, with
alternating baseline/optimized order. No other validation jobs ran during these
timed comparisons. Loading, initialization and presentation are excluded.

| Guest | Fields | Baseline | Optimized | Throughput improvement |
| --- | ---: | ---: | ---: | ---: |
| Optional authorized private BIOS | 30 | 11014.7 ms | 8143.0 ms | 1.35× |
| Original cache/RAM loop | 5 | 1023.7 ms | 809.3 ms | 1.26× |

All recorded guest snapshots match baseline, optimized and repeated cold runs.
The original five-field snapshot also matches native exactly. A preliminary
native private-BIOS 60-field observation improved from 15.66 to 11.64 seconds
with identical clocks/instructions/registers (Homebrew Rust 1.94.0); this single
observation is supplementary, not the repeated comparison above.

An additional three-run Wasm trial of thin LTO plus one codegen unit yielded
8138.0 ms versus 8219.2 ms with identical guest results. It did not demonstrate
an execution benefit, so Cargo release settings are unchanged. JIT, decoder
caches, replay redesign and Workers were not introduced without measurements
justifying their additional implementation and validation cost.

This improves the private-BIOS measurement from about 2.72 to 3.68 fields/sec,
still far below real-time 60 Hz. The measured interval is early boot; later
sound/video-heavy phases, other hosts and real presentation may differ. Full
stock BIOS/menu compatibility and a universal speedup are not established.

## First-pass validation

All checks are local; no commit, push or hosted CI result is claimed.

| Check | Result |
| --- | --- |
| Working tree: stable fmt, locked build and complete native tests | Passed; 1212 tests, zero failures |
| Working tree: native Clippy, all targets, warnings denied | Passed; the existing private BIOS probe adopted `as_chunks` for a new stable-Clippy warning |
| Working tree and isolated latest-main copy: production Wasm build and Wasm Clippy | Passed |
| Latest-main copy: complete fixture reproduction | Passed; 106 registered commands, including the original BIOS initialization guest |
| Latest-main copy: native BIOS initialization and no-media CD tests | Passed; 4 + 2 tests |
| Latest-main copy: browser runner regressions | Passed; all 13, including intentional failing-child controls |
| Latest-main copy: complete browser inventory and aggregate/artifact verification | Passed; all 74 groups, 307305 ms |
| Original benchmark guest in native and production Wasm | Selected five-field state matches exactly |
| Optional private BIOS: 200-field native startup comparison | Distinct baseline/optimized binaries have identical recorded state and nonblack framebuffer checksum |

The complete native suite ran before a one-line formatting/API update in the
optional PPM-export helper; the final all-target Clippy run checked that update.
These checks describe the first pass. The cache-execution follow-up below has its own validation.

The 200-field comparison uses separate native build output directories: sharing
an output directory between same-named packages can leave the common example
executable pointing at the previous package's artifact. An initial shared-output
comparison was discarded after identical executable hashes exposed that problem;
the separate optimized build has a different hash. This does not affect the
Wasm comparison, which uses separately saved and distinct runtime files.

For handoff, an ignored `target/runtime-performance.patch` contains only this
follow-up against upstream `e69250e`, excluding the workspace's earlier BIOS
prototype. Its application is checked against the original source. It does not
contain firmware or generated runtime binaries.


## Cache-execution follow-up

The first pass was still far below real time. The frame runner now uses a
cache-only execution path in `src/bus/cached.rs`, running the **same** `Cpu::step`
interpreter and IF/ID/load/multiplier model against a local clock. It batches up
to 128 retired instructions or 256 main clocks, whichever ends first, strictly
before the next CPU-observable event. Ordinary `step` and `run_steps` remain the
reference scheduler. There is no guest-identity hook, generated code or JIT.

Only canonical ROM/WRAM cache hits are eligible. An active peer CPU, DMA/bus
ownership, reset, NMI, eligible IRQ, user-break comparisons, missing cache line,
store or unsupported instruction returns execution to the established scheduler.
Non-1:1 PLL ratios also retain that path. The local cache LRU, retained IF word
and instruction epoch count are transactional; a declined instruction publishes
none of its speculative changes. Real address-array replacement is retained;
a forged MMIO tag cannot bypass peripheral width/lane checks.

The CPU horizon includes interrupt recognition, blanking, on-chip events,
refresh, SCU/DSP, VDP1, CD and SMPC. Video acquisition still executes at every
original slot through `Bus::advance_clocks`. Internal sound slots can commute
with a cache-only CPU burst only when the sound CPU is disabled, no DSP/DMA or
reset work can fail, the main interrupt mask is zero, and no medium can produce
CD-DA effects. Otherwise the normal sound deadline also bounds the burst. No
sample, display acquisition, DMA beat or emulated clock is dropped.

After a burst the last retired instruction uses the same post-retirement board
advancement as an ordinary step. This matters when a field edge precedes the
CPU's next issue time: an earlier prototype advanced one extra instruction at
that boundary. The differential tests exposed it; sharing the ordinary finish
path corrected it without relaxing comparisons. Fresh external interrupt inputs
are sampled before calculating the burst horizon.

Six native differential tests compare ordinary steps with optimized fields,
including full CPU state, clocks/readiness, RAM/VRAM, LRU, complete pixels, audio,
faults and reset state. They exercise original cache/RAM loops with changed
operands and an omitted store, nine existing hardware ROMs, NMI/restart, a new
capture IRQ between fields, PAL/UBC fallback and a forged MMIO cache tag.
`scripts/cached-scheduler.mjs` compares six original guests for two fields each
in production Wasm and is part of the existing `hardwareBoundaries` group.
The required browser inventory remains 74 groups.

### Follow-up measurements

Same Chrome/toolchain and alternating three-run methodology as the first pass,
comparing the original upstream Wasm files directly with the final candidate:

| Guest | Fields | Original upstream | Final candidate | Throughput improvement |
| --- | ---: | ---: | ---: | ---: |
| Optional authorized private BIOS | 30 | 10326.1 ms | 2327.3 ms | 4.44× |
| Original cache/RAM loop | 5 | 963.7 ms | 782.5 ms | 1.23× |

All recorded snapshots match. The memory-writing guest deliberately stresses
fallback traffic; its smaller gain is evidence that speedup depends on workload.
The BIOS measurement is about 12.9 fields/sec, still below real-time 60 Hz.
Active slave CPUs, DMA/DSP workloads, uncached execution and other clock modes
retain the detailed scheduler and can remain slow. This does not establish
stock-menu/game compatibility or general real-time Saturn emulation.

A cold-diagnostics boxing trial reduced native `BusError` from 96 to16 bytes
but showed about 1% improvement in an exploratory comparison, so it was not
adopted and the public `Copy` API is preserved. An instruction-continuation
trial gave about 14% in a short probe but added complexity and differed in the
stopped scheduler state on a device fault; it was also discarded. The final
change is the event-bounded cache path and its explicit ordinary fallback.


### Follow-up validation

Validated with rustup stable Rust 1.99.0 and the same pinned Wasm/browser tools:

| Check | Result |
| --- | --- |
| Locked native build, fmt and complete native suite | Passed; 1218 tests, zero failures |
| Native Clippy, all targets, warnings denied | Passed |
| Working tree production Wasm build and Wasm Clippy | Passed |
| Upstream-copy Wasm build/Clippy and 106 fixture reproduction commands | Passed |
| Upstream-copy native BIOS initialization/no-media regressions | Passed; 4 + 2 tests |
| Production-Wasm cache scheduler differential checks | Passed; six original guests, two fields each, inside hardwareBoundaries |
| Browser runner regressions | Passed; 13 tests |
| Complete browser inventory and aggregate/artifact verification | Passed; 74 groups, 312768 ms |
| Original five-field native/Wasm benchmark snapshot | Exact match |
| Optional private BIOS, 200-field startup | Distinct original/optimized binaries match recorded state and nonblack framebuffer checksum |

Private startup is supplemental; no firmware is required by the new tests. The
full native and browser suites ran after the final runtime changes. The exposed
sound/video scheduling, IRQ/reset, negative controls and existing guest oracles
were retained. No tests were weakened or skipped. No hosted CI, commit, push,
publishing, live Pages inspection or hardware-completion claim is implied.

## Real-time throughput iteration (in progress)

The current requested target is 60 fields/sec. Early-boot throughput alone is
insufficient: a local Canvas-inclusive phase survey of the previous cache-burst
build measured approximately 12.7 fields/sec over fields 0–60, 3.5 over 180–240,
and 3.4 over 1200–1260. Presentation cost was about 0.15 ms/field. These exploratory
single samples identify workload differences; they are not repeated acceptance
measurements or proof of full firmware/menu compatibility.

The next implementation keeps debugger stepping as the reference and reduces
host work in the frame runner:

- Convert cached instruction horizons and retirement clocks through the exact
  NTSC/PAL PLL phase, retaining a pending second main tick.
- Admit the remaining read-only MOV load families, including GBR-relative
  polling. The original wait guest varies width, operand data and branch outcome.
- Keep zero-initialized DSP programs quiescent only with zero TEMP/accumulator
  and no delayed input. Nonzero TEMP, pending reads and live program writes keep
  instruction deadlines. Distributed synthesis and DRAM grants still execute.
- Advance independent display acquisitions and silent sound work locally while
  preserving observable completion, interrupt and reset boundaries. Normal-screen
  acquisition cannot observe VDP1 pixel progress; rotation keeps finer bounds.
- Retain compact replay result/error indices and small IF/ID fault payloads.
  The public `BusError` remains `Copy`; large diagnostics are retained on errors.
- Avoid idle DMA arbitration and repeated immutable VDP2 configuration/grant
  decoding; register writes invalidate the corresponding memoized policy.
- Complete eligible pending bus operations within the existing interpreter call
  instead of cloning and replaying the instruction header at every wait. The
  journal remains the fallback, shared sound RAM retains every DRAM/sound edge
  with a bounded continuation budget, and CPU readiness on faults is preserved.

An alternating baseline/candidate pair over fields 180–210, including Canvas
presentation and identical controller input, measured 8762.9 ms versus 3426.4 ms
(3.42 versus 8.76 fields/sec, 2.56×). Recorded CPU, sound and framebuffer snapshots
matched. The candidate's 95th-percentile field took 247.9 ms. This pair predates
the in-place bus continuation; it is an intermediate result, not a 60 Hz claim.

A later exploratory build was rejected when a 60-field comparison found a sound
CPU mismatch. Its evaluation copy had omitted the shared-sound-RAM exclusion for
buffered-write overlap. The working implementation and evaluation copy were
resynchronized, and an original cache-enabled sound-RAM guest and explicit
negative admission test were added. Measurements from the rejected build are
not acceptance evidence.

The synchronized build matched the ordinary scheduler's master/sound state and
clocks through 250 optional private BIOS fields. A subsequent Canvas-inclusive
180–240 comparison matched its recorded baseline CPU, sound and framebuffer
snapshot: 17348.6 ms versus 9742.1 ms (3.46 versus 6.16 fields/sec, 1.78×), with
95th-percentile times of 293.5 and 230.6 ms. These are single paired observations.
Later-menu results remain pending. The 60 fields/sec target is **not achieved**
at this checkpoint.

Further iterations preserve the same reference scheduler:

- Proven cache-only fixed-point loops can repeat a period without reinterpreting
  each instruction. Full architectural/pipeline/LRU/prefetch state must repeat
  after clock/counter translation. DT/BF countdowns additionally prove the sole
  changing counter and stop before its zero exit. Sound retirement positions use
  a compact periodic sequence, bounded independently of fractional PLL phase.
- Cache-local clocks and retained IF words use individual cells; rollback keeps
  an explicit snapshot. Register-only specialization and in-place instruction
  trials did not demonstrate a material gain and were removed; at that checkpoint the burst
  executor always called the ordinary `Cpu::step` interpreter. The later block
  implementation is described in the continuation below.
- FRT advancement handles earlier plain increments arithmetically and retains
  the actual compare/overflow edge. The timer/SCI/WDT/DIVU event horizon is a
  countdown until the next event, invalidated by register writes and DIVU reads.
  BSC/UBC deadlines remain independently queried. A mixed transaction test checks
  cached deadlines against a fresh calculation after each operation.
- PCM activity queries reuse a derived summary until a KEYON/register write,
  reset or pipeline transition invalidates it. Actual voices, retained slots,
  waveform grants, synthesis and samples are unchanged.

An alternating three-run early-boot comparison of the timer/one-clock-wait
iteration against the preceding candidate measured median 3137.3 versus
2810.9 ms for 60 fields including Canvas (19.12 versus 21.35 fields/sec, 1.12×).
All recorded CPU, sound and framebuffer snapshots matched. This is a comparison
between intermediate optimized builds, not the original upstream baseline.
The separate scalar/copy and PCM-summary trials were approximately 1% or less;
those timings do not establish a repeatable independent throughput gain.

Current local validation:

| Check | Result |
| --- | --- |
| Complete working-tree native suite before compact periodic sound bookkeeping | Passed; 1231 tests, zero failures |
| Later cached-scheduler differential suite | Passed; 12 original scenarios, with complete CPU/pipeline, RAM, VRAM, framebuffer, audio, readiness and fault comparisons |
| Added compact retirement cutoff and all-phase PLL bound tests | Passed; 2 tests |
| Sound regressions before the later bookkeeping change | Passed; 122 tests |
| Native all-target Clippy and upstream-port Wasm Clippy | Passed, warnings denied |
| Upstream-port production Wasm build | Passed |
| Complete browser suite for the compact-bookkeeping runtime | Passed; 74 groups, aggregate/artifact checks, 268386 ms |
| Production browser cached scheduler | Passed; 15 original CPU/RAM/countdown, sound, video, fault and PLL scenarios against ordinary steps |
| Optional private firmware, after shared-RAM continuation change | Master/sound state and clocks matched ordinary steps at every field through field 250 |

The two later tests add independent arithmetic/cutoff evidence and do not change
production behavior. Firmware and private traces remain local. No commit, push,
PR, deployment or hardware-completion claim was performed.

### Active audio horizon follow-up (validation checkpoint)

The phase survey after the timer/compact-cycle iteration measured 20.76 fields/sec
for fields 0–60, 6.54 for 180–240 and 3.95 for 1200–1260, each including Canvas.
These are single exploratory samples, not acceptance against a paired baseline.
Private diagnostics then identified a conservative admission condition: active
PCM or a nonzero DSP program forced the main CPU back to short sound deadlines,
even when sound had no main interrupt route, DMA or shared-RAM interaction.

The revised guard validates all DSP instruction error conditions after program
writes. With MCIEB zero and no SCSP DMA, valid DSP/PCM work can commute with
cache-only SH-2 execution. Sound still runs every instruction, grant and sample;
a queued sound control write or RESET stops lookahead before its observable
phase. Invalid MPRO words, enabled interrupt routes, DMA, CD media and shared
sound-RAM transfers retain the detailed boundaries. This is independent-domain
scheduling, not audio suppression or a timing-accuracy mode.

An original active-PCM/DSP-memory guest compares complete CPU pipeline state,
sound CPU/RAM, DSP register banks, pixels and audio for three fields in both
clock modes. DSP validation vectors compare the guard with actual execution at
all 128 positions, including reserved bits, invalid input selectors, illegal
memory phases and live repairs. The browser differential inventory has 17 cases,
including cache-enabled original PCM and effects guests. The complete browser
suite passed all 74 groups (268173 ms). Native and Wasm Clippy passed. An optional
450-field private audit matched full master CPU/pipeline state, sound CPU and
clocks at every field, including active intro audio. No firmware is required by
the original tests or published in the patch.

The final paired menu measurement is recorded below. The 60 fields/sec target
remains unmet.

### Final scheduler refinement

The frame runner also coalesces between-instruction waits while the CPU cannot
issue before its existing ready time. It still uses the ordinary first
post-retirement advancement and does not return a field before that reference
contract. Cache-only bursts grant only an already accepted buffered write;
otherwise they preserve idle cycle-steal bookkeeping without repeating all
arbitration. Ordinary step/debugger behavior is unchanged.

Three alternating cold early-boot pairs measured median 2801.3 versus 2729.0 ms
(21.42 versus 21.99 fields/sec, 1.026×), with equal recorded snapshots. Three
successive two-field windows of the original effects guest measured median
102.5 versus 67.9 ms (19.51 versus 29.46 fields/sec, 1.51×). The original store
loop showed no material gain in this refinement, demonstrating the workload
limit. These comparisons precede removal of the ineffective scalar experiments.

An exact rational timestamp-cache trial preserved all sound regression results
but measured 173.9 versus 176.7 ms on three original effects windows (0.984×).
It was removed. The original conversion arithmetic remains in production.
No accuracy-reduction mode, frame skipping or firmware-specific hook was added.

A single paired optional menu comparison before the last scheduler refinement
matched all recorded snapshots: 15298.6 versus 10451.0 ms over fields 1200–1260
(3.92 versus 5.74 fields/sec, 1.46×), including Canvas. Some independent test
work overlapped that exploratory run, so it is not the final isolated acceptance
measurement. The final isolated comparison is recorded below.

### Final isolated menu result

After all compilation and validation jobs stopped, three alternating successive
60-field windows over fields 1200–1380 measured the following medians, including
Canvas. The baseline is the preceding timer/compact-cycle build, before widening
the active-audio horizon; both use the same pinned upstream checkout and inputs.
Each corresponding end-field CPU, sound and framebuffer snapshot matched.

| Guest / interval | Baseline | Final runtime | Improvement |
| --- | ---: | ---: | ---: |
| Optional private BIOS menu, three 60-field windows | 15369.4 ms / 3.90 fields/sec | 10349.0 ms / 5.80 fields/sec | 1.49× |

The candidate's per-window 95th-percentile field times were 287.8, 287.7 and
287.3 ms; the slowest field was 290.3 ms. Sixty fields/sec requires about 16.67 ms
per field. The average needs a further 10.35× reduction, and the slowest sampled
field about 17.4×. **The requested 60 fields/sec goal is not achieved.** The
remaining instruction/bus-boundary and device-scheduling work needs substantially
more architectural improvement; these results do not establish game compatibility
or real-time stock firmware operation.

Final local checks on the adopted runtime, after removal of the ineffective
scalar and timestamp-cache trials:

| Check | Result |
| --- | --- |
| Complete working-tree native suite | Passed; 1234 tests, zero failures |
| Working-tree native all-target Clippy and upstream-port Wasm Clippy | Passed; warnings denied |
| Working tree and pinned-upstream port: production Wasm builds | Passed |
| Complete pinned-upstream browser inventory and aggregate verification | Passed; all 74 groups, 266911 ms |
| Cached scheduling in production Wasm | Passed; 17 original guests including active PCM/DSP, faults, DMA/peer fallbacks and PLL |
| Optional private BIOS: full master/pipeline, sound state and clocks against ordinary steps | Matched at every field through field 450 |
| Portable source-only patch against upstream `e69250e` | Applies cleanly with `git apply --check`; firmware and the older workspace initialization prototype are excluded |

The generated runtime in the working tree is rebuilt for local use. No commit,
push, PR or deployment was performed. Issue #17 remains an open hardware tracker.

The same final production runtime also passed three alternating cold 0–60-field
pairs: baseline median 2807.8 ms (21.37 fields/sec), candidate 2804.9 ms
(21.39 fields/sec), with matching recorded snapshots. This last refinement has
no material additional early-boot gain; the active-audio improvement is specific
to later workloads. These values are actual unthrottled Chrome throughput, not a
claim that the application meets real-time pacing.

### Continued throughput work (2026-11-09)

The preceding isolated result is an earlier checkpoint. Work continues toward
60 fields/sec; improvements below do not establish that target or close #17.
The reference scheduler, original guests and optional private measurements remain
separate evidence.

The adopted continuation adds:

- Buffered-store planning at the actual one-state admission boundary. The CPU
  uses the ordinary instruction semantics. Endian conversion, physical width,
  ownership, deferred faults and data publication still use `CpuBus` and the
  actual write buffer. Catch-up splits at an accepted data write's completion
  before subsequent texture/display acquisitions. Control writes keep their
  completion horizon; sound RAM remains excluded.
- Inline storage for the maximum four physical byte beats of a buffered longword,
  and for the 68000's two retained instruction words. Neither changes transfer
  order, retained values or the number/timing of external accesses.
- A register-only block executor sharing the reference ALU operations. It retains
  real IF words, cache replacement, decode readiness and multiplier clocks. All
  other instruction/pipeline work remains in the reference interpreter. DIV1 is
  eligible for cached execution, including Q/M/T and register aliases.
- Independent rotation acquisition and VDP1 back-buffer drawing. A pending
  six-state pixel captured before a swap retains its front-buffer deadline.
  Forced stops, erase, texture/table reads, end codes and physical port ownership
  remain observable at their normal phases.
- A conservative lower bound on drawing completion from the remaining dots and
  rows. Rectangular rows with no end-code exit can use their retained clipped
  width. The next command's full decode time remains in the bound. Cache bursts
  remain bounded by all actual device events, with caps of 1024 main clocks and
  512 instructions; a larger cap cannot cross those events.
- Whole-program release optimization (`lto = "fat"`, one code-generation unit).
  The default Wasm feature requirements are unchanged.

Paired Chrome comparisons, including Canvas and equal recorded snapshots:

| Workload / isolated change | Before | After | Ratio |
| --- | ---: | ---: | ---: |
| Original RAM store loop: store planning | 12.99 fields/sec | 23.77 fields/sec | 1.83× |
| Optional firmware 0–60: store planning | 22.61 fields/sec | 27.85 fields/sec | 1.23× |
| Optional menu 1200–1380: rotation synchronization, before later renderer work | 6.27 fields/sec | 11.62 fields/sec | 1.85× |
| Original unrolled division: shared blocks and release optimization | 33.50 fields/sec | 129.76 fields/sec | 3.87× |
| Original combined scene: continuous pixel runs | 21.26 fields/sec | 24.95 fields/sec | 1.17× |
| Original combined scene: inline 68000 prefetch | 24.82 fields/sec | 26.21 fields/sec | 1.06× |
| Original RAM store loop: inline transfer payloads | 24.84 fields/sec | 26.31 fields/sec | 1.06× |
| Original combined scene: remaining-drawing bound and wider burst caps | 26.34 fields/sec | 32.31 fields/sec | 1.23× |
| Original combined scene: rectangular-row bound | 32.76 fields/sec | 35.90 fields/sec | 1.10× |

These are separate alternating three-run comparisons between intermediate
builds. Ratios cannot be multiplied into an unmeasured final result. The larger
horizon did not improve cold startup: its paired comparison stayed around
28.6 fields/sec. The isolated full-menu checkpoint and complete checks are recorded below.

Trials without a useful isolated gain were removed: extended waiting-CPU sound
lookahead, per-line instruction reuse, display storage transposition/reuse,
individual display-latch Cells, redundant pixel-division special cases, boxed
CPU fault status, and SIMD-only builds. Default feature requirements and the
public CPU fault-status representation remain unchanged.

The new original fixture combines a register/division loop, live RBG0 acquisition,
cyclic VDP1 texture drawing and the authored SCSP effects program. Regenerate and
measure it without personal firmware:

```sh
python3 examples/build-runtime-scene.py --check --binary target/runtime-scene.bin
sh scripts/build-web.sh
BROWSER_CHANNEL=chrome node scripts/benchmark-runtime.mjs \
  --bios target/runtime-scene.bin --skip 5 --frames 15 --runs 3 --draw --continuous
```

`--bios` here loads the original fixture ROM, not proprietary firmware. The
source and hex bytes are in `examples/runtime-scene.s` and `.hex`. The shared
native scheduler comparison and production browser comparison include that
combined workload. Existing literal video, sound, DMA, reset and bus-access
oracles remain required; throughput snapshots do not replace them.

Native evidence added in this continuation covers every register-only encoding
against complete ordinary CPU/pipeline state, store widths and live BSC endian/
bus-width changes, continued drawing with rotation, and pending translucent
publication across a framebuffer swap. Pixel-run comparisons alternate both
physical port owners and include end-code and six-state effects; every predicted
observable bound is checked against actual single-clock completion.


#### Validated continuation checkpoint

After validation jobs stopped, three alternating successive menu windows over
fields 1200–1380 matched all recorded snapshots. The baseline is the runtime at
the start of this continuation: median 10163.0 ms / 5.90 fields/sec. The candidate
measured 4848.2 ms / 12.38 fields/sec, a 2.10× improvement, including Canvas.
The target still needs about 4.85× additional throughput. **60 fields/sec is not
achieved.**

The complete native suite passed 1242 tests. Both production builds succeeded;
native all-target and Wasm Clippy passed with warnings denied. The complete
browser inventory passed all 74 groups and its aggregate checks; its cached
scheduler comparison covered 22 original workloads. An optional 450-field audit
matched full master/pipeline state, sound CPU state and clocks at every field.
The private audit is supplemental; the original combined guest and existing
hardware oracles run without personal firmware.

#### Stack transfers, ready-time retries and normal display dependencies

The next increment admits the six system/control stack loads, six stack stores
and RTE through the existing instruction interpreter. Special stores are limited
to work RAM. A change to SR.I ends the burst before another instruction can
observe a different interrupt mask. Planned stores update local replacement
state at acceptance; real publication precedes committing final replacement
state. Younger IFs see accepted cache writes while already retained instruction
words keep their earlier values. Post-acceptance IF tails still grant and publish
each external beat at its normal boundary.

When an existing hardware wait reaches CPU readiness, the frame runner retries
one cached instruction before returning to instruction replay. This retains
ordinary stepping's completion contract when the wait crosses a field boundary,
as well as the existing bounded yield. Public debugger stepping is unchanged.

Normal display acquisition omits dependency traversal for disabled vertical-cell
scrolling and for bitmap pattern names. Enabled paths still resolve and retain
their physical operands at the same slot. A literal three-path comparison checks
the complete read history, retained operands, samples and rendered pixels,
including VRAM writes and live dependency changes between acquisition edges.

Separate alternating three-run comparisons, with Canvas and equal recorded
snapshots, measured:

| Workload / increment | Before | After | Ratio |
| --- | ---: | ---: | ---: |
| Original stack/RTE guest: stack transfers and ready-time retry | 14.37 fields/sec | 24.54 fields/sec | 1.71× |
| Optional firmware 0–60: stack transfers and ready-time retry | 28.68 fields/sec | 30.07 fields/sec | 1.05× |
| Original combined scene: unused display dependencies | 34.67 fields/sec | 35.27 fields/sec | 1.02× |
| Optional menu 1200–1380: unused display dependencies | 12.05 fields/sec | 12.42 fields/sec | 1.03× |

The original stack guest is in `examples/runtime-stack.s` and `.hex`. Native
comparisons exercise all thirteen memory/control forms, self-modifying successor
fetches versus retained IF words, and ready-time waits crossing a field edge.
The browser differential inventory includes this guest as its 23rd case.

Additional trials were removed after measurement: extending scalar blocks across
preceding load hazards, a specialized cached MOV-load executor, bit-filtered SCSP
interrupt selection, repeated VDP1 texture-address memoization, and shared VDP2
acquisition storage. These produced approximately 0–2% gains or regressions in
the measured original workloads. Binaryen 127 `wasm-opt -O4 --converge` improved
cold startup by 2.4% but did not improve the combined scene; it is not a build
dependency. No frame, guest instruction, transfer or synthesis work is dropped.


#### Final local result for this increment

After all validation jobs stopped, three alternating successive 60-field menu
windows over fields 1200–1380 matched every recorded corresponding snapshot.
The comparison uses the same pinned upstream checkout and controller inputs;
Canvas presentation is included. The continuation-start runtime measured median
10302.0 ms / 5.82 fields/sec; the final candidate measured 4827.1 ms /
12.43 fields/sec, a 2.13× improvement. Candidate 95th-percentile field times were
116.4, 116.7 and 116.6 ms. **The requested 60 fields/sec is not achieved**: average
throughput still needs another 4.83× improvement, and the sampled slow fields are
well beyond the 16.67 ms real-time budget.

The final candidate passed all 1245 native tests, native all-target and Wasm
Clippy with warnings denied, and formatting checks in both source trees.
Both working-tree and pinned-upstream production Wasm builds succeeded.
All 74 browser groups and aggregate/artifact checks passed (41 scenario groups,
33 shared groups); the cached scheduler covered 23 original workloads.
The existing fixture reproduction suite and original combined-scene generator
also passed. The portable source patch applies cleanly to the pinned upstream.
The earlier 450-field private audit belongs to the previous checkpoint; this
increment adds original full-state stack/fetch/field-edge comparisons and paired
optional firmware snapshots, without claiming a new full private audit.

A fresh optional menu profile before removal of the ineffective load experiment
continues to show work spread across CPU/bus scheduling, display acquisition and
sound processing. A compiler flag or one isolated hot function does not account
for the remaining gap. No general game/stock-menu compatibility or hardware
completion is established. All firmware and private observations remain local.
No commit, push, PR or deployment was performed; #17 remains open.


The rebuilt working-tree application independently measured median 2046.2 ms /
29.32 fields/sec across three cold 0–60-field runs with Canvas. This is a local
working-tree observation, separate from the pinned-upstream paired comparison.

The same working-tree application also completed three continuous menu windows
at fields 1200–1380 with the same controller inputs: median 4787.9 ms /
12.53 fields/sec, including Canvas. Its 95th-percentile field times were
115.9, 116.1 and 116.6 ms. Thus the rebuilt local application also remains below
60 fields/sec; the upstream-copy speedup is not presented as a working-tree
baseline comparison.


### Passive status reads and transactional sound evaluation

This continuation admits side-effect-free EDSR/LOPR polling through a private
BSC protocol model, bounded before drawing completion, frame changes and other
CPU-visible events. Physical byte/word beats, endian and bus-width controls,
waits and protocol state are preserved. The original draw operation still
produces END and its interrupt; the optimization does not fabricate completion.

Slow read retirements can exceed the independent sound domain's control-write
cutoff. A sound checkpoint retains the synthesis/CPU/controller state and uses
physical-byte RAM undo records, allowing selection of an earlier CPU prefix.
An output-ring undo record retains only overwritten samples rather than copying
the entire host audio queue for each checkpoint. Compact sound replay entries
keep original error diagnostics in a separate indexed list. DSP zero words use
the original signed accumulator equation, with inert slots skipped only when
all other synthesis, memory and distributed slot requirements permit it.

An original polling guest and a combined polling/rotation/drawing/effects guest
exercise the optimization without firmware. Focused differential tests cover
both PLL domains, 16/32-bit bus configuration, endian controls, word/long loads,
real draw completion and PTMR restart, sound RAM/DSP writes and rollback,
queued control/reset/fault boundaries, and empty/partial/full/wrapped/overrun
host audio queues. A literal zero-word DSP decoder compares full private state.

Measured alternating production-Wasm comparisons with Canvas and matching
recorded snapshots:

| Increment / workload | Before | After | Ratio |
| --- | ---: | ---: | ---: |
| Transactional sound and address planning: original combined polling scene | 18.23 fields/sec | 35.96 fields/sec | 1.97× |
| DSP zero specialization: original combined polling scene | 35.96 fields/sec | 36.78 fields/sec | 1.02× |
| Passive reads, sound checkpoints and partial address planning: optional menu | 12.57 fields/sec | 13.92 fields/sec | 1.11× |
| Output-ring undo and compact sound replay: optional menu | 13.62 fields/sec | 14.03 fields/sec | 1.03× |

Each row compares its own saved runtime pair; independent medians from different
comparisons are not a cumulative before/after ratio. Menu samples are successive
60-field windows over fields 1200–1380 with identical controller events.
A native 120-field sample after the same initialization measured 19.12
fields/sec, including framebuffer production, so the remaining limitation also
exists outside Wasm. This native sample used the subsequently rejected VDP2
address-retry experiment and is not a final-runtime measurement.

Further experiments were removed despite functional parity: VDP1 command-chain
status lookahead measured 1.014×, exact incremental sound-clock conversion
measured 0.982×, and pending-request/within-tile VDP2 address reuse measured
0.994× in the optional menu. They did not establish a useful throughput gain.
The current profile spreads sampled self time across bus scheduling (28.4%),
sound/68000 processing (18.8%), VDP2 (16.5%), SH-2 (11.8%), machine scheduling
(8.3%) and VDP1 (7.3%). In that 60-field sample, emulation took 4232 ms while
Canvas/audio extraction took 10 ms. Categories depend on compiler inlining and
are diagnostic samples, not exact component accounting.

**60 fields/sec remains unmet.** Later increments and their measured scope are
recorded below; the earlier full-suite checkpoint does not validate later changes. No hardware completion or general stock-firmware
compatibility is claimed. No commit, push or publication is authorized.


### Rotation, normal bitmaps, ROM execution and WRAM stores

The rotation path caches decoded matrix operands for the current parameter,
line and density. Products retain their original individual rounding, and live
per-dot coefficient overrides still apply. Table/line reloads invalidate the
cache; debugger decoding continues to use live operands.

Cache-disabled execution in the BIOS address range can use the same bounded
BSC sequence for instruction fetches and ROM data reads. Aligned 32-bit physical
IF reads retain their second halfword; actual waits, endian controls and protocol
history are preserved. Stores and unsupported operands return to the ordinary
arbiter. This improves an original uncached division guest substantially but
showed no cold-firmware benefit in its paired trial.

Unit-step normal bitmaps resolve coordinates once per acquisition edge and
consume retained physical words directly. The path retains the real per-bank
commands and grants, captured dot operands and source groups. A literal three-path
oracle compares reads, retained words, samples and pixels, including fractional
scroll, wrapping, non-unit vertical scaling and between-edge memory/register
writes. Other modes use the existing dependency path.

The CPU planner can continue after isolated WRAM stores, up to sixteen writes
within the existing event horizon. Its private BSC sequence preserves each
single-entry-buffer wait and physical beat. Ordered cache overlays feed younger
reads and IF, while already retained instruction words remain unchanged and
cache-through writes retain stale cache aliases. Only when no admitted master
can observe intermediate WRAM contents are completed earlier payloads published
together. The last accepted store uses the original arbiter, including a pending
external tail. A previously accepted final beat drains through the real scheduler.
VRAM, sound RAM and control writes do not use deferred WRAM publication.

Separate alternating three-run comparisons, including Canvas and equal recorded
guest snapshots, measured:

| Increment / workload | Before | After | Ratio |
| --- | ---: | ---: | ---: |
| Decoded rotation operands: original combined scene | 39.77 fields/sec | 40.84 fields/sec | 1.03× |
| Uncached ROM sequence: original division guest | 31.46 fields/sec | 156.25 fields/sec | 4.97× |
| Unit bitmap acquisition: original normal bitmap/effects scene | 66.57 fields/sec | 78.00 fields/sec | 1.17× |
| Unit bitmap acquisition: optional menu | 14.21 fields/sec | 14.56 fields/sec | 1.02× |
| Multiple WRAM stores: original memory-writing loop | 22.78 fields/sec | 42.11 fields/sec | 1.85× |
| Multiple WRAM stores: optional menu | 14.30 fields/sec | 15.02 fields/sec | 1.05× |
| WRAM overlay address bound: original memory-writing loop | 41.38 fields/sec | 43.06 fields/sec | 1.04× |

These are distinct saved-runtime pairs, not a cumulative speedup. The menu
samples cover successive 60-field windows at fields 1200–1380. Fast original
workloads do not demonstrate 60 fields/sec for the firmware/menu workload.
Native differential cases additionally compare complete CPU/pipeline state,
main clocks, BSC state, RAM, retained IF and cache bytes at every batch across
224 combinations of PLL, WRAM bank, cache-through alias, bus width, endian
and ordinary/SDRAM/DRAM/pseudo-SRAM timing and row settings.
They require multiple stores actually to execute within one batch.

Broader independent sound-control admission was removed after only 1.002×
additional menu throughput; the conservative control-write barrier remains.
An explicit rotation-capture callback refactor was also removed after no
improvement in the original bitmap workload. Replacing PCM operand shifts with
a seven-stage ring passed a literal-shift oracle but improved the original
combined polling scene by only 1.006×, so it was removed too. The validated checkpoint below records the final retained source; further
performance evaluation continues.


### Wasm inlining

The web build's default LLVM inline threshold is 1000, allowing hot CPU/bus
helpers to simplify together before Wasm tiering. Explicit caller `RUSTFLAGS`
(including an empty value) override this default. This does not change native
release settings or emulated work.

Against the previous default, three alternating successive optional menu
windows with Canvas and matching snapshots measured 14.97 to 16.38 fields/sec
(1.09×). The original combined polling scene measured 36.38 to 42.27 fields/sec
(1.16×) in its own comparison. Raising the threshold to 5000 improved that
original scene another 5% but reduced menu throughput by about 2%; it was not
adopted. A threshold of 20000 more than tripled the default Wasm size and added
only 1.2% over 5000 in the original scene, so it was also discarded. A size-first
Rust optimization profile reduced original-scene throughput to 0.68×.

These trials support the selected build setting, not a claim of 60 fields/sec.


### Validated WRAM and Wasm checkpoint

The selected source passed all 1258 native release tests and all 74 browser groups,
including 27 original cached-scheduler comparisons. Native all-target and Wasm
Clippy passed with warnings denied; both source trees passed formatting and
production Web builds. The working tree independently passed all 803 library
tests, including the 224-setting memory-interface comparison and explicit
interrupt-recognition state equality. The 58-file portable performance patch
applies cleanly to the pinned upstream and reproduces the selected file contents.
These are local results, not hosted CI or hardware-completion evidence.

After validation jobs stopped, the rebuilt working-tree application was compared
with its saved previous checkpoint. Each row alternates three corresponding
60-field runs and includes Canvas; every recorded corresponding guest snapshot
matches. The menu uses the same controller events and successive windows over
fields 1200–1380. The cold comparison resets each run and measures fields 0–60.

| Working-tree workload | Previous checkpoint | Selected build | Ratio |
| --- | ---: | ---: | ---: |
| Cold initialization, fields 0–60 | 29.13 fields/sec | 38.42 fields/sec | 1.32× |
| Menu, fields 1200–1380 | 12.21 fields/sec | 16.47 fields/sec | 1.35× |

Menu candidate 95th-percentile field times are 105.0, 104.8 and 105.3 ms.
**60 fields/sec is still unmet**: the menu needs another 3.64× average throughput,
and slow fields remain well beyond the 16.67 ms real-time budget. This checkpoint
is a tested basis for further work. No stock-firmware/menu compatibility or
hardware-completion claim follows from these selected measurements; #17 stays
open. No commit, push, PR or deployment was performed.


The subsequent standard-profile check exposed two checkpoint comparisons where
admission's debug assertion populated the DSP-validity memo before its snapshot.
The snapshot now precedes this diagnostic validation, retaining the original
memo on rollback in debug and release builds alike. The existing full-state
rollback comparisons remain unchanged. Standard-profile validation now also passes all 1258 tests and the working-tree
library's 803 tests. Both regenerated production Wasm binaries are byte-for-byte
identical to the browser-validated/measured checkpoint, so the existing production
browser and throughput evidence applies to this diagnostic fix.


### Elapsed sound barriers and visible-image composition

The sound replay adapter now completes an internal retirement barrier immediately
when its real target clock has already elapsed and no older operation is pending.
An identical recorded barrier still consumes its journal entry. Future internal
waits, data cycles, trace processing, interrupts and faults retain the existing
scheduler path. Original arithmetic, trace, reset, STOP/IRQ, MOVEM and bus-error
programs compare the optimized path with literal barrier replay at multiple
advancement sizes, including physical RAM traces and synthesis/output state.

The original combined scene previously stopped its sound CPU after 56
instructions. `examples/runtime-poll-sound-busy-scene.s` and `.hex` keep that
source-authored sound loop running, making active sound-CPU performance measurable.
Generate and verify it without firmware:

```sh
python3 examples/build-runtime-scene.py --poll --sound-busy --check --binary target/runtime-poll-sound-busy.bin
BROWSER_CHANNEL=chrome node scripts/benchmark-runtime.mjs --bios target/runtime-poll-sound-busy.bin --frames 30 --runs 3 --skip 5 --continuous --draw
```

Register ALU instructions can retire after their ordinary instruction-fetch and
writeback-interlock phases, avoiding the remaining operand/control dispatch.
Their shared ALU semantics, delayed target, counters, inhibition flags and clocks
are preserved. An exhaustive encoding comparison covers seven pipeline/status
contexts and compares the entire private CPU plus fetch, wait, break and discard
traces with the original dispatch tail.

VDP2 keeps one priority rank when calculation is disabled and two for ordinary
calculation. Extended calculation and horizontal gradation keep all four ranks.
For one/two-rank composition, register priorities and the special-priority low-bit
range give an upper bound for each background. Candidates are ordered by that
bound, including the existing screen tie precedence. A screen's RGB conversion
can be omitted once it cannot enter the required ranks. Sprite window and shadow
information is still decoded. All clocked normal/rotation VRAM acquisitions,
retained operands and display edges continue through the existing sequencer.
A register write flushes the preceding beam prefix before changing the bounds.

The literal four-image compositor remains a test oracle. It matches pixels and
gradation history across 11,520 original settings spanning normal backgrounds,
both rotation screens, sixteen sprite types, priority ties, special priorities,
color windows, line color, offsets, shadows, additive/second-ratio calculation,
extended calculation and gradation. The rotation cases require actual samples
from both independently configured parameter tables and the fixed RBG1 banks.

| Distinct saved-runtime comparison | Before | After | Ratio |
| --- | ---: | ---: | ---: |
| Elapsed sound barriers: original active-sound scene | 38.18 fields/sec | 39.23 fields/sec | 1.028× |
| Elapsed sound barriers: optional menu | 16.62 fields/sec | 16.85 fields/sec | 1.014× |
| Register ALU retirement: optional menu | 17.04 fields/sec | 17.26 fields/sec | 1.013× |
| Visible-image composition: original active-sound scene | 39.42 fields/sec | 42.11 fields/sec | 1.068× |
| Visible-image composition: optional menu | 17.21 fields/sec | 18.02 fields/sec | 1.047× |

These are separate alternating runtime pairs, not cumulative factors. Each menu
pair covers three successive 60-field windows at fields 1200–1380 with the same
controller events and Canvas transfer. Corresponding recorded guest snapshots
match. The final composition pair's candidate p95 times were 97.7, 97.9 and
97.9 ms. **60 fields/sec remains unmet**: even its average needs another 3.33×
throughput, and its long frames require a larger reduction.

Removing lower priority ranks without omitting hidden RGB conversion regressed
the menu by about 0.7%; it was not selected on its own. Other discarded trials
included persistent cache-loop proofs, retained future sound-retirement state,
a separate branch-retirement path and outlining operand dispatch. They passed
focused functional comparisons but did not improve the target workload enough.
An additional Wasm tail-call build changed only cold/diagnostic call sites;
All inspected CPU, sound, video and scheduler function bodies were identical, so the build
feature was not adopted.

Current validation for these retained increments:

- Standard native library: 802 passed; working-tree library: 806 passed.
- Native all-target and Wasm Clippy passed; production builds passed.
- Browser: all 74 groups passed with two workers in 242,490 ms, including 28
  original cached-scheduler cases. The comparator cleanup emitted byte-identical
  JavaScript and Wasm artifacts to the browser/throughput-tested build.
- Full standard native validation: 1,261 passed across 110 result groups, retaining
  debug assertions and overflow checks.

The working-tree application was also rebuilt and compared with the saved
Round 5 working-tree runtime. Each pair uses three alternating Canvas runs and
requires equal recorded guest snapshots; cold runs reset independently, while
the menu runs are successive windows at fields 1200–1380.

| Working-tree application | Saved earlier runtime | Current runtime | Ratio |
| --- | ---: | ---: | ---: |
| Cold initialization, fields 0–60 | 29.61 fields/sec | 39.90 fields/sec | 1.35× |
| Menu, fields 1200–1380 | 12.86 fields/sec | 18.86 fields/sec | 1.47× |

The working-tree menu's p95 times were 92.5, 93.3 and 94.2 ms. **The working-tree
application still does not reach 60 fields/sec**: its average needs another 3.18×
improvement. These application results are distinct from the isolated upstream
runtime comparisons above.

Related to #17. The firmware observations supplement
original-guest evidence; they do not establish stock-menu/game compatibility or
hardware completion.


### Cached instruction metadata and quiescent write queues

The private cached CPU attempt uses the shared interpreter with a discarded-error
policy. An unsuccessful attempt restores its private CPU/cache state; ordinary
execution still supplies the complete fault diagnostic. Register-only ISA
classification uses a 64 KiB table generated from the literal decoder. A separate
512 KiB table holds instruction timing, dependency masks and control flags.
Neither table contains guest code. Exhaustive comparisons retain the literal
classifiers and check every opcode, previous load destination, split and
forwarding state.

The frame runner can resume a bounded burst after a hardware wait within the
same field. If the wait crosses the field boundary, it completes one instruction
as before. The frame instruction budget counts the number actually retired.
The single-step interface retains its existing completion behavior.

The isolated WRAM write plan also accepts VDP1 command/texture RAM while VDP1 is
stopped. The 16-bit video bridge, BSC bus width, endian setting, original
first/last signals and pending write-buffer phases remain part of the plan.
An accepted multi-phase data write can drain before another cached store: a
private BSC preview computes the available time and protocol state, while real
board catch-up still grants and publishes each already-accepted continuation at
its clock. Earlier writes from a new isolated prefix are materialized only where
no admitted reader can observe their intermediate values; the final write uses
the ordinary acceptance/publication path. Active VDP1, framebuffer, VDP2 VRAM,
CRAM and control writes do not gain this multi-store permission. Peer CPUs, DMA,
interrupt, blanking, refresh and sound horizons retain their existing bounds.

Scanout now renders directly into its existing RGBA buffer. Reused black pixels
overwrite all four channels, and unsupported configurations preserve the already
rendered prefix. The allocated debugger/snapshot rendering API remains available.

Paired production-Wasm measurements used three alternating runs and compared
recorded guest snapshots. Values below are fields/sec and include Canvas:

| Comparison | Baseline | Candidate | Ratio |
| --- | ---: | ---: | ---: |
| Original VDP1 write loop: remaining-phase preview versus preceding candidate | 18.73 | 93.11 | 4.97× |
| Original WRAM loop: this checkpoint versus preceding verified checkpoint | 51.33 | 52.20 | 1.017× |
| Original active-sound scene: same checkpoint comparison | 42.05 | 42.19 | 1.003× |
| Working-tree cold initialization, fields 0–60: same checkpoint comparison | 38.89 | 39.99 | 1.028× |
| Working-tree menu, fields 1200–1380: same checkpoint comparison | 17.79 | 18.35 | 1.031× |

The large write-loop gain does not carry over to the optional firmware menu.
An incremental menu comparison of the remaining-phase preview measured 0.997×;
the full retained checkpoint improved the working-tree menu by about 3.1%.
Absolute rates vary across sessions, so compare the paired baselines rather than
combining rates from different sessions. **The BIOS menu still misses 60
fields/sec**: this working-tree sample needs another 3.27× average throughput,
and its candidate p95 times were 95.5, 95.6 and 95.7 ms.

The original write workload is reproducible without personal firmware:

```sh
BROWSER_CHANNEL=chrome node scripts/benchmark-runtime.mjs --vdp1-stores --skip 5 --frames 30 --runs 3 --continuous --draw --baseline /path/to/baseline/pkg
cargo run --locked --release --example runtime_benchmark -- --vdp1-stores --frames 30 --runs 3 --draw
```

The option changes the original loop's destination to VDP1 RAM; it cannot be
combined with `--bios`. Both benchmark outputs include the captured command word.

Validation for this checkpoint:

- Full standard native suite: 1,264 passed across 110 result groups. Working-tree
  library: 809 passed. Native all-target and Wasm Clippy passed with warnings denied.
- The new store oracle covers 120 valid combinations of bus width, endian,
  physical/bypass alias, byte/word/long access, PLL and active/stopped VDP1. Every
  stopped case actually uses multiple stores, including an already-queued
  multi-phase write; active cases retain serial behavior. CPU, RAM, IF state and
  complete BSC state match ordinary execution at each compared boundary.
- Full browser suite: all 74 groups passed in 243,892 ms, including 30 original
  cached-scheduler cases and command-RAM comparisons. Public native/browser
  benchmark options also passed one-field functional smoke runs.
- Both production builds, formatting, JavaScript syntax and patch whitespace
  checks passed. A compact per-pixel acquisition representation and a packed
  private memory-result representation were discarded after paired measurements.

Related to #17. These throughput checks leave the wider hardware acceptance
criteria open and do not establish stock-firmware or commercial-game compatibility.


### Cached admission and clock scheduling

The next local checkpoint accepts a scalar block when its preceding load metadata
has already been consumed by a split IF. A retained MA/EX clock still goes through
ordinary issue. The first successful scalar clears the spent pipeline metadata;
a rejected block leaves it untouched. An exhaustive comparison covers every
scalar encoding, 15 incoming contexts and one/three-instruction limits against
the original dispatch path, including retained overlap, delay slots and faults.

Cached instruction eligibility and store width now come from a 64 KiB table
generated from literal ISA classifiers. All 65,536 entries match those classifiers.
Decode-only cache peeks preserve hit/alignment/alias checks while avoiding a copy
of the 64-entry replacement array. These peeks do not alter cache replacement.

The sound scheduler retains the known integer DSP tick within one advancement,
rather than converting its rounded reference deadline back to a tick. It
reevaluates slot eligibility after CPU/control/DRAM changes and preserves an
unprocessed DSP edge when reset has priority at the same deadline. Standard
native checks compare each planned deadline with the absolute converter. An
already settled sound lookahead also needs no second zero-clock advancement;
restored checkpoints and pending control/reset work keep the ordinary call.
Focused tests compare MIDI, timers, RAM/DSP writes, reset, control changes and
subsequent execution with the literal zero-clock follow-up.

Measured working-tree production builds, with Canvas and matching CPU, sound,
clock, exception and image snapshots:

| Optional BIOS workload | Previous checkpoint | This checkpoint | Ratio |
| --- | ---: | ---: | ---: |
| Cold fields 0–60 | 41.06 fields/sec | 43.07 fields/sec | 1.049× |
| Successive menu windows 1200–1380 | 19.13 fields/sec | 19.99 fields/sec | 1.045× |

Both rows use three alternating 60-field samples from the same session and
saved working-tree artifacts. The menu's p95 frame times remain 86.8–87.6 ms.
Individual comparisons found 1.017× for spent-WB admission, 1.011× for known DSP
ticks and 1.010× for opcode eligibility. Inactive-device guards, zero-clock
follow-ups and read-only cache peeks had little or no isolated menu effect.
Those individual ratios come from separate pairs and are not cumulative rates.

Validation: 1,268 standard native tests across 110 result groups passed, with
813 working-tree library tests. All 74 browser groups, including 30 cached
scheduler cases, passed in 236,878 ms. Native all-target and Wasm Clippy,
production builds, formatting and JavaScript syntax checks passed. Longer VDP1
command horizons, including invalidated command-preview reuse, were discarded
after slower paired BIOS measurements despite their passing boundary oracles.

**60 fields/sec remains unmet:** the measured menu still needs about three times
the average throughput, with a larger improvement needed for its slowest frames.
Related to #17; hardware-completion and general firmware compatibility criteria
remain open. No commit, push or publication is part of this checkpoint.


### Retained PCM operands and isolated WRAM updates

The next checkpoint moves small PCM stage tokens through OP1–OP7 while active
voices retain their operands in a fixed seven-entry pool. Inactive voices still
advance LFOs and perform stack and DSP publication at their original edges.
Captured inhibition and routing survive later register writes. Adding a zero
sample still normalizes the DSP's signed 20-bit MIXS value. No voice, sample or
hardware clock is omitted. A test-only full-operand mode compares every stage
through key-on/off, live writes, slot 31, reset and checkpoint cloning; the
existing physical DRAM and interpolation vectors remain independent checks.

For an isolated WRAM store with no pending peer or external transfer, bus
acceptance and each physical publication retain their absolute clocks. Devices
that cannot read WRAM advance once over the admitted interval. DMA, peer CPU,
refresh, interrupt and raster events still bound that interval. This change
preserves the actual write-buffer and BSC grant protocol. Independent catch-up
without a transfer also avoids polling unchanged IRQ pins; standard native
checks assert that both source requests remain identical across each such span.
VDP1 retains its large drawing state in one allocation per command, reducing
copies when a pixel phase temporarily takes and restores that state. Cached
retirement vectors reuse their allocation across bursts.

Three alternating 60-field samples with Canvas, using actual working-tree
builds and matching CPU, sound, clock, exception and image snapshots, measured:

| Optional BIOS workload | Previous checkpoint | This checkpoint | Ratio |
| --- | ---: | ---: | ---: |
| Cold fields 0–60 | 42.56 fields/sec | 44.14 fields/sec | 1.037× |
| Successive menu windows 1200–1380 | 19.44 fields/sec | 20.13 fields/sec | 1.036× |

Menu p95 frame times remain 86.2–86.4 ms. Separate upstream-port comparisons
measured 1.011× for vector reuse, 1.018× for PCM tokens, 1.018× for WRAM device
catch-up, and 1.024× for drawing-state retention plus IRQ polling. These separate
ratios are not cumulative rates. Raster check hoisting, native-word VDP2
registers, priority-order reuse and broader write-buffer catch-up were discarded
after measurements showed no gain or regressions.

Validation: 1,269 standard native tests across 110 result groups, 814 working-tree
library tests, native all-target Clippy and Wasm Clippy passed. All 74 browser
groups, including 30 cached scheduler cases, passed in 233,421 ms. Source and
production-artifact hashes and an independently applied portable patch record
this checkpoint without including firmware data.

**60 fields/sec remains unmet:** menu throughput still needs another 2.98× on
average, and its slowest frames need a larger improvement. Sampling shows costs
spread across CPU admission, display acquisition, sound, drawing and device
updates. A bounded count also identifies successor instruction-cache misses as
a frequent reason for abandoning a cached attempt; cache-fill planning remains
future work. Related to #17; no hardware-completion or general firmware
compatibility claim, commit, push or publication follows from these checks.


### Cache-only execution across silent refresh

Captured display words now retain their address, nonzero transfer width and
physical data directly. Per-pixel entries no longer copy the normal port's
acquisition-group key. Normal retained groups still keep their key; rotation,
VRAM wrapping, physical read edges and word widths are unchanged.

An already cached instruction/data interval may cross a BSC refresh deadline
when refresh interrupts are disabled, the external bus is empty, and no peer,
DMA, self-refresh, recovery or other observable event intervenes. The real board
arbiter still executes every refresh request, grant and completion. The CPU
interval alone grows; guest clocks and external transfer timing are preserved.
A discovered store before the original deadline narrows both CPU execution and
bus planning back to that deadline. A later store ends the cache-only interval.
This preserves the existing mixed read/write path without planning writes over
an unmodeled refresh. The initially empty prefix also omits arbitration and IRQ
polling until an actual refresh edge; native checks assert unchanged IRQ sources.

The independent native oracle compares ordinary instruction boundaries across
two original guests, both PLL settings, three refresh periods and enabled/disabled
refresh IRQs. It checks full BSC state, grant ownership, remaining grant clocks,
CPU/cache state, RAM, sound and images. Required positive controls cross actual
refresh edges, mixed guests use real cached store acceptance, and enabled IRQs
retain the original horizon. Six additional browser controls cover fast/slow,
stopped and interrupting refresh plus mixed RAM writes, bringing the production
Wasm cached-scheduler inventory to 36 cases.

Three alternating 60-field comparisons of actual working-tree builds with
Canvas and matching CPU, sound, clock, exception and image snapshots measured:

| Optional BIOS workload | Previous checkpoint | This checkpoint | Ratio |
| --- | ---: | ---: | ---: |
| Cold fields 0–60 | 43.57 fields/sec | 49.36 fields/sec | 1.133× |
| Successive menu windows 1200–1380 | 19.92 fields/sec | 21.15 fields/sec | 1.062× |

Menu p95 frame times remain 86.6–86.9 ms. A separate 24-field diagnostic after
the primary measurements found a four-field pattern around 25, 45, 86 and 33 ms.
The six slow fields averaged about 262,252 main-CPU instructions, versus 237,549
for the other fields; that modest count difference does not explain the much
larger time difference. Sampled costs remain distributed across display
acquisition, CPU admission, sound, drawing and bus synchronization. These
profiled timings are diagnostics, not the primary throughput measurements.

Validation: 1,270 standard native tests across 110 result groups on the pinned
upstream port, 815 working-tree library tests, native all-target Clippy and Wasm
Clippy passed. The actual working-tree browser suite passed all 74 groups,
including 36 cached-scheduler cases, in 235,123 ms. An earlier exploratory serial
run timed out waiting for a UI reset; the isolated case and this final standard
suite passed. The cold benchmark emitted all six successful samples and its
complete matching-state JSON before its browser cleanup required interruption.

Sparse per-layer acquisition buffers, fixed cache allocation geometry, a DSP
zero-state scheduling extension, rotation-product recurrence and simple raster
interpolation shortcuts were discarded after gains were too small or negative.
Instruction-cache-fill planning remains parked after its earlier comparisons.

**60 fields/sec remains unmet:** menu throughput needs another
2.84× on average, and the slowest fields need a larger
improvement to fit 16.67 ms. Related to #17; hardware-completion and general
firmware compatibility criteria remain open. No commit, push or publication is
part of this checkpoint.

### Cached memory multiply-accumulate

A numeric-only diagnostic of the prior checkpoint found about 9,597 unsupported
cached-entry attempts in each slow menu field. Of those, 9,120 were `MAC.L`;
8,702 already had both operands in cache. No uncached-WRAM MAC operands were seen
among those classified entry attempts. This identified an omitted cached instruction class rather
than a reason to broaden external-memory admission.

`MAC.L` can now remain in an observer-bounded cached CPU burst when both operand
addresses are ordinary aligned cache hits. The second address includes the first
postincrement when Rn and Rm alias. The existing interpreter still performs the
reads, accumulator saturation, multiplier interlocks and permitted IF overlap.
At this checkpoint, cache misses and non-cacheable operands retained ordinary
scheduling; the following increment adds bounded WRAM refills.

The new native oracle compares every instruction boundary over both PLLs,
48-bit saturation, aliased/distinct operands, signed extremes, low/high RAM and
cache-bypass controls. It checks the complete CPU/pipeline, RAM, cache replacement,
BSC timing, ownership, pixels and PCM. Two original browser guests additionally
compare cached MAC operands and uncached fallback, increasing the cached-scheduler
inventory to 38. The existing multiplier timing guest also passed in Wasm.

Three alternating 60-field comparisons of the actual working-tree builds,
including Canvas and equal CPU, sound, clock, exception and image snapshots,
measured the following. Absolute rates vary between sessions; these are paired
comparisons within one session, not ratios against historical table rows.

| Optional BIOS workload | Previous checkpoint | MAC.L checkpoint | Ratio |
| --- | ---: | ---: | ---: |
| Cold fields 0–60 | 49.43 fields/sec | 49.71 fields/sec | 1.006× |
| Successive menu windows 1200–1380 | 20.74 fields/sec | 21.45 fields/sec | 1.034× |

The cold difference is too small to establish a useful startup improvement.
Menu p95 frame times decreased from 88.7–89.0 ms to 81.0–81.8 ms. Both benchmark
processes completed their samples, snapshot assertions and browser cleanup.
A separate upstream-port comparison of an original MAC-heavy CPU/drawing/sound
workload measured 11.45 to 42.97 fields/sec (3.75×); it does not represent general
game or BIOS-menu throughput.

Validation passed in the standard test profile: 1,271 native tests in each source
tree, including 816 working-tree library tests and 812 upstream-port library
tests. Native all-target and release-Wasm Clippy passed with warnings denied.
Both release Wasm builds passed the original guest checks. The actual working-tree
standard two-worker browser suite passed all 74 groups, including all 38 cached
cases, in 240,007 ms with matching aggregate artifact digests.

Tighter display-capture keys, merged display-edge iteration, untextured VRAM-write
batching, solid-color spans, broader sound lookahead and a separate SIMD build
were evaluated and left out after small gains or regressions. The retained change
adds only cached MAC admission and its regression evidence; guest clocks,
transfers, synthesis and rendering remain accounted for.

**60 fields/sec remains unmet:** the measured menu needs another 2.80× average
throughput, and its slow fields need a larger improvement. Issue #17 remains open;
this checkpoint does not establish hardware completion or general firmware
compatibility.

### Bounded WRAM cache refills

A numeric-only profile found about 2,385 initial and 2,044 subsequent high-WRAM
load misses per slow menu field. Previously each ended the cached CPU burst and
returned its line transfer to the ordinary scheduler. The scheduler now plans
these stable low/high-WRAM refills inside the same observer-bounded burst.

Each line still performs four physical longword reads, wrapping to the requested
longword last. The private BSC sequence preserves first/last signals, external
width splitting, byte order and elapsed clocks. Transactional cache-set versions
retain tags, contents and LRU; a rejected instruction or earlier sound-control
prefix discards its unaccepted tail. CCR.ID and CCR.OD independently control
instruction/data allocation, including two-way mode. Mixed writes, bypass and
peripheral accesses retain ordinary scheduling. The first physical read narrows
a cache-only refresh window to its original strict BSC horizon.

The native line-version oracle compares real fills, hits, eviction and rollback.
Instruction-boundary comparisons cover byte/word/longword loads and MAC.L over
both PLLs, low/high WRAM, bus width, byte order, replacement-disable and two-way
controls. Existing live-sound/control-prefix checks remain in force. Two original
browser guests repeatedly access eight conflicting WRAM lines, with data
replacement enabled/disabled; all 40 cached-scheduler cases match ordinary steps.

Three alternating 60-field comparisons of the actual working-tree builds, with
Canvas and equal CPU, sound, clock, exception and image snapshots, measured:

| Optional BIOS workload | MAC.L checkpoint | WRAM refill checkpoint | Ratio |
| --- | ---: | ---: | ---: |
| Cold fields 0–60 | 49.06 fields/sec | 48.81 fields/sec | 0.995× |
| Successive menu windows 1200–1380 | 22.07 fields/sec | 23.23 fields/sec | 1.053× |

Menu p95 frame times decreased from 78.3–78.5 ms to 72.7–73.1 ms. Cold throughput
changed by about -0.5%, within the baseline's sample spread; this does not
establish a startup gain or a material regression. Both comparison processes
completed their samples, snapshot assertions and cleanup. Absolute rates must be
compared within each pair, not against historical rows from another session.

A separate original guest, combining eight conflicting cache lines with live
sound and VDP1 drawing, improved from 23.99 to 47.70 fields/sec (1.988×) on the
upstream port. Its local source/ROM hashes are recorded with the checkpoint.
This deliberately memory-heavy result does not represent general game throughput.

Validation passed: 1,275 standard-profile native tests per source tree, including
820 working-tree and 816 upstream-port library tests; native all-target and
release-Wasm Clippy with warnings denied; both release builds; and all 74 standard
working-tree browser groups in 244,585 ms with matching aggregate artifact
digests. The upstream-port focused hardware-boundary, load and multiplier browser
checks also passed. No firmware is required for the original regression guests.

**60 fields/sec remains unmet:** measured menu throughput still needs another
2.58× average improvement, with a larger reduction needed for slow fields.
Issue #17 remains open. These results establish neither hardware completion nor
general firmware compatibility. No commit, push or publication is part of this
checkpoint.

### Binaryen postprocessing

Every `sh scripts/build-web.sh` build runs Binaryen 127 `wasm-opt -O4` after
wasm-bindgen. The build requires the measured optimizer version and fails early
with installation instructions if it is missing or mismatched. CI installs the
pinned official archive and checks its SHA-256 before building. No optimization
option is required, and there is no silent unoptimized fallback or build-time
download. The existing sign-extension, mutable-global, saturating-conversion,
bulk-memory, reference-type and multivalue features are retained. No fast-math or
trap-relaxing option is used. The output is replaced only after success, and
failure removes temporary output. See [tool setup](setup.md#build-and-serve-the-browser-client).

The measurements below were collected while this step was initially opt-in. The
standard build now produces the same measured artifact without an environment
variable.
The measured working-tree build is byte-identical to the Root-derived
artifact measured and checked here. Its Wasm size decreases from 1,879,797 to
1,672,104 bytes. In a separate same-session pair against the WRAM-refill build,
three alternating menu windows measured 23.22 to 24.04 fields/sec (1.035×), with
p95 72.3–73.0 ms versus 69.5–70.3 ms. This is an incremental comparison; ratios
from separate sessions must not be multiplied into a claimed combined gain.

The following direct same-session comparison measures both retained increments
against the pre-refill MAC.L checkpoint. It uses three alternating 60-field
samples per build, Canvas presentation and equal guest CPU, sound, clock,
exception and image snapshots:

| Optional BIOS workload | Pre-refill checkpoint | Refills + Binaryen | Ratio |
| --- | ---: | ---: | ---: |
| Cold fields 0–60 | 49.51 fields/sec | 50.33 fields/sec | 1.016× |
| Successive menu windows 1200–1380 | 21.97 fields/sec | 23.40 fields/sec | 1.065× |

Menu p95 decreases from 78.2–79.5 ms to 72.0–72.5 ms; cold p95 is 39.9–40.3 ms.
All six samples and snapshot assertions completed in each comparison, and both
browser processes exited successfully, including cleanup. The original combined
rotation/VDP1/sound guest showed no useful gain from this postprocessing step.

The postprocessed working-tree artifact passed all 74 standard browser groups,
including 40 cached-scheduler cases, in 238,669 ms with matching aggregate artifact
digests. The upstream port passed its focused hardware-boundary, load and
multiplier checks. The Rust/Cargo inputs are unchanged from the refill checkpoint;
its 1,275 standard-profile native tests per tree and native/release-Wasm Clippy
results therefore remain applicable. Pre-optimization wasm-bindgen output was separately
regenerated and matched that checkpoint byte for byte. Optimizer failure was
checked for a nonzero result, preservation of the prior artifact and cleanup.

For additional context, the same Rust core generated 180 native framebuffers over
fields 1200–1380 at 30.17 fields/sec in one isolated release run. Its eight common
CPU/clock/frame/exception/register/mailbox/image snapshot fields matched the
browser result. This native measurement has no Canvas presentation, does not
isolate code-generation or presentation costs, and also remains below 60.

Per-edge screen-enable masks, sparse per-layer captured-word/sample arrays,
scanline allocation reuse and an encoded cache-valid tag were checked and left
out after negligible gains or regressions. Binaryen `-O3` reduced code size but
improved the menu by only about 1%; the retained setting is `-O4`.
These rejected variants are absent from both source trees.

**60 fields/sec remains unmet:** the latest direct comparison still needs another
2.56× average throughput improvement, with more needed in slow fields.
The compiler step is now part of every standard browser build, together with
the WRAM-refill increment. Issue #17 remains open. No hardware-completion or general firmware
compatibility claim, commit, push or publication follows from these results.

## Post-checkpoint VM optimization evaluation (2026-11-10)

This local increment builds on `2d11e97`. It retains two measured changes:

- A short-lived scalar execution plan handles register operations following a
  load's retained EX/WB state. The plan uses the existing opcode semantics and
  timing table. It runs only with a ready CPU-visible IF/ID pair, a proven
  ordinary cache/retained-word successor fetch, and the existing event and sound
  horizons. Other instructions continue through the reference path. This avoids
  the full CPU/local rollback copies and common instruction setup for this case.
- Completed 68000 bus operations use a lossless 64-bit identity plus an
  eight-byte `Result<u32, u32>`. Release records shrink from 40 to 16 bytes
  on the native 64-bit target, and from 24 to 16 bytes on Wasm32.
  The pending operation remains typed; physical transfers, values, ordering and
  completion clocks are unchanged. Native unit builds additionally retain the
  original operation and compare its full equality on every replay.

Both changes apply automatically. The scalar plan is prepared from the current
CPU-visible instruction, rather than cached by PC or reconstructed from RAM.
This increment does **not** introduce an SH-2-to-Wasm JIT, a persistent decoded
block cache, or generated guest machine code.

### Combined production-Wasm comparison

The standard build produced a 1,673,072-byte module with SHA-256
`03ac1abbba7fc10553ff82fe5d7a071cb29ff0c708903e959011c94bf1afddc4`.
The actual working-tree JS and Wasm are byte-identical to the frozen candidate
used for the comparisons and complete browser suite. Measurements used local
Chrome 157.0.8103.38, Canvas presentation, alternating variant order and three
runs per variant. Timed intervals did not overlap builds, native tests, browser
validation or other benchmark warm-ups.

| Workload | Previous default fields/s | New default fields/s | Throughput ratio |
| --- | ---: | ---: | ---: |
| Local menu, fields 1200–1740, 180 fields/run | 23.367 | 23.693 | 1.01395× |
| Local startup, first 60 fields/reset | 46.809 | 46.959 | 1.00321× |
| Original conflicting-WRAM-read/audio guest, 60 warm fields | 45.966 | 46.559 | 1.01288× |
| Original drawing/rotation/audio guest, 60 warm fields | 47.199 | 47.103 | 0.99796× |

The menu uses the same late button sequence as the earlier comparison; changing
that sequence selects a different scene and is not an equivalent measurement.
All paired CPU, clock, exception, register, sound, mailbox and image snapshots
matched. The menu's median per-run p95 field time fell from 72.1 to 71.0 ms.
The original drawing/audio difference is about 0.2%; it does not establish a
speedup for that scene. Separate trials of the scalar plan and compact journal
improved the menu by about 1.0% and 1.7%; their effects must not be added together.

### Adoption decisions

The following isolated variants were implemented and tested, then left out of
the working-tree runtime. Ratios compare each variant with the previous default
in its own paired run; absolute rates from different runs are not interchangeable.

| Rejected variant | Menu throughput ratio | Decision |
| --- | ---: | --- |
| Resume a completed non-traced 68000 instruction at its final clock barrier | 0.99448× | No improvement |
| Box cold SH-2 fault status | 1.02013× initially; 0.79691× on a drifting repeat | Initial gain did not reproduce |
| Hoist raster mode/PLL invariants | 1.00939× | Marginal gain did not justify extra state |
| Bound Binaryen one-caller inlining/function size | 0.99551× | No improvement |
| LLVM inline threshold 225 plus bounded Binaryen inlining | 0.84301× | Regression |
| Predecode memory/control opcode families | 0.99614× | No improvement |
| Also predecode ordinary MOV operand addresses | 0.99736× | No improvement |
| Retain sound-clock quotients/remainders for narrow arithmetic | 0.99676× | No improvement |
| Reduce PLL ratios and use narrow arithmetic with a wide fallback | 1.00065× | No material improvement |
| Explicit four-way cache hit search | 1.00343× | No material improvement |
| Preflight cache-hit loads while keeping full instruction execution | 0.98584× | Regression |
| Extend that copy-elision proof to register operations | 0.99391× | No improvement |

A diagnostic native run over 24 menu fields counted 1,178,035 register-block
retirements in 571,078 blocks: an average of 2.06 operations, representing about
20.1% of 5,857,770 retired SH-2 instructions. It also counted 1,915,999 ordinary
cached-step attempts and 1,568,108 sound-CPU replays for 446,296 issued sound-CPU
instructions. These workload-specific counts informed the experiments; they are
not inclusive host-time fractions or a bound on a broader JIT's potential.
A future compiled-block backend still needs memory/control operations, exact
pipeline timing, CPU-visible cache/prefetch identity and precise side exits.

### Validation and remaining target

The combined source passed 1,277 native tests (822 library tests), native
all-target and Wasm Clippy with warnings denied, and the complete 74-group
browser suite including 40 cached-runtime cases. The browser suite took
240,692 ms. New differential evidence compares every scalar encoding across
load-writeback states against complete reference CPU state, fetch events and
clocks, and asserts that the new path executes in the existing cache-refill
retirement oracle. The packed journal retains the old full-operation replay
comparison in native unit tests and has identity-boundary tests. These checks
use original licensed guests; personal firmware is not a validation gate.

**60 fields/s remains unmet.** The measured menu still needs approximately
2.53× average throughput, and its slow fields need more. This is a limited
interpreter-bookkeeping increment, related to #17; it does not establish hardware
completion or general stock-firmware compatibility. Changes remain local,
without a new commit, push or publication.

## Prepared MOV execution and final CPU-only selection (2026-11-10)

This follow-up remains related to #17. The final working-tree increment retains
the compact 68000 replay records above and extends short-lived execution plans:

- Ordinary MOV loads now cover indirect/post-increment, indexed, displacement,
  GBR and PC-relative forms, including legal delay slots. The existing MA timing
  helper and load-result semantics preserve sign extension, register aliasing,
  WB dependencies, retained EX state and redirected prefetch. System-register
  loads, unproved accesses and other instructions retain the reference path.
- Prepared scalar/load classes share the previously unused high byte of the
  existing 64-bit ISA metadata. No per-PC code cache or extra guest-memory
  decoder is introduced. The literal decoder remains the exhaustive oracle.
- The general interpreter's broader opcode/operand admission checks now run
  after independently certified scalar blocks and prepared plans. Initial burst
  admission and all fallback checks remain in place.

Plans require a ready CPU-visible IF/ID pair, aligned ordinary cached operands,
actual successor fetch hits and the existing device/sound horizons. Real IFs,
LRU touches, memory accesses, pipeline clocks and peripheral work still execute.
All retained changes apply automatically. This is a prepared interpreter backend;
**SH-2-to-Wasm JIT compilation and a persistent decoded-block cache remain
unimplemented.**

### Direct comparison with the committed default

The baseline is the saved production runtime for `2d11e97`, before all changes
in this post-checkpoint follow-up. The selected standard build is
1,676,305 bytes, SHA-256
`a97785653ff57353b57086ee763ae0e5552852445a841a01696af19fdb32f7cb`.
The actual working-tree JS and Wasm are byte-identical to the frozen tested and
measured runtime. All 118 Rust source files match the selected validation tree;
two comment clarifications retain source line counts and preserve the binary.

Chrome 157.0.8103.38 measured three alternating runs per variant with Canvas
presentation and no frame skipping. Timed intervals did not overlap builds,
tests, profiles or other benchmark warm-ups. Each paired end-field snapshot
checks master CPU, clocks, exceptions, registers, sound CPU, mailbox and pixels.
These selected snapshots supplement the independent functional tests.

| Workload | Committed default fields/s | Selected working tree fields/s | Ratio |
| --- | ---: | ---: | ---: |
| Optional local menu, fields 1200–1740, 180 fields/run | 23.940 | 25.221 | 1.05350× |
| Optional local startup, first 60 fields/reset | 48.956 | 53.567 | 1.09419× |
| Original conflicting-WRAM-read/audio guest, 60 warm fields | 47.919 | 50.450 | 1.05280× |
| Original drawing/rotation/audio guest, 60 warm fields | 49.237 | 49.867 | 1.01280× |

All paired snapshots matched and all four measurement processes exited zero.
The menu's median per-run p95 field time decreased from 71.0 to
66.2 ms. The same late button sequence was used throughout; a different
sequence selects a different workload. These are unthrottled emulated-field
throughput measurements, including presentation, rather than a 60 Hz UI cap.

### Selection and rejected combinations

Isolated trials informed selection but their gains were not added together:

| Trial | Its own paired menu ratio | Final decision |
| --- | ---: | --- |
| Broader MOV/slot plans with unified metadata, versus the preceding scalar-plan build | 1.04093× | Retained |
| Defer fallback-only admission checks, versus the preceding MOV/slot build | 1.02444× | Retained |
| Defer display acquisitions across isolated spans | 0.99312× | Not retained |
| Reuse two immutable cache-hit receipts | 0.99931× | Not retained |
| Select SCSP IRQ priority with parallel bit operations | 0.99045× | Not retained |
| Compact captured display words alone, versus the MOV/slot build | 1.01541× | Not retained in the final build |
| Also pack per-dot sample descriptors | 1.01630× | No demonstrated additional gain over captured-word packing |
| Delay loop-proof snapshots until a backedge | 0.99447× | Not retained; original wait guest also regressed by about 1.3% |

A combined CPU/display candidate passed all functional checks and improved
startup and original guests, but its direct menu comparison regressed from
23.590 to 22.695 fields/s (0.96207×). It was rejected. The final selection removes
all display-storage changes and reproduces the earlier CPU-only runtime byte for
byte. The cause of that combination's regression was not established; an isolated
gain is insufficient evidence to enable the union. Additional branch plans also
lacked a demonstrated improvement beyond the preceding prepared backend.

### Final validation and remaining scope

- Complete native suite: **1,280 passed**, including **825 library tests**,
  across 109 result groups.
- Exhaustive ordinary MOV encodings and legal scalar/load delay slots compare
  complete CPU/pipeline state, MA addresses and clocks, and combined ordered
  IF/data access events against literal execution. Existing cached-burst tests
  retain positive path coverage and negative event/fault/admission controls.
- Native all-target and Wasm Clippy passed with `-D warnings`; standard build,
  formatting and whitespace checks passed.
- The exact selected Wasm passed all **74 browser groups**, including the
  **40 cached-runtime cases**, with original licensed guests. The complete
  process, reports and aggregation exited zero; Chrome teardown made this
  local run take 416,100 ms.

This update applies to the working branch. It does not claim that the separate
latest-main source port has been updated, and it does not close #17. No private
firmware is included in public assets or source. Local optional firmware is
measurement input, not a mandatory acceptance fixture. No commit, push or
publication follows from these results.

**60 fields/s remains unmet:** the measured menu average still requires about
**2.38×** additional throughput; its slow fields need a larger reduction.

## Dual-SH-2 register retirement and sound deadlines (2026-11-11)

Related to #17. When both SH-2s are enabled, the frame scheduler can retire a
register instruction directly at its existing issue clock. It uses the
interpreter's scalar operation and requires a ready IF/ID pair and a successor
word already in the CPU's cache or retained fetch word. The cache keeps its
normal replacement updates. An in-flight transaction, an eligible interrupt or
an instruction that needs a bus phase uses the ordinary scheduler. A native
differential test compares both CPUs and their issue clocks while one CPU waits
for an uncached operand.

The SCSP scheduler takes the next DSP tick from the last consumed tick. It
carries that tick through deadline selection and retains an unprocessed tick
when reset takes precedence at the same clock. This removes repeated conversion
from the reference clock in the sound loop.

The standard production Wasm, SHA-256
`9b9d746274ff3afb428ab202eaf11f90771db1c17d73ca7ad4fe5673f1e0853a`,
was compared with the production build for `3f95b42`. Chrome 157.0.8103.39
presented each field to Canvas. Each workload used three alternating pairs with
matched controller input and no concurrent benchmark or validation work in the
timed windows.

| Workload | Baseline fields/s | Candidate fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local BIOS menu, fields 1200–1560, 120 per window | 24.232 | 24.240 | 69.4 → 69.1 ms |
| Local Sound Novel Machi Disc 1, fields 1200–1380, 60 per window | 2.912 | 3.042 | 365.7 → 344.5 ms |

Each paired end state matched, including CPU and sound state, clocks and pixels.
The Machi comparison also checked the slave SH-2 and used the ordinary local
file picker and external BIOS. The candidate reproduced the previous title,
book menu and operation guide images byte for byte, including Start and A input.
Disc and BIOS files remain private and optional for these measurements.

All 1,289 native tests and 74 standard browser groups passed, including the
browser artifact digest and aggregation. Native and Wasm Clippy passed with
warnings denied. The measured game window gained 4.46% throughput. The BIOS menu
was effectively flat, and both workloads remain below 60 fields/s.

### Sound scheduling with a mounted disc

A mounted disc can supply a constant zero external sample during a pause, seek,
or data-track operation. The cached scheduler now uses the same audio-source
check as the CD block. It can advance sound ahead of CPU retirement while that
sample stays zero and no CD interrupt is asserted. The next CD event still
bounds the entire interval. An accepted CD register write retains its bus
completion boundary. Audible CD-DA uses the ordinary sound deadline.

The production Wasm with this change has SHA-256
`240554c48792ec0f21e311cde69f9fac584948a8a431b40901ad2f67700d29a2`.
Compared with the dual-SH-2 and SCSP build above, it produced the following
isolated, alternating results. Both used the original Star Collector ISO in the
drive and presented every field to Canvas.

| Workload | Earlier build | Mounted-disc scheduling |
| --- | ---: | ---: |
| Original drawing and live sound guest, fields 60–420 | 10.818 fields/s | 48.982 fields/s |
| Supplied BIOS startup, fields 260–620, total time | 68.828 s | 29.352 s |

The drawing guest row uses the median of three windows. The BIOS row sums the
three consecutive windows because they cover different stages of startup.
Paired CPU, clock, sound and image snapshots matched throughout. An original
mixed-mode native test compares sample values against ordinary execution as the
drive starts CD-DA and returns to pause. It checks that the audible interval
produces nonzero output.

Two smaller sound trials were left out of the selected build. Simplifying the
tick conversion without retaining the tick changed the BIOS menu from
24.199 to 24.143 fields/s. Reducing repeated MIDI and sound-CPU observations
improved the original drawing and sound scene from 48.806 to 49.251 fields/s.
Its standalone gain was too small to justify adding the change.

The final build was also compared directly with `3f95b42` in three isolated
pairs without a disc. The BIOS menu measured 24.462 → 24.317 fields/s, and
the first 60 fields from reset measured 52.156 → 51.971 fields/s. These changes
are below 1%. The sampled snapshots matched. The mounted-disc gain drove the
decision to keep the change.

An initial rebuild picked up another trial's Wasm from the shared Cargo output.
A separate build directory and a rebuild of the default crate output both
reproduced the measured module byte for byte. That module passed all 74 standard
browser groups and artifact aggregation. The native suite passed 1,290 tests.
A subsequent test-only Clippy fix was checked with the affected CD-DA test and
both native and Wasm Clippy.

The final Wasm ran the supplied Machi disc from BIOS reset through the operation
guide. Both SH-2s, the sound CPU, clocks and Canvas checksums matched the earlier
build at six checkpoints through field 3252. The title, book menu and operation
guide images matched the previous captures byte for byte.

## Dual-SH-2 instruction continuations (2026-11-11)

Related to #17. Resident control transfers and ordinary MOV loads can retain
their prepared execution state through a wait when both SH-2s are enabled.
The scheduler schedules the load's actual operand phases, including a cache
refill or an internal FTCSR read. A peer capture strobe can change the value
up to the ordinary sample edge. The same prepared branch semantics also apply
inside single-SH-2 cached bursts. Private branch IF stages can share one wait
when no local break or reset transition intervenes. The plan publishes the
retained word and ordered cache touches before the next instruction. Each
instruction retires at its original clock.

The replay journal retains the first observation at each distinct clock and
uses the operation cursor to reconstruct later observations. The board uses
the one-clock path when the requested span already fixes its duration. SCU
interrupt selection compares the highest pending factor in each bank, with the
same vector order at equal levels. Short CD advances bound the drive and
report phases before evaluating their rational deadlines.

[QEMU's translator documentation](https://www.qemu.org/docs/master/devel/tcg.html)
describes carrying CPU state assumptions and chaining validated blocks.
[Its instruction-counting design](https://www.qemu.org/docs/master/devel/tcg-icount.html)
shows how timer deadlines and I/O constrain those blocks. Here the SH7604's
actual IF, MA and IRQ recognition clocks constrain the prepared execution.
[Dolphin's idle-skipping description](https://nb.dolphin-emu.org/blog/2025/12/22/dolphin-progress-report-release-2512/#complex-idle-loops)
informed a separate experiment on the slave's repeated FTCSR polling.

The production module SHA-256 is
`660d88d7ec0603aedff44bfeb539c82ed00ecd412df0bd2d6634cb64f8939b16`.
Chrome 157.0.8103.39 presented each field to Canvas. The BIOS menu used
three alternating pairs of 120 fields. Machi Disc 1 used the normal file
picker and three alternating pairs of 60 fields with matched input. Both
comparisons used the preceding mounted-disc scheduling build.

| Workload | Earlier fields/s | Continuations fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local BIOS menu, fields 1200–1560 | 23.601 | 24.789 | 72.2 → 68.8 ms |
| Local Machi Disc 1, fields 1200–1380 | 2.914 | 5.935 | 361.7 → 197.9 ms |

Both SH-2s, the sound CPU, clocks and Canvas checksums matched at each timed
endpoint. The continuation build also reproduced six prior checkpoints
through field 3252 and the title, book menu and operation guide screenshots
byte for byte. The standard suite passed all 74 browser groups and verified
the tested Wasm digest. All 1,293 native tests passed in 109 result groups.
Differential checks include a peer strobe
during an FTCSR load, delayed replay across clock boundaries and SCU priority
pairs. Native and Wasm Clippy passed with warnings denied.

The game window still needs approximately 10.1× average throughput for
60 fields/s. In a diagnostic sample from the continuation build, substantial
time remained in CPU issue, bus grants, video acquisition, IRQ sampling and
sound advancement. The profile reports self samples for the built Wasm
functions, including inlined work.


## Private slave polling periods (2026-11-11)

The frame runner can combine master execution with a verified slave FTCSR
polling period. It reconstructs a complete resident cycle using the shared
interpreter, then compares the architectural state, IF/ID and load pipeline,
retained instruction word and local clocks. A reusable plan also checks the
fetched bytes and their physical cache ways. The period contains one private
FTCSR read. Retirement counters and clocks advance by the verified period.
The cache replacement updates and status-read latch retain their ordinary
results.

Master execution ends on a matching slave-period boundary before any relevant
interrupt, reset, peripheral or bus event. Accepted master writes retain their
actual bus acceptance and publication edges. A write that can affect the peer
remains a strict horizon. Sound lookahead uses a checkpoint when the shared
endpoint requires trimming a speculative prefix. Failed proofs return to
ordinary scheduling. These paths are available automatically.

The local production-Wasm comparison used the integrated main engine from
`3e16d6b` as its baseline. Chrome 157.0.8103.39 presented every field to Canvas.
Machi Disc 1 used the normal local file picker, the same controller input and
three alternating pairs of 60 fields. The BIOS menu used three alternating
pairs of 120 fields. Other local builds and correctness suites were idle during
the timed intervals.

| Workload | Main fields/s | Polling fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local Machi Disc 1, fields 1200–1380 | 6.077 | 6.771 | 187.4 → 175.7 ms |
| Local BIOS menu, fields 1200–1560 | 25.936 | 25.737 | Unrecorded |

Every measured endpoint matched CPU registers, instruction counts, exceptions,
sound CPU state, clocks and Canvas pixels. The original native and browser
`dual-poll` guest exercises guest-started slave polling alongside buffered
master RAM writes. Additional native checks cover accepted overlapping writes,
cache contents, both CPUs' retirement state and a peer capture strobe.
All 1,305 native tests passed in 111 result groups. All 74 standard browser
groups passed, including 41 scheduler-parity cases using original guests.
Registered fixtures reproduced and native/Wasm Clippy passed with warnings
denied. Exact-head CI and the extended local game recheck are recorded in
the associated PR.

The baseline Wasm SHA-256 is
`1d72ef3c130ff378a0b2caa0494812726784af0a750301f00e575935d11a6314`.
The measured polling Wasm SHA-256 is
`ab220fab647f06be4ebb089ca60ca64e769f1a381e76401da40e05f047ef3351`.

The game window still needs about 8.9× average throughput for 60 fields/s.
A separate work-RAM store-continuation experiment regressed from a median
6.081 to 5.462 fields/s and was discarded. A broader device-RAM continuation
experiment improved 6.201 to 6.402 fields/s and remains under evaluation.
A voice-slot sound-lookahead experiment changed the menu median from
25.087 to 25.183 fields/s and was discarded because the measured gain was small.

The loop work builds on the previously cited event-bounded execution and
idle-skipping designs. Yabause also describes block compilation, invalidation,
cycle counting and delay-slot handling for SH-2 dynamic recompilation. Those
techniques inform further work. This increment implements a verified polling
period within the existing interpreter. [Yabause SH-2 dynamic recompiler](https://wiki.yabause.org/index.php5?title=SH2_dynamic_recompiler)

An additional Wasm build enabled LLVM's `simd128` target feature. Three
alternating game pairs measured medians of 6.776 fields/s for both builds,
with matching guest state and pixels. The selected production build uses the baseline feature set. SIMD can accelerate data-parallel code, while this experiment
showed no throughput gain in the selected window. [V8 SIMD documentation](https://v8.dev/features/simd), [Rust Wasm target documentation](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html)
