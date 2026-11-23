# Runtime performance investigation

For the primary-source comparison of Saturn and general emulator techniques and
the next experiment priorities, see [Web optimization research](web-optimization-research.md).

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
Native emits CPU, sound, audio and display endpoint fields and checks repeated
cold-run equality, including PCM delivery.
These snapshots supplement functional regressions, not prove complete state
equivalence. No wall-clock threshold is a correctness test or CI gate.

Both tools optionally accept `--bios FILE` for an authorized raw local BIOS.
Both harnesses accept `--disc FILE` and repeatable `--track FILE`
arguments for CUE selections. Native disc measurements require `--bios` and
mount media without HLE. The browser also supports BIOS-less HLE disc boot.
Browser files are selected through a
browser-local file input, never exposed as HTTP resources. `--candidate DIRECTORY`
selects a frozen candidate build instead of `web/pkg`. Both runtime files are
loaded into the loopback server's immutable allowlist and their SHA-256 hashes
are included in the report.

Loading/reset is outside the measured interval. Native `--draw` includes
framebuffer production/copying on every field. Browser `--draw` now also includes
Canvas `putImageData`; the first-pass measurements below predate this addition. An
ending-frame checksum is always computed outside that interval. The browser
tool's optional `--profile OUTPUT` saves a local DevTools CPU profile. Its server
binds only to loopback and serves only the selected public Wasm runtime files;
firmware is never an HTTP resource and no external request is allowed.
Profiles now cover the first timed window after boot/skip and require a single
runtime, so startup does not obscure a selected game scene. The browser harness
always drains PCM and compares its rolling checksum, sample count and generated/
dropped counters, along with both SH-2 register sets and instruction counts.
It reports p50/p95/p99 field times per window and combined fields/elapsed-time
throughput. `--capture OUTPUT --draw` optionally saves a local final Canvas image;
private captures must remain unpublished. PCM draining measures core production
and delivery overhead, not actual WebAudio playback or consumer underflows.

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


## CD HIRQ passive word reads (2026-11-11)

Related to #17. A bounded cached burst can read the CD block's HIRQ word
without replaying the host scheduler for each read. The snapshot expires at
existing CD, IRQ, bus-owner and other observer deadlines. Byte and longword
accesses retain their ordinary faults. CR4 response release and data FIFO
reads retain their ordinary device transactions.

The read plan clones the existing SCU A-bus protocol alongside the SH7604 BSC.
Its accesses and elapsed clocks use the same precharge, refresh and lane-timing
methods as ordinary transactions. The selected prefix commits both protocol
states. An early trial omitted the A-bus state and changed the game instruction
count despite equal register and pixel results. That trial was corrected before
publication. The original differential guest now explicitly exercises pending
A-bus refresh, 8/16-bit SCU interfaces, SH-2 bus widths, byte ordering and both
system clocks. Omitting the A-bus plan makes this check fail with a pipeline
clock mismatch. Invalid HIRQ widths have independent fault-parity checks.

Chrome 157.0.8103.39 drew every field to Canvas. Three alternating pairs used
the preceding polling build as their baseline, with each timed endpoint
checked against the other engine.

| Workload | Baseline fields/s | HIRQ reads fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local Machi Disc 1, fields 1200–1380 | 6.832 | 6.998 | 172.4 → 170.7 ms |
| Local BIOS menu, fields 1200–1560 | 25.609 | 25.475 | 65.6 → 66.3 ms |

The game median improved 2.4%. The menu median decreased 0.5%. The game check
includes both SH-2s, the sound CPU, instruction and exception counts, clocks
and Canvas pixels. All 1,307 native tests passed in 111 result groups. All
74 browser groups passed, including 44 cached-scheduler cases. The 13 runner
regressions, registered fixture reproduction, formatting and native/Wasm
Clippy with warnings denied passed.

The measured game module SHA-256 is
`f91a3ed5469c9de7ee2f310f9342ab2e71bd37f3873e7732b760e93bcd4c4728`.
The final module, also used for the menu and browser checks, is
`2e7c0ba87e65c8e0a420d03d15776b04f34d708ad4fad895f6e3ac65df5be16e`.
Their code sections are identical. The data differences are confined to
20 Rust panic source-line fields after comments and tests were added.

60 fields/s remains unmet. Additional Web trials of SIMD code generation,
SCSP tick-conversion memoization, continuous polling stamps and isolated
slave waiting produced no selected gain. Work remains in dual-CPU scheduling,
board arbitration, interrupt sampling, sound advancement and video acquisition.


## Guarded polling prefixes and host cache lookups (2026-11-11)

Related to #17. The slave polling proof retains interpreter-computed CPU states
at individual retirement offsets. A cached master burst can start at multiple
validated loop positions and finish at one of these offsets. A bounded set of
eight templates checks complete CPU/pipeline state under clock translation,
retained instruction words, fetched bytes and physical cache ways. Publication
rebases the selected prefix's instruction counters and local clocks, applies
its ordered cache touches and records any FTCSR read. The ordinary scheduler
handles a rejected proof. Device and shared-bus observer deadlines bound the
whole interval.

Recording CPU-state assumptions and recovering state at instruction boundaries
also underpins dynamic translation. Execution here uses the shared interpreter
and memoized prefix states. [QEMU translator internals](https://www.qemu.org/docs/master/devel/tcg.html), [QEMU instruction counting and I/O](https://www.qemu.org/docs/master/devel/tcg-icount.html)

A host-only lookup retains the most recently matched tag/way for each SH7604
cache set. Mode changes, purges, tag-array writes and committed shadow fills
invalidate the affected entries. Relaxed `AtomicU32` accesses preserve the
public `Cache` type's `Send + Sync` traits. Native regressions cover duplicate
tags, two-way mode, purges and a warmed lookup across a shadow eviction. The
polling retirement mask checks its recorded range before shifting.

The original dual-CPU guest now also runs with live PCM, DSP memory activity,
sound interrupt-control writes and sound reset output. Native differential
checks exercise both clocks, emitted samples, complete CPU states and renewed
SCU sound requests. The production browser scheduler runs an original coupled
guest and requires nonzero PCM while comparing ordinary steps against fields.
All 1,311 native tests passed in 111 result groups. All 74 browser groups passed,
including 45 scheduler cases. The 13 runner checks, registered fixtures,
formatting and native/Wasm Clippy with warnings denied passed.

Chrome 157.0.8103.39 drew every field to Canvas. Three alternating pairs used
the CD HIRQ build from the preceding main merge as their baseline. Every timed
endpoint matched CPU, sound, clocks and pixel state.

| Workload | Earlier fields/s | Combined fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local Machi Disc 1, fields 1200–1380 | 6.782 | 7.136 | 176.4 → 176.0 ms |
| Local BIOS menu, fields 1200–1560 | 25.775 | 26.393 | 65.4 → 63.6 ms |

The measured module is the final default build, SHA-256
`255c85ba1d97c4f8813c47eec9e7c877591d33563c526b04926fa0831d2bab74`.
It reproduced all six local game checkpoints through field 3252, including
byte-identical title, book-menu and operation-guide screenshots. The game
median improved 5.2% and the menu median improved 2.4%. 60 fields/s remains
unmet, requiring about 8.4× additional throughput in the measured game window.


## Short CPU waits and display acquisition deadlines (2026-11-11)

Related to #17. A completed instruction can use the existing independent
hardware path for a one-main-clock wait when both CPUs remain behind their
readiness boundaries and the normal DMA and observer guards pass. A PAL PLL
edge that produces zero main clocks retains the ordinary reference-clock
advance. A new regression compares 8,192 boundaries across both video
standards and PLL selections, including fractional phase, pending main ticks,
raster counters, interrupt recognition and emitted samples.

The board retains the next normal/rotation display acquisition deadline.
Repeated scheduler calls before that deadline reuse it. VDP2 register writes,
video-standard changes and PLL resets invalidate it. Buffered WRAM publication
can advance board time before display catch-up. That catch-up invalidates the
memo at each intermediate beam position to retain every physical acquisition.
The existing original shared-hardware guest detects an omitted invalidation
through its captured pixels.

The 1,311 existing native tests and the added PLL regression passed. All 74
browser groups passed, including 45 cached-scheduler cases. The 13 runner
checks, registered fixtures, formatting and native/Wasm Clippy with warnings
denied passed. Trials that shortened longer post-retirement waits produced
clock mismatches and were corrected to the one-clock admission above. A
separate sound-checkpoint elision trial produced no selected throughput gain.

Chrome 157.0.8103.39 drew every field to Canvas. Three alternating comparisons
used the guarded-prefix build from PR #118 as their baseline. Every timed
endpoint matched CPU, sound, clock and pixel state.

| Workload | Earlier fields/s | Combined fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local Machi Disc 1, fields 1200–1380 | 6.989 | 7.201 | 177.8 → 173.7 ms |
| Local BIOS menu, fields 1200–1560 | 26.436 | 26.382 | 63.0 → 63.2 ms |

The measured module is the default build, SHA-256
`5a752de3b192c5b591b4bb6c0cc94e3974b5a18e7ae8bb2a60c6b040beefbb54`.
The game median improved 3.0%. Menu throughput changed by −0.2%. 60 fields/s
remains unmet, requiring about 8.3× additional throughput in the measured game
window.


## Prepared MOV store continuations (2026-11-11)

Related to #17. The default dual-CPU frame runner prepares ordinary MOV stores
when their instruction stream is resident and existing on-chip guards pass.
It latches the source value, retains the successor IF and queues the actual MA
wait. Resumption uses the existing buffered-write acceptance, bus arbitration,
endianness, width splitting, cache update and deferred-fault paths for the
issuing processor. CPU state publishes after acceptance. System-register
stores retain their existing MDU and control paths.

Load and store preparation share the existing WB/EX and fetch timing. Native
literal vectors compare all 21 ordinary store forms across both IF alignments
and three completion schedules. Additional regressions preserve a successor
fetched before its code is overwritten and check slave stores while the master
reads their shared RAM.

All 1,314 native tests passed in 111 result groups. All 74 browser groups passed,
including 45 cached-scheduler cases. The 13 runner checks, registered fixtures,
formatting and native/Wasm Clippy with warnings denied passed.

Chrome 157.0.8103.39 drew every field to Canvas. Three alternating comparisons
used the short-wait/display build from PR #119 as their baseline. Every timed
endpoint matched CPU, sound, clocks and pixels.

| Workload | Earlier fields/s | Store continuation fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local Machi Disc 1, fields 1200–1380 | 7.092 | 7.203 | 174.4 → 164.8 ms |
| Local BIOS menu, fields 1200–1560 | 26.247 | 26.250 | 63.1 → 63.2 ms |

The measured default module SHA-256 is
`6a2fc8b9a9168187d83d9b5a7aaba4877f5d1c4683fb7014a30cf3b9c6f4e024`.
Six extended local game checkpoints and three screenshots match the preceding
production captures. The game median improved 1.6% and menu throughput was
unchanged at this precision. 60 fields/s remains unmet.


## Private polling continuations (2026-11-12)

Related to #17. The default frame runner can reuse private slave polling while
that processor is suspended inside a resident instruction. A bounded proof
executes the existing interpreter with the actual private continuation, including
its journal, retained IF, pipeline and local instruction epoch. It records the
transient prefix and a closed polling period. Reuse checks full CPU and
continuation state, FTCSR, cache control, fetched bytes and physical ways.

Two cache-replacement seeds establish each phase's keep/value transformation.
Publication rebases CPU and journal timestamps, local counters, read/clear
bookkeeping and replacement bits. The actual board and sound domains advance
through their existing event-bounded paths. Master retirement records retain
both retirement and readiness times. A selected endpoint yields master issue
before publishing a pending slave phase, preserving same-clock fault ordering.
The proof cache holds 32 templates and retains its scratch machine between
attempts. Every attempt resets probe transaction ownership and journals.

Original native guests compare every published phase across both PLLs,
PAL/NTSC, two code alignments and ordinary/delayed-slot FTCSR loads. Comparisons
include complete CPUs, both caches and bus timing. A prepared branch can retain
its existing readiness representation during a partial replay wake. Subsequent
publications check completion. Negative controls cover status, cache bytes,
cache control and a rejected external probe followed by a valid private proof.

An asserted sound/CD pin with an unprocessed SCU effect is an immediate
hardware horizon. This includes a cleared IST factor, CD acknowledgement
consumption and a newly armed sound-triggered DMA channel. The fixed tick
publishes that effect before cached work resumes. An original regression covers
both sources, preserves the CPUs on refusal and checks the subsequent ordinary
publication. Negative controls remove the horizon, acknowledgement handling and
DMA-trigger handling independently. Each fails the regression.

The complete native suite passed 1,318 tests in 111 result groups before the
final acknowledgement/DMA refinement. The refined guard passed all 849 native
library tests locally. The full native suite runs again in CI. All 74 production-Wasm
browser groups passed, including 46 scheduler cases and an original delayed
FTCSR guest. The first shared-shard run exposed a duplicated fixture mutation
in the new test. After correcting that test, all 33 shared groups passed. All
41 scenario groups passed. The 13 runner checks, registered fixtures, formatting
and native/Wasm Clippy with warnings denied passed.

Chrome 157.0.8103.39 drew every field to Canvas. Three alternating comparisons
used the prepared-store build from PR #120 as their baseline. Every timed
endpoint matched CPU, sound, clocks and pixels.

| Workload | Earlier fields/s | Private continuations fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local Machi Disc 1, fields 1200–1380 | 7.324 | 7.590 | 161.0 → 157.3 ms |
| Local BIOS menu, fields 1200–1560 | 26.438 | 26.220 | 63.0 → 63.5 ms |

The measured default module SHA-256 is
`c0c08ea383f48eff5edbee6c3af655d46299cc5ea1b9d9cfa6c59d184aae0a0b`.
Six extended local game checkpoints through field 3252 and byte-identical title,
book-menu and operation-guide screenshots match the preceding captures.
Game median throughput improved 3.6%. Menu throughput changed by −0.8%.
60 fields/s remains unmet, requiring about 7.9× additional throughput in the
measured game window.


## Uncached WRAM read bursts (2026-11-12)

Related to #17. The Web field runner can include a cache-bypassing byte, word
or longword read from canonical low/high WRAM in an existing bounded burst.
Admission snapshots up to four bytes at the current operand address. The
existing passive-read protocol charges the SH7604's actual external bus width,
waits, DRAM/SDRAM state and lane order, and commits the selected prefix's bus
state. Quiet-bus admission, a private peer proof and the existing DMA,
interrupt, refresh and device horizons bound the snapshot's lifetime.

An original native matrix covers both PLL selections, both WRAM areas, narrow
and wide interfaces, BCR1's byte-order setting, all three operand widths, the
last byte of WRAM and data changed between fields. Both CPUs, RAM, cache
replacement, BSC state, clocks, sound and pixels match ordinary execution.
The production-Wasm scheduler adds low-WRAM byte and high-WRAM longword cases
to the original dual-CPU guest. All 48 scheduler cases passed. The complete
native suite passed 1,319 tests in 111 groups, and all 74 browser groups passed.
Native and Wasm Clippy passed with warnings denied. Fixture reproduction and
the 13 browser-runner checks passed. The runner needed the local HTTP sandbox
permission and the installed Chrome channel after the initial environment-only
failures.

A negative control that reverses WRAM operand lanes fails the new native
regression. An initial control that suppressed area-2 little-endian handling
passed because canonical WRAM belongs to areas 0/3. The revised control changes
the WRAM data path actually exercised by the regression.

Chrome 157.0.8103.39 drew every field to Canvas. Each result is the median of
three alternating comparisons with PR #121's default module. Every timed
endpoint matched the exposed CPU, sound, clock and pixel state.

| Workload | Earlier fields/s | WRAM reads fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local Machi Disc 1, fields 1200–1380 | 7.681 | 8.144 | 155.8 → 153.3 ms |
| Local BIOS menu, fields 1200–1560 | 25.865 | 25.706 | 64.7 → 64.7 ms |

Game median throughput improved 6.0%, with a 5.7% improvement across the
combined measured time. An earlier three-pair run of the same module measured
7.305 → 7.885 fields/s, a 7.9% median improvement. The menu median changed by −0.6%. The measured module
SHA-256 is
`7f96c0e53b67bc9bb1d6e63a7beaabffd470cb5b0c4a56824ae065b6cbff93e6`.

A scalar Wasm JIT trial passed 27,660 reference vectors generated by the shared
Rust scalar executor, 60 optimized cross-module ABI cases and the game state
comparisons. Its first version changed game median throughput by −1.2%.
Separate full/prefix functions and a set-associative plan lookup changed the
median by +0.3%, with +0.1% across combined measured time. A wider WRAM region
view measured +6.0% in combined time, and dense-retirement sound checkpoint
elision measured +0.6%. The selected increment uses the four-byte operand
snapshot. These results continue the event-bounded execution investigation
above.

60 fields/s remains unmet. The measured game window needs approximately 7.4×
additional throughput. Broader dual-CPU synchronization remains under
investigation after a prototype first diverged at field 990.

## Reuse private polling proofs during probe backoff (2026-11-12)

Related to #17. The default Web field runner now reuses a validated private
Slave polling proof during the existing 64-clock backoff for constructing
new proofs. Each lookup validates the full CPU/pipeline/readiness state,
private transaction, FTCSR observation, cache control, resident code bytes and
cache ways through the existing matcher. A last-successful template hint tries
that same matcher before the linear search. Repeated misses during backoff keep
the scheduled retry time, allowing construction to resume when due. The existing
bus, interrupt, DMA, sound and raster horizons bound publication.

Two original native regressions exercise reuse during backoff, compare two
complete fields against ordinary scheduling with both CPUs/caches/BSCs and
clocks, and reject stale proofs after FTCSR, resident code and cache-control
changes. Restoring the old blanket retry refusal in an isolated negative control
fails the positive reuse assertion. The complete native suite passed 1,321 tests
in 111 groups. All 74 production-Wasm browser groups, all 48 scheduler cases,
13 browser-runner checks, fixture reproduction, formatting and native/Wasm
Clippy passed.

Chrome 157.0.8103.39 drew every field to Canvas. Three rotated game comparisons
and three alternating menu comparisons matched every exposed CPU, sound, clock
and pixel endpoint against PR #122's default module. Extended local checks
also matched fields 1500, 1800, 2100, 2400, 2824 and 3252. The title, book and
guide captures were byte-identical to the earlier reference captures.

| Workload | Earlier fields/s | Proof reuse fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local Machi Disc 1, fields 1200–1380 | 8.204 | 8.999 | 148.6 → 152.6 ms |
| Local BIOS menu, fields 1200–1560 | 25.706 | 25.627 | 64.5 → 65.0 ms |

The game median improved 9.7%, with a 12.4% improvement across combined measured
time. A separate three-pair repeat measured 8.007 → 8.472 fields/s, a 5.8%
median improvement and a 9.2% improvement across combined time. Every timed
endpoint matched again. Menu median throughput changed by −0.3%. The measured default module's
SHA-256 is
`b6dbef290593ccde10f69c62a28d387cef71933bdd798a436b154f6034cb2d8c`.

An independent single-instruction admission trial allowed cached issue during
BSC waits while charging the pending wait ledger at retirement. Its original
BSC/wait regressions and real-game state comparisons passed, and combined game
throughput changed by −1.6%. A broader private-CPU macro trial corrected its
field-990 divergence by refusing an already accepted peripheral write, then
matched all fields through 1200. It reduced combined throughput by 17.2%.
A phase predictor reduced that regression to 6.7%. These trials remain excluded
from the default module.

A read-route diagnostic over game fields 1200–1500 counted 8,339,235 Master
high-WRAM cache hits and 807,589 allocating misses through the ordinary issue
path, excluding complete cached bursts. No disabled-cache or disabled-data-fill
misses occurred in that sample. The diagnostic endpoint matched the ordinary
reference. Reusing private peer states across more issue phases remains under
investigation. The measured game window still needs approximately 6.7× more
throughput to reach 60 fields/s.


## Cached loads at a single Slave issue boundary (2026-11-12)

Related to #17. A cached Master load can retire while Master is immediately
ready to issue again at the same clock. The ordinary scheduler attempts one
Slave issue and then arbitrates the bus at that boundary. The private polling
proof now records the number of due issue attempts at each clock, with agreement
between both cache-replacement seeds. A burst can end at such a Master retirement
when the proved phase needs zero or one Slave issue attempt. The existing full
CPU, transaction, FTCSR and resident-code validation certifies each reused proof.
The field runner applies this automatically.

The new native cached-load guest compares individual admitted retirements with
ordinary inline issue at two peer phases and both PLL settings. Complete fields
also match uncoalesced replay, including both CPUs, cache contents, BSC state,
RAM, sound and pixels. A second original guest drives capture strobes at four
initial peer phases in both PLL settings and compares FTCSR and both capture
register byte lanes. The complete native suite passed 1,323 tests in 111 groups.
Formatting, native/Wasm Clippy and fixture reproduction passed. All 74 browser
groups, 48 scheduler cases and 13 browser-runner checks passed on the final
production Wasm.

Restoring the zero-issue restriction fails the positive coverage assertion.
Allowing additional same-clock Slave issues fails the instruction-boundary
comparison. An initial scalar-only negative control passed because it did not
exercise these cached-load retirements. The refined control checks the affected
boundary directly. The first capture-register check used unsupported 16-bit
accesses. It was corrected to compare successful 8-bit reads of both lanes.

Chrome 157.0.8103.39 presented every measured field to Canvas. Three rotated
60-field game windows used the local supplied BIOS and Machi Disc 1 through
the ordinary file pickers. Every exposed CPU, sound, clock and pixel endpoint
matched PR #123's default module. Extended checks matched fields 1500, 1800,
2100, 2400, 2824 and 3252, and the title, book and guide captures matched the
previous reference.

| Workload | Earlier fields/s | With peer boundary proof, fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local Machi Disc 1, fields 1200–1380 | 8.930 | 10.085 | 152.1 → 143.0 ms |
| Local BIOS menu, fields 1200–1560 | 25.986 | 26.238 | 64.1 → 63.7 ms |

Median game throughput improved 12.9%. Throughput across all three timed
windows improved 13.5%. Menu median throughput improved 1.0%, with a 0.9%
improvement across combined timed throughput. The measured production Wasm SHA-256 is
`f3dc558b73a7c380aeab53644843831efd645fbadc2b0e5f0ee16577633ff6dd`.

An earlier trial that accepted idle peer phases measured 8.782 → 9.627 fields/s
in a separate three-pair comparison. Its menu median changed from 24.876 to
24.453 fields/s. A trial combining cache refills, grouped WRAM writes and shadow
tag lookup reuse improved throughput by 1.6% in a five-way comparison. Those
trials informed this smaller selected increment. The measured game window still needs approximately
6× further throughput to reach 60 fields/s.


## Publish the first private peer issue at a ready Master endpoint (2026-11-12)

Related to #17. The default Web field runner now retains the Slave state after
its first issue attempt and bus grant when a proven clock phase needs further
zero-time attempts. If Master can issue again immediately, publication selects
that first state, matching one ordinary shared issue boundary. If Master is
waiting, publication selects the complete phase. Both publications validate CPU,
pipeline, transaction, FTCSR observation and epoch under both LRU seeds. Their
replacement masks retain the initial replacement bits that ordinary execution
would preserve. Existing code, interrupt, reset, DMA, sound and raster guards
still bound reuse and publication.

This removes the prior restriction that an immediately ready Master endpoint
must have at most one total due Slave attempt. The pending-peer schedule no
longer needs the proof pointer or a per-retirement readiness value. Retirement
positions, compressed periods and sound-prefix selection retain their existing
clock positions.

The original cached-load boundary regression now requires actual publication of
the first state from a phase with additional due attempts. It compares 512
boundaries at two initial phases, two complete fields against ordinary replay,
both PLL settings and full CPU/cache/BSC state. The inherited strobe matrix also
checks successful capture-register byte accesses and FTCSR ordering. An isolated
negative control that always publishes the completed phase fails the boundary
comparison: Slave readiness is incorrectly left at `u64::MAX` instead of the
current issue clock. The source was restored before the final build.

Chrome 157.0.8103.39 presented every field to Canvas. Three rotated game
comparisons and three alternating menu comparisons used the ordinary local file
pickers and matched input. No other local build, test or warm-up ran during the
timed intervals. Each timed endpoint matched exposed CPU/sound state, clocks,
counts and Canvas pixels. Extended checks matched fields 1500, 1800, 2100, 2400,
2824 and 3252, and all three reference PNG captures matched byte for byte.

| Workload | Previous default fields/s | Partial peer fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local Machi Disc 1, fields 1200–1380 | 9.637 | 10.022 | 148.0 → 144.5 ms |
| Local BIOS menu, fields 1200–1560 | 25.266 | 25.289 | 65.9 → 66.3 ms |

Game median throughput improved 4.0%, with a 3.7% improvement across combined
measured time. Menu combined throughput changed by +0.3%. A preliminary version
retaining the now-unused scheduling metadata measured a 4.6% combined game gain
in the same rotated comparison; the smaller representation was selected. The
selected module SHA-256 is
`b5a8bee5d7fbdbb5c16cb330dd1fe1c891dcc6d8cf26d96b828f9a929552d9e5`.
The standard public build is byte-identical to the measured module.

The native suite passed 1,323 tests in 111 groups. Formatting, native/Wasm Clippy,
native build and fixture reproduction passed. All 74 production-Wasm browser
groups, all 48 scheduler comparisons and 13 browser-runner checks passed.
Exact-head hosted validation is recorded in the associated PR.

60 fields/s remains unmet; this measured game scene still needs approximately
6× additional throughput. A fresh profile of the preceding default module puts
51.2% of samples inside cached-burst execution, including its callees, and 22.3%
in that function itself. Further work targets repeated setup, state copies and
execution dispatch. QEMU describes direct block chaining to avoid repeated main
loop dispatch and precise per-instruction state restoration. Its instruction
counting documentation also separates timer budgets from cycle-accurate timing
and handles I/O with short blocks. Those techniques inform further trials;
Saturn's bus and pipeline timing remain part of every comparison.
[QEMU translator internals](https://www.qemu.org/docs/master/devel/tcg.html),
[QEMU instruction counting](https://www.qemu.org/docs/master/devel/tcg-icount.html).


## Cached replay and same-clock peer publication (2026-11-12)

Related to #17. A cached load followed by an independent ALU instruction can
retire both Master instructions at one clock. Publishing the Slave state after
one issue attempt then leaves it behind ordinary shared scheduling, which issues
Slave once per Master boundary. The burst now counts Master retirements sharing
the selected final clock. A private polling proof retains the state after each
possible same-clock Slave issue and grant, validated under both LRU seeds.
Publication selects the state after the corresponding number of attempts, or
the completed phase when Slave is already waiting. A waiting Master still uses
the completed phase. All selected Master retirements are retained.

General cached replay also updates the current private CPU directly and keeps
its prior state for rollback. Failed instructions and the sound retirement-gap
check restore the CPU and existing memory/protocol snapshot. Scalar blocks,
prepared operations, observer horizons and actual bus publication are unchanged.

An original regression stops a burst after the load and ALU operation with an
unsupported LDC SR successor. Both PLL settings and 16 initial Slave phases
compare complete CPUs, cache contents, BSC state, clocks, FTCSR observation and
private epochs against ordinary shared scheduling. The test requires actual
two-instruction, immediately ready Master endpoints. It fails on both the main
baseline and the initial replay-only candidate, with one versus two Slave
retirements. Forcing one-attempt publication on the corrected implementation
also fails the test.

A second original guest runs predecrementing WRAM stores with slow byte-wide
BSC transfers alongside a live sound CPU at both PLL settings. It compares each
selected prefix with ordinary execution. Omitting failed-instruction CPU
restoration fails 27 inherited cases. Omitting sound-gap restoration initially
passed all 43 inherited cases; this new guest detects a 36-versus-70
reference-clock mismatch. Both mutations were restored. The fixture's initial
reserved WCR idle encoding was corrected to the accepted `0xAAFF` setting.

Chrome 157.0.8103.39 presented every field to Canvas through the ordinary Web
entry points. Four alternating game comparisons and three alternating menu
comparisons matched every exposed CPU/sound, count, clock and pixel endpoint
against PR #125's default module. Timed intervals ran without another local
build, test or warm-up. The byte-identical corrected module also matched six
extended checkpoints through field 3252 and three reference PNG captures.

| Workload | Main fields/s | Corrected replay fields/s | Median p95 field time |
| --- | ---: | ---: | ---: |
| Local Machi Disc 1, fields 1200–1440 | 10.653 | 10.653 | 142.3 → 144.6 ms |
| Local BIOS menu, fields 1200–1560 | 25.335 | 25.538 | 66.2 → 66.0 ms |

Combined-window throughput was essentially unchanged: +0.07% in the game and
−0.06% in the menu. Earlier isolated CPU-copy comparisons showed small gains;
those results are not a claim of a gain for this corrected implementation.
A conservative endpoint fix that trims same-clock Master work passed the
state comparisons but regressed combined game throughput by about 2.2% against
the initial replay-only candidate. The selected fix retains all that work.
A separate-code replay helper reduced emitted burst code from about 73 KB to
53 KB, but changed combined game throughput by only +0.6% and was not selected.

The corrected module SHA-256 is
`65da67ad7a73d13d2301dd698b488178fc05a5691db6fdebcf739c677823faa6`.
The standard public build is byte-identical. Formatting, native build, all
1,325 native tests in 111 groups, fixture reproduction, native/Wasm Clippy,
all 74 browser groups, 48 scheduler comparisons and 13 browser-runner checks
passed. Additional assertions of the Slave epoch, observation and FTCSR passed
the focused regression. Exact-head hosted validation is recorded in the PR.
60 fields/s remains unmet; the measured game scene needs approximately 5.6×
additional throughput.


## Borrowed comparison of private CPU and transaction state

Related to #17. Private polling guards previously cloned a complete CPU and
transaction journal, translated their timestamps, then compared them with the
current state. The default runner now compares borrowed fields with the same
clock translation. It preserves every CPU field except the same two cumulative
counters already normalized by the old guard. Journal operations, values, phase
indices, pending state and all other fields remain strict comparisons.

Exhaustive destructuring makes a newly added field a compile-time review
requirement. Every native unit-test invocation additionally compares the new
answer with the old full clone/shift/derived-equality oracle. Existing code-byte,
cache-way, FTCSR, global bus-owner and observer guards remain in place. This
changes host validation work, not guest execution or timing.

The selected combination also retains independent WRAM refills after an
accepted Master write and reuse of VDP2 scanline vector capacities. Refills
wait for every remaining physical write beat, continue its BSC protocol and
reject overlap with the complete physical refill line. Actual writes still
drain through the ordinary arbiter. VDP2 clears every logical entry and all
other fetch state at the original boundaries before reusing storage.

### Web comparison and adoption

Chrome 157.0.8103.39 drew every field to Canvas through the normal local file
pickers. Four rotated 120-field windows compared main, CPU-only matching,
CPU+journal matching and matching on main without the two earlier changes.
Timing did not overlap builds, tests or warm-ups. All 32 timed endpoints
matched CPU, sound, clocks and pixels.

| Variant | Game combined throughput change | Menu combined throughput change |
| --- | ---: | ---: |
| CPU comparison with the earlier changes | +3.90% | +1.82% |
| Selected CPU+journal comparison with the earlier changes | +5.59% | +0.21% |
| CPU+journal comparison on main alone | +2.99% | −0.11% |

The selected combination improved every paired game window. Game median was
10.344 → 11.001 fields/s over fields 1200–1680; median p95 fell from 165.2 to
159.6 ms. Menu median was 24.668 → 24.665, with median p95 67.8 → 67.4 ms.
The menu result is effectively unchanged. Individual changes are not additive;
the selected combination is the measured result.

The earlier refill/scanline combination initially measured +2.08% game and
+2.40% menu, but repeats measured only +0.12% game and then +0.03% game/+0.35%
menu. Those measurements did not establish a stable isolated gain. They are
retained here as part of the faster measured combination, whose main-only
counterfactual is included above. A broader passive-RAM stream and a shared
passive-read/refill sequence did not provide a selected improvement. Separating
cold proof construction measured only a preliminary approximately 1% difference
and is not included.

The measured selected Wasm is 1,810,826 bytes, SHA-256
`2697a2161dcba3aad01a0c061d8ef075e7d44d05fb1042630975b28774d86309`.
Personal media and captures are not included.

### Validation and limits

The selected isolated source passed all 857 library tests, including the
old/new full-state matching oracle on every call. The accepted-write/refill
guest requires real accelerated independent reads and overlap fallback at both
PLLs with byte-wide and longword-wide buses. It compares complete CPU, cache,
BSC, RAM, readiness and clocks. The former implementation fails its positive-path
assertion. Removing the drain wait makes execution clocks six states early;
removing overlap protection admits the prohibited stale line. Both negative
controls fail and restored-source validation passes. Existing original VDP2
multi-line capture and literal-pixel oracles cover allocation reuse.

The selected implementation passed all 1,326 native tests in 111 groups,
formatting/build, native all-target and Wasm Clippy with warnings denied,
registered fixture reproduction, all 74 production-Wasm browser groups,
48 scheduler comparisons and 13 runner checks. Browser elapsed was 207,100 ms.
The standard JS/Wasm bytes are identical to the measured module. Five extended
game checkpoints at fields 1800, 2100, 2400, 2824 and 3252 and three reference
PNG byte comparisons passed.

An earlier unrestricted native run on the preceding refill/scanline-only head
was SIGKILLed in BSC without an assertion failure. Its full retry and this
selected implementation's complete command passed at `RUST_TEST_THREADS=2`.
The signal's cause was not established. No test content or CI concurrency was
weakened. Hosted exact-head validation remains mandatory before normal merge.

60 fields/s and hardware completion remain unmet. The measured game median
still requires approximately 5.5× further throughput. This increment does not
establish general firmware compatibility.


## Ordered cache refills and isolated writes

Related to #17. The default Web runner now keeps cache refills and isolated
WRAM/idle-VDP1 writes in the same bounded private execution interval. Previously,
a physical read ended the write path and a staged write prevented later refills,
forcing repeated ordinary scheduler admission in copying and drawing loops.

The optimization uses ordered copy-on-write cache-set versions and external-bus
protocol checkpoints. Each physical refill waits for all preceding write beats
and sees earlier staged physical bytes, including P2 aliases. A cached write
updates every matching valid way, preserving two-way restrictions and literal
LRU order. Later refills may evict those versions normally. Failed instruction
attempts roll back the cache versions, write prefix and bus event sequence
together. Read-only repeat acceleration requires an unchanged bus-event count.

The final external write is still accepted and drained through the existing
arbiter. Read protocol is published after that write only when a later completed
refill exists. Private raw-byte forwarding precedes the normal CPU-endian
conversion. Already decoded instructions retain their original word; redirected
fetches see the updated cache or physical line. Observed MMIO stores after a
refill or RAM prefix retain ordinary execution. Existing DMA, peer, sound, IRQ,
blanking, ownership, address and refresh bounds are unchanged, including the
16-store limit. No instruction, device edge or rendered field is skipped.

### Web comparison and adoption

Chrome 157.0.8103.39, the standard production JS/Wasm build and normal local
file pickers were used. Four rotated 120-field windows over fields 1200–1680
drew every field to Canvas. Game and BIOS-menu timing ran separately, without
local builds, tests or warm-ups. Complete CPU/sound/clock/pixel endpoints matched
between all four variants after every window (32 endpoints in total).

| Variant | Game fields/s | Change | Menu fields/s | Change |
| --- | ---: | ---: | ---: | ---: |
| Main before this increment | 10.952 | — | 24.534 | — |
| Per-set host cache lookup only | 11.010 | +0.53% | 24.298 | −0.96% |
| Selected ordered read/write versions | 11.811 | +7.84% | 25.801 | +5.16% |
| Ordered versions plus per-set lookup | 11.797 | +7.72% | 25.978 | +5.89% |

Throughput is 480 fields divided by the combined elapsed time, not an average
of individual rates. The selected version improved every paired game and menu
window; game elapsed was 43,828.0 → 40,640.3 ms and menu elapsed was
19,564.9 → 18,604.1 ms. Median window p95 was 161.75 → 156.8 ms for the game
and 67.7 → 62.2 ms for the menu. The per-set lookup change is not adopted:
it adds no game gain to the selected implementation in this comparison.
These local samples do not establish hosted, cross-browser or cross-game rates.

The selected Wasm is 1,818,827 bytes, SHA-256
`f710a7cd75e1b7d6503d8a8e400a44411ce6f90224b0fc0acf606fff6ecde385`.
The standard build's JS/Wasm bytes match the measured frozen files. Private
firmware, media, traces and captures are not published.

### Validation and remaining work

Four original native tests add ordered duplicate-way/LRU/rollback comparison,
real forward copies with aliases/overlap/conflicting tags, a write to an already
decoded instruction and a write before a cold target refill. Both PLLs and
four/two-way caches are covered, with selected narrow-bus and live-sound cases.
The guests compare full CPU, memory, cache and BSC state against ordinary
execution after each accelerated prefix, and require actual mixed read/write
acceleration. The previous implementation fails that positive-path assertion.

Three intentional runtime defects are independently rejected: omitting physical
write forwarding fails the cold-target guest, omitting cache writes fails the
retained-decode guest, and omitting bus-event rollback fails the existing
instruction-refill/eviction/write-boundary guest. The copy guest alone did not
reject missing bus rollback; the existing boundary guest supplies that coverage.
The restored source passes. These cases add defect detection at the native
layer without another browser scenario or a larger CI concurrency setting.

Five extended game CPU/sound/clock/pixel checkpoints at fields 1800, 2100, 2400,
2824 and 3252 and three reference PNG byte comparisons pass. All 1,330 native
tests in 111 groups, formatting/build, native all-target and Wasm Clippy with
warnings denied, registered fixture reproduction, all 74 production-Wasm
browser groups, 48 scheduler comparisons and 13 runner checks pass. Complete
browser elapsed was 227,627 ms. Native validation used two test threads; no test
content or CI concurrency changed. Exact-head hosted CI remains mandatory before
merge.

60 fields/s remains unmet: the measured game requires approximately 5.1× further
throughput. This is a partial performance increment, not hardware completion or
a general firmware-compatibility claim. Required post-merge verification: none.


## Prepared instructions with held passive reads

The private cached interpreter previously required `passive.is_none()` before
using a prepared scalar or load instruction. A burst beginning with a proven
stable uncached WRAM or passive device read therefore copied CPU and local
rollback state and entered the general interpreter even for later resident
loads and register instructions.

Allow the existing prepared executor in non-ROM passive snapshots. For a held
operand, require the exact address and permitted access width, then predict its
completion using a copy of the existing BSC read sequence and the shared CPU
scheduled-MA boundary. Reject the plan before execution if that completion
exceeds the observer horizon. Prediction consumes no value, clock, instruction
epoch or bus protocol state. The existing executor still performs the actual
read and instruction semantics. Future instruction fetches must remain real
resident cache/retained-word hits; ROM snapshots retain the ordinary path.
Sound lookahead and retirement records use the predicted external completion,
rather than the shorter resident-load estimate.

### Web comparison

Chrome 157.0.8103.52 and the normal production JS/Wasm path drew every field to
Canvas. Four rotated 120-field windows over fields 1200–1680 ran separately for
the game and BIOS menu, with all builds, tests and warm-ups stopped during timing.
Complete CPU, sound, clock and pixel snapshots matched after every window and
through paired boot/input/scene checkpoints before field 1200.

| Variant | Game fields/s | Menu fields/s |
| --- | ---: | ---: |
| Main before this increment | 11.695 | 27.613 |
| Only allow resident prepared instructions in a passive burst | 11.935 | 27.634 |
| Also plan the actual held passive operand | 12.407 | 27.863 |

The combined elapsed times were 41,043.9 → 38,687.2 ms for the game (+6.09%
throughput) and 17,382.9 → 17,227.4 ms for the menu (+0.90%). These are local
samples; they do not establish cross-browser or cross-game performance.
A fresh two-way comparison with new browser instances measured game throughput
12.381 → 12.643 fields/s (+2.12%; 38,770.5 → 37,965.9 ms), with all four paired
game windows improving. Menu throughput was 27.483 → 27.568 (+0.31%; 17,465.2 →
17,411.5 ms), effectively unchanged. All 16 repeat endpoints matched. The repeat
supports a modest game gain, not a stable six-percent improvement. First/repeat
median game-window p95 was 159.0 → 151.55 ms / 151.25 → 150.60 ms.

An independent mixed scalar/load/store Wasm compiled-block prototype was also
checked and rejected in a separate comparison: it slowed the game by 6.79% and
the menu by 16.83%. Generated arithmetic was a small part of sampled work;
root-side admission and metadata remained costly. Passing value, timing and
browser comparisons alone was not grounds to adopt that prototype. Its compiler,
ABI additions, table allocation and flags are excluded from this increment.

### Validation and remaining work

Three original native test groups cover a real passive-plus-resident burst,
byte/word/long prediction with exact/short deadlines and non-consuming rejection,
and 24 combinations of PLL, physical bus width, instruction alignment and load
width. Integrated guests compare full CPU, RAM, cache, BSC, audio and pixel state
with ordinary execution and require actual prepared passive reads. Reinstating
the old guard fails the positive-path guest. Replacing external read completion
with the nominal cache-hit time fails the independent timing assertion.
All 1,333 native tests in 111 groups, formatting/build, native all-target and
Wasm Clippy with warnings denied, registered fixture reproduction, all 74
production-Wasm browser groups, 48 scheduler comparisons and 13 runner checks
pass. Full browser elapsed was 195,342 ms. Native validation used two test
threads; test content and CI concurrency are unchanged. Local runner validation
requires the installed Chrome channel and permission for its owned loopback
servers; initial environment-only attempts could not start the browser/server.

Five extended game checkpoints at fields 1800, 2100, 2400, 2824 and 3252 and
three reference PNG byte comparisons pass. Standard-build JS/Wasm bytes match
the measured frozen candidate. Exact-head hosted CI remains mandatory before merge.

The frozen selected Wasm has SHA-256
`a1c02008624ea5361e69732660567205ec018f6d8d68b4d7bcf42282a82df24d`.
No optional optimization switch, guest patch, frame skipping or relaxed clock
model is introduced. Private firmware, media, opcode data and captures remain
unpublished. **60 fields/s remains unmet**, as do the umbrella hardware and
general firmware compatibility criteria. Related to #17.

## Preserve Web animation pacing

Resetting the animation clock to each accepted RAF timestamp discarded its
fractional remainder. With rounded timestamps, a callback just before the
16.667 ms threshold was repeatedly skipped. Keeping the phase, allowing 1 ms
of timestamp rounding, and rebasing after a missed callback period using that
same rounding boundary avoids the artificial limit. Each callback still advances at most one
complete emulated field. A slow host does not accumulate a catch-up loop.
Rebasing uses the same rounding boundary as admission, so residual phase cannot
qualify a duplicate timestamp as another field.

RAF supplies a timestamp to the callback, and high-resolution time may be
coarsened or jittered by the browser. See the
[HTML animation-frame algorithm](https://html.spec.whatwg.org/multipage/imagebitmap-and-animations.html#run-the-animation-frame-callbacks)
and [High Resolution Time's coarsening algorithm](https://www.w3.org/TR/hr-time-3/#dfn-coarsen-time).
The 1 ms allowance is a host presentation choice; hardware clocks and field
contents are unchanged.

An isolated Chrome check drove the actual app with three seconds of controlled
callbacks and both 0.1 ms and 1 ms timestamp rounding:

| Callback frequency | Previous presentation requests | Phase-preserving requests |
| --- | ---: | ---: |
| 60 Hz | 120 | 180 |
| 120 Hz | 120 | 180 |
| 144 Hz | 144 | 180 |

The regression in the existing Play UI check uses the repository-authored
BIOS-only guest and public Start/Pause/Resume controls. It initializes one real
guest field, then counts presentation requests independently of core speed by
replacing only that test instance's `advance_frame` method. Canvas, audio
draining and status updates still run. Timer polling leaves the controlled RAF
queue exclusively to the app. Coverage includes sustained 30 Hz callbacks,
the frequencies above, pause/resume, a 500 ms stall, a duplicate after every
ordinary callback as well as the stall, and
the one-field-per-callback bound. These are scheduler results, **not evidence
that a BIOS or game executes at real-time speed**.

Local validation passes all 1,333 native tests in 111 groups, formatting/build,
native all-target and Wasm Clippy with warnings denied, registered fixture
reproduction, the complete 74-group production-Wasm browser suite with 48
scheduler comparisons, and 13 runner tests. Browser-suite elapsed was 196,042 ms.
The new pacing cases run within the existing Play UI group, including 90
requests for three seconds of 30 Hz callbacks. Exact-head hosted CI and normal
review remain required before merge; no post-merge verification is required.

Further isolated core experiments retained matching paired CPU, cache, bus,
audio and pixel checkpoints, but did not justify adoption. Each ratio below
comes from its own four rotated 120-field game windows with Canvas; absolute
rates from separate runs must not be combined.

| Unselected core experiment | Game throughput change |
| --- | ---: |
| Prepare cold WRAM loads with physical refill timing | -0.62% |
| Reuse the prepared refill protocol at actual MA | -0.27% |
| Stop before replaying a provably rejected cold load | -0.48% |
| Elide rollback copies for already accepted MOV stores | -2.12% |
| Use explicit power-of-two tile-address shifts | -0.15% |
| Publish only the latest transactional cache-set version | -1.13% |
| Save only MOV-mutable CPU state for ordinary store rollback | +0.68% |

The small positive result in the last row does not justify its additional undo
API and maintenance. None of these trial executors, switches or diagnostics is
included. The selected change affects only host animation pacing. The normal
build's JS glue and Wasm remain byte-identical to the preceding selected build
(`a1c02008624ea5361e69732660567205ec018f6d8d68b4d7bcf42282a82df24d`).
Measured game throughput remains about 12.6 fields/s and menu throughput about
27–28 fields/s on this host. **The real Web 60 fields/s objective remains unmet**;
general firmware compatibility and the umbrella hardware criteria also remain
unmet. Related to #17.

## Keep transactional sound snapshots in stable storage

Cached CPU execution saves sound state before advancing sound speculatively.
Returning that large state inline through `Result`, error conversion and `Option`
caused bulk moves in the surrounding execution loop. Keep the saved `Sound` in
a `Box` so these wrappers transfer its handle. The snapshot contents, physical
RAM/audio undo logs, admission, clocks and rollback behavior are unchanged.
This is always enabled and adds no public configuration or API.

Native source sampling identified bulk moves around sound checkpoint creation
and return as a candidate cost. That sample is diagnostic only: adoption uses
the production Web JS/Wasm path, drawing every field to Canvas and draining PCM.
Each comparison below covers 480 fields, from 1200 through 1680, with rotated
variant order and no concurrent local builds, tests, profiles or warm-ups.
Rates are total fields divided by combined elapsed time, not averages of window
rates. All paired boot/input/scene and timed-window CPU, bus, audio and pixel
snapshots match.

| Comparison | Previous game fields/s | Boxed state game fields/s | Previous menu fields/s | Boxed state menu fields/s |
| --- | ---: | ---: | ---: | ---: |
| Eight rotated 60-field windows | 12.744 | 13.369 | 27.823 | 28.445 |
| Fresh instances, six rotated 80-field windows | 12.629 | 13.162 | 28.145 | 28.466 |

Game throughput improved 4.91% initially and 4.22% in the independent repeat;
menu throughput improved 2.23% and 1.14%. These are local measurements, not a
cross-browser or cross-game guarantee. Boxing the entire checkpoint instead
produced similar results (13.286 / 13.167 game fields/s); boxing only its large
state preserves the existing checkpoint interfaces. Merely combining the error
conversions did not improve throughput and is excluded.

Existing tests exercise complete sound restoration, overlapping/aliased RAM
writes, audio FIFO overwrites and error rollback. All 1,333 native tests in 111
groups, formatting/build, native all-target and Wasm Clippy with warnings denied,
registered fixture reproduction, all 74 production-Wasm browser groups with 48
scheduler comparisons, and 13 runner tests pass. Full browser elapsed was
208,577 ms. Five extended optional game checkpoints at fields 1800, 2100, 2400,
2824 and 3252 and three reference PNG byte comparisons also pass. Private firmware,
media and captures remain unpublished; original licensed guests supply mandatory
native/Wasm/browser coverage.

Standard-build JS glue and Wasm bytes match the measured frozen candidate. The
selected Wasm SHA-256 is
`2472f517b1a3840d295cff67ea8c941443d8ab1408c0fffb0d5308259e541de0`.
Exact-head hosted CI and normal review remain mandatory before merge; required
post-merge verification: none. **Real Web 60 fields/s remains unmet**, as do the
umbrella hardware and general firmware compatibility criteria. Related to #17.

## Elide sound rollback before the first possible control change

Cached bursts previously saved complete sound state whenever a passive read,
private peer or refill could produce widely spaced CPU retirements. Some selected
endpoints still precede the existing lower bound on the next possible sound-CPU
control write or RESET pulse. For those endpoints, use ordinary sound lookahead
without a checkpoint. Independent SCSP, DSP, RAM, MIDI and synthesis events still
execute. The strict `desired < cached_cpu_deadline()` comparison keeps potentially
rollback-requiring intervals on the existing transactional path. No deadline is
extended and no guest work, clock or output is skipped. This is always enabled.

Unlike an earlier unselected no-event experiment, this admits intervals containing
real independent sound work. The existing original dual-CPU polling guest now
requires such an elision through a test-only counter. It still compares complete
CPU/pipeline, sound, RAM, emitted PCM and interrupt state against ordinary shared
execution with live PCM/DSP work, control writes and RESET at both PLL settings.
Removing the control-bound guard causes the inherited instruction-refill oracle
to detect an unrollbackable sound prefix; restoring the guard passes that check.

### Web comparisons

Chrome 157.0.8103.52 used production JS/Wasm with Canvas presentation and PCM
draining for every field. Each comparison covers fields 1200–1680, rotates variant
order and stops all other local builds, tests, profiles and warm-ups during timing.
Rates use combined elapsed time. Paired boot/input/scene and timed game endpoints
match exposed CPU, sound, clock and pixel state.

| Comparison | Previous game fields/s | Control-window game fields/s |
| --- | ---: | ---: |
| Ten rotated 48-field windows | 13.137 | 13.738 |
| Fresh instances, six rotated 80-field windows | 13.048 | 13.625 |

Game throughput improves 4.57% initially and 4.42% in the independent repeat.
The fresh menu comparison is effectively unchanged: 27.881 → 27.911 fields/s.
All 18 menu endpoints match and that harness exits normally. An earlier optional
menu harness was stopped after its timing samples because browser shutdown
stalled; final-window assertion completion was unconfirmed, so that run is
excluded from acceptance. The fresh harness reports assertion completion before
closing contexts and the browser.

A compact display-word representation separately improved game throughput by
1.83% and 1.46%, but its combination with this change measured 13.531 versus
13.625 fields/s for this change alone. The compact representation is excluded.
A transposed display layout, single-integer captured words and idle-slot filter
also did not justify adoption. Conservatively shortening every burst to the
sound control bound lost existing accelerated refill/store coverage and was
rejected at native validation. No rejected trial is included.

### Validation and remaining work

The selected source passes all 1,333 native tests in 111 groups, formatting/build,
native all-target and Wasm Clippy with warnings denied, registered fixture
reproduction, all 74 production-Wasm browser groups with 48 scheduler comparisons,
and 13 runner tests. Full browser elapsed is 198,167 ms. These include the added
positive-path assertion and restored negative control above. Five extended game
checkpoints at fields 1800, 2100, 2400, 2824 and 3252 match, as do three explicit
reference PNG byte comparisons. Standard-build JS and Wasm bytes match the frozen
measured candidate. Exact-head hosted CI and normal review remain mandatory
before merge.

The measured selected Wasm SHA-256 is
`32771a07365f119a46d0cab4c38cff3eb99dce8a09c8b4327e30eb8c77664586`.
Private firmware, media and captures remain unpublished. **Real Web 60 fields/s
remains unmet**, as do umbrella hardware and general firmware compatibility
criteria. Required post-merge verification: none. Related to #17.


## Reuse coherent normal-tile address runs

Normal display acquisition repeatedly resolves the same pattern name and tile
geometry for adjacent source dots. Reuse the existing per-dot address plan within
one eight-source-dot cell when the normal screen has unit horizontal steps, no
mosaic or line/cell scroll and standard resolution. Eight-/sixteen-dot characters
and both flips retain their ordinary addressing. Character words still consume
the real port, bank grants and per-dot captures; color, palette and composition
remain live. Register and line-coordinate invalidation clears these derived runs.
This is always enabled, with no runtime setting.

Historical per-dot name captures take precedence. Filling an uncaptured dot
requires the currently retained name's complete address/width/data and source
group to match. Two-word names additionally prove that the held packet contains
both raw words actually consumed by the decoder. A one-to-two-word mode change
can otherwise combine an old first word with a new second packet; a valid
per-dot plan must not spread that mixed name into a future uncaptured dot.
Rotation and unsupported coordinate cases retain their existing decoders.

### Initial Web comparison (Rust 1.99)

Chrome 157.0.8103.52 runs production JS/Wasm with Canvas presentation and PCM
draining for every field. Each variant covers fields 1200–1680 in six rotated
80-field windows, with other local builds, tests, profiles and warm-ups stopped
throughout timing. Rates divide 480 fields by combined elapsed time.

| Scene | Previous fields/s | Coherent tile runs fields/s |
| --- | ---: | ---: |
| Game | 13.569 | 14.142 |
| BIOS menu | 27.847 | 28.085 |

Game throughput improves 4.23%; all six windows improve individually. A fresh
eight-window repeat measured 13.527 → 13.854 fields/s (+2.42%), with all
32 game and 32 menu endpoints matching; menu measured 28.101 → 28.339. Paired
boot/input/scene and all 18 timed endpoints per scene match exposed CPU, sound,
clock and pixel state. A one-word-only specialization measured 13.687 game
fields/s and was excluded because its smaller benefit did not justify selecting
it over the coherent two-word implementation.

### Correctness and remaining work

Three original tests add an independent seven-color pixel grid covering 72
name-width/tile-size/flip/unaligned-scroll combinations; timed group/data identity
cases at both name widths with independently predicted pixels and exact port
traces; and the mixed-word mode transition above. Test-only counters require the
optimization to execute. Reference paths bypass reuse while retaining historical
per-dot plans where a fully literal second decode would consume a lost word.
Removing group, data, flip or two-word coherence guards produces the independently
predicted wrong pixels; restoring each guard passes its oracle.

The selected source passes all 1,336 native tests in 111 groups, formatting/build,
native all-target and Wasm Clippy with warnings denied, registered fixture
reproduction, all 74 production-Wasm browser groups with 48 scheduler comparisons,
and 13 runner tests. Full browser elapsed is 206,747 ms. An initial formatting
check caught indentation in the expanded fixture; formatting was corrected before
the complete passing validation run.

Five extended optional game checkpoints at fields 1800, 2100, 2400, 2824 and 3252
match, as do three explicit reference PNG byte comparisons. Standard-build JS and
Wasm bytes match the measured frozen candidate; Wasm SHA-256 is
`dc970316cba7def5f392d5829176b6d61c1ab08c7b4ddb214ba5ff7a209f006a`.
CI subsequently updated its stable compiler to Rust 1.100 and rejected four
existing indexed loops under the new Clippy diagnostics. The cache-way and
window/rotation table loops, plus one test-only on-chip initialization loop, now
use enumerated array iterators with the same order and bounds. No lint is
suppressed. Native all-target and Wasm Clippy pass with Rust 1.100. The normal
Rust 1.100 Web build matches the frozen final candidate byte for byte; its Wasm
SHA-256 is `10bed658d7331e34fec0c6df364269712bd49003175b7a6bff816e35857d4a6c`.
The earlier hash identifies the initial Rust 1.99 comparison, not this final
compiler build. Revised-head Web timing and validation are recorded in
[PR #133](https://github.com/takahirox/saturn-rust/pull/133).

Private firmware, media and captures remain unpublished; original licensed guests
supply mandatory native/Wasm/browser coverage. Exact-head hosted CI and normal
review remain mandatory before merge.

**Real Web 60 fields/s remains unmet**, as do umbrella hardware and general
firmware compatibility criteria. Required post-merge verification: none.
Related to #17.

## Web optimization experiments (2026-11-13)

Related to [#138](https://github.com/takahirox/saturn-rust/issues/138) and #17.
The [primary-source research](web-optimization-research.md) compares Yabause,
Kronos, Yaba Sanshiro, Beetle Saturn, QEMU and v86, including cached execution,
JITs, idle recognition, SIMD, GPU rendering and host threading.

### Reproducible game measurement

Baseline source: `2d6b17f0979fb798dc826723fe7a50e44d1ffdc0`. Local measurements
use macOS 27.2.1 and headless Chrome 157.0.8103.52. The exact processor model was
unavailable to the sandbox. Production builds retain wasm-bindgen 0.2.129,
Binaryen 127 and the unchanged build script; compiler versions below are explicit.
These measurements characterize this host/toolchain rather than other devices.

Each module warms for three fields, then cold boots independently. Comparisons
alternate execution order over matching continuous windows. Boot/skip is outside
timing. Every measured field includes Canvas upload and PCM draining/checksum;
competing local builds, tests and profiles are stopped during timed intervals.
The BIOS and mounted CUE/track data remain local. No game-specific reset or
firmware patch is used. The late-game interval visibly reaches the game's book
selection screen with both SH-2 processors running. It is one defined workload,
not evidence covering arbitrary gameplay.

To reproduce with authorized equivalent local media, save both runtime files
from each build in separate directories, then use the benchmark's `--candidate`
and `--baseline` arguments together with `--bios`, `--disc` and `--track`.
The controller timeline uses Right (`0008`) at fields 300, 316, 332, 348, 364 and
380, releasing eight fields after each; A (`0040`) at 416, released at 424; and
Start (`0080`) at 2800, released at 2808. The late-game measurement uses
`--skip 3600 --frames 80 --runs 6 --continuous --draw`. Earlier game measurements
omit the later Start event and use `--skip 600 --frames 80 --runs 4`; BIOS-only
uses `--skip 1200 --frames 80 --runs 4`. Both also use `--continuous --draw`.
The original guest uses `--frames 60 --runs 4 --continuous --draw` without private
inputs. Throughput divides total measured fields by total elapsed time. Different
scenes/inputs cannot be compared directly with historical rates above.

A Rust 1.99 late-game baseline CPU profile attributes 23.8% of sampled self time to
cached-burst execution, 7.8% to display acquisition, 7.0% to sound advancement and
5.9% to VDP1 advancement. These are sampled self times, not total subsystem shares.

The benchmark now accepts mounted discs, freezes/hashes runtime files, records
p50/p95/p99/max timing, and compares exposed master/slave CPU endpoints plus
sound, clocks, framebuffer and PCM results. Browser endpoint equality supplements
native ordinary-step differential oracles; it does not prove every internal
state equal. ISO and multi-track CUE smoke checks use original repository fixtures
and pass equality in cold and continuous modes. Syntax, local links and staged
whitespace checks pass.

### Rejected passive-read protocol versions

A candidate replaced complete bus-protocol checkpoint copies with immutable
version indices and reused vector capacity. The initial Rust 1.99 late-game
comparison appeared promising, but fresh-browser and actual CI-artifact results
did not reproduce it:

| Comparison over fields 3600–4080 | Baseline fields/s | Candidate fields/s | Change |
| --- | ---: | ---: | ---: |
| Initial Rust 1.99, six 80-field windows | 18.229 | 19.076 | +4.65% |
| Fresh Rust 1.99, four 120-field windows | 18.800 | 18.742 | −0.31% |
| Actual Rust 1.100 CI artifacts, six 80-field windows | 18.938 | 18.611 | −1.73% |

All six CI-artifact windows regress. The change is rejected, including its host
scratch field and version-specific test. The initial median window p95/p99 of
56.35/57.45 → 53.10/53.80 ms is not an adopted improvement. The fresh repeat has
53.95/54.55 → 54.20/54.90 ms, with 56 ms maximum for both variants. These are
medians of window percentiles, not pooled percentiles or long-run latency bounds.

The initial candidate also measured 16.172 → 16.133 fields/s in the earlier game
(−0.24%), 62.367 → 60.787 on the original loop (−2.53%), and 41.480 → 40.969 on
BIOS-only (−1.23%). All comparisons preserve exposed guest endpoints.

Artifact SHA-256 inventory for the rejected trial:

- Rust 1.99 baseline: `033febfddf664aaabbbb9a4064bc117925fdbc62cfb7d417d6a6f54fc36102b1`.
- Initially measured candidate: `092a447915aea1d4b80a1f5aff07c15b0c337a913c6d9a1698818824b90eb2da`.
- Fresh-repeat candidate: `261e9f0e5f36c264110220bb87bd3067e0f44acb8d055f609b1476ea27112cee`.
- Rust 1.100 CI baseline: `bba1401a67073fe75236d01afdbf2e840d4a72d37301964387a4fdf4da3bfdc7`.
- Rust 1.100 CI candidate: `e83a9a56717edb81bdb7a9cc74dcb8734aace7d28084c0bbbbe88155e00cf52f`.

The two Rust 1.99 candidates have identical JS and complete Wasm code sections;
only 68 source-location data bytes differ after comments. Correctness passed
1,338 native tests in 111 groups, native/Wasm Clippy, fixture checks, Web build,
13 runner tests and focused read/repeat invalidation negative controls. The
runner's first sandbox-restricted attempt failed at loopback listeners; rerunning
with the required local access passed. Correctness alone did not justify adoption.

### Other trials excluded from the implementation

All comparisons use matching baseline/candidate endpoints. Different scenes are
identified explicitly; these independent trials cannot be multiplied together.

| Isolated trial | Observed throughput change | Decision |
| --- | ---: | --- |
| Borrow initial CPU/local cycle anchors | +0.53% in a historical game interval | Inconsistent windows; revert. |
| Copy-free cycle repeat plus global SIMD128 | +0.73% in that interval | Inconsistent windows; revert both. |
| Prepared MOV stores | +0.43% in that interval | Inconsistent windows; revert. |
| Stage MOV bus phases before CPU publication | −2.76% in the late-game interval | Revert despite passing targeted correctness tests. |
| Shared cached dispatcher with an outlined Wasm function | −0.78% in the earlier game | Revert; a smaller function did not improve throughput. |
| Box the rare CPU fault payload | −0.21% in the earlier game | Revert; smaller CPU snapshots alone did not help. |
| Cap one-caller inlining | −0.15% on the original guest | Keep standard settings. |
| Rust opt-level 2 | +0.02% on the original guest | Keep standard settings. |
| Replay every rejected CPU step from burst origin, without per-instruction CPU copies | +4.60% on the original guest, −2.83% in the earlier game | Reject the unbounded-replay design; evaluate retained checkpoints separately. |
| Retain per-instruction CPU copies and reuse checkpoints only for existing shortened-prefix replay | −0.13% on the original guest, −0.82% in the earlier game | No demonstrated benefit in those workloads; exclude this isolated variant. |
| Box the detailed SMPC payload inside BusError | −2.02% on the original guest, −1.23% in the earlier game | Revert the payload/API and ownership changes; compact types alone did not help. |

### CPU recovery prototype and final execution-class guard

The initial CPU-recovery prototype removes full CPU copies before fallible
ordinary instructions. After a rejected step or sound retirement-gap rejection,
the local bus journal rolls back immediately. Publication reconstructs the
selected prefix from an existing cycle checkpoint at or before that endpoint,
falling back to the immutable burst origin. The same checkpoint can shorten an
already-required shared-horizon replay. No periodic checkpoint or instruction
class is added; reconstruction uses the ordinary CPU interpreter.

With local Rust 1.100, the prototype improves the late-game interval from
18.821 to 19.410 fields/s (+3.13%) in six 80-field windows. A fresh four-window
120-field repeat reproduces 18.993 → 19.601 (+3.20%). Every paired window
improves and exposed guest endpoints match. The repeat's median window p95/p99
is 53.60/54.00 → 52.15/53.05 ms, with maxima 54.70 → 54.00 ms. Earlier game is
flat (16.189 → 16.154, −0.22%); the original loop improves 64.137 → 67.314
(+4.95%), but BIOS-only regresses 43.669 → 41.715 (−4.47%).

An uncached-ROM snapshot guard does not fix BIOS-only (43.166 → 40.745,
−5.61%); its measured endpoint is outside ROM. That variant is excluded. It
measures +5.03% on the original loop and +1.36% in the earlier game, illustrating
why one scene alone is insufficient for selecting the recovery strategy.

The final candidate uses the existing peer schedule to select recovery:
private-peer bursts use deferred CPU recovery and retained checkpoints, while
solo CPU execution keeps ordinary per-instruction rollback snapshots. A single
lazy snapshot slot is reused within a solo burst. Scalar/prepared paths,
hardware deadlines, guest clocks, memory effects and ordinary errors retain
their existing rules.

Both local baseline and final candidate use Rust 1.100.0 with unchanged
production flags. Baseline source is `2d6b17f`; its Wasm SHA-256 is
`d4190de73d52f737f526f285ade9ee91ab024c66f36b12a8bd4d04e13db16dd2`.
Final candidate SHA-256 is
`778be960b1243209bdf90a16219dd3c63e9713c72cd290ab63ccfeaaf8c86532`.
The earlier unconditional prototype has SHA-256
`b5cd3badbdffc17787e9d2445fa19901625da3b4e6bd71f2d606bc773b60482e`;
its game rates must not be attributed to the final candidate without measurement.
Local and hosted builds are distinct artifacts; compare matched pairs from each
build environment.

| Final candidate, local comparison | Baseline fields/s | Candidate fields/s | Change |
| --- | ---: | ---: | ---: |
| BIOS-only, fields 1200–1520, four 80-field windows | 43.342 | 43.509 | +0.38% |
| Original loop, fields 0–240, four 60-field windows | 63.238 | 63.083 | −0.24% |
| Earlier game, fields 600–920, four 80-field windows | 16.060 | 16.233 | +1.08% |

The BIOS slowdown is removed and the original loop is effectively flat. Final
private-peer game comparisons, fresh-browser repeats and actual hosted-artifact
results are recorded in the PR linked from #138 and are required before adoption.
Initial gains alone are insufficient, as the rejected protocol-version trial
shows. All measured exposed guest endpoints match.

The final candidate passes all 52 existing cached-execution differential tests
with Rust 1.100, retaining full CPU/cache/bus assertions against the independent
ordinary-step scheduler. A negative control restoring the wrong retained CPU fails two existing cases,
covering polling/sound-control phases and uncached WRAM boundaries. Native
all-target and Wasm Clippy with warnings denied pass. The standard Web rebuild
matches the frozen final candidate JS/Wasm byte for byte. Complete
native/Wasm/browser/aggregate CI must pass for the final PR head; its results
are recorded in the PR. Earlier prototypes pass the full hosted suite, but those
runs do not validate this execution-class guard.

### Acceptance limits

The benchmark drains audio but does not run the production AudioWorklet consumer
or measure audible underflows. **Sustained real Web 60 fields/s remains unmet.**
Broader gameplay, transition and audio-delivery acceptance remain open under
#138; general hardware/firmware compatibility remains outside this increment.
Private media, profiles, captures, logs and experiment artifacts are not published.
Required post-merge verification: none.

## Follow-up from main 87a18a9

This iteration uses the previous final local Wasm (`778be960…86532`) as its
baseline, rather than the older pre-PR #139 artifact. A fresh 120-field Chrome
profile after field 3600 attributes 23.76% of self samples to `cached_batch`,
7.88% to sound-domain advancement, 7.74% to display acquisition, 5.96% to VDP1
advancement and 1.66% to the local cached-memory snapshot helper. These are
sampled self times, not additive inclusive costs of the CPU path.

A separate temporary instrumented build counts work over the same interval.
Its exposed endpoint equals the uninstrumented build; its timings are excluded.
Of 1,301,046 attempted bursts, 935,668 are admitted and 839,485 publish a prefix.
All admissions use a peer schedule. Published prefixes retire 39,899,601
instructions, averaging 47.53 per publication. Preparation plans 40,611,050
instructions, including 189,788 skipped through proven cycle repetition.
Control preparation is attempted 10,362,123 times, ordinary fallback 2,987,094
times, and local memory snapshots are taken 16,272,744 times. CPU recovery
replays 1,812,881 instructions, 4.49% of physically planned instructions.
The mixed prepared-load/scalar/control workload gives block reuse room beyond
the existing register-only blocks. Replay alone cannot explain the whole cost.

The first trial suppresses sound reference-clock conversions when only
checkpointed sound can consume the retirement records. It passes the existing
cached tests and Clippy but measures 16.383 → 16.422 fields/s (+0.24%) in the
earlier game and 64.019 → 63.613 (−0.63%) on the original loop. Those four-window
comparisons do not justify adoption; the trial is reverted.

The native harness now supports mounted CUE media, explicit track files and
continuous windows, and drains/checks PCM rather than leaving the output ring
unconsumed. On the baseline implementation, the later game interval 3600–4080
measures 23.799 fields/s across six 80-field native windows. Its existing master,
clock, RAM and pixel endpoint fields equal the matching Web endpoint. Against
the earlier approximately 19.8 fields/s Web result, native is only about 1.20×
faster. This is evidence to prioritize core work, rather than attribute the
remaining roughly threefold Web gap entirely to browser execution. Native
framebuffer copying and browser Canvas presentation are different host paths;
this is not a pure compiler/backend ablation.

The next candidate recognizes an already-ready cached branch before broad
operand admission. Those branch families have no operand transfer. Their IFs
use the ordinary speculative-fetch filter, so they cannot consume a passive
MMIO read sequence; uncached ROM and WRAM aliases explicitly retain the full
protocol snapshot. Clock, replacement state, retained IF, epochs, refill/store journals
and failure restoration remain active. This specializes the existing path
rather than caching plans by raw RAM PC, which would lose cache visibility.

With standard production flags, the initial branch prototype's Wasm SHA-256 is
`261ffac04b81b41843eb01851cd91327972fc733dfc4b45f712fa9331713b25f`.
Its six-window Web comparison improves 18.766 → 19.638 fields/s
(+4.65%) over fields 3600–4080. Every paired window improves and exposed CPU,
sound, PCM and display endpoints match. Existing cached-related tests pass
(58 selected tests), as does native all-target Clippy with warnings denied.
A negative control removing the uncached-ROM snapshot exception fails the
existing ROM-fetch differential test on read-protocol clock rollback; the
guard is required, rather than dead compatibility code. Review also found that
a passive WRAM alias can be a branch target. The final guard retains that
protocol too, with an original regression that actually fetches the alias and
restores the consumed bus state. The initial prototype's timings do not validate
this final guard and are not evidence for adopting the earlier variant.
The guarded artifact is
`5988f81b89b797aa7151217edb43c25b3a1c9a75900c3a4f4ab7c69107e0330e`;
all 59 cached-related tests and native all-target Clippy pass for that source.
Fresh-browser repeats, other workloads and final-head CI are required before
adoption; their results belong in the linked validation PR. These absolute
rates are from a different session than PR #139 and must not be combined into
an additional unmeasured speedup.

### Executed Beetle Saturn comparison

The pinned reference is `libretro/beetle-saturn-libretro` at
`30f4c12072d265c9e553dcb29386059114945dea`, built locally with Apple Clang,
`make platform=osx LTO=1 WANT_JIT=0`, and, for the synchronous renderer,
`NEED_THREADING=0`. The libretro runtime options also disable SH-2, SCU and SCSP
JIT and auto RTC, select full CPU cache emulation and retain the default exact
interleave setting (`0`). Audio and software video remain enabled; the local
frontend consumes PCM with a rolling checksum and copies every delivered frame.
Both cores use the same authorized Japanese BIOS; the reference automatically
selects Japan/NTSC from the disc and retains its default cartridge selection.
There is no throttle, vsync or frameskip. Boot/loading and endpoint captures are
outside timing. The core reports an NTSC target of 59.826503 Hz and 44,100 Hz audio.

The original controller timeline and fields 3600–4080 reach the same visible
book-selection scene with full-cache emulation. Six continuous 80-field windows
give 198.982 fields/s with synchronous rendering and 176.286 with the render
thread. Reference variants produce equal delivered PCM/pixel endpoints at each
window, with 80 video callbacks per window. Thus renderer parallelism does not
explain the reference's advantage in this scene; its queue costs more here.
Default data-only cache runs reach a different point in the boot/game sequence
under these inputs and are excluded from this scene's speed comparison.

The reference is approximately 8.36× faster than our 23.799 native baseline on
this host and visible workload. This is not a correctness equivalence claim:
the cores have different timing models, display cropping and host delivery paths,
and the reference's default `_LOW_ACCURACY_` dispatch fusions are enabled in
these builds. Our native/Web ratio is much smaller. Together with the measured
snapshot/admission counts and inspected reference event/dispatch loop, these
results support investigating execution/synchronization overhead rather than
assuming Web itself or lack of renderer threads causes the entire gap.
They do not assign an exact portion of the gap to any one algorithm.

Only our repository's original licensed native/Wasm/browser guests gate changes.
The externally built core and all private inputs, captures and raw logs remain
local. These unthrottled reference measurements are not a claim that our Web
implementation meets real-time or general compatibility acceptance.

### Pair preparation correction

The guarded prototype's first six-window comparison gives +1.88%, but a fresh
four-window repeat gives only +0.11% overall (18.127 → 18.148 fields/s). It does
not reproduce a convincing combined gain. The first pair is −6.00%, while the
remaining pairs are positive. These results remain provisional rather than
adoption evidence.

Inspection finds that the old harness boots and measures the first runtime,
then boots the second runtime before measuring its matching window. With 3600
skipped fields, that first pair is separated by several minutes; subsequent
pairs alternate nearby. This permits host-state drift to confound the first
comparison. It does not prove that drift explains the discrepancy.

The harness now prepares every runtime to the requested field before timing
either one, for both cold and continuous runs. Reports identify this method as
`preparation: "all-variants-before-timing"`. Repeat the guarded comparison with
this method and reverse the artifact assignments in a fresh browser to
counterbalance initialization/execution order. Guest skip fields, input and
state equality remain unchanged. A green earlier CI head or the initial
prototype's +4.65% cannot substitute for this final performance validation.

### Decision after corrected, counterbalanced comparisons

Both runs use the corrected preparation method, standard production artifacts,
the same input and fields 3600–4080, Canvas output and PCM checks. The first
uses six 80-field windows; the fresh browser uses four 120-field windows with
the artifact assignments reversed. The rates below identify the actual source
artifact rather than the harness's swapped `baseline`/`pkg` labels.

| Preparation and initial execution order | Main fields/s | Guarded prototype fields/s | Change |
| --- | ---: | ---: | ---: |
| Main then guarded prototype | 19.101 | 19.403 | +1.58% |
| Guarded prototype then main, fresh browser | 19.542 | 19.396 | −0.75% |

Every pair improves in the first run; every pair regresses in the reversed run.
All exposed guest endpoints match. This is not an order-independent performance
gain, so **the branch-admission/protocol-snapshot optimization is rejected**.
Both runtime changes and the trial-specific alias regression are removed from
the final implementation. Its emulator core equals main `87a18a9`; only the
benchmark tools and investigation record change. Earlier prototype rates must
not be described as adopted speedups.

The preparation correction also equalizes live guest population. Previously,
the first continuous baseline window had only one live guest, while the second
runtime's first window had two. This and the multi-minute gap were actual
measurement asymmetries. Correcting them does not identify the cause of the
remaining order dependence; compiler placement, host state and memory effects
are not individually isolated by these measurements.

Five-second samples of the owned native processes supplement the source
comparison. In our baseline, display acquisition is 11.41% inclusive and sound
lookahead/checkpoint is 17.68% inclusive, including actual sound execution;
VDP1 advancement is 8.37% inclusive. These overlap inside cached bursts and
must not be added as CPU-only costs. Memory copying is 4.20% self time, with
97 of 160 copy samples immediately attributed to sound lookahead/checkpoint.
The reference profile instead concentrates in its compact master loop, slave
continuation, instruction fetch and specialized memory handlers. Different
inlining and timing models prevent a numerical attribution of the entire
cross-core speed gap to one mechanism.

The supported next targets are reusable execution/admission plans and resumable
execution, narrower display capture and sound state preservation. Copying a
native recompiler backend or moving rendering to a thread alone is not shown
to solve the gap. Sustained Web 60 fields/s and production audio-delivery
acceptance remain open under #138. Required post-merge verification: none.


## Own VDP2 fetch state directly (2026-11-14)

Continuation from main `bbd4a48`, related to #138 and #17. The actual Web game
still runs near 20 fields/s; this is a small production improvement, not completion
of the real-time objective. `DisplayFetch` now owns its buffers directly. Decoder
queries update individual `Cell` values, while resize/reset still requires
exclusive VDP2 access. This removes whole-pipeline runtime borrow guards from
word/dot acquisition and composition. Physical read edges, retained words,
per-dot captures, address planning, sample values and bus arbitration are unchanged.
The existing literal-address, mosaic, rotation and contending-write oracles retain
their expected results; only their access to the private representation changes.

The workload remains the authorized BIOS-mounted CUE, the recorded controller
sequence, skip 3600, and six continuous 80-field windows ending at 4080. Chrome
157.0.8103.52 includes Canvas presentation and PCM generation/draining. Every
variant completes boot before timing starts; no local compilation, tests or
profiles run alongside either preparation or throughput measurement. Private
inputs, captures, profiles, raw logs and the experimental reference port remain
local and ignored.

| Actual-game comparison | Baseline fields/s | VDP2 fields/s | Change |
| --- | ---: | ---: | ---: |
| Alternating pair | 20.16273 | 20.49942 | +1.67% |
| Fresh rotating-order three-variant repeat | 19.90396 | 20.15587 | +1.27% |

All six VDP2 windows improve in both runs, and every corresponding exposed CPU,
clock/counter, PCM and pixel endpoint matches. The repeat also evaluates the
independent rejected CPU trial below; all three variants are prepared before
measurement and their order rotates across the six windows. Its third variant
must not be counted as part of the VDP2 improvement. The original busy video/audio
guest measures 53.20014 → 53.62090 fields/s (+0.79%).

Median window p95/p99 field times are 50.50/51.40 → 49.65/51.10 ms in the first
pair and 51.15/52.20 → 50.65/51.25 ms in the fresh repeat. These are medians of
window percentiles, not pooled distributions or long-run latency guarantees.
The baseline Wasm SHA-256 is
`0920c35a9f27c2d06e5a4da816c3dda5a024b75c8ff263284f3487c803fb685e`;
the first candidate is
`e4ea32ada17549aded346085175075ea53e40615ee2642a154edca2317824e74`;
the finalized candidate used in the repeat is
`f3150bbb55ae15afe5040f4ab962bc826ddd4b183f8edc13286ffd117c131d20`.

Local checks pass 65 VDP2 tests, 10 bus VDP2 acquisition tests, 12 video-write
competition tests and all-target/all-feature Clippy. Two initial bus test filters
matched zero tests; the corrected `bus::transactions` filters run all 22 tests.
Complete native/Wasm/browser CI and full review remain required before merge.

### Rejected independent trials

| Trial | Actual-game baseline → candidate fields/s | Decision |
| --- | ---: | --- |
| Exact incremental rational sound-clock conversion with rewind/large-jump fallback | 20.17502 → 19.91528 (−1.29%) | Revert, despite clock/checkpoint/SCSP/cache differential tests passing. |
| Real B-bus RAM load/store continuations retaining bus and sound arbitration | 20.17214 → 20.16358 (−0.04%) | Revert; no measured improvement. Both SH-2s, all transfer widths, B-bus endpoints and concurrent 68000 writes match the ordinary scheduler. |
| Defer cycle anchors until a short backward branch appears | 20.15587 → 19.45147 (−3.49%, both include VDP2 ownership) | Revert; original-busy +0.21% does not carry over to the game. Existing 52 cached scheduler tests pass. |

These variants preserve the measured endpoints but do not justify adoption.
The last trial removes early recovery checkpoints as well as detection copies;
its regression does not isolate those two effects.

A local single-threaded, non-JIT Emscripten build of the pinned Beetle core has
also been executed. Its apparent 226–230 fields/s is **invalid game evidence**:
the captured screen is black, display remains disabled and the BIOS waits for
SCU DSP completion. All 256 DSP program words are zero at the diagnostic boundary.
The port's startup failure is still under investigation; native Beetle results
above remain separate. No external core code is included in this change.

Sustained 60 fields/s, broader gameplay/transitions and real-time audio delivery
including underflows remain unmet. #138 and the active real-time goal stay open;
this checkpoint does not establish general firmware compatibility or hardware
completion.


## Execute resident branches from proven cache words (2026-11-14)

Continuation from main `b3ae14e` and the VDP2 ownership change, related to #138
and #17. The production cached executor now constructs a complete resident-branch
IF schedule before publishing CPU, replacement or retained-prefetch state. Its
admission step resolves the actual
cache words, retained halfword and ordered LRU edits once, including staged cache
versions. Only after the complete schedule and existing sound/observer horizon
are proven does it publish those edits and retire the branch. Cache misses,
uncached/peripheral targets, unavailable IF/ID pairs and faults retain the original
path. Branch conditions, delay slots, link state, WB interlocks, retained EX,
interrupt inhibition, physical cache visibility and clock endpoints are preserved.
This is a production optimization, not completion of the separate kernel prototype.

An initial version proves each hit and then performs the original cache reads.
Its first paired game observation is 19.91561 → 20.45086 fields/s (+2.69%), but
it falls to 20.48236 → 20.52124 (+0.19%) in a fresh rotating-order comparison.
That version is not selected. Reusing the proven words and LRU edits removes the
second cache lookup and demonstrates the following repeated improvement:

| Actual-game comparison, fields 3600–4080 | VDP2 baseline fields/s | Selected branch fields/s | Change |
| --- | ---: | ---: | ---: |
| Rotating-order baseline/initial/selected, six 80-field windows | 20.48236 | 20.95466 | +2.31% |
| Fresh reversed-label pair, six 80-field windows | 20.51405 | 20.95923 | +2.17% |

Every selected-candidate window improves and corresponding exposed CPU,
clock/counter, PCM and pixel endpoints match. All runtimes finish preparation
before measurement; local builds, tests and profiles remain separate from both
preparation and timing. Chrome 157.0.8103.52, the same deterministic controller
sequence, continuous execution, Canvas and PCM draining remain unchanged.
The original busy video/audio guest measures 53.01446 → 52.90461 (−0.21%, flat).
These observations are not a general gameplay or real-time audio acceptance claim.

Median window p95/p99 is 49.65/50.25 → 48.50/49.60 ms in the rotating-order run,
and 49.65/51.00 → 48.55/49.55 ms in the reversed pair. These are medians of window
percentiles, not pooled distributions or long-run guarantees. The VDP2 baseline
Wasm SHA-256 is
`f3150bbb55ae15afe5040f4ab962bc826ddd4b183f8edc13286ffd117c131d20`;
the rejected initial branch variant is
`68e9104e53bb7b718b2ffb1fc22e556b65a8b5b5753cf8bc0bbc70e019c17562`;
the selected word-reuse variant is
`40bb2d54f694cf75ca3aa9ae1038e0e80bd430f806457779bd0c2d22c3e5ed30`.

Local validation passes an exhaustive branch-encoding differential test against
the original timed IF/ID helpers, with both conditional outcomes and nine WB/EX
configurations. It compares complete CPU state, fetch/discard ordering and their
clocks. The existing 52 cached-scheduler differential tests pass, including an
assertion that the resident branch path executes. Eight cache tests pass; the
existing peek/read oracle additionally verifies that applying an opaque LRU proof
matches an ordinary cache read without publishing replacement changes early.
Native all-target/all-feature Clippy, Wasm all-feature Clippy, formatting and
whitespace checks pass. Complete exact-head CI and full review are required
before merge.

### Reference instruction-volume investigation

A separate local diagnostic build of the pinned, synchronous, full-cache Beetle
core disables JIT and SH-2 fusions and counts issued ISA words. Identical fresh
SMPC save seeds and controller input produce matching PCM, pixel, audio-frame and
field endpoints in all six windows against the unmodified conservative core.
The instrumentation's FPS is excluded from performance evidence. Counting begins
at instruction issue, so an unfinished field-boundary instruction can differ from
a retirement count by about one per CPU.

The reference issues approximately 345,330 master and 84,226 slave instructions
per field in this book-selection interval. Our exposed logical retirement counts
over the late 3840–4080 interval average approximately 336,419 master and 192,516
slave instructions per field. The master volumes and dominant call/stack/poll
instruction families are similar. The slave remains in a three-instruction FRT
poll in the reference; our scheduler already folds proven peer polling periods
while retaining logical retirement counts. These figures therefore do not measure
host instruction executions equally, establish timing equivalence or identify a
hardware defect. They support investigating the cost of host execution and
synchronization, rather than attributing the large master throughput gap solely
to a larger guest instruction volume. Private diagnostic code, inputs, saves,
captures and raw counters remain ignored and local; no reference-core code is
adopted.

The selected Web game still runs near 21 fields/s, requiring about 2.86 times
more throughput for 60. Representative broader gameplay/transitions and real-time
audio-underflow acceptance remain unmet. #138, #17 and the active real-time goal
stay open; no general firmware compatibility or hardware-completion claim is made.

## Dispatch cached execution by the existing instruction class (2026-11-15)

Related to #138 and #17. Baseline is main `6a90c1b`; this is another partial
throughput increment. A fresh local diagnostic build counts 161,481,747 master
retirements over fields 3600–4080. It attempts 152,579,210 scalar blocks, of which
117,648,013 (77.1%) return zero. Successful blocks average 1.279 instructions.
Prepared WB scalars, loads and resident branches account for another 29,995,187,
34,151,807 and 41,340,704 planned instructions respectively. Its final exposed
CPU/clock/PCM/pixel endpoint equals the corresponding uninstrumented run;
instrumented elapsed times are excluded from throughput evidence.

The candidate reads the existing instruction descriptor's execution class once
before trying cached paths. Register-only instructions enter the scalar paths,
ordinary MOV loads enter prepared execution, and recognized branches enter
resident control preparation. RTE, TRAPA, system transfers and other instructions
retain their existing fallback. Resident preparation already checks the running
state, address error, delay slot and matching IF/ID pair, so its caller's duplicate
control-readiness probe is removed. Cache reads, pipeline rules, sound guards,
observer horizons, recovery and guest retirement positions keep their existing
behavior. No address-indexed decoded-code cache is introduced.

Production builds use Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127 with
unchanged release flags; the browser is headless Chrome 157.0.8103.52. Baseline
Wasm SHA-256 is
`40bb2d54f694cf75ca3aa9ae1038e0e80bd430f806457779bd0c2d22c3e5ed30`;
classification-only candidate is
`464639fa99636abb0a2682c09a6cad87f01d94f6abd5d8f6af71d73fe2f09548`.
Both use the workload/controller sequence above: cold boot, skip 3600 fields,
then six continuous 80-field windows, drawing every field and draining/checking
PCM. All variants finish preparation before timing, with no concurrent local
builds, tests or profiles during preparation or measurement.

| Comparison | Main fields/s | Classification fields/s | Change |
| --- | ---: | ---: | ---: |
| Alternating pair, main initialized first | 20.93976 | 21.45203 | +2.45% |
| Fresh rotating three-way run, classification initialized first | 20.69938 | 21.44676 | +3.61% |
| Original busy sound/video guest | 52.82310 | 53.11850 | +0.56% |

All twelve paired game windows improve and corresponding exposed endpoints
agree. Candidate window p95 ranges are 47.1–48.0 ms initially and 47.4–47.8 ms
in the repeat; p99 ranges are 47.7–49.5 and 48.1–49.1 ms. Main's repeat includes
a 55.9 ms field, so the larger second aggregate gain includes host variation;
the candidate's absolute throughput is consistent across the two fresh runs.

The third artifact adds an early check of the scalar block's existing pipeline
readiness condition. It measures 21.04405 fields/s, 1.88% slower than the
classification-only artifact, despite passing the targeted pipeline test and
all 52 cached-execution differential tests. That addition is rejected. A prior
resident MOV-load/WB-scalar proof-value reuse trial also passes corrected
sign-extension and cached-execution checks but measures 20.94725 → 20.79065
fields/s (−0.75%); it is excluded.

The exhaustive instruction-descriptor test compares all 65,536 execution classes
with independent literal scalar/load/control classifiers. All 52 cached-execution
scheduler differential tests and native all-target Clippy pass. Exact-head
native/Wasm/browser CI and final review remain required before merging.

Approximately 21.45 fields/s still needs about 2.80 times more throughput for
60. Sustained real-time, broader gameplay/transitions and production audio
underflow acceptance remain unmet. #138, #17 and the active goal stay open.
Private inputs, diagnostic traces and captures remain local and ignored.
Required post-merge verification: none.

## Rejected follow-ups and refreshed profile (2026-11-15)

Baseline is the classification-only artifact above, now merged as main
`86e55e3`. Two isolated follow-ups preserve their measured exposed endpoints but
do not justify adoption:

| Trial | First game comparison | Fresh rotating comparison | Decision |
| --- | ---: | ---: | --- |
| Keep scalar-run LRU borrow, retained IF and bookkeeping local when no shadow/passive/store path is active | 21.12564 → 21.27282 (+0.70%) | 21.39085 → 21.31922 (−0.33%) | Reject; five of six repeat windows regress. |
| Precompute all 65,536 fractional Q30 sound gains with the original ordered integer truncation | Not previously measured | 21.39085 → 21.23255 (−0.74%) | Reject; all six game windows regress. |

The scalar-fetch trial passes 53 cached-execution differential cases, including
partial runs, a missing next line, retained words and LRU comparison with the old
fetch path. The gain-table trial passes 134 sound cases, including every
fractional value and integer exponents −20 through 1 against the original factor
loop. Both pass native all-target and Wasm Clippy. The latter improves the
original busy sound/video guest by 0.72%, but that does not establish a game gain.
Their source changes and additional prototype tests are excluded.

The repeat prepares all three variants before six rotating 80-field continuous
windows at fields 3600–4080, with Canvas and PCM checking. It collects an
additional **diagnostic-only** profile of the unchanged baseline at fields
4080–4160 after all throughput measurements finish. Profiling time is excluded
from the comparison. Sampled self shares are 22.2% in cached-batch execution,
8.9% in sound-domain advancement, 8.8% in display acquisition and 3.9% in VDP1
advancement. Cached-batch inclusive time is 72.3%, which contains sound/display
catch-up and must not be presented as pure CPU execution or added to its child
shares. These samples prioritize repeated event processing and scheduling work;
they do not establish the cost of a particular statement or forecast a speedup.

A subsequent sound experiment replaces the eleven-source IRQ-priority scan with
three routing-bit intersections. Its extension also removes pin samples between
source-changing events while retaining the pre-event sample at a MIDI completion.
The routing oracle covers every 11-bit pending mask, 256 routing patterns, three
enable masks and ignored high source bits. All 134 sound and 52 cached-execution
cases, native/Wasm Clippy and production builds pass. Removing the real MIDI
pre-edge sample is rejected by the inherited serial/CPU equal-edge oracle;
restoring the exact selected source passes that case. An initial fully qualified
test filter matched zero cases and was corrected before accepting this evidence.

Despite those checks, a fresh rotating game run measures 21.42436 → 21.38161
fields/s (−0.20%) for routing alone and 21.42953 (+0.02%) for the extension.
Exposed endpoints agree. Original busy sound/video improves 1.17% and 1.00%,
respectively, but neither variant demonstrates a game gain. Both changes and
the additional routing test are excluded.

Two VDP2 variants are also excluded: replacing the access-policy cache's
whole `RefCell` with per-value cells measures 21.38656 → 21.20263 fields/s
(−0.86%, all six windows regress), and additionally memoizing register-derived
normal-layer availability measures 21.38275 (−0.02%, mixed signs). The former
passes 65 VDP2 cases; the latter passes 66, including a literal register-change
invalidation case, plus all 52 cached-execution cases. Both pass native/Wasm
Clippy and preserve all measured exposed endpoints. Original busy-guest gains
of 0.62%/1.17% do not change the game-based rejection. Their source/test changes
are excluded. Bounded read-only configuration inspection outside timing confirms
that this game interval uses the NBG1 RGB tile path; a unit-bitmap-only kernel
would not target its active drawing path.

Finally, display-buffer reuse tracks the layer/path columns written in the
previous line, clears only those columns, and retains pixel-buffer lengths.
Its extension applies the same treatment to normal and planned address samples.
A line/width-change lifecycle case, all 66 VDP2 and 52 cached-execution cases,
three original video integration suites, native/Wasm Clippy and production
builds pass. All corresponding game endpoints match. However, the fresh rotating
comparison is 21.42886 → 21.17074 fields/s (−1.20%) for pixel buffers and
21.22701 (−0.94%) for the extension. The latter includes a slow final window;
the other windows do not show a convincing gain either. Original busy-guest
changes are −0.33%/+1.56%. Both variants and their prototype lifecycle test are
excluded. Reducing bytes initialized alone does not establish faster execution.


### Ordered video Worker vertical slice (#138)

The [renderer Worker record](video-render-worker.md) describes an original-Rust,
feature-enabled producer/consumer split with immutable ordered payloads and a
bounded two-field transferable queue. Same-feature exact-field game comparisons
improve 27.6823 to 35.1764 fields/s and 28.5930 to 36.6049 in reverse initialization
and execution order; all twelve full-device/CPU/PCM/final-image endpoints agree.
A separate every-frame-image-sequence check also agrees. These measurements
include Canvas presentation and PCM draining, but the normal player is not yet
connected to this optional path. They do not replace the current production
selection/story results or establish real-time or AudioWorklet acceptance.


### Peer-admissible sound checkpoint elision (#138)

This increment retains the original Rust core and its existing guest clock/bus
schedule. After planning a cached SH-2 burst, it measures the largest gap between
usable retirement endpoints, including the initial gap from the board clock and
filtering endpoints through the peer's eligibility rule. If the existing PLL
reference-span bound is strictly less than nine clocks, the existing four-clock
68000 control-write/reset phase guarantees a usable endpoint at or after sound
lookahead stops and before the control effect. The full Sound rollback snapshot
is unnecessary in that case. Larger gaps retain checkpoint/rollback; the existing
specialized Pending-peer certificate remains available. SCSP ticks, DSP/PCM work,
RAM arbitration, interrupts and guest retirement positions keep their schedules.
A feature-only diagnostic counter records the new elisions without adding
counter storage to normal builds.

Frozen baseline source is main `f25b2199bf2cfa6bd022a049c070aae2a7c0fa80`.
Local builds use Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127 with unchanged
release flags and LLVM inline threshold 1000; Chrome is 157.0.8103.52. Production
Wasm SHA-256 values are:

- Baseline: `87b5bf7dac8ec709be0008800bd132052b32d5e6c373262668e8ecae73f00098`.
- Candidate: `ad3c2111d3ac3d6f10bdca70b4acf4338a9474f91d8e6d9d75209cae94a1ba75`.

The authorized BIOS/CUE selection workload uses the established deterministic
input, 3600 warmup fields and six continuous 80-field windows with Canvas and
PCM checking. Both variants finish preparation before either is timed. No local
native build/test/profile or second owned Chrome runs during preparation, timing
or browser cleanup.

| Production-artifact comparison | Baseline fields/s | Candidate fields/s | Change |
| --- | ---: | ---: | ---: |
| Baseline initialized first | 27.67703 | 28.64030 | +3.4804% |
| Candidate initialized first | 28.10815 | 28.20659 | +0.3502% |
| Fresh candidate-first repeat | 28.46705 | 28.69303 | +0.7938% |

All corresponding CPU/video/PCM endpoints agree. Six, five and six paired windows
improve respectively; the slower reverse window remains in its aggregate.
Candidate p95 ranges in the first two runs are 35.3–36.2 and 35.6–42.1 ms; p99
ranges are 36.1–37.0 and 36.0–42.4 ms. The first 3.48% result is not a stable gain
forecast. A separate feature-enabled run matches all 22 device hashes at every
window and the final normal-build endpoint. It records 75013, 75037, 77326,
76249, 75238 and 76434 newly avoided snapshots per 80 fields. Its timing is
instrumented diagnostic evidence, separate from the production comparisons.

All 57 cached-execution native tests pass, including the full-board dual-poll
PCM/control/reset/both-PLL differential that now asserts the new path executes.
Executed mutants ignoring peer eligibility or the initial gap fail the new
boundary oracle. A mutant relaxing the horizon to 1000 reference clocks fails
the inherited long-refill/MCIEB guest because a sound control write precedes the
next valid CPU retirement. Exact byte restoration, targeted positive controls,
native all-target Clippy and Wasm Clippy pass. A separate initial zero-match
filter is not counted among the 57. Main/Worker browser conformance digests stay
unchanged. Final-head CI, review and the continued story comparison are recorded
in the associated pull request before adoption.

Original production controls change from 65.93648 to 66.70125 fields/s for the CPU
loop (+1.16%), 51.98931 to 51.65289 for busy sound/video (−0.65%), and 127.17702 to
126.80075 for normal video (−0.30%). Corresponding CPU/video/PCM endpoints agree.
These synthetic results do not establish game real-time execution.

Independent follow-ups remain excluded: empty SCSP tick coalescing improves an
original audio fixture but regresses the game 28.08299 to 27.93394 (−0.53%);
STOP/HALT poll coalescing changes 27.25554 to 26.19315 (−3.90%); retaining the
completed 68000 opcode-fetch prefix changes 27.45634 to 27.47552 (+0.07%) and
26.30151 to 27.00999 (+2.69%), with an effectively flat first comparison.

This is a partial #138/#17 increment. Around 28.7 fields/s in this production-Wasm
benchmark still needs about 2.09 times more throughput for 60. The normal player's
paced animation loop and live AudioWorklet underflows are not measured by these
comparisons; sustained real-time, broader playback/transitions and hardware
completion remain unmet. The optional renderer Worker remains a separate path.
Private inputs, raw traces/counters/captures and external builds stay local and
ignored. Required post-merge verification: none.

### Worker-owned normal player (#138)

The normal player now keeps one original Rust Machine in a module Worker during
play, pause and debugging, and enables the existing original Rust VDP2 renderer
Worker. Input, media, backup RAM and CPU/device state stay with that Machine.
The main thread presents every completed field on Canvas and sends its PCM to
the normal AudioWorklet. The two-field credit is released after both the renderer
receipt and main-thread presentation; separate ports join image and PCM streams
before presenting. This uses owned transfers without shared memory or special
cross-origin isolation headers. The production build enables `video-worker`;
the separate `kernel-experiment` engine and diagnostic exports remain disabled.

Paused native operations use an asynchronous facade with ordered, bounded
batches. Large reads split at 4096 operations, and a callback from an early chunk
cannot move a later reset ahead of the remaining reads. Actual raster-field
playback retains the native clock and device boundaries. Host pause/navigation
waits for at most two already committed fields; the paused CPU then remains
exactly stationary. Existing eight-field movement and guest CPU/RAM/pixel/PCM
expectations are retained. Navigation preserves existing faults, and backup
reads cancelled before acceptance are distinguished from accepted imports whose
acknowledgement arrives after a boot change.

The current normal Wasm SHA-256 is
`8b433ede11998b45f572c368dc3c86d98069dab1defa519a5ff8654540644fd4`.
Main uses the preceding production Wasm
`ad3c2111d3ac3d6f10bdca70b4acf4338a9474f91d8e6d9d75209cae94a1ba75`.
In Chrome 157.0.8103.52, the authorized game menu uses a target warmup of 3600
fields, its existing local controller sequence and six 5-second intervals.
Both players use their actual UI pacing, every Canvas presentation and an enabled
AudioWorklet. Each owned browser exits before another browser or native job.

| Normal UI menu measurement | Main | Worker player |
| --- | ---: | ---: |
| Initial observed fields/s | 28.1267 | 36.7111 |
| Subsequent fresh-browser fields/s | 28.4721 | 36.5827 |
| Subsequent measured underflow output frames, approximately 30 s | 696274 | 514143 |
| Host audio drops in measured intervals | 0 | 0 |

The subsequent observations give a 28.49% throughput difference. The Worker
player's p95 presentation-gap range is 28.0–28.4 ms and p99 is 28.3–29.2 ms.
Every measured field count equals its Canvas presentation count. The initial
Worker measurement predates the later paused-UI/input corrections; the subsequent
Worker measurement uses the final production JavaScript. The normal UI input
threshold follows presented fields and can reach the two architectures at
different in-flight field edges, so these runs do not claim a strict private
CPU/device endpoint differential. **60 fields/s and normal-audio real time remain
unmet.** No emulated field or timing edge is discarded to improve FPS.

A separate normal-player narrative run reaches the photographed character and
Japanese text after the 12814-field warmup and 109 local input events. The same
six 5-second measurement intervals yield 39.0438 fields/s, with every field
presented, p95 gaps of 37.1–37.7 ms, p99 gaps of 39.9–43.1 ms, 460045 underflow
output frames and zero host drops. The local final image was inspected. This is
a broader actual-game observation, not a comparison with the menu throughput.
A fresh-browser main run with that input and warmup reaches the same inspected
photo/text scene and measures 31.3459 fields/s (the Worker observation is 24.56%
faster), p95 gaps 43.5–45.6 ms, p99 46.4–47.4 ms and 636838 underflow output
frames. It also presents every field and has zero host drops. This narrative
comparison is one sequential pair, distinct from the repeated menu observations;
the same presentation-relative input and private-endpoint limitation applies.

`node scripts/benchmark-player.mjs` reproduces the actual-player measurement with
an original licensed firmware fixture by default. It reports runtime hashes,
field/draw counts, p95/p99 presentation gaps and real Worklet counters. Optional
local `--bios`, `--disc`, repeated `--track` and `--buttons` paths select an
authorized workload; `--web` and `--pkg` select frozen runtime assets. `--warmup`,
`--seconds` and `--runs` define the interval, and optional `--capture` stays local.
Controller TSV rows are a presented-field threshold and a hexadecimal port-1
mask. Default/original-fixture results do not establish game throughput. No local
firmware/media is served or uploaded by the measurement tool.

Local validation passes 1400 native tests in 111 result groups, formatting,
native all-target/all-feature Clippy, production Wasm Clippy/build, the shared
33-group browser trace and 13 runner/publication-gate tests. The independent
41-group shard passed before the final large-batch facade correction; that
correction separately passes complete CHD byte/TOC/PCM checks. Exact-head CI must
rerun both shards against one artifact digest before merging.

The stricter original-SCSP real-time probe
`node scripts/verify-playback-audio.mjs --direct` is also executed against the
normal production package. It **fails** its real-time pacing assertion: 240
fields in 5083 ms, 43985 Worklet underflow output frames, zero drops, nonzero
original waveform peak 0.75, and both pending queues drained. This is an unmet
performance acceptance probe, not a passing correctness check or a CI gate.
Its pacing/zero-underflow requirements remain intact.

Initial PR CI at `ea3429a` passes Wasm and the 13 runner tests, then rejects a
shared-shard scheduler test that incorrectly requires host throughput of at least
58 fields/s: the CI host achieves 27.50 fields/s. The scheduler regression now
requires real Worker progress with RAF withheld, no synchronous guest advance
from future/duplicate RAF callbacks, the 62-fields/s upper bound and exact
stationarity after pause. It does not set a host-speed floor. This change does
not relax the separate game/SCSP real-time acceptance or native guest timing
oracles; those remain measured and unmet where reported above.

The next CI shared shard reaches the hardware-lab trace, then exposes a valid-disc
replacement race: a status-only wait can accept the old Machine before the new
selection finishes resetting, and the test issues a step against the stopped
intermediate CPU. Valid-selection waits now require the accepted local-disc
message, completed UI action and enabled Start in addition to native Ready. The
same correction covers equivalent original-disc, large-media and HLE-player
waits. All original hardware-lab assertions pass in a focused run with local
File reads delayed 100 ms; guest expected values are unchanged.

Feature-enabled original-guest browser comparisons preserve all 22 private CPU/cache/memory hash words, CPU clocks/registers, every returned image and PCM through direct/relay
paths, pause, in-flight reset and delayed receipts. The equal-100-ms delay control
initially failed to expose an early-credit mutant; the retained 250-ms renderer
receipt versus 100-ms PCM control passes normally and detects that mutant at the
ownership-drain assertion. A real callback-triggered reset following 8193 queued
reads preserves their original values; the executed interleaving mutant fails.
The new native long self-refresh regression also checks exact CPU/clock/device/
image/PCM state while transport respects its existing per-chunk clock budget.

This is partial work related to #138 and #17. Exact-head CI and normal PR review
remain pre-merge requirements. This increment does not establish general
stock-firmware compatibility or hardware completion. Private inputs, captures
and raw profiles remain local.
Required post-merge verification: none.

### Owned Sound Worker and deadline-bounded SLEEP waits (2026-11-17, partial)

The original Rust Sound domain now has an experimental browser integration in a
separate `sound-worker` feature, which includes `video-worker`. Kernel tracing
and full-state diagnostics remain under `kernel-experiment`; normal playback
uses the same SH-2, M68000, SCSP and device implementations as before.

Sound runs ahead on a dedicated Worker only while no new shared inputs are
known. The main board consumes an already computed prefix with a constant,
verified Sound interrupt level. Sound RAM/SCSP, SMPC writes and CD accesses
that can supply CD-DA stop Main before their effects. An outstanding admitted access also prevents starting
another forecast. Pending control/device deadlines and the first Sound IRQ
transition bound the prefix. At the next input or field end, the Sound owner
rewinds to Main's actual reference time and returns canonical hardware and PCM.
No future field, PCM or backup state is published early.

The bounded Sound history preserves CPU replay, synthesis, DMA RAM writes and
PCM overflow with checkpoints and physical RAM undo. Handoffs carry hardware
metadata and only changed 4 KiB RAM pages. Packet generations, checksum, page
order/index, size, framing and expected committed time are checked before state
mutation. Main retains its hardware at the remote origin; if transport fails,
it recomputes that prefix without shared inputs and finishes the field on the
ordinary synchronous core. Native CPU errors still propagate. Public reset and
paused reset batches reset the owner's state and transport generations. A healthy
Worker retains its loaded module; a failed owner is replaced.

The first implementation copied 512 KiB RAM on every handoff and regressed the
local game comparison from 35.31 to 24.36 fields/s. Across 160 measured fields it
transferred approximately 1.61 GB. Page differences reduced that overhead; the
corresponding six-window diagnostic reached 41.80 versus 36.27 fields/s. Avoiding
handoffs for idle CD status/report ticks reached 44.13 versus 35.05 fields/s.
Those ticks and their interrupts still execute on Main. An original CD-DA guest
checks that a new play command still ends the forecast before real nonzero audio
starts. These direct diagnostics are separate from normal-player performance.

Starting speculative handoffs during upload/control clusters was also costly.
The integrated player keeps an ordinary whole-field path when the Sound CPU is
disabled or the boot mode is HLE. Eligible runs wait for 512 reference clocks
without a shared access before another handoff. This is a performance policy;
all synthesis and guest timing continue on the synchronous fallback.

Ordinary cooperative waits initially stopped at every main clock. That was
nearly flat in the busy game diagnostic, but the normal SCSP AudioWorklet probe
regressed to 85 fields in 5041.6 ms with 154913 underflow output frames. Strictly
bounding projected waits before main, bus and fixed-domain events improved it
to 270 fields in 5014.3 ms, still failing the unchanged real-time assertions.

The remaining overhead included repeated zero-state SLEEP polls every eight
clocks. Only an already settled sleeping Master, with Slave disabled and no
eligible IRQ, NMI, reset, DMA, UBC or journal/bus debt, may skip those fixed-point
polls. Hardware still stops before every possible input/event and the Sound
frontier. The original CPU readiness phase is reconstructed, including a poll
that is due exactly at the endpoint. Native checks compare all CPU fields,
readiness, all four CPU/DMA replay journals, Sound, SCU interrupt/status/pending
signals, PCM and images against ordinary execution. Both PLL domains include
SCU/on-chip DMA and an original FRT guest that actually wakes through vector 32.
A positive witness requires waits longer than the ordinary eight-clock budget.

The resulting normal-build AudioWorklet probe passes without changing its
280–320-field or zero-underflow/no-drop assertions: 297 fields in 5000.8 ms,
zero underflow, zero drops, nonzero waveform peak 0.75, drained queues, and at
most one pending field. This is an original-guest result, not game acceptance.
Healthy browser playback and injected Sound return failure both preserve
registers, clocks, private CPU/cache hashes, PCM, images, pause and in-flight
reset. Sound job counts prove the original SCSP fixtures exercise actual offload;
the failure case proves synchronous recovery rather than an unexercised branch.

The first **normal UI** local-game menu comparison uses 3600 startup fields,
the same recorded button sequence and six five-second windows, with Canvas and
an actual AudioWorklet enabled throughout:

| Runtime | Fields/s | Presentation gap p95 | Presentation gap p99 | Underflow frames / 30 s | Audio drops |
| --- | ---: | ---: | ---: | ---: | ---: |
| Main `f6eb28b`, production Wasm `8b433ede…` | 36.25 | 28.3–28.7 ms | 29.0–29.5 ms | 522570 | 0 |
| Isolated Sound candidate, production Wasm `356f9acc…` | 42.67 | 24.3–25.4 ms | 24.9–26.1 ms | 379826 | 0 |

Every measured field is drawn. The aggregate improvement is 17.74% in this first
sequential pair; it needs reverse-order and gameplay-scene follow-up before
adoption. The actual game still does not sustain real time or zero underflow.
Normal UI timing does not assert exact private state equality for asynchronous
host input; independent native, original-browser and controlled local-game
comparisons provide the state/PCM/image evidence described above. The 22 hash
words cover private CPU/cache/memory/readiness state, not every device.

A separate normal-player story interval uses 12814 startup fields, 109 recorded
input changes and six five-second windows. Baseline runs first in this pair:
39.61 → 42.45 fields/s (+7.17%). Candidate presentation gaps are p95
36.2–37.5 ms and p99 38.9–40.8 ms. Every field is drawn; both captures contain
937 colors. Audio underflows decrease from 447863 to 384725 output frames over
30 seconds; audio drops remain zero. The actual owner completes 144951 jobs
including startup, with the owner healthy at the final paused inspection.
This validates another scene improvement, while substantial tail latency and
audio starvation remain.

A separate diagnostic attaches matching-symbol CPU profilers to the two owned
Workers. Main spends 36.81% of samples in cached CPU planning, excluding nested
Sound and VDP work, and 12.75% in VDP1; 10.08% is idle. The Sound owner is 52.74%
idle. These are disjoint sample groups, not summed inclusive percentages.
Profile timings are excluded from speedup claims. CPU planning is the next
measured bottleneck; no additional CPU optimization is adopted by this record.

The first complete browser run caught an unwanted network side effect: public
reset recreated the Sound Worker and fetched its modules again during local CUE
replacement. The unchanged no-network assertion failed. Healthy reset now clears
the Rust owner through an acknowledged command on the existing Worker; only a
failed owner is replaced. This lifecycle correction is separate from the v3
throughput results above, which used the earlier reset implementation. The full browser regression and the subsequent contracts and performance checks
below pass for the corrected JS.

Complete native validation passes 1422 tests in 111 result groups, all registered
fixture checks, formatting, native build and all-target/all-feature Clippy with
warnings denied. Wasm production/all-feature Clippy and builds also pass.
The browser runner's 13 regression tests and all 74 registered browser groups
pass with the corrected reset lifecycle.

The corrected lifecycle also passes the healthy/fault-injected Worker contracts,
paused facade and unchanged real AudioWorklet assertions: 297 fields/5003.5 ms,
zero audio underflow/drops, p95 21.3 ms and p99 23.0 ms presentation gaps.
A fresh normal-player menu pair reverses the earlier execution order, running
baseline first: 36.55 → 43.01 fields/s (+17.69%). Candidate p95 is 23.9–24.3 ms
and p99 is 24.2–25.0 ms; every measured field is drawn. Both captures retain
2840 colors. Audio underflows decrease from 514722 to 372291 output frames over
30 seconds, with zero drops. The final inspection reports 62977 actual Sound
jobs including startup and a healthy owner. The repeat corroborates the earlier
menu gain, while game real time and zero-underflow acceptance remain unmet.

Hosted run 43900313512 subsequently fails the shared browser shard's initial
250 ms no-RAF progress check; Wasm succeeds. The same CI-produced artifact
passes three local pacing repeats. A controlled 500 ms host delay reproduces
the old failure despite 101 fields completing over 2254.8 ms: its first 250 ms
contain zero fields, which does not establish RAF dependence. The test now
requires two completed fields with RAF still withheld before its unchanged
250 ms and rounded/duplicate-callback cadence checks. This is a readiness
barrier, not a real-time benchmark or a guest timing change. With the injected
cold delay, all eight cadence cases pass; a deliberately stopped producer still
fails the readiness wait. The hosted failure remains historical evidence; a
fresh complete exact-head run is required before merge.

A separate CPU experiment tries the last selected captured-loop phase before
the ordinary PC-index search, retaining every private-state/cache/timing guard.
It compares flag on/off in the identical feature-enabled Wasm on main f6eb28b.
Native positive/stale-input and cached-execution checks pass, and all six local
game windows match private CPU/cache/memory hashes, PCM and image endpoints.
However, 29.1857 → 29.1949 fields/s (+0.032%) is effectively flat, despite
2274192 successful hints out of 6883741 attempts including startup. The original
guest control is also flat (+0.081%) and does not exercise the hint. This direct
feature diagnostic is separate from normal-player metrics. The CPU change is
excluded.

Run 43903382086 passes native/Wasm and the corrected no-RAF UI checks, but
later fails both browser shards when the changed Prism Courier disc and the
CD-RW companion case step a temporarily stopped CPU during asynchronous media
replacement. An empty error or a zeroed old guest marker is not an accepted-disc
signal. Both cases now use the existing `waitForLoadedDisc` boundary before
querying/advancing the replacement guest. The external-startup selection setup
likewise waits for the enabled Start control before capture. With controlled
350 ms local-file and 100 ms reset delays, both old cases reproduce the stopped-
CPU error; both corrected cases pass all their CD-RW/changed-texture assertions.
The complete external-startup scenario, including both selection orders and
overlapping reads, also passes. Guest operands, expected state/pixels/PCM and
all hardware timing checks are unchanged. Fresh complete exact-head CI remains
a pre-merge requirement.

Run 43908500995 passes native, Wasm and all 41 independent browser groups,
including the repaired CD-RW selection. The shared trace passes Prism Courier
and the subsequent hardware cases, then catches another asynchronous selection
wait in CHD rejection: the old guest still reports Ready before the new parser
has produced the expected cyclic-self error. The CHD selection helper now also
requires `aria-busy=false` before accepting its Ready/error condition. Its exact
rejection expressions and stale-state checks are retained. The complete controlled CHD scenario now reproduces the old premature-result
failure with 100 ms reset delays and passes all original codec, rejection and
stale-state assertions with the corrected wait. Complete fresh exact-head CI
remains required before merge.

A subsequent dependency audit removes handoffs for read-only SMPC operations
and CD accesses on immutable media without Audio tracks. SMPC reads take `&self`
and cannot issue a Sound command; all writes and pending control deadlines keep
their cuts. `CdBlock::audio_source` requires `TrackMode::Audio`, including audible
scan. With data-only/no media, its external SCSP input is always zero. Main still
executes every CD register effect, clock, DMA and interrupt. All Sound RAM/SCSP
accesses and CD accesses on media with Audio tracks remain fenced, including
already admitted beats. A kernel-only switch retains the old dependency cuts
for comparisons in the identical Wasm; production uses the proven narrower set.

Ten native rendezvous cases pass, including the new read-only/write/Audio-track
negative control and inherited full CPU/journal/Sound/SCU/PCM/image comparisons,
both PLLs, DMA, FRT wake and actual nonzero CD-DA. In six alternating 80-field
windows after 3600 startup fields, the same-Wasm real-owner comparison improves
42.86 → 45.18 fields/s (+5.43%), with full Sound byte equality and private CPU,
PCM and image checks. Owner jobs decrease from 2880 to 1920 across 480 fields.
The original busy guest is flat (−0.073%), with 360 actual projections per route.

Normal production playback confirms the gain with actual AudioWorklet and
every-field Canvas in six five-second windows:

| Scene | Previous Sound Worker fields/s | Dependency-filter fields/s | Change | Underflow output frames / 30 s, before → after |
| --- | ---: | ---: | ---: | ---: |
| Menu, baseline first, 3600 startup fields | 42.47 | 44.97 | +5.89% | 383065 → 328518 |
| Story, candidate first, 12814 startup fields and 109 inputs | 42.71 | 45.61 | +6.78% | 379216 → 314159 |

Both owners remain healthy and audio drops are zero. Menu candidate gaps are
p95 22.9–23.1 ms / p99 23.6–24.6 ms; story gaps are p95 34.5–35.6 ms / p99
37.8–38.8 ms. Corresponding captures retain 2840 and 937 colors respectively.
Normal UI measurements retain the asynchronous-input limitation stated above.
This dependency filter is selected for the implementation. Final-source native
validation passes 1423 tests in 111 result groups, fixture regeneration checks,
formatting, build and all-target/all-feature Clippy. Final browser regression
and exact-head CI are pending. The game still has audio starvation
and remains below real time.

The game 60-fields/s objective, broader gameplay/transitions, exact-head CI
and final review remain unmet at this checkpoint. No change is
merged by this record. Related to #138 and #17; both remain open. Local inputs,
raw profiles, packets and captures are not published. Required post-merge
verification: none.

PR #170 was subsequently merged as `d6b0fdca5f0532ec7fa3dad97006a6f61408751a`.
Its final head `2a35526bb5aaace1e2d4385c6e4f4b1f00dc3acd` passes
[complete native/Wasm/browser/aggregate CI](https://github.com/takahirox/saturn-rust/actions/runs/43916103336).
Final local validation passes 1423 native tests in 111 result groups and all
74 browser groups. The final original AudioWorklet guest delivers 298 fields in
5009.2 ms with zero underflow/drops. The recorded author self-review finds no
blocking findings; it does not claim independent approval. Game real time,
zero game underflow and broader gameplay acceptance remain open.

## Compare Sound RAM pages eight bytes at a time (checkpoint)

The normal Sound handoff compares all 128 RAM pages against the previous
owner's mirror. Each page has 4096 bytes. Inspection of the frozen production
Wasm `a5fd3157…` finds that `Delta::export` calls a 67-byte, byte-at-a-time
`memcmp` with length 4096. This is the actual page comparison, rather than an
unrelated library function. The earlier normal-player profile attributes 3.82%
of main-board Worker samples to `Delta::export` itself; that percentage is not
an isolated attribution to the comparison loop.

The candidate compares each complete page as 512 exact `u64::from_le_bytes`
values, with early exit on a difference. It uses safe Rust and requires neither
SIMD nor aligned input. Changed-page selection/order, packet checksums,
generations, ownership, emulated clocks and all sound execution are preserved.
The generated candidate replaces the page's `memcmp` call with unaligned i64
loads and comparisons. Small fixed CPU comparisons already have wide generated
loads; this finding does not justify changing their guards or replacing libc.

The two existing delta tests and three complete-state wire tests pass. The
alternating-owner fixture now changes one byte per page at offsets selected
from 0, 7, 8, 31, 2048 and 4095, covering word boundaries and the page end.
Deliberately omitting the last word fails that actual handoff fixture; exact
source restoration passes. Production/all-feature Wasm Clippy and optimized
builds also pass.

A first normal UI menu pair runs baseline first, then candidate, with the same
3600-field startup, recorded inputs, six five-second windows, every-field Canvas
and a real AudioWorklet:

| Runtime | Fields/s | Presentation gap p95 | Presentation gap p99 | Underflow output frames / 30 s | Audio drops |
| --- | ---: | ---: | ---: | ---: | ---: |
| Main, production Wasm `a5fd3157…` | 44.81 | 22.9–23.2 ms | 23.4–24.8 ms | 332071 | 0 |
| Word comparison, production Wasm `3d1e7ea5…` | 47.05 | 22.0–22.3 ms | 22.7–23.5 ms | 282519 | 0 |

This first pair improves 4.98%; both Sound owners remain healthy and captures
retain 2840 colors. Normal UI retains its asynchronous-input limitation and does
not establish a strict private-state differential. Reversed execution order,
full Sound/private CPU/PCM/image comparisons, the story scene, complete exact-head
CI and final review are pending at this checkpoint. No adoption or merge of this
candidate is claimed here. Sustained game real time and zero underflow remain
unmet; #138 and #17 stay open. Private inputs and raw captures remain local.
Required post-merge verification: none.


## Exact private scheduling arithmetic (2026-11-17 checkpoint)

Related to #138 and #17. This candidate combines two independently measured
reductions in repeated CPU scheduling work, continuing from main `dedc8cd`.
It preserves the original Rust core and every guest-visible clock boundary.

The BSC refresh counter now counts divided-clock edges with integer arithmetic
instead of looping over every divider edge. It retains the first wrapped compare,
subsequent RTCOR periods, match flag, request eligibility, divider phase and
self-refresh recovery. Requests still wait for the real board arbiter; calculating
a request does not close SDRAM rows or grant refresh. The unchanged bus-aging
operation composes exactly across the former subdivisions. A 64-bit intermediate
handles the entire 32-bit input-clock range.

Private Slave publications retain the existing masks derived from zero/63 cache
replacement seeds. Each full or partial publication also retains a 64-bit set
bitmap. Publication updates only sets whose six replacement bits can change,
without copying and transforming all 64 sets. Code, cache-way, CPU, transaction,
interrupt and deadline guards remain unchanged.

The original BSC regression compares complete state against a separate phi-clock
oracle across clock selections, zero/nonzero/wrapped compare values, counter and
phase boundaries, Master/Slave mode, refresh/self-refresh and real bus aging. It
also compares subsequent refresh grants. A literal full-width clock case covers
overflow. The existing per-publication CPU regression now starts every cache set
with nonuniform replacement bits and compares all intermediate states against
ordinary execution, covering untouched sets and partial/delayed publications.

Negative controls separately shorten the wrapped compare period, overflow the
clock sum and omit one changed cache set. Each is detected by its targeted oracle;
source bytes are restored exactly before successful tests. A final-field-only
cache test did not detect the omitted-set control, so the effective control uses
the existing per-publication comparison. An initial new BSC test-helper scope
compile error was corrected before the positive tests.

Combined local checks pass formatting, 59 on-chip tests, 57 cached-scheduler
tests, 4 private-poll tests and 4 loop-probe tests. Normal and all-feature Wasm
Clippy and both release builds pass. Each source tree uses its own Cargo target
directory. An incorrectly reused normal artifact was caught before measurement
and discarded; the corrected sparse normal build and the previously measured
sparse feature build both contain the expected set-bit traversal.

Separate diagnostic game comparisons use six alternating 80-field windows after
field 3600 with the same mounted game and controller inputs. Every paired window
matches full Sound bytes, 22 private CPU/cache hashes, rolling PCM and final
image. All six pairs improve for each independent candidate:

| Independent diagnostic | Baseline fields/s | Candidate fields/s | Change |
| --- | ---: | ---: | ---: |
| Sparse private cache publication | 48.3978 | 48.8913 | +1.02% |
| BSC counter arithmetic | 47.4627 | 48.0418 | +1.22% |

These are separate comparisons; their baseline values must not be compared across
runs and their gains must not be added. Original-busy controls change −0.63% and
−0.25%, respectively. A separate six-pair combined diagnostic improves 47.8302
to 49.0161 fields/s (+2.48%), with every pair faster and all corresponding Sound,
private CPU/cache, PCM and image endpoints matching. Its original-busy control
changes +0.29%. Normal-player comparisons remain pending at this checkpoint.
Normal-player acceptance requires the real
AudioWorklet, every measured field drawn, a fresh reversed order and story
input/scene coverage; diagnostic timing alone is insufficient.

A separate early private-peer planning gate remains unadopted. Its normal menu
comparisons improve +3.61% and +1.28%, but its first story comparison regresses
47.5402 to 47.0050 fields/s (−1.13%) and increases underflows. That gate is absent
from this combined candidate. Its guard-order revision remains a separate local
experiment.

Combined normal Wasm SHA-256:
`715e48cf521c7b95d7ae454aecd210d7a80df6d6fb42c4d9b7c3db3b1669b05e`.
Combined experimental Wasm:
`2591097cb5104871ca8597a411c95ec0b08f6933dbe9481c72c1d42e512bbdde`.
Complete exact-head CI and final review are pending. Sustained 60 fields/s,
zero audio underflows, broader gameplay and hardware completion remain unmet;
#138 and #17 remain open. Private inputs and raw captures/profiles stay local
and ignored. No post-merge verification is required.


## Live-origin WRAM write plans (2026-11-17 checkpoint)

Related to #138 and #17. Both remain open. The original Rust core is retained.
This checkpoint is below real time; it does not establish hardware completion
or general firmware/game compatibility.

Captured CPU replay previously ended before the strict BSC event even when its
cache inputs stayed unchanged. The new path upgrades a successful replay by
validating a longer CPU prefix against the canonical arbiter in a persistent
private board initialized from the **live** BSC and A-bus state. Captured store
request and acceptance times must match. Physical byte completions, refresh
row closure, ownership and the final pending write buffer are recomputed;
physical writes are never copied from an old captured bus trace.

This first slice requires an empty initial external transfer/buffer,
cache-resident inputs, big-endian WRAM stores, a private Pending peer or no peer,
and no Sound lookahead. Complete CPU-origin, cache-input and mapped LRU proofs
remain required. Existing IRQ/DMA/UBC/reset, PLL, device and observer horizons
remain. The model clocks only BSC/A-bus state; real OnChip peripherals, Sound,
VDP/raster, CD and SMPC advance during ordinary independent catch-up. Rejection
uses the previous execution path. No frame skipping or timing quantum change
is introduced. Initially pending WRAM buffers/refresh remain future work.

### Measured evidence

Rust 1.100.0, wasm-bindgen 0.2.129, Binaryen 127 `-O4`, inline threshold 1000,
headless Chrome 157.0.8103.52. Each source uses a unique Cargo target and frozen
inputs. Private media, captures, snapshots and raw measurements stay local.

Normal baseline Wasm SHA-256:
`715e48cf521c7b95d7ae454aecd210d7a80df6d6fb42c4d9b7c3db3b1669b05e`.
First production candidate:
`92c86c76a2ec41a4cc8e4f74bb4f948a2c27c3228a30ae4381991c89a89c332a`.
Its feature companion:
`9ef4fa17502d4414f018b01237cf41f609ae2f851cf93f48e23b81a12ad79666`.

Normal UI measurements use separate fresh Chrome processes with one page each,
the authorized mounted game and existing 16-event menu input, 3600 warmup fields,
six consecutive five-second windows, every field drawn, actual AudioWorklet and
active Rust Sound Worker. Repeating reverses the process order.

| Normal player comparison | Baseline fields/s | Candidate fields/s | Change | Audio underflow sample frames, baseline / candidate |
| --- | ---: | ---: | ---: | ---: |
| Baseline first | 47.402 | 53.205 | +12.24% | 274892 / 146537 |
| Candidate first | 46.471 | 53.499 | +15.12% | 295342 / 139903 |

All twelve normal windows draw every field; audio drops are zero. Display-gap
p95 ranges improve from 21.6–22.8 to 19.2–19.7 ms in the first comparison and
22.2–22.4 to 19.2–20.0 ms in the repeat. Corresponding p99 ranges are 22.1–23.6
to 19.4–20.5 ms and 22.8–23.8 to 19.8–21.4 ms. These are Canvas delivery gaps,
not isolated CPU latency. Normal UI measurements do not claim strict private
state equivalence; use the native and feature companions for that proof.

The preceding opt-in Lite prototype, one identical diagnostic feature artifact
with the flag off/on, improves 47.348 to 56.098 fields/s (+18.48%) across six
alternating 80-field windows after 3600 warmup fields; all six pairs improve.
Full Sound snapshot bytes, CPU/cache fingerprints, rolling PCM and image
endpoints match in every window. The original fixture changes 123.900 to
124.150 (+0.20%) with zero optimized commits. These prototype timings do not
include a real AudioWorklet and must not stand in for normal-player FPS.

### Correctness and remaining acceptance

The production-default path passes all 958 native library tests, formatting,
normal/all-feature Wasm Clippy and both normal/feature release builds. New
checks cover literal byte publication at 8/16/24/32 clocks, wrong accepted-time
rejection, and 4096 every-clock comparisons against ordinary bus operation
across bus widths, RTC phases, refresh, SDRAM row history and nondefault A-bus
configuration/protocol. Dual-SH2 execution compares full Sound bytes, PCM,
CPU/cache/BSC and active-buffer state, including SDRAM and nondefault A-bus.
Another 176 actual burst exits cover both PLLs and 16/32-bit buses. They require
an actual optimized commit with physical `1234A5A5` before the low half drains,
and compare active/buffer/context/ownership before the caller's next tick.

Three executed final-source mutants fail assertions for premature whole-word
publication, omitted final BSC state and a dropped pending write buffer.
Exact source restoration passes the positive controls. The additional row/A-bus
checks are test-only; final production artifact identity is being rechecked.
Complete all-feature native/integration checks, exact-head CI, full review and
meaningful story measurements are pending at this checkpoint. PR evidence must
record their actual results before merge. Sustained 60 fields/s, broader gameplay
and real-time audio/latency acceptance remain unmet. No post-merge verification
is required.


## Certified short CPU cycles and pending-origin writes (2026-11-17 checkpoint)

Related to #138 and #17; both remain open. This is an original Rust-core
increment. Meaningful story execution remains below real time.

The preceding quiet-origin write plan improves the menu but does not improve
all scenes. A fresh normal-player story comparison of the prior main, quiet
plan and pending-origin prototype measures 47.581, 46.177 and 46.638 fields/s,
respectively. Those results are a scene tradeoff, not a story speedup.
A production-path diagnostic on the story finds no commits through either new
write-plan path. Instead, most Master instructions use the older short-cycle
folder, while captured replay never succeeds. Main execution is the bottleneck;
Sound and Video workers remain approximately 48% and 66% idle. Profiling and
instrumented timings are excluded from adoption FPS.

This increment makes two bounded changes:

- A short cache-only cycle becomes a learning header only after the existing
  folder has actually proved its complete CPU/local-cache recurrence. Countdown
  and passive external-read cycles are excluded. Private learning records at
  least 512 actual retirements of the proved recurrence, reusing the existing
  guarded captured-loop backend and its unchanged instruction/boundary caps.
  CPU-origin, input bytes, cache identities/LRU, pipeline, IRQ/DMA/UBC/reset,
  PLL, Sound and observer proofs remain required. Failed proofs use the existing
  paths; learning does not advance the live machine.
- The live canonical write plan can begin with a pending Master WRAM buffer or
  refresh. It preserves initial ownership, accepted beats and bus context,
  independently computes completion of older accepted bytes without reapplying
  their cache acceptance, and retains the final active/buffer state. A separate
  end limit keeps the general non-replay store path unchanged.

### Measurements

Rust 1.100.0, wasm-bindgen 0.2.129, Binaryen 127 `-O4`, LLVM inline threshold
1000 and Chrome 157.0.8103.52. Each build uses unique Cargo targets and frozen
inputs. All private media, raw profiles, snapshots and captures remain ignored
locally. Normal player timings use a fresh Chrome process/page for each variant,
all-field Canvas presentation, the actual AudioWorklet and active Rust Sound
Worker. They do not claim private-state equality; separate differential evidence
below supplies that proof.

| Normal workload | Baseline fields/s | Candidate fields/s | Change | Underflow sample frames, baseline / candidate |
| --- | ---: | ---: | ---: | ---: |
| Menu, pending-origin only, candidate first | 52.036 | 59.829 | +14.98% | nonzero / 0 |
| Menu, pending-origin only, baseline first | 54.833 | 59.835 | +9.12% | nonzero / 0 |
| Story, pending-origin baseline then short-cycle candidate | 47.638 | 52.178 | +9.53% | 270178 / 168708 |

Menu runs use the existing 16-event input, 3600 warmup fields and six five-second
windows. All twelve pending-origin candidate windows sustain 59.756–59.965
fields/s with zero underflows and drops. The unchanged slow PLL and raster
period imply `26874100 / (427 * 4 * 263) = 59.826` fields/s, the nominal 60-Hz
rate; no guest clocks were accelerated. This menu evidence precedes the
short-cycle addition and does not prove that the combined candidate preserves
menu performance. That check is pending.

Story runs use 109 deterministic input events, 12900 warmup fields and six
five-second windows after actual narrative gameplay has begun. Every window
draws every field; drops and browser errors are zero. Combined-candidate windows
range 51.996–52.368 fields/s. Display-gap p95 improves from 32.7–34.2 to
31.0–31.6 ms and p99 from 35.9–37.6 to 33.7–34.8 ms. A fresh reverse-order
production comparison and a direct comparison with current main are pending.

Normal Wasm SHA-256:

- Current main quiet-origin plan: `92c86c76a2ec41a4cc8e4f74bb4f948a2c27c3228a30ae4381991c89a89c332a`.
- Pending-origin-only baseline: `325076cd0cce8fdd8bad79db1cbec5ac6b21082f67feb5123e81b14c332fd5cf`.
- Combined candidate: `1deb04a3dac2a59a72aca3ba5746746ac270f0d1f92420897f6c47fe5dcb033f`.
- Combined feature companion: `6b23b4e62f46c0ce65adbb4b66a2addaff3c0b2ed6c52703c367d4e2d48d87d4`.

The opt-in short-cycle screen uses one identical feature artifact
`336062f2198d97a07cc42ab44992b1c2b5bb1a0b6db8c87256dad83c0e894b8e`
with the flag off/on from boot. All variants finish preparation before timing.
Across six alternating 80-field story windows it improves 45.444 to 49.578
fields/s (+9.10%). Full Sound snapshot **bytes**, all 22 CPU/cache fingerprints,
rolling PCM and image endpoints match in every window. Roughly 17 million
instructions per candidate window use captured replay; the control uses none.
Original-guest endpoints also match, with zero short-cycle captures and −0.35%
variation. This diagnostic has no real AudioWorklet; it is not production FPS.
The separate pending-origin feature screen improves 55.518 to 61.617 (+10.99%)
in the menu with all six corresponding full-state endpoints equal; its original
control is flat (−0.28%).

### Correctness and outstanding acceptance

The combined-default candidate passes all 1432 native tests in 111 result
groups, native all-target/all-feature Clippy, formatting, normal/all-feature Wasm
Clippy and both release builds. Three original dual-SH2 fields prove actual
short-cycle capture/replay and compare full CPU, readiness, cache/LRU, both BSCs,
active bus state, full Sound bytes and PCM with the older folder. Changing real
cached input and code bytes compares subsequent execution again. Another sparse
four-instruction cycle checks both certified capture and the old folder against
ordinary execution. The first default-enabled test run passes 961 library tests
and fails the old assertion that sparse cycles never use capture; no guest-state
comparison fails. The updated test preserves both paths and their state oracles.

Two actually executed short-cycle mutants fail assertions: ignoring cached
input/code bytes and publishing the wrong CPU-ready clock. Exact source
restoration passes the positive control and matches all 115 frozen build inputs.
Pending-origin coverage adds 96-clock-cut canonical bus comparisons and 168
actual two-burst exits across both PLLs and bus widths, requiring a real
pending-origin commit that ends with buffered data. An earlier field-end-only
check let the dropped-final-buffer mutant survive; the raw-exit test closes
that demonstrated coverage gap. All three final pending-origin mutants fail,
with exact source restoration. Inherited quiet-origin controls remain intact.

Exact-head CI, final review, repeated production comparisons, combined menu and
advancing-story checks remain pending at this checkpoint. Neither this PR nor
menu real time satisfies the outstanding story/audio/latency and broader-gameplay
acceptance. No required post-merge verification is added.


## VDP1 display erase rows checkpoint (#138, 2026-11-17)

The current narrative workload makes 72,576 display erase pixel calls per field.
This candidate writes each clipped row as one contiguous run and records the
same front-buffer patch. Uniform bytes use a fill; other colors repeat their
big-endian two-byte pattern, retaining odd indexed-pixel phase and trailing
bytes. The timed erase engine, drawing phases, bus contention and event
boundaries retain their existing execution.

The proposed default is enabled. The old pixel path and counters remain
available to tests and the opt-in kernel experiment. The production candidate
is based on main `9de019ee33df765ac2abf110a5396827b32715d5`.

### Measured results and artifacts

An identical feature artifact with the flag disabled/enabled improves the
narrative scene from 51.993 to 53.058 fields/s (+2.05%). Every one of six
alternating continuous 80-field windows improves; full Sound snapshot bytes,
all 22 CPU/cache fingerprints, rolling PCM and image endpoints match. Each
candidate window records 17,920 rows and 11,612,160 bytes, exactly 224 rows and
72,576 pixels per field. This diagnostic has no real AudioWorklet.

The first normal-player comparison uses the same 109 controller events,
12,900-field warmup and six five-second windows in fresh browser processes.
Every measured field is drawn, the actual AudioWorklet and Sound Worker remain
active, and errors and audio queue drops are zero.

| Normal narrative workload | Main | Candidate |
| --- | ---: | ---: |
| Fields/s | 50.736 | 52.471 |
| Per-window p95 (ms) | 30.9–33.4 | 30.5–32.8 |
| Per-window p99 (ms) | 33.9–36.4 | 33.5–35.0 |
| Audio underflow sample frames over 30 s | 200,859 | 163,333 |

The aggregate first-order gain is 3.42%; five of six corresponding windows
improve. **A fresh reverse-order normal comparison remains pending.** These
normal UI measurements are throughput/audio observations, not strict private
state differentials. Do not treat this checkpoint as final adoption evidence.

Wasm SHA-256:

- Normal main: `1deb04a3dac2a59a72aca3ba5746746ac270f0d1f92420897f6c47fe5dcb033f`.
- Normal candidate: `158d14e03570d9554706ab228be489c2dd00fb2a698288d08f1ae99134940a5c`.
- Paired diagnostic: `1e8c07c62bf9ce88953e643449832b6c329c4f6edeacb9b6c1effaa0cf05d4f4`.

Toolchain: Rust 1.100.0, wasm-bindgen 0.2.129, Binaryen 127 with `-O4`,
inline threshold 1000, Chrome 157.0.8103.52. Private firmware, mounted media,
controller scripts, captures and raw measurements remain local and ignored.

### Correctness and remaining scope

An original 144-case matrix covers framebuffer widths, indexed/RGB modes,
uniform/nonuniform colors, both buffers and display clips. It compares every
byte against independently computed expected writes and the prior pixel path,
and compares the exact front-buffer journal offsets and bytes after each line.
Three actually executed mutants fail assertions: swapped color bytes, omitted
front patches and writes beyond the display clip. Exact source restoration
passes the positive control.

The default-enabled candidate passes all 1433 native tests in 111 result groups,
native all-target/all-feature Clippy, formatting, normal/all-feature Wasm Clippy
and both release builds. The feature comparison also passes all six complete
state endpoints. Exact-head CI, final review and the reverse-order production
repeat remain pending. Menu and advancing-story effects are not yet measured.

This is partial work related to #138 and #17. Sustained real-time narrative
execution, zero sustained audio underflows and broader gameplay acceptance
remain unmet; neither Issue is closed. No required post-merge verification.


## Controller-independent Sound ownership checkpoint (#138, 2026-11-17)

The normal narrative workload still needs more throughput after the VDP1 row
increment. An actual normal-player observer records 3255 Sound jobs over 787
fields (4.136 per field): 787 field returns, 787 admitted-range returns and
1681 shared-input returns. The leading input cuts are one SF write per field
and the Sound RAM mailbox read at offset 0x404. Main awaits total approximately
0.770 ms for job starts, 0.637 ms for prefixes and 0.783 ms for returns per
field; serialization adds approximately 0.143 ms export and 0.137 ms import.
These are instrumented wall-time observations, not CPU self-time or adoption FPS.

An IRQ-calendar prototype does not address those cuts: 51.977 to 52.074
fields/s (+0.19%) with exactly the same 1986 jobs, 7112 extensions and byte
counts over 480 fields. All six full-state endpoints match. It is held locally
and is not adopted. Its original workload also keeps the same jobs and varies
by -0.25%. Native/Worker correctness alone does not establish a performance gain.

### Focused change

The proposed default permits owned Sound to continue across pure SMPC
controller-data and INTBACK accesses. This applies only when the SMPC has no
pending non-INTBACK command and no deferred control. Sound/CD enable, reset,
clock changes, unknown commands and pending/deferred controls retain the
original cuts and conservative deadlines. Sound RAM/SCSP accesses retain their
existing rendezvous. The controller deadline is omitted only from the Sound
forecast's independent horizon; Main still executes every SMPC clock, IRQ,
physical transfer and device deadline at the original time.

The original-controller browser fixture completes the real SCSP boot and then
runs an original SH-2 INTBACK loop. The new policy uses 21 jobs over 22 presented
fields. Its image, PCM, CPU/register/cache fingerprints and reset/drain behavior
match canonical execution. Selecting the old policy actually fails the fixture's
ownership-count assertion. Normal, direct-presentation and forced Sound-return
failure variants pass all seven original fixtures.

### Measured opt-in evidence

One identical feature artifact uses the policy off/on, with both runtimes fully
prepared before timing. On the narrative scene, six alternating continuous
80-field windows improve 52.587 to 56.774 fields/s (+7.96%); every window
improves. Full Sound snapshot bytes, all 22 CPU/cache fingerprints, rolling PCM
and final image endpoints match in all six windows. Jobs decrease 1986 to 1026
over 480 fields, as expected from removing two controller cuts per field.
Extensions are 7112 and 7134. This diagnostic has no actual AudioWorklet.

The original busy sound/video control changes 113.558 to 114.723 (+1.03%),
with identical 360 jobs and six matching state endpoints. No controller-cut
reduction is claimed for that control; this small difference is not isolated
performance evidence.

The feature artifact SHA-256 is
`48c4ec27d48d6ecf032b20d08729efb35561cd5cd3746e2581c594c017ee9bad`.
Toolchain: Rust 1.100.0, wasm-bindgen 0.2.129, Binaryen 127 `-O4`, inline
threshold 1000, Chrome 157.0.8103.52. The narrative input has 109 events, a
12,900-field warmup, six continuous 80-field windows, Canvas and PCM draining.
Private media, firmware, controller inputs and raw evidence remain local.

### Correctness and remaining acceptance

The opt-in candidate passes 60 focused native tests, formatting, native
all-target/all-feature Clippy, both Wasm Clippy configurations and the feature
build. Original SMPC status, Sound/CD controls, peripheral acquisition, SCU/FRT
interrupts, Sound RAM/SCSP arbitration and failed transport are compared across
full-board fields. Tests require real ownership across controller event edges.
A literal deferred INTBACK/SOUNDON oracle retains 8600 then 807 reference clocks.
Rejected Main accesses execute with a running original 68k; after ownership
returns, error, CPU/cache, clocks, full Sound and PCM match canonical execution.
An initial test checked the retained origin CPU count before handback; the same
activity assertion now runs after handback, preserving all state comparisons.
Two initial Clippy test-initializer findings are corrected.

Three actually executed mutants fail assertions: ignoring control deadlines,
ignoring deferred controls and treating every command as controller-only.
All 118 frozen inputs are restored, and all 60 native tests pass again.
The first mutation script scopes matched too many or no sites after formatting;
it stopped without accepting those attempts and restored the source. The final
mutations each change the intended predicate and fail their expected assertions.

The production-default candidate additionally extends the original controller
and rejected-access comparisons to PAL/NTSC and both PLLs. Its full native suite,
normal release build, actual normal UI/AudioWorklet comparisons, exact-head CI
and final review remain pending at this checkpoint. Sustained real time, zero
sustained audio underflows and broader gameplay acceptance remain unmet. This
partial work is related to #138 and #17; both stay open. No required post-merge
verification is added.


### Retain VDP1 texture ownership inside drawing runs (2026-11-17)

Related to #138 and #17. The accepted normal artifact at this checkpoint is
`d9231cc6b0a38f4ce28173d7c96688760312d86d6d329f3cedfcf39d044751b0`
from main `8385628`. Its simultaneous 15-second worker profile attributes
12.435% of Main samples to VDP1 and 25.864% to remaining cached-batch planning;
Main is idle 4.293%, Sound 56.941%, and Video 61.101%. Profiling timings are
not throughput evidence.

The candidate retains the drawing cursor while an already admitted private
interval crosses texture fetches. Every word still consumes two main clocks;
an unfinished final word keeps its partial phase. A busy VRAM port prevents
new fetches while retained words remain usable. Table reads, pending six-state
pixels, forced stops and erase use the existing phase engine. Every pixel,
ordered display patch and blanking-clock deduction remains accounted for.

An original full-state differential test also finds that the old unfused
multi-clock engine can discard the prefix before ENDR's final clock. Disabling
the new run reproduces the same failure. Splitting immediately before that last
clock preserves one-clock execution's retained state. An independent 36+30-clock
vector checks the partial texture phase and absence of a fabricated draw-end
interrupt with both run settings. This does not establish physical-hardware
stop latency or general hardware completion.

The focused native checks pass 60 tests, including three new tests with 96
mode/port scenarios and comparison of every retained VDP1 field against the
original one-clock engine. Native all-target/all-feature Clippy, both Wasm
Clippy configurations, formatting and the feature build pass. Five executed
mutants fail assertions: dropping the pre-stop prefix, ignoring VRAM ownership,
completing a partial fetch early, fetching the wrong word and omitting blanking
clocks. All 119 frozen inputs are restored. Three browser modes over seven
original guests preserve full private CPU/cache hashes, PCM, every-frame pixels,
reset and drain behavior. The public verifier now forces the reference's old
run setting after each reset while the owned player keeps the candidate.

The identical-feature artifact is
`801f388840e4e1c74592279023d03d25252ecb0d1316c10f3c56de3415787cef`.
Six alternating 80-field windows per scene produce:

| Diagnostic scene | Old run fields/s | Owned run fields/s | Change |
| --- | ---: | ---: | ---: |
| Actual game, waiting | 56.5604 | 60.0510 | +6.1715% |
| Actual game, advancing conversation | 55.3710 | 58.8740 | +6.3265% |
| Original busy drawing/sound guest | 119.4069 | 147.1069 | +23.1979% |

Every game window improves; all 12 game endpoints and six original endpoints
match full Sound bytes, 22 CPU/cache hashes, rolling PCM and image results.
Preparation uses the production Sound owner; every endpoint additionally matches
the saved canonical-boot run. Ownership counters therefore include preparation
and must not be compared directly with older steady-only counters.

These diagnostics draw every field but do not run an actual AudioWorklet. They
are not normal-player FPS or evidence of real-time completion. The prior local
phase/address-reuse trial improves waiting/advancing by 2.7326%/2.2514%; the
compact-PC proof-search trial gives only 0.2951%/0.4977% and is held. Neither is
included here. Final full-suite, normal-player, CI and review results belong to
the implementation PR. Both source Issues remain open while their broader
acceptance criteria remain unmet.


### Preserve Worker pacing debt through short slow fields (2026-11-17)

Related to #138 and #17. Main `93f78fd` retains the normal Wasm
`231dc41a3839365d10ecd57603063688573ba43f2de3805e2935a6f6ed26a65a`.
Its normal player reaches about 54.3 fields/s even though an identical-instance
capacity diagnostic, with Canvas and real AudioWorklet, measures 54.2761 paced
and 60.1699 uncapped fields/s. Reversing the mode order with temporary scheduling
counters measures 59.7757 uncapped and 54.1796 paced. These are diagnostics, not
uncapped playback acceptance: underflows/drops and timing remain recorded.

The paced observer averages 16.6720 ms in field advancement, versus a 16.7151 ms
emulated period, but adds 1.7702 ms of waiting per produced field. Presentation
and renderer-capacity waits are negligible. It rebases wall time 73 times in
30 seconds. The two-field lag cutoff discards the phase after ordinary slow
fields, so subsequent fast fields wait instead of recovering the delay.

The Worker now retains debt up to eight actual field periods, about 134 ms in
the measured NTSC mode. Longer stalls still rebase; every guest field, clock,
PCM packet and picture is produced normally. The two-field transport bound is
unchanged. This changes host pacing only, with no Rust/Wasm or guest-timing
change and no dependence on RAF frequency.

Four deterministic tests execute the actual production start loop with a virtual
host clock: NTSC/PAL mixed-cost cadence, long-stall rebasing, permanently slow
hosts, and uncapped operation. They do not require CI CPU headroom. Restoring
the old two-field policy makes the mixed-cost cadence assertion fail; restoring
the candidate passes. The CI JavaScript command runs the 13 existing runner
checks plus these four checks. The initial isolated-worktree runs lacked local
dependencies and generated assets; supplying the existing dependencies and
licensed build outputs gives 17 passing tests without weakening assertions.

A separate real Worker/Canvas/AudioWorklet experiment imposes 14 ms fields and
one 54 ms field every 22 fields on the original SCSP PCM guest. The old policy
produces 271 fields in 5015.7 ms and 19509 underflow output frames, failing the
unchanged cadence assertion. The candidate produces 300 fields in 5014.4 ms,
with zero underflows/drops and the original nonzero waveform. Normal and direct
public audio checks each produce 299 fields in about 5001 ms with zero
underflows/drops. All three public playback modes over seven original guests
also preserve pixels, PCM, state, reset and bounded drain behavior.

The first normal actual-game waiting run improves approximately 54.3 to
59.7945 fields/s over six five-second windows, with every field drawn, an actual
AudioWorklet, zero drops/errors and 937 final colors. p95 is 30.1–30.7 ms and
p99 31.6–32.6 ms. Underflow output frames are [147, 0, 0, 0, 0, 0]: the initial
transient is reported, and the remaining 25 seconds have none. The longer
conversation-input measurement and final exact-head CI are outside this
checkpoint record; the implementation PR reports their actual results. This
record does not claim general firmware compatibility or hardware completion.


## Private peer bus admission before proof lookup (2026-11-17)

Related to #138 and #17. Heavy conversation profiling identifies Main CPU and
bus scheduling as the main remaining cost. A temporary production-path observer
counts about 9,285 cached-batch calls per field, including 3,876 initial admission
rejections, after private Slave proof matching has already run.

The private-peer path now checks the same bus predicate before proof lookup.
This preliminary check ignores exactly the Slave journal and internal phase
that the existing path temporarily removes before final admission. It retains
all Master and shared-bus conditions. A passing check still requires the full
Slave CPU, pipeline, code, cache and transaction proof and the ordinary cached
burst admission. This changes host preparation work without enlarging guest
execution intervals.

Three focused regressions verify rejection before proof construction without
guest changes, actual private polling batches against ordinary execution, and
130 bus-phase cuts against the real remove/check/restore sequence. Two actual
mutants fail the phase-cut assertion: retaining the removable Slave state, and
ignoring Master state as well. Restoring the exact source passes. The focused
native and scheduler groups total 17 passing tests; native and Wasm Clippy and
the feature build pass. The public normal/direct/Sound-return-failure contracts
pass all seven original fixtures in each mode.

One feature artifact compares the flag disabled/enabled during 12 alternating
80-field windows after 14,700 fields of the 157-event conversation input script.
Every field is drawn; both modes use owned Sound and Video Workers. Full Sound
bytes, 22 CPU/cache fingerprints, PCM and images match at every endpoint, also
matching the preceding canonical build at all 12 endpoints. Aggregate diagnostic
throughput rises from 41.5254 to 42.5604 fields/s (+2.4924%); all 12 windows improve.
The busiest windows remain near 19 fields/s. These capacity diagnostics have no
AudioWorklet and do not establish normal playback speed or the 60-fps objective.
The original busy-scene control has zero private-peer admission coverage and
its +0.23% difference is not evidence of this optimization.

Normal production-player evidence and exact-head CI belong to the associated
PR. The broader game-performance and hardware-completion Issues remain open.

## Reuse quiet bus arbitration at the same clock (2026-11-19)

Related to #138 and #17. CPU issue, reference-clock conversion and board
advancement can consult the arbiter at the same main clock. When the first
consultation runs no allocator, its result can serve the later calls at that
clock. Display acquisition only changes VDP2 fetch state. Synchronization of the
Sound RAM request and reporting of the current Sound boundary have already
occurred. The first board slice consumes the result. A later slice consults
the arbiter at its own clock. A consultation that ran the allocator retains
the existing checks. IRQ sampling keeps its independent guard.

The native comparison gives the arbiter a Master WRAM read followed by a Slave
Sound RAM read. The later clock grants the first Sound bridge word, and resuming
the read grants the second word. Both PLL settings preserve the two sampled
words, clock, IRQ observations and complete Sound state. Retaining the result
across clocks or disabling board reuse fails the focused test. Restoring the
source passes. Native and Wasm Clippy pass. The normal, direct and forced
Sound-return-failure browser checks pass on all seven original fixtures.

The feature diagnostic improves 51.73 to 52.20 fields/s over 28 continuous
80-field windows. Every window improves. The four slow windows gain 0.58% to
1.24%. Full Sound, PCM, image and CPU/cache/RAM endpoints match in both modes
and at 12 independently recorded overlapping endpoints.

The normal Web player runs six 10-second windows after 12,900 warmup fields in
each order. Each comparison applies the same 151 input events and draws every
emulated field. These results use the normal AudioWorklet.

| Measurement | Main first | Candidate first |
| --- | ---: | ---: |
| Mean fields/s | 51.450 → 51.535 | 51.352 → 51.720 |
| Same 361 slow fields, fields/s | 38.823 → 39.402 | 39.253 → 39.651 |
| Four matched slow windows, fields/s | 27.434 → 27.558 | 27.381 → 27.816 |
| AudioWorklet underflow output frames | 367087 → 362929 | 369997 → 355188 |

Both orders have zero audio drops and zero runtime errors. The highest
10-second p99 moves from 107.8 to 110.0 ms in the first order and from 109.9 to
104.4 ms in the reverse order. Fields 13821–13900 regress by 0.55% in the first
normal order and improve by 1.44% in the reverse order. The Web player continues
to miss 60 fields/s and the audio supply target during the measured gameplay.
The broader performance and hardware-completion Issues remain open.

## Restore a rejected cached MOV load (2026-11-19)

Related to #138 and #17. When a cached batch uses a proven peer schedule, an
ordinary MOV load can sample its operand and then run out of time for the
remaining memory phase. The original path restores the private bus state and
replays the accepted CPU prefix. The new path saves the destination, optional
postincrement source and private IF, MA and issue state. A rejected load
restores those fields with the existing private bus snapshot. The actual load
and fetch use the existing interpreter. A shorter endpoint selected by Sound
or peer timing still uses the existing prefix rebuild.

The native regression stops four MOV forms after operand sampling and checks
the complete CPU and the sampled address and clock. The forms exercise signed
word reads, postincrement, overlapping registers and R0-indexed addressing.
Omitting destination, source or fetch restoration independently fails the
regression. Restoring the exact source passes. The focused load, cached batch
and private polling groups pass. Native and Wasm Clippy pass. The normal,
direct and forced Sound-return-failure browser checks pass for seven original
fixtures in each mode.

The feature diagnostic runs 28 continuous 80-field windows after 13,420 warmup
fields with Sound and Video Workers active. Throughput improves from 52.229 to
52.659 fields/s. Twenty-seven windows improve. In fields 13821–13900 the
candidate restores about 750 rejected loads per field and reduces the mean
number of rebuilt instructions from 16,052 to 6,188 per field. Every pair
agrees on CPU, cache, RAM, clocks, images, generated PCM and full Sound state.
The last 12 pairs agree with independently recorded endpoints.

The production player draws every field and applies the same 151 inputs in
both orders. Each order uses six 10-second windows after 12,900 warmup fields.
Chrome 157.0.8103.52 runs the normal player with its AudioWorklet.

| Measurement | Main first | Candidate first |
| --- | ---: | ---: |
| Mean fields/s | 51.623 → 51.742 | 51.619 → 51.686 |
| Four matched slow windows, fields/s | 27.726 → 27.792 | 27.687 → 27.750 |
| Same 361 slow fields, fields/s | 39.477 → 39.080 | 39.013 → 39.570 |
| AudioWorklet underflow output frames | 358334 → 356720 | 359120 → 358743 |
| Highest 10-second p99, ms | 103.9 → 105.3 | 104.6 → 85.4 |

Both orders have zero dropped audio frames and zero runtime errors. Across the
same 3,101 fields, p95/p99 move from 41.7/62.5 to 42.1/62.8 ms in the first
order and from 42.7/64.0 to 42.1/63.5 ms in the reverse order. The normal Wasm
changes from `9e8293c7aaeb9d72c7bf3cebba1d164c62d91b080e920c12662a429c34745777`
to `c1f744eddeb35d9d6d058814245bff0f1ba6f5e2c8b9e18b86aa747802535cc1`.

The measured game still falls short of 60 fields/s and continues to have audio
underflows. The broader performance and hardware-completion Issues remain open.

## Accumulate CD clocks between events (2026-11-19)

Related to #138 and #17. When the drive is idle, a sequence of small board
advances can accumulate the CD elapsed clocks until the earliest drive phase,
report, standby transition or command completion. The countdown exposes the
same next event to the board and to the Sound projection. At the deadline,
the CD block settles the retained clocks through the existing event scheduler.
Starting a command or replacing media settles the previous interval first.
Playback, scanning, seeking, copying, authentication and R-W transfer activity
continue to advance through the existing scheduler on every call.

The native tests check simultaneous idle edges, a new command's full latency
after a retained interval and the remaining CD deadline seen by owned Sound.
An original mixed-mode CD guest produces the expected nonzero CD-DA samples
through owned Sound with NTSC and PAL and both PLL settings. Three mutations
fail the relevant assertions by crossing the event edge, starting a command
before settling the previous clocks or exposing an outdated Sound deadline.
The restored implementation passes all focused tests and both Wasm builds.

The diagnostic comparison uses the same cached-load recovery and bus grant
settings in both modes. The 28 continuous 80-field game windows improve from
51.501 to 52.869 fields/s (+2.66%). Every window improves, including 1.54% to
2.59% in the four slowest windows. The candidate combines about 24,780 CD
advance calls per field. CPU, cache, RAM, clocks, images, PCM and full Sound
state match at every paired endpoint and at 12 independently recorded endpoints.
The production Web player measurements and AudioWorklet results are recorded
in the associated pull request. Sustained real-time and the broader hardware
acceptance criteria remain open.

## Shorten physical WRAM access and borrow the cached entry state (2026-11-19)

Related to #138 and #17. Once the existing physical address decoder selects
aligned work RAM, the byte transfer can address the decoded bank directly.
The arbiter still observes the raw bus address and schedules its ordinary BSC
grant, wait states and completion. The cached SH-2 scheduler now borrows its
entry CPU and local state as the first cycle anchor. An anchor moved inside
the burst retains an owned snapshot for cycle folding and prefix recovery.

The native tests exercise SDRAM bank timing, cached and uncached aliases,
physical widths and device errors, and compare the two anchor modes during
countdown loops across both PLL settings. Reversing the WRAM bank selection
and offsetting the borrowed cycle period each fail their focused test.
Restoring the implementation passes all 60 related native tests and both
Wasm build configurations.

The same-build browser comparison enables both changes against the previously
accepted CPU and CD paths. The original workload improves from 148.112 to
148.717 fields/s. The game improves from 53.168 to 53.926 fields/s over 28
continuous 80-field windows (+1.43%). All 28 windows improve. CPU, cache, RAM,
clock, pixel and PCM results and the full Sound state match at each endpoint.
Each also matches the independently retained endpoint from the prior build.

Production player performance and AudioWorklet evidence are recorded in the
associated pull request. Sustained 60 fields/s, continuous audio delivery and
the broader hardware acceptance criteria remain open.

## Reduce VDP1 and SMPC deadline work (2026-11-19)

Related to #138 and #17. VDP1 calculates a newly needed row's command-deadline
bound from the same clipped dot positions without preparing texture or Gouraud
values for the estimate. An already retained partial row still supplies its
current dot position. The actual draw path prepares and consumes all pixel
inputs at the original phases.

SMPC accumulates elapsed clocks before its earliest controller or command
event. The existing clock update settles the interval at an event or before
a change that alters it. The board sees the remaining command and CPU reset
deadlines, and RTC state is settled before commands can observe it.

The restored source passes 52 VDP1, 40 SMPC and 22 Sound rendezvous tests.
Increasing the VDP1 bound by a clock or exposing an unadjusted SMPC reset
deadline fails the relevant native original. Both Wasm configurations pass.

The browser diagnostic improves from 53.334 to 53.791 fields/s over 2,240
game fields (+0.86%). Twenty-six of 28 paired windows improve. The four
slow intervals improve from 0.85% to 1.64%. CPU, cache, RAM, clocks, pixels,
PCM and full Sound state agree in all pairs and with the independently
retained reference. Production player and AudioWorklet results are recorded
in the associated pull request. Sustained 60 fields/s, continuous audio
delivery and the broader hardware acceptance criteria remain open.

## Reuse resident branch schedules during dual-CPU issue (2026-11-19)

Related to #138 and #17. A branch at an ordinary dual-CPU issue boundary can
use the resident instruction schedule already used inside cached bursts. The
proof uses the existing bounded BranchMemory cache reads, including the
retained halfword and ordered replacement touches. It queues the original
wait and then publishes the effects through the existing branch continuation.
A failed resident proof restarts the original branch planning path with its
initial private state.

Native validation compared the live prepared state, wait, retained halfword
and replacement edits with the original. It passed 58 cached-burst tests, 10
scheduling tests and the exhaustive branch-encoding test. An executed
one-clock-early mutation failed the dual-CPU interleave original. The restored
source passed that original and native and Wasm Clippy.

The real-game diagnostic improved from 53.940 to 54.175 fields/s over 2,240
paired game fields (+0.44%), with 24 of 28 faster windows. The four matched
slow ranges together improved by 0.10%. All corresponding CPU, cache, RAM,
clock, pixel, PCM and full Sound checks matched the paired and independent
reference. Six original browser pairs also matched. Production results are
recorded in the associated pull request. Sustained 60 fields/s, continuous
audio delivery and broader hardware acceptance remain open.

## Accumulate CD clocks during data-track playback (2026-11-19)

Related to #138 and #17. The CD scheduler can accumulate elapsed clocks
while the pickup plays data and the next sector, report, or command edge
is still ahead. It keeps the existing deadlines and settles elapsed
clocks before using the original edge handler. Seek, scan, copy,
authentication, and R-W activity retain their dedicated progression.
An exact sector transition can enter an audio track and start CD-DA
and R-W at the same edge as the reference.

Native tests cover data delivery, command and report ordering, and
the transition from a data track to an audio track with R-W enabled
and disabled. Deliberately allowing the deferred interval on an audio
track fails the transition test. The restored implementation passes it,
the 58 cached CPU cases, 49 CD cases, the CD-DA case, and the relevant
native and Wasm lint and build checks.

The browser comparison improved from 54.298 to 54.662 fields/s over
2,240 actual game fields (+0.67%). The four slow intervals together
improved from 29.256 to 29.864 fields/s (+2.08%). All six intervals
that used the path improved, with a combined gain of 1.78%. The other
22 intervals combined gained 0.11%. All 28 endpoint comparisons retain
the same CPU, cache, RAM, clocks, pixels, PCM, and complete Sound state
as the paired run and the independently retained reference. Six original
browser pairs also matched.

The associated pull request records the standard Web player comparison
and AudioWorklet results. Sustained 60 fields/s, continuous audio delivery,
and broader hardware acceptance remain open.

## Resume prepared SH-2 CD reads (2026-11-19)

Related to #138 and #17. The ordinary dual-SH-2 issue path can prepare a
MOV read from the cache-through CD window when the instruction and its
following fetch satisfy the existing readiness checks. The admitted
operands are the existing halfword interrupt, mask, and command response
registers, and the supported FIFO lanes. A longword FIFO read retains
both physical phases. The existing bus journal still performs arbitration,
device sampling, read effects, and retirement in their original order.

The native original compares both CPUs and the board at each boundary for
an interrupt register load, a halfword FIFO read, and a longword FIFO read.
It also checks the next FIFO value. Adding a clock to the prepared wait
fails that original. Restoring the implementation passes all 11 scheduling
cases, the CD A-bus timing case, 49 CD cases, native and Wasm
lint, and both Wasm builds.

The repeated two-window screen improved the heavy interval by 1.13%,
with all four pairs faster. A subsequent 2,240-field game comparison
improved from 53.381 to 53.714 fields/s (+0.62%). The four slow ranges
improved from 28.952 to 29.437 fields/s (+1.68%) and executed about
717 prepared loads per field. All 28 CPU, cache, RAM, clock, image, PCM,
and complete Sound endpoints match the paired and independently retained
reference. Six original browser pairs also matched.

The associated pull request records standard player and AudioWorklet
results. Sustained 60 fields/s, continuous audio delivery, and the
broader hardware acceptance criteria remain open.

## Keep A-bus state with passive CD reads (2026-11-19)

Related to #138 and #17. The staged WRAM and video bus sequence now contains
the SH-2 BSC state and its timing only. The passive CD interrupt-register
path continues to carry the SCU A-bus state through each physical beat and
commit it at the original boundary. The existing CPU write plans and held
read horizons still govern which transfers can be prepared.

The native cached-bus suite passes 58 cases. The existing CD
interrupt-register original rejects a build that omits the A-bus snapshot.
Restoring it passes the original, native lint, both Wasm lint
configurations, and standard and instrumented Web builds.

In a matched 2,240-field Chrome game diagnostic, throughput improves from
54.577 to 54.759 fields/s, with 18 of 28 intervals faster. The four slow
intervals together improve from 29.954 to 30.192 fields/s. Their individual
gains are 0.32%, 1.01%, 0.78%, and 1.07%. The original comparison is 148.613
to 148.588 fields/s. All six original pairs and 28 game pairs match the
independent CPU, cache, RAM, clock, pixel, PCM and complete Sound endpoints.
The three standard browser modes pass across seven original fixtures each.

The normal Chrome player drew every field and applied the same 151
controller
inputs in both fresh launch orders. It improved from 52.750 to 52.868
fields/s
forward and from 52.603 to 52.835 in reverse. AudioWorklet underflow sample
frames fell from 310,307 to 305,009 forward and 317,277 to 306,940 in
reverse.
There were zero PCM drops and browser errors. Over the same 3,161 field
positions, presentation throughput rose by 0.20% and 0.41%, respectively.
Matched field-gap p99 improved from 58.0 to 57.3 ms and from 59.8 to 57.2
ms.
Both versions recorded nine gaps over 100 ms in each order. The slowest
10 seconds in the reverse run changed from 42.99 to 42.89 fields/s. The pull
request records the matched slow ranges and the timing outliers.

Game real time, continuous sound delivery and broader hardware acceptance
remain open.

## Compile captured loop suffixes in one backward pass (2026-11-19)

Related to #138 and #17. Sort the captured reads and writes once and walk
backward through their suffixes. Track which entry bytes are still needed
and the first owner of each replacement touch. Build each guard from the
original cache image for that phase, including the tag at its physical way.
Stores and instruction reads still use their observed order. The existing
literal algorithm handles observations with stores spanning a cache line.

The native suite compares the new guard vectors against the literal
algorithm. A separate original checks byte overlaps and same-clock
read-before-write ordering using distinct store request and acceptance
clocks. Changing the compiler to sort a store by acceptance time fails that
case. The restored source passes the loop and cached-bus tests, formatting,
and native and both Wasm Clippy configurations.

The matched Chrome game diagnostic on Machi Disc 1 improves from 53.341 to
54.347 fields/s over 2,240 drawn fields. It is faster in 25 of 28 intervals.
The four slow intervals improve from 28.978 to 29.366 fields/s. All 28 CPU,
cache, RAM, clock, pixel, PCM and complete Sound endpoints match. The six
original browser diagnostics also match and run at 148.093 and 147.765
fields/s respectively. Standard Wasm matches seven original fixtures in each
of the normal worker, direct, and forced Sound-return failure modes.

The production player draws every field and applies the same 151 controller
inputs across both fresh launch orders. Each run measures six ten-second
windows after a 12,900-field warmup with the real AudioWorklet active.

| Production player | Main → candidate fields/s | Audio underflow sample frames | Matched field-gap p99 |
| --- | ---: | ---: | ---: |
| Forward order | 52.084 → 52.266 | 340,445 → 333,149 | 60.6 → 62.8 ms |
| Reverse order | 52.669 → 52.803 | 316,815 → 307,825 | 59.0 → 59.1 ms |

Both orders have zero audio queue drops and browser errors. Matched gap
statistics cover the same 3,128 field positions. The four slow ranges gain
0.11% and 2.06% in the two orders. The earlier 361-field range regresses by
0.91% in the first order and improves by 1.17% in reverse. The normal player
shows a small aggregate gain and a lower audio deficit in both orders.
Sustained real-time gameplay, continuous audio delivery, and broader
hardware acceptance remain open.


## Share unchanged cache lines across captured loop phases (2026-11-19)

Related to #138 and #17. During Master SH-2 loop capture, consecutive phases
share immutable cache tag and data bytes while the contents version is stable.
Each phase records its own control and replacement state. Refills, cached
writes, array writes and purges, cache control changes, and shadow publication
invalidate the shared contents. Capture reads the initial cache image when it
reaches the requested start and records images for the ensuing phases.

Native checks compare shared images against a full serialization. The focused
test exercises writes, refills, array data, shadow publication and cache
control, with replacement-only changes retaining the shared allocation.
Removing write-hit invalidation fails that test. The restored source passes
the cache, loop and cached-burst suites, formatting and native/Wasm Clippy.

The 2,240-field Chrome diagnostic on Machi Disc 1 improves from 54.381 to
54.662 fields/s, with 23 of 28 intervals faster. The four slow intervals
improve from 29.775 to 30.050 fields/s. All 28 CPU, cache, RAM, clock, image,
PCM and complete Sound endpoints match. Six original diagnostic fixtures
match, with 148.173 and 148.307 fields/s respectively. The standard Wasm build
matches seven original fixtures in each of the normal, direct, and forced
Sound-return failure modes.

The production player draws every field and applies the same 151 controller
inputs. It runs six ten-second windows after a 12,900-field warmup with the
real AudioWorklet. The reverse comparison was rerun after a local build
overlapped the first attempt. Only the isolated comparisons are shown.

| Launch order | Main → candidate fields/s | Audio underflow sample frames | Matched gap p95 / p99 |
| --- | ---: | ---: | ---: |
| Forward | 52.704 → 52.854 | 313,053 → 306,002 | 39.6 / 58.8 → 37.9 / 58.7 ms |
| Reverse | 52.584 → 52.853 | 318,379 → 305,905 | 39.8 / 59.1 → 38.0 / 59.7 ms |

Both comparisons have zero audio queue drops and browser errors. Across the
same 3,159 field positions, each run has nine gaps above 100 ms. The four
selected slow ranges improve by 2.04% and 1.72%. For the earlier 361-field
range, forward p99 increases from 67.6 to 100.9 ms, with one additional gap
above 100 ms and a lower maximum gap. Reverse p99 in that range decreases
from 115.0 to 100.7 ms. Sustained real-time throughput, continuous audio and
broader hardware acceptance remain open.


## Compare four SH-2 cache tags together (2026-11-19)

Place the four physical ways' tags and valid bits in one array. The Wasm
build compares the array with one SIMD128 vector. Native and scalar Wasm
use the same tag encoding and ordered mask operations. Reads still use the
lowest matching physical way. Writes still touch every matching way in the
original order. Two-way mode, the address/data arrays, staged cache versions,
LRU, and the captured loop image format retain their guest behavior.

The browser comparison uses Chrome 157.0.8103.79, Rust 1.100.0,
wasm-bindgen 0.2.129, and Binaryen 127. Each package boots Machi Disc 1 in a
fresh player through field 12,900, then measures six continuous ten-second
windows with the ordinary paced playback, Canvas, and active AudioWorklet.
Both orders apply the same 151 controller events through their common endpoint.
Every field is drawn. Audio queue drops and runtime errors are zero.

| Measure | Main to candidate, forward | Main to candidate, reverse |
| --- | ---: | ---: |
| Mean fields/s | 52.686 to 52.836 | 52.768 to 52.936 |
| Matched fields 12,901–16,064, fields/s | 52.668 to 52.808 | 52.776 to 52.910 |
| Four matched slow ranges, fields/s | 29.668 to 29.973 | 29.782 to 29.994 |
| Matched p95 field time, ms | 37.7 to 37.0 | 37.8 to 37.1 |
| Matched p99 field time, ms | 58.7 to 57.5 | 59.4 to 57.8 |
| Worklet underflow sample frames | 313,891 to 307,425 | 307,185 to 302,204 |

The four slow ranges begin at fields 13,661, 13,821, 14,861, and 15,341.
Each includes 80 fields. Two individual slow ranges lose throughput in the
reverse order. Their pooled throughput improves in both orders. The diagnostic
companion matches all 28 current reference endpoints, including private CPU
and cache state, WRAM, clocks, pixels, PCM, and complete Sound state. Its four
slow ranges are effectively flat in aggregate. The original production
browser contracts pass in normal, direct, and forced Sound return modes.

The baseline WebAssembly SHA-256 is
`dc7d39781f2bf5d08187c61294414b286da8de3f3490d97ec381a69b0271742c`.
The candidate is
`f8f7bc0c1fc70176925a21f43d84a5201be3c7c7f6177719ce446a8996a72c76`.
Building through `scripts/build-web.sh` reproduces the measured Wasm and
JavaScript binding. Native cache, cached scheduler and loop tests pass.
Removing the two-way mask fails the native oracle. Restoring it passes the
same test. Native and Wasm Clippy pass with warnings denied.

Related to #138 and #17. Sustained 60 fields/s through the measured game,
continuous production audio, and hardware completion remain open.
Private firmware, media, controller timelines and raw captures stay local.


## Reuse validated private SH-2 code until the cache changes (2026-11-19)

A private Slave polling template validates the exact instruction bytes and
physical cache ways required for replay. Retain a successful comparison for
the current cache revision. Cache-content, tag, and control mutations clear
the revision through the existing mutation sites. LRU-only updates preserve it.
Distinct cache instances receive distinct revisions. The complete CPU,
transaction, interrupt, timer, and publication checks still run.

Chrome 157.0.8103.79 runs the normal paced player on Machi Disc 1 with Canvas,
Sound and Video Workers, and an active AudioWorklet. Each fresh instance warms
to field 12,900 and runs six continuous ten-second windows. Both orders apply
the same 151 controller events across their common fields, and each field is
drawn. Queue drops and runtime errors are zero.

| Measure | Main to candidate, forward | Main to candidate, reverse |
| --- | ---: | ---: |
| Mean fields/s | 52.718 to 53.034 | 52.867 to 52.884 |
| Matched fields 12,901–16,067, fields/s | 52.718 to 52.971 | 52.820 to 52.855 |
| Four matched slow ranges, fields/s | 29.774 to 30.381 | 30.047 to 30.093 |
| Matched p95 field time, ms | 36.8 to 36.7 | 37.2 to 36.8 |
| Matched p99 field time, ms | 57.8 to 57.2 | 56.7 to 56.7 |
| Worklet underflow sample frames | 311,901 to 299,644 | 306,940 to 305,020 |

The four slow ranges begin at fields 13,661, 13,821, 14,861, and 15,341 and
each includes 80 fields. Two of those ranges lose throughput in the reverse
order. Pooled slow-range p99 changes 119.7 to 118.5 ms and 117.8 to 119.2 ms.
Each order retains eight field gaps above 100 ms across the slow ranges.
The differential companion measures 55.560 to 55.727 fields/s over 28 windows
and matches the independent CPU, cache, WRAM, clock, pixel, PCM, and complete
Sound endpoints in every window. Original production browser cases match in
normal, direct, and forced Sound return modes.

The baseline Wasm SHA-256 is
`f8f7bc0c1fc70176925a21f43d84a5201be3c7c7f6177719ce446a8996a72c76`.
The candidate is
`0ea017dfc9918976a65b5c1330f9011d7f6e520f18798d2cf5e98a5476a39c09`.
The standard Web build reproduces the measured Wasm and JavaScript binding.
Native cache, private-poll, cached scheduler, and loop tests pass. Omitting
revision invalidation fails the original code-mutation test. Restoring it
passes that test. Native and Wasm Clippy pass with warnings denied.

Related to #138 and #17. Sustained 60 fields/s through the measured game,
continuous production audio, and hardware completion remain open.
Private media and raw diagnostic inputs stay local.



## Keep Sound ownership across CD buffer commands (2026-11-20)

CD buffer queries, sector reads into DATATRNS, and sector deletion can
complete while a Sound forecast owns the audio domain. Keep the original
A-bus grant, command deadline, FIFO and selector state, response latch,
and interrupt delivery on the main board. Deleting a sector can free
space for playback, which still resumes at the next existing drive edge.
Commands that can change CD-DA and the strict dependency controls retain
their existing Sound boundaries.

The diagnostic comparison covers 28 consecutive 80-field windows on
Machi Disc 1. Full CPU, cache, WRAM, clock, image, PCM, and Sound endpoints
match in every window. Throughput improves from 55.740 to 56.402 fields/s,
with 22 of 28 windows faster. The four slow windows improve from 30.569
to 31.608 fields/s. Sound jobs fall from 6,362 to 5,816 overall and from
1,912 to 1,434 in the slow windows. A total of 844 newly admitted commands
complete while Sound is owned, including 741 in the slow windows.

The standard Chrome 157.0.8103.79 player warms each fresh instance to
field 12,900 and then runs six ten-second windows with ordinary pacing,
Canvas presentation, both Workers, and an active AudioWorklet. Both orders
apply the same 151 controller events through the common endpoint. Every
field is drawn. Audio queue drops and browser errors are zero.

| Measure | Main to candidate, forward | Main to candidate, reverse |
| --- | ---: | ---: |
| Six-window mean, fields/s | 52.985 to 53.417 | 52.858 to 53.256 |
| Matched fields 12,901–16,075, fields/s | 52.989 to 53.364 | 52.856 to 53.229 |
| Four matched slow ranges, fields/s | 30.240 to 31.036 | 29.993 to 30.766 |
| Matched p95 field time, ms | 36.6 to 36.7 | 36.9 to 37.3 |
| Matched p99 field time, ms | 56.1 to 55.7 | 57.7 to 55.7 |
| Slow ranges p95 / p99 field time, ms | 62.2 / 118.5 to 59.9 / 117.9 | 62.8 / 118.2 to 60.7 / 117.4 |
| Worklet underflow sample frames | 300,354 to 281,693 | 306,684 to 288,597 |

The slow ranges start at fields 13,661, 13,821, 14,861, and 15,341. Their
pooled throughput improves by 2.63% and 2.58%. The final slow range loses
0.10% in the reverse order. Each run has nine matched gaps above 100 ms.

The baseline Wasm SHA-256 is
`0ea017dfc9918976a65b5c1330f9011d7f6e520f18798d2cf5e98a5476a39c09`.
The candidate is
`c78e701ba2d74701ffd29340fe135fe21167ea18765e83dca4dbfffccd4ed4c0`.
The standard build uses Rust 1.100.0, wasm-bindgen 0.2.129, and Binaryen 127.
The JavaScript binding matches the baseline byte for byte. All seven
original guests match in each standard browser mode, including forced
Sound return. The native test submits each changed command through a real
SH-2 A-bus write in both PLL modes and compares the complete Sound state.
A separate native test verifies the earliest CD-DA resume after deletion.
Removing command 0x52 from the allowed set fails the focused test, and
restoring it passes. Native and Wasm Clippy pass with warnings denied.

Related to #138 and #17. Sustained real-time throughput, continuous
production audio, and hardware completion remain open. Private media
and raw captures stay local.


## Run resident SH-2 work across quiet buffered CD writes (2026-11-20)

A granted final 16-bit Master write to CD HIRQ or a command parameter
can complete while the SH-2 retires instructions using only resident
cache data. An HIRQ acknowledgment must preserve the CD interrupt level.
Command parameters require an enabled CD block with no outstanding
packet. The ordinary horizon still bounds physical CPU stores, cache
fills and passive reads. The physical write, A-bus timing and IRQ
sampling use their existing paths.

In 28 consecutive 80-field diagnostic windows on Machi Disc 1, all
compared CPU, cache, WRAM, clock, image, PCM and complete Sound endpoints
match. Overall throughput is 56.128 to 56.212 fields/s and the four slow
windows are 31.522 to 31.781 fields/s. The original diagnostic workload
is 150.357 to 148.970 fields/s. A separate same-instance test switches the
optimization in both orders. All four repeats improve in the selected
slow window, from 23.028 to 23.104 fields/s pooled, with 9,970 real
buffered completions crossed per 80 fields. The adjacent window and
original workload are effectively flat in that same-instance test,
with no such crossings. The diagnostic and normal player timings measure
different paths.

The standard Chrome 157.0.8103.79 player warms each fresh instance to
field 12,900 and measures six ten-second windows in both execution orders.
Canvas presentation, both Workers and an active AudioWorklet run normally.
Both orders replay the same 153 controller events through field 16,113.
The player draws every field with zero audio queue drops or browser errors.

| Measure | Main to candidate, forward | Main to candidate, reverse |
| --- | ---: | ---: |
| Matched 3,213 fields, fields/s | 53.621 to 53.707 | 53.477 to 53.818 |
| Four matched slow ranges, fields/s | 31.289 to 31.543 | 31.453 to 31.806 |
| Matched p95 / p99 field time, ms | 36.3 / 54.6 to 36.5 / 54.1 | 36.2 / 53.7 to 35.9 / 53.3 |
| Slow ranges p95 / p99, ms | 59.4 / 117.3 to 59.6 / 117.4 | 58.6 / 117.7 to 58.1 / 116.9 |
| Worklet underflow sample frames | 275,956 to 271,701 | 278,594 to 266,484 |

The slow ranges start at fields 13,661, 13,821, 14,861 and 15,341.
Each improves in both orders. Nine matched gaps exceed 100 ms per run,
including eight in these ranges.

The baseline Wasm SHA-256 is
`c78e701ba2d74701ffd29340fe135fe21167ea18765e83dca4dbfffccd4ed4c0`.
The candidate is
`737d791bf8e7ad7d5f5357fbb314b7e14199de4ae25d32ce9366ebe117a672c5`.
The standard build uses Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127.
The JavaScript binding is byte-identical. The seven original guests match
in all three standard browser modes, including forced Sound return.
The native buffered-write fixture crosses an actual A-bus completion in
both PLL modes and checks complete machine state against ordinary steps.
A separate guard test covers IRQ and pending command cases. Disabling the
new path and permitting unsafe HIRQ writes each fail the intended
original. The restored source passes the focused native tests and native
and Wasm Clippy with warnings denied.

Related to #138 and #17. Sustained real-time throughput, continuous audio
and hardware completion remain open. Private media and captures stay local.


## Reuse private-poll keys and one-clock Worker waits (2026-11-20)

After a miss on the most recent Slave polling proof, a compact key
compares PC, cache control, FTCSR and the recorded peripheral observation.
The lookup preserves template order and runs the full CPU, cache, clock
and journal validation on each matching key. When Sound rendezvous
already limits an advance to one main tick, the board omits the wider
deadline calculation and advances through its existing clock path.

The 28 consecutive 80-field diagnostic game windows on Machi Disc 1
match CPU, cache, WRAM, clocks, pixels, PCM and full Sound state. Combined
throughput improves from 55.552 to 55.967 fields/s, with 24 of 28 windows
faster. The four slow windows improve from 31.208 to 31.623 fields/s.
The original diagnostic workload improves from 149.819 to 150.458 fields/s.
The slow game windows average 155,667 skipped key mismatches and 30,260
avoided wider deadline checks per field. Separate switch tests in the same
Wasm reproduce a 0.39% improvement for lookup and 1.29% for the one-tick
wait in the selected 80-field slow window. The adjacent window changes
by +0.02% and +0.68%, respectively.

The standard Chrome 157.0.8103.79 player warms each fresh instance to
field 12,900, then runs six ten-second windows with normal pacing,
Canvas presentation, both Workers and an active AudioWorklet. Both
orders replay the same 153 controller events through field 16,096.
Every completed field is drawn. Audio queue drops and browser errors
are zero.

| Measure | Main to candidate, forward | Main to candidate, reverse |
| --- | ---: | ---: |
| Matched 3,196 fields, fields/s | 53.201 to 53.663 | 53.599 to 53.683 |
| Four matched slow ranges, fields/s | 30.669 to 31.528 | 31.518 to 31.643 |
| Matched p95 / p99 field time, ms | 37.4 / 56.7 to 36.5 / 54.3 | 36.2 / 54.3 to 36.5 / 54.2 |
| Slow ranges p95 / p99 field time, ms | 61.3 / 118.7 to 58.7 / 112.4 | 58.2 / 117.8 to 58.8 / 112.0 |
| Worklet underflow sample frames | 291,029 to 273,109 | 276,340 to 272,116 |

Seven of the eight slow comparisons improve. The range starting at
field 13,661 loses 0.55% in the reverse order, as does the separately
reported 361-field range by 0.31%. Nine matched gaps exceed 100 ms in
each run, including eight in the four slow ranges.

Baseline Wasm SHA-256 is
`737d791bf8e7ad7d5f5357fbb314b7e14199de4ae25d32ce9366ebe117a672c5`.
Candidate Wasm is
`63d484e87c607452e102fd26e4959c86c404a9b832c741991051e16a212edefd`.
The standard build uses Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127,
with byte-identical JavaScript bindings. Seven original guests match in all
three standard browser modes, including forced Sound return. The native
tests verify a matching nonrecent key still rejects changed cache bytes
and compare real one-tick waits in both PLLs and video regions. A mutation
bypassing the full proof and a mutation disabling the one-tick path fail
their intended fixtures. The restored native tests and native/Wasm Clippy
pass with warnings denied.

Related to #138 and #17. Sustained real-time execution, continuous
audio and hardware completion remain open. Private media and raw
captures stay local.


## Batch passive CD reads across memory refresh (2026-11-21)

A cached Master SH-2 prefix can include passive CD interrupt-status reads
across up to two refresh requests with the refresh interrupt disabled.
The board grants each refresh through the existing arbiter. An accepted
CD read can finish before the refresh obtains the bus, and a pending CD
read can first acquire it at the refresh completion. Each planned window
uses the existing device and peer deadlines and retains the next
unplanned refresh compare as a fence. The Sound lookahead can select an
earlier retirement and preserves the canonical Sound state at that boundary.

The standard Chrome 157.0.8103.79 player warms each fresh instance through
field 12,900 and measures six twelve-second windows on Machi Disc 1.
All four runs draw every completed field and replay the same 157 timed
controller events through the common endpoint. Both Workers and the
AudioWorklet remain active. Browser errors and audio queue drops are zero.

| Measure | Main to candidate, forward | Main to candidate, reverse |
| --- | ---: | ---: |
| Matched 3,852 fields, fields/s | 53.545 to 53.665 | 53.451 to 53.637 |
| Four matched slow ranges, fields/s | 31.686 to 32.368 | 31.562 to 32.274 |
| Fields 16,231–16,270, fields/s | 21.720 to 22.681 | 21.639 to 22.618 |
| Matched p99 field time, ms | 55.3 to 52.3 | 55.1 to 52.6 |
| Slow ranges p99 field time, ms | 112.5 to 110.9 | 112.4 to 111.4 |
| Worklet underflow sample frames | 333,721 to 323,902 | 338,713 to 325,930 |

Fields 15,341–15,420 average 1.16% and 1.10% less throughput in the
forward and reverse orders. The sustained real-time and continuous audio
targets remain open.

Baseline Wasm SHA-256 is
`a9b4de0d47b957714f319cee7231fd45176cf6aa98f7080ea4f3a442bc8463e0`.
Candidate Wasm is
`c826a2990569e7131950b32e0b1bcad52ca5f98a895f45d5c5edf76c96bd0481`.
The standard build uses Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127
with identical JavaScript bindings.

The native fixture compares real physical grants and CPU retirements
against the ordinary path in both PLLs. It includes refresh interrupts,
queued reads, overlapping reads and concurrent local Sound. Omitting the
second grant and advancing Sound twice at the second grant fail the
intended checks. Restored fixtures, native/Wasm Clippy, and Chrome
A-bus, SH-2 instruction fetch and CD runtime tests pass.

Across 3,040 private replay fields, the two standard packages match at
38 checkpoints through field 16,860. The comparison covers public SH-2
state and clocks, all work RAM by SHA-256, and the full canonical Sound
snapshot byte for byte. Every field contributes to the PCM and pixel
digests.

Related to #138 and #17. Private media and raw captures stay local.

## Compact cached SH-2 checkpoints (2026-11-21)

Cached Master SH-2 execution keeps its passive BSC and A-bus snapshots
in three reusable slots for the batch entry, a loop anchor and one
speculative attempt. Per-instruction rollback and loop replay use the
attempt slot at separate times. Periodic loop checks compare the live
protocol with the saved anchor directly. Publication keeps the final
protocol together with the committed cache and instruction state.

The standard Chrome 157.0.8103.79 player warms each fresh instance through
field 12,900 and measures six twelve-second windows on Machi Disc 1.
All four runs draw every completed field and replay the same 157 timed
controller events through the common endpoint. The Sound and Video
Workers and AudioWorklet remain active. Browser errors and audio queue
drops are zero.

| Measure | Main to candidate, forward | Main to candidate, reverse |
| --- | ---: | ---: |
| Matched 3,865 fields, fields/s | 53.632 to 53.752 | 53.673 to 53.752 |
| Four matched slow ranges, fields/s | 32.205 to 32.582 | 32.215 to 32.429 |
| Fields 16,231–16,270, fields/s | 22.594 to 22.633 | 22.873 to 22.642 |
| Matched p99 field time, ms | 52.9 to 52.3 | 52.1 to 52.4 |
| Slow ranges p99 field time, ms | 112.4 to 111.5 | 111.6 to 111.9 |
| Worklet underflow sample frames | 327,100 to 320,386 | 325,134 to 322,841 |

Fields 14,861–14,940 improve by 2.26% and 2.02% in mean throughput,
with p99 field time increasing from 129.2 to 133.8 ms and from 130.3
to 133.9 ms. The longest gap moves one field earlier, and the six fields
around it take 297.7 to 264.6 ms and 299.9 to 262.8 ms. The former fault
range runs about 1% slower in the reverse order. Its p99 field time
increases in both orders. Sustained real-time execution and continuous
audio remain open.

Baseline Wasm SHA-256 is
`c826a2990569e7131950b32e0b1bcad52ca5f98a895f45d5c5edf76c96bd0481`.
Candidate Wasm is
`46783d789b0d6bde2342e9fb9980a1cf2eb9a255a8ff2d4d3e7a7b06813350bc`.
The standard build uses Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127
with identical JavaScript bindings.

The original native fixture saves three different states after physical
passive Work RAM reads and restores them independently. Aliasing the
entry and attempt checkpoints fails the fixture, and the restored source
passes. The existing refresh, cached execution, video journal and Sound
rendezvous fixtures, native and Wasm Clippy, and Chrome A-bus, SH-2 fetch
and CD runtime checks pass.

Across 3,040 private replay fields, the standard Wasm packages match at
38 checkpoints through field 16,860. The comparison covers public SH-2
state and clocks, all work RAM by SHA-256, and the canonical Sound state
byte for byte. Every field contributes to the PCM and pixel digests.

Related to #138 and #17. Private media and raw captures stay local.

## Reuse private SH-2 transaction storage (2026-11-21)

After a verified Master SH-2 burst, the existing Slave polling proof
publishes its translated private transaction using the journal buffers
saved at entry. The translation preserves the pending transfer, replay
values and positions, and absolute guest clocks. If the burst fails,
the original transaction is restored with its contents intact.

Each fresh standard Chrome 157.0.8103.79 run warms through field 12,900
and measures six twelve-second windows on Machi Disc 1. All four runs
use the same 157 controller events, draw every measured field, and keep
the Sound and Video Workers and AudioWorklet active. Browser errors and
audio queue drops are zero.

| Measure | Main to candidate, forward | Main to candidate, reverse |
| --- | ---: | ---: |
| Matched 3,869 fields, fields/s | 53.703 to 53.882 | 53.680 to 53.728 |
| Four matched slow ranges, fields/s | 32.010 to 32.601 | 32.267 to 32.414 |
| Fields 16,231–16,270, fields/s | 22.458 to 22.860 | 22.856 to 22.928 |
| Matched p99 field time, ms | 53.5 to 52.3 | 52.4 to 52.4 |
| Slow ranges p99 field time, ms | 112.0 to 111.0 | 112.3 to 111.4 |
| Worklet underflow sample frames | 326,138 to 317,721 | 324,522 to 322,149 |

In the reverse order, two of the four slower ranges lose under 0.1%
of mean throughput while both of their p99 field times improve. Fields
14,861–14,940 show a higher p99 in the forward order, from 130.4 to
133.2 ms. Sustained real-time execution and continuous audio remain open.

Baseline Wasm SHA-256 is
`46783d789b0d6bde2342e9fb9980a1cf2eb9a255a8ff2d4d3e7a7b06813350bc`.
Candidate Wasm is
`9ebd8377595dd0016eea93b0ae48d1ef53afe79b65f598d137c044e8b10c985f`.
The standard build uses Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127
with identical JavaScript bindings.

The native transaction fixture compares the complete rebased journal
with an independent copy and checks that the original buffers are reused.
Replacing reuse with fresh allocations fails that fixture. The restored
source passes the polling, cached scheduling, video journal and Sound
rendezvous tests, native and Wasm Clippy, and the Chrome A-bus, SH-2 fetch
and CD runtime checks.

The two standard Wasm packages match throughout a separate 3,040-field
replay across 38 checkpoints through field 16,860. The comparison covers
public SH-2 state and clocks, all work RAM by SHA-256, and the canonical
Sound state byte for byte. Every field contributes to PCM and pixel
digests. The replay passes through the former VDP1 error scene.

Related to #138 and #17. Private media and raw captures stay local.

## Select SH-2 DMA requests from the two physical channels (2026-11-21)

The on-chip DMA selector checks its two physical channels directly in
hardware priority order. The CPU contention check asks whether either
channel requests the peripheral bus. A previously accepted DMA unit
retains priority and continues through the existing grant and data phases.

Each fresh standard Chrome 157.0.8103.79 run warms through field 12,900
and measures six twelve-second windows on Machi Disc 1. All six runs
use the same 157 controller events, draw every measured field, and keep
Sound and Video Workers and AudioWorklet active. Browser errors and
queue drops are zero.

| Measure | First forward order | Reverse order | Confirming forward order |
| --- | ---: | ---: | ---: |
| Common fields, fields/s | 53.767 to 53.709 | 53.752 to 54.194 | 53.764 to 53.941 |
| Four matched slow ranges, fields/s | 32.541 to 32.400 | 32.436 to 33.246 | 32.621 to 32.801 |
| Fields 16,231–16,270, fields/s | 22.338 to 22.907 | 23.024 to 23.415 | 22.747 to 22.902 |
| Four slow ranges p99 field time, ms | 111.5 to 108.1 | 111.7 to 106.8 | 111.3 to 107.7 |
| Worklet underflow sample frames | 319,599 to 323,132 | 318,031 to 299,227 | 317,233 to 314,362 |

The first two orders share 3,871 matched fields across all four runs.
The confirming pair shares 3,875 fields. Overall throughput and audio
shortages improve in two pairs. Mean throughput in fields 14,861–14,940
falls by 1.49% and 0.25% in the two forward pairs, with lower p99 field
times in both. The former error scene improves in all three pairs.
Sustained real-time execution and continuous audio remain open.

Baseline Wasm SHA-256 is
`9ebd8377595dd0016eea93b0ae48d1ef53afe79b65f598d137c044e8b10c985f`.
Candidate Wasm is
`0d774eebaeecdcfccea986fd8d8571fa8a31b30289961e9001a1d28555774e2a`.
The standard build uses Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127
with identical JavaScript bindings.

The native regression compares the selected channel and both internal
bus queries across 110,592 combinations. Reversing DMA priority or
losing an accepted request fails the regression. The restored source
passes the DMA, monitor, buffered write, cached execution, video journal
and Sound rendezvous tests, native and Wasm Clippy, and the Chrome DMAC,
SH-2 pipeline, SH-2 reset and DMA startup checks.

The standard packages match over a separate 3,040-field replay at 38
checkpoints through field 16,860, including the former VDP1 error scene.
Public CPU state and clocks, all work RAM by SHA-256, and canonical
Sound state byte for byte agree at every checkpoint. Every field
contributes to the matching PCM and pixel digests.

Related to #138 and #17. Private media and raw captures stay local.

## Bound the Sound retirement-gap check (2026-11-21)

Cached SH-2 execution uses the gaps between actual peer-admissible
retirements to decide whether Sound lookahead needs a rollback checkpoint.
The scheduler converts the existing eight-reference-clock allowance to
its exact phase-independent main-clock limit. The scan stops at the first
gap beyond that limit. Borrowed and materialized retirement positions use
their existing order. Repeated periods retain the same logical timeline.

Fresh Chrome 157.0.8103.79 runs warm through field 12,900, then play six
twelve-second intervals on Machi Disc 1. The four runs share 3,880 matched
fields and the same 157 controller events. Every field is drawn with
Sound and Video Workers and AudioWorklet active. Browser errors and
queue drops are zero.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Common fields, fields/s | 53.952 to 54.030 | 53.837 to 53.890 |
| Four matched slow ranges, fields/s | 32.775 to 33.049 | 32.715 to 32.621 |
| Fields 16,231–16,270, fields/s | 23.298 to 23.215 | 22.562 to 22.990 |
| Common fields p99 field time, ms | 51.3 to 51.2 | 52.6 to 52.1 |
| Four slow ranges p99 field time, ms | 108.1 to 107.2 | 107.6 to 107.6 |
| Worklet underflow sample frames | 313,210 to 309,145 | 318,427 to 314,673 |

The candidate improves overall throughput and reduces shortages in both
orders. In fields 14,861–14,940 the forward p99 rises from 124.5 to
129.0 ms, and reverse throughput falls by 1.76%. Throughput around the
former VDP1 error also varies by order. Sustained real-time execution and
continuous audio remain open.

Baseline Wasm SHA-256 is
`0d774eebaeecdcfccea986fd8d8571fa8a31b30289961e9001a1d28555774e2a`.
Candidate Wasm is
`4ecdebbe64a5c25e1985c4894dce26b9298a471902cc99c958d0b046db5bbc50`.
The standard build uses Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127
with identical JavaScript bindings.

The native regression compares the bounded check with the literal scan
at each prefix for materialized, repeated and borrowed retirements, and
checks the allowance for all four PLL modes. Making the gap comparison
exclusive fails the exact-boundary regression. The restored source
passes original dual-polling Sound control, HIRQ and physical refresh,
and Sound arbitration fixtures, cached scheduling, video journal and
Sound rendezvous tests, and native and Wasm Clippy. Original Chrome
A-bus timing, refresh arbitration, SCSP clock and SCSP CPU-memory
scenarios pass on the standard candidate.

The standard packages match over a separate 3,040-field replay at 38
checkpoints through field 16,860, including the former VDP1 error scene.
Public SH-2 state and clocks, all work RAM by SHA-256, and canonical
Sound state byte for byte agree at every checkpoint. All fields
contribute to the matching rolling PCM and pixel digests.

Related to #138 and #17. Private media and raw captures stay local.

## Execute resident instructions around passive HIRQ reads (2026-11-21)

An admitted CD HIRQ window can retire register operations, resident branches
and 16-bit HIRQ loads using the existing SH-2 instruction plans. Each load
uses the current operand, sign extension and one shadow plan for the real
BSC and A-bus read. An eight-entry table reuses instruction words inside
this read-only window while applying the original retained-fetch and cache
replacement effects. The scheduler still selects the retirement prefix and
publishes physical refresh, Sound and Slave events at the existing boundaries.

Fresh Chrome 157.0.8103.79 runs warm through field 12,900, then play six
twelve-second intervals on Machi Disc 1. The four runs share 3,886 matched
fields with 157 controller events at the same fields. Every field is drawn
with Sound and Video Workers and AudioWorklet active. Browser errors and
audio queue drops are zero.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Common fields, fields/s | 53.964 to 53.971 | 53.924 to 54.071 |
| Four matched slow ranges, fields/s | 32.816 to 32.738 | 32.732 to 33.055 |
| Fields 16,231–16,270, fields/s | 22.886 to 23.229 | 22.906 to 23.359 |
| Common fields p99 field time, ms | 51.9 to 51.4 | 52.1 to 50.6 |
| Worklet underflow sample frames | 311,504 to 310,992 | 316,154 to 304,879 |

The former VDP1 error interval gains 1.50% and 1.98% across the two orders.
Its p99 field time changes from 78.4 to 78.7 ms and from 79.5 to 78.1 ms.
Forward throughput falls in three of the four matched slow ranges, with
the largest loss of 0.81% in fields 14,861–14,940. Reverse throughput rises
in all four. Sustained real-time execution and continuous audio remain open.

Baseline Wasm SHA-256 is
`4ecdebbe64a5c25e1985c4894dce26b9298a471902cc99c958d0b046db5bbc50`.
Candidate Wasm is
`eabdb4ae3c77a54e458891ffc87ff40cc8a1528d20a67c7a6b9fd40af2ca257f`.
The standard build uses Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127
with identical JavaScript bindings.

The native regressions compare against literal execution with both NTSC PLL
settings, physical refresh and an armed refresh IRQ, an active Sound CPU
and a pending private Slave transaction. Adding one clock to a read or
swapping the two bytes of the retained instruction halfword fails the
CPU-retirement oracle. The restored source passes cached scheduling, video
journal, Sound rendezvous and native and Wasm Clippy checks. Original
Chrome A-bus timing, refresh arbitration, SH-2 fetch and CD runtime
scenarios pass on the standard candidate.

The standard packages agree over a separate 3,040-field replay at 38
checkpoints through field 16,860. Each checkpoint matches public SH-2 state
and clocks, all work RAM by SHA-256 and full canonical Sound bytes. Every
field contributes to matching rolling PCM and pixel digests.

Related to #138 and #17. Private media and raw captures stay local.

## Skip empty local arbitration during a granted SH-2 DMA beat (2026-11-22)

A granted SH-2 DMA read in Work RAM or buffered video write holds the
external bus. The arbiter can still accept an independent peripheral read
or buffered-write request on either SH-2. The early return checks each
local port. Its slot must already be occupied, blocked by the active DMA
owner, or free of peripheral work in the journals and request selector.
The ordinary allocator handles a queued CPU or DMA contender. Sound
request synchronization precedes the test.

A diagnostic run of Machi Disc 1 found 51,024–68,864 Master SH-2 DMA reads
from Work RAM and comparable VDP2 video writes in each of three long
fields. The Slave SH-2 read its free-running timer about 60,000 times per
field. Those fields had no CD data events. Across the full diagnostic
window, about 70% of missing AudioWorklet output samples occurred near a
CD data event and about 21% near a large VDP2 DMA transfer. The counts use
adjacent PCM producer fields and AudioWorklet report intervals.

In the independent same-instance comparison the allocator guard improves
both 80-field windows in all four pairs. The slower matched field
improves from 144.1 to 135.5 ms in that diagnostic. The production
measurements below use separate, fresh Chrome 157.0.8103.79 runs, the
normal player, Sound and Video Workers, and AudioWorklet. After warmup
through field 12,900, each run measures six twelve-second intervals.
All four runs share 3,882 compared fields and present all 157 scripted
inputs at their requested fields. Every field is drawn. Runtime errors
and audio queue drops are zero.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Common fields, fields/s | 53.869 to 54.021 | 54.137 to 54.247 |
| Four matched slow ranges, fields/s | 32.825 to 32.937 | 33.380 to 33.564 |
| Common fields p95, ms | 36.1 to 36.0 | 35.7 to 35.4 |
| Common fields p99, ms | 51.4 to 50.8 | 50.5 to 50.6 |
| Common fields maximum, ms | 124.7 to 115.3 | 129.4 to 117.6 |
| Common fields above 100 ms | 11 to 10 | 11 to 10 |
| Worklet underflow output sample frames | 311,729 to 307,792 | 300,721 to 294,224 |

The repeated long field intervals near 13,721, 14,918 and 15,969 improve
by 8.0–11.8 ms across the two orders. The overall gain is 0.20–0.28%.
Sustained real-time throughput and continuous audio remain open.

Baseline Wasm SHA-256 is
`eabdb4ae3c77a54e458891ffc87ff40cc8a1528d20a67c7a6b9fd40af2ca257f`.
Candidate Wasm is
`ba433b16385a3701db52d553e0fd65a9c1e01cc5d05f1a2d3fdf115316be2f35`.
Both use Rust 1.100.0, wasm-bindgen 0.2.129, Binaryen 127 and identical
JavaScript bindings.

The focused native comparison advances real DMA and an independently
queued peer timer status read in both PLL modes with either SH-2 as the
DMA owner. Omitting the shortcut fails the exercised-path assertion.
Ignoring the peer's pending peripheral read fails its arbitration
assertion. The restored candidate passes the DMA, IRQ, video and Sound
suites and native and Wasm Clippy. The five original Chrome groups cover
DMAC bus access, DMA startup, SH-2 pipeline and reset behavior, and video
transactions.

The standard packages agree over a separate 3,040-field replay at 38
checkpoints through field 16,860. Each checkpoint matches public SH-2 state
and clocks, all work RAM by SHA-256 and full canonical Sound bytes. Every
field contributes to matching rolling PCM and pixel digests.

Related to #138 and #17. Private media and raw captures stay local.

## Start CD HIRQ planning at learned resident entries (2026-11-22)

The Master SH-2 can plan a read-only CD status window before its first
HIRQ load. A committed resident prefix of up to 16 instructions can
record its starting PC and opcode when the next decoded instruction
loads the 16-bit status word. A small table uses that observation to
try the existing passive planner on a later visit. The current fetch,
operand and clocks still determine each retirement. The existing
observer horizon and quiet-bus checks govern admission and publication.

An initial controlled comparison over the former VDP1 error interval
finds 13,483 admitted prefixes that retire real HIRQ reads. The measured
40-field interval gains 0.93% across four pairs. The adjacent interval
changes by −0.04%. Both match the independently archived CPU, clock,
RAM, image and PCM endpoints and full canonical Sound state.

The production build uses Chrome 157.0.8103.79, the normal Web player,
active Sound and Video Workers, and AudioWorklet. Each fresh run warms
through field 12,900 and plays six twelve-second intervals of Machi
Disc 1. All four runs share 3,893 compared fields. They apply all 157
scripted inputs at the requested fields and draw every completed field.
Browser errors and audio queue drops are zero.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Common fields, fields/s | 54.242 to 54.558 | 54.020 to 54.419 |
| Four matched slow ranges, fields/s | 33.501 to 34.015 | 32.998 to 34.046 |
| Fields 16,231–16,270, fields/s | 23.310 to 24.124 | 22.929 to 24.025 |
| Common fields p95 field time, ms | 35.4 to 35.1 | 36.1 to 35.3 |
| Common fields p99 field time, ms | 50.7 to 49.2 | 51.5 to 49.2 |
| Common fields maximum field time, ms | 118.5 to 114.7 | 118.3 to 114.1 |
| Worklet underflow output sample frames over six intervals | 295,760 to 277,437 | 308,560 to 285,403 |

Overall throughput gains 0.58% and 0.74%. Worklet shortages fall 6.20%
and 7.50%. The four slow ranges' p99 field time changes from 102.8 to
103.3 ms in forward order and from 104.1 to 103.0 ms in reverse order.
Fields 14,861–14,940 lose 1.45% in forward order and gain 0.88% in
reverse order. One twelve-second interval in each order has more
missing PCM output samples. Sustained 60 fields/s and continuous audio
remain open.

Baseline Wasm SHA-256 is
`ba433b16385a3701db52d553e0fd65a9c1e01cc5d05f1a2d3fdf115316be2f35`.
Candidate Wasm is
`6fabb12f9099cccb52fee682537c129b81e6eaf40df560338d3ebf00e52bc811`.
Both use Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127 with
byte-identical JavaScript bindings.

The native originals use both PLL modes, the private Slave and active
Sound. A learned entry is also tried with a changed live CD operand.
Suppressing entry learning fails the exercised-path assertion. Accepting
an unrelated operand fails the ordinary CPU state comparison. The
restored source passes the HIRQ, cached scheduling, video and Sound
suites and native and Wasm Clippy. Original Chrome A-bus timing,
refresh arbitration, SH-2 fetch, CD runtime and SH-2 bus pipeline cases
pass on the standard candidate.

The two standard packages agree over a separate 3,040-field replay
through field 16,860. Each of 38 checkpoints matches public SH-2 state
and clocks, all work RAM by SHA-256 and full canonical Sound bytes.
Every field contributes to matching rolling PCM and pixel digests.

Related to #138 and #17. Private media and raw captures stay local.

## Retain bounded playback debt through slow fields (2026-11-22)

The playback Worker retains up to eight guest field periods of pacing debt
when expensive fields overrun the schedule. A gap of at least sixteen field
periods between completions starts a fresh pacing phase. The Worker still
emulates and presents each field using the guest's existing clock, PCM and
frame-credit paths.

Chrome 157.0.8103.79 runs the normal Web player with active Sound and Video
Workers and AudioWorklet. Each fresh run warms through field 12,900 and plays
six twelve-second intervals of Machi Disc 1. Both execution orders share
3,915 presented fields and all 157 scripted input events at the requested
presentation fields. Audio queue drops and browser errors are zero. The
shared Wasm SHA-256 is `6fabb12f9099cccb52fee682537c129b81e6eaf40df560338d3ebf00e52bc811`. The
modified playback Worker SHA-256 is `08ce8a937cf9b388a722b4bf9f665a944cf1f5b9fc63fca74566e8abaa4c1c19`.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Common fields, fields/s | 54.319 to 54.857 | 54.433 to 54.790 |
| Four matched slow ranges, fields/s | 33.813 to 34.475 | 34.098 to 34.289 |
| Common fields p99 field time, ms | 49.6 to 49.5 | 49.3 to 49.6 |
| Common fields maximum field time, ms | 117.1 to 117.5 | 117.3 to 117.5 |
| Worklet underflow output sample frames over six intervals | 295,290 to 261,072 | 286,330 to 265,040 |

Worklet shortages fall 11.59% and 7.44% overall. The third twelve-second
interval increases from 17,194 to 19,015 missing frames in forward order
and from 17,507 to 18,728 in reverse order. The schedule still produces
nine field gaps over 100 ms in each arm. Sustained nominal gameplay rate
and continuous audio remain open.

The focused virtual-clock tests exercise the production pacing loop,
including repeated overruns, a long host stall, a permanently slow host
and uncapped playback. A full-reset variant fails the repeated-overrun
witness. A variant that retains debt across the long gap fails the
long-stall witness.

Related to #138 and #17. Private media and raw captures stay local.

## Fold counter-controlled CD status polling loops (2026-11-22)

The resident CD status executor can advance an observed loop that changes a
single SH-2 register by a fixed immediate. It checks the live instruction body,
comparison, pipeline, retained fetch, cache replacement, A-bus and BSC state.
Each fold ends before the next physical refresh or a change in the branch's
result, including a register wrap. The original retirement times remain
available for selecting the exact peer and Sound endpoint.

In a separate diagnostic trace of Machi Disc 1, 1,380,456 instruction
retirements are folded in the busiest 40-field interval. These account for
60.4% of the 2,285,178 committed typed retirements in that interval. The
following 40 fields fold 31,851 of 65,989. The last 40 fields fold none.
All three checkpoints agree on both exposed SH-2 states, clocks, all work RAM,
full canonical Sound state, and rolling PCM and pixel digests.

Chrome 157.0.8103.79 runs the standard Web player with active Sound and Video
Workers and AudioWorklet. Each fresh run warms through field 12,900 and then
plays six twelve-second intervals of Machi Disc 1. Both orders share 3,917
compared fields and all 157 scripted inputs at their requested presentation
fields. Every field is drawn. Browser errors and audio queue drops are zero.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Common fields, fields/s | 54.346 to 54.742 | 54.784 to 54.845 |
| Four matched slow ranges, fields/s | 32.714 to 34.120 | 34.101 to 34.337 |
| Common fields p95, ms | 36.3 to 35.6 | 35.4 to 35.3 |
| Common fields p99, ms | 52.5 to 49.7 | 49.7 to 49.2 |
| Common fields maximum, ms | 120.1 to 115.1 | 115.3 to 115.8 |
| Common fields above 100 ms | 11 to 9 | 10 to 11 |
| Worklet underflow output sample frames | 290,543 to 268,368 | 265,808 to 261,999 |

Matched throughput improves 0.73% and 0.11%. Worklet shortages fall 7.63%
and 1.43%. Sustained nominal playback and continuous audio remain open.

Baseline Wasm SHA-256 is
`6fabb12f9099cccb52fee682537c129b81e6eaf40df560338d3ebf00e52bc811`.
Candidate Wasm SHA-256 is
`033b64e541da01ce62521ec0040299493b538ebfb84f1237678a64361679b866`.
Both use Rust 1.100.0, wasm-bindgen 0.2.129, Binaryen 127 and identical
JavaScript bindings.

The focused original covers both PLL modes, real physical refresh and nonzero
Sound. Deliberately shifting the timestamps by one extra loop period fails the
retirement witness. The restored candidate passes the CD HIRQ and private-peer
native suites and the original Chrome A-bus, refresh arbitration, CD runtime,
SH-2 fetch and SH-2 bus pipeline cases.

The two standard packages agree over a separate 3,040-field replay through
field 16,860. All 38 checkpoints agree on public SH-2 state and clocks,
all work RAM by SHA-256, full canonical Sound bytes and rolling PCM and
pixel digests. The unpaced aggregate is 51.925 to 51.896 fields/s (−0.056%).
The scoped PR source rebuild matches both measured package files. Native and
Wasm Clippy pass with warnings denied.

Related to #138 and #17. Private media and raw captures stay local.

## Reuse a settled bus grant after local CPU allocation (2026-11-22)

When an external transfer already owns the bus, the allocator can still grant
an independent SH-2 peripheral operation or write-buffer acceptance. Once that
local allocation has run, another pass at the same clock will see the ports
unchanged or occupied. The scheduler can reuse that result through the first
unadvanced clock boundary. A pass that prepared a local DMA request keeps the
ordinary arbitration and interrupt sampling path. Actual bus operations,
peripheral completion and Sound request synchronization still take place.

The original native witness queues a real peer FTCSR read during a Master
SH-2 DMA transfer in both PLL modes. The compared runs match journals, active
owners, interrupt observations, clocks and the transferred word. A second
original requires an unsettled result when a real local SCI DMA is prepared.
An intentionally omitted DMA preparation guard fails that original witness.
Native and Wasm Clippy pass with warnings denied. The scoped PR source
rebuild matches the measured package bytes.

A separate continuous 1,600-field replay through field 16,460 matches all 40
checkpoints, including public SH-2 state and clocks, full WRAM, canonical Sound
bytes, and rolling PCM and pixel digests. Unpaced throughput changes from
49.670 to 50.259 fields/s (+1.19%), with 34 of 40 windows faster. The measured
producer intervals at fields 14,918 and 15,969 change from 120.6 to 119.2 ms and
from 117.5 to 116.8 ms.

The standard candidate passes the original Chrome SH-2 DMAC, DMA startup,
SH-2 pipeline and video transaction checks. The normal production player uses
Sound and Video Workers and AudioWorklet. Each fresh Chrome 157.0.8103.79 run
warms up through field 12,900 and measures six twelve-second intervals.
Both orders share 3,956 fields and all 157 inputs at their requested
presentation fields. Every field is drawn. Browser errors and audio queue
drops are zero.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Common fields, fields/s | 54.910 to 55.154 | 54.883 to 55.164 |
| Four matched slow ranges, fields/s | 34.387 to 35.243 | 34.452 to 35.317 |
| Common fields p95, ms | 35.2 to 34.6 | 35.1 to 34.6 |
| Common fields p99, ms | 49.5 to 48.5 | 49.5 to 48.5 |
| Common fields maximum, ms | 116.2 to 114.8 | 119.0 to 116.6 |
| Common fields above 100 ms | 9 to 3 | 10 to 3 |
| Worklet underflow output sample frames | 261,231 to 247,089 | 262,736 to 246,383 |

Matched throughput rises 0.45% and 0.51%. Worklet shortages fall 5.41% and
6.22%. Sustained nominal 60 fields/s and continuous audio remain open.

Baseline Wasm SHA-256 is
`033b64e541da01ce62521ec0040299493b538ebfb84f1237678a64361679b866`.
Candidate Wasm SHA-256 is
`f9fc6c9b269b8e6fdc67b4c474f08162565d44a500ca380d92b9998f0b71a85f`.
Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127 produce identical binding
bytes for both packages.

Related to #138 and #17. Private media and raw captures stay local.

## Issue prepared CD word reads through the physical journal (2026-11-22)

Prepared SH-2 loads for aligned CD words can issue their physical beat directly
through the same bus journal used by ordinary scheduled accesses. The load
still retains its MA timing and checks the live bus width. Arbitration, sampled
data, little-endian conversion and user-break observation use the existing
clocked paths. FIFO reads advance at physical completion and a CR4 read
releases the response latch there.

The original CPU fixture compares the direct path against both an ordinary
instruction replay and the previous prepared-load path. The same fixture exercises the retained longword FIFO path. The candidate
passes the Chrome `cdCopy`, `cdRuntime`, `abusTiming` and `sh2Load` originals.

A continuous 240-field comparison through field 16,420 has six matched
checkpoints. Both builds agree on public SH-2 state and clocks, all work RAM,
canonical Sound bytes and rolling PCM and pixel digests. Unpaced throughput
changes from 43.516 to 43.858 fields/s. Five of six windows improve. The
37-field CD-intensive interval takes 1,914.6 ms on the baseline and 1,894.3 ms
with the candidate. Normal playback is measured separately.

A broader unpaced replay through field 16,860 matches all 3,200 fields
at 80 CPU, clock, full WRAM, Sound, PCM and pixel checkpoints. Its pooled
throughput changes from 52.506 to 52.248 fields/s (−0.49%). The same
37-field CD interval changes from 1,881.2 to 1,884.7 ms in this run.

Chrome 157.0.8103.79 runs the normal Web player with both Workers and
AudioWorklet. Each fresh run warms through field 12,900 and measures six
twelve-second intervals of Machi Disc 1. The four runs share 3,894 fields
and 157 input events at the requested presentation fields. Every field is
drawn. Browser errors and audio queue drops are zero.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Common fields, fields/s | 54.877 to 54.964 | 54.032 to 54.452 |
| Four matched slow ranges, fields/s | 34.674 to 34.907 | 32.194 to 33.657 |
| Common fields p95, ms | 35.5 to 35.5 | 38.3 to 37.1 |
| Common fields p99, ms | 49.9 to 48.9 | 53.9 to 53.2 |
| Common fields maximum, ms | 115.1 to 113.7 | 120.5 to 121.5 |
| Common fields above 100 ms | 3 to 3 | 3 to 2 |
| Worklet underflow output sample frames | 258,927 to 253,776 | 307,792 to 283,150 |

Matched throughput rises 0.16% and 0.78%. Total audio shortages fall 1.99%
and 8.01%. The third twelve-second interval has more shortages in both
orders. Sustained nominal gameplay and continuous audio remain open.

Baseline Wasm SHA-256 is
`f9fc6c9b269b8e6fdc67b4c474f08162565d44a500ca380d92b9998f0b71a85f`.
Candidate Wasm SHA-256 is
`1a371f6c4c4b3c0ffeeb6947f25432824486f91323fee3b1df54fd89f1d24bcd`.
Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127 produce identical
JavaScript binding bytes for both packages.

Related to #138 and #17. Private media and raw captures stay local.

## Plan CD response words after an accepted WRAM write (2026-11-22)

The cached Master can plan a nonconsuming CR2 or CR3 word behind the final
physical beat of its already accepted WRAM write. The planned read starts at
the later of its normal MA boundary and the existing grant's end. The WRAM
beat completes on the real arbiter. The cached span keeps the original
peer, interrupt, CD, Sound, refresh and raster deadlines. The planner can
retire the one following WRAM word store in the same span. It publishes
the response read's BSC and A-bus protocol state before the normal store
acceptance. CR4 still releases the response latch at physical completion.

Original native tests exercise both PLL settings and the actual selected
paths, including a CD response while the Master owns the final WRAM phase.
The literal single-instruction comparison checks CPU and readiness state,
clocks, WRAM, active physical/internal transfers, buffered contents, and
BSC/A-bus protocol. All 60 tests in the cached-execution group pass. Native
all-target/all-feature and Wasm all-feature Clippy pass with warnings denied.
The standard browser fixtures `cdRuntime`, `cdReports`, `abusTiming`,
`sh2Store`, and `refreshArbitration` pass.

The standard Chrome build uses Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen
127. Stock Wasm SHA-256 is
`1a371f6c4c4b3c0ffeeb6947f25432824486f91323fee3b1df54fd89f1d24bcd`.
Candidate Wasm SHA-256 is
`97da6c95937afe5368538de4960c373fb5e07289d86945bf84799ff17ca3ec75`.
The browser binding and Worker files match stock.

An exact local Chrome comparison on the authorized game and one controller
timeline matches all 520 fields in thirteen checkpoints. It compares both
SH-2 states and clocks, SHA-256 for all WRAM, rolling video and PCM digests,
and the full canonical Sound snapshot size and checksum. Unpaced diagnostic
throughput is **45.9084 → 46.3037 fields/s (+0.86%)**, with nine of thirteen
windows improving. The dense CD interval (37 fields) takes
**1883.6 → 1849.4 ms**. A second correctness-only fixed-field run matches a
frozen exact stock reference in all 80 checkpoints over 3,200 fields, including
the three acute DMA fields, dense CD interval and subsequent story. The first
59 of those stock checkpoints were also reproduced independently from the
same stock package. These diagnostic intervals use the standard Sound and
Video Workers and drain generated PCM directly.

Chrome 157.0.8103.79 runs the normal Web player with its Sound and Video
Workers and AudioWorklet. Each fresh run warms through field 12,900, then
measures six twelve-second intervals of Machi Disc 1. The four runs share
3,966 compared fields and all 157 controller inputs at their requested
presentation fields. Every field is drawn. Runtime errors and Worklet queue
drops are zero.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Overall playback, fields/s | 55.267 to 55.499 | 55.014 to 55.347 |
| Common fields, fields/s | 55.242 to 55.451 | 55.030 to 55.319 |
| Four matched slow ranges, fields/s | 35.372 to 36.305 | 34.741 to 35.751 |
| Common fields p95, ms | 34.7 to 34.2 | 35.6 to 34.7 |
| Common fields p99, ms | 47.0 to 46.0 | 48.5 to 47.0 |
| Common fields maximum, ms | 111.9 to 113.8 | 115.6 to 115.2 |
| Common fields above 100 ms | 2 to 2 | 3 to 3 |
| Worklet underflow output sample frames | 241,872 to 229,938 | 253,745 to 237,423 |

Matched throughput rises 0.38% and 0.52%. Audio shortages fall 4.93%
and 6.43%. Shortages increase in the third twelve-second interval in
both orders. The candidate still lacks 5.21–5.38 seconds of audio sample
frames at 44.1 kHz over each 72-second run. Sustained nominal gameplay
and continuous audio remain open.

Related to #138 and #17. Private media and raw captures stay local.

## Continue 16-byte SH-2 DMA units from physical completion (2026-11-22)

The scoped path retains the already sampled halves of aligned 32-bit Work
RAM reads inside a 16-byte dual-address SH-2 DMA unit. A pending physical
read is revalidated against the selected channel and live bus width. Each
real phase still uses the ordinary arbiter and external observation. The
fourth logical read can request the first eligible VDP2 buffered store. The
next store is requested after actual internal write acceptance. The final
write acceptance uses the existing channel completion, interrupt, request
acknowledgement and cycle-steal state. Other modes use the ordinary replay.

The native differential checks two PLLs and compares active and internal
transfers, journals, channel state, clocks and video memory at every tick.
It exercises the split reads, read-to-write transition and both complete
16-byte units. Three separate DME withdrawal points preserve the literal
results. The 22 board DMA regressions and three OnChip DMA regressions pass.

The standard Chrome packages use Rust 1.100.0, wasm-bindgen 0.2.129 and
Binaryen 127. Baseline Wasm SHA-256 is
`97da6c95937afe5368538de4960c373fb5e07289d86945bf84799ff17ca3ec75`.
Candidate Wasm SHA-256 is
`1072c0107e01a086435ec4e418eaeeb74a62a7809cfcd96fac9c8cdc745931b8`.
JavaScript bindings and playback Workers match.

A local fixed-field replay of Machi Disc 1 matches 520 fields at 13
checkpoints. The checkpoints compare the public state of both SH-2s,
clocks, the SHA-256 of all WRAM, rolling PCM and pixels, and canonical
Sound checksum and size. Video transport counts and bytes also match
in each checked window. Unpaced throughput improves from 46.306 to
46.599 fields/s (+0.63%), with 11 of 13 windows faster. The long field with
no CD phase changes from 114.8 to 104.9 ms, four adjacent late DMA fields
change from 352.3 to 339.5 ms in total, and the 37-field CD/DMA interval
changes from 1853.4 to 1828.1 ms. These results measure the fixed-field
runner with both playback Workers and direct PCM draining.

A separate correctness replay matches the same stock package in all 80
checkpoints over 3,200 fields. The candidate passes the Chrome `dmacBus`,
`sh2BusPipeline`, `videoTransactions` and `refreshArbitration` originals.
Native all-target/all-feature and Wasm all-feature Clippy pass with warnings
denied.

Chrome 157.0.8103.79 runs the ordinary Web player with both playback Workers
and AudioWorklet. Each independent run warms through field 12,900, then
plays six twelve-second intervals. The four runs share 3,992 fields and all
157 controller inputs at the requested presentation fields. Every field is
drawn. Runtime errors and audio queue drops are zero.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Overall playback, fields/s | 55.460 to 55.528 | 55.379 to 55.489 |
| Common fields, fields/s | 55.452 to 55.525 | 55.383 to 55.473 |
| Four matched slow ranges, fields/s | 36.079 to 36.404 | 35.859 to 35.951 |
| Common fields p95, ms | 34.0 to 34.1 | 34.6 to 34.1 |
| Common fields p99, ms | 46.5 to 46.3 | 46.7 to 46.3 |
| Common fields maximum, ms | 113.9 to 103.9 | 114.3 to 102.9 |
| Common fields above 100 ms | 2 to 2 | 3 to 2 |
| Worklet underflow output sample frames | 231,760 to 227,536 | 235,600 to 230,608 |

Matched throughput rises 0.13% and 0.16%, and total audio sample shortages
fall 1.82% and 2.12%. The third interval in the forward order and fourth
interval in the reverse order have more shortages. The earlier video-error
range changes by −0.37% and +2.01%, respectively. About 5.16–5.23 seconds
of 44.1 kHz samples are still missing during each candidate run. Sustained
nominal gameplay and continuous audio remain open.

Related to #138 and #17. Private media and raw captures stay local.

## Keep paced catch-up within the AudioWorklet queue (2026-11-22)

The player can retain up to sixteen fields of catch-up after sustained
slow frames. A single long host pause starts a new pacing phase. Each
AudioWorklet acknowledgement reports the number of PCM frames in its ring.
The player reserves space for sent packets until they are acknowledged.
When a packet cannot fit, the Worklet reports available space before the
player sends it and releases its presentation credit. The ring holds
8,192 stereo sample frames and the playback Worker retains at most two
pending fields. Pausing or suspending audio releases blocked playback.

The browser package uses the same Wasm and bindings as the base revision.
Only the five Web player files differ. The original Chrome
`scspCpuMemory`, `scspPcm` and `videoTransactions` groups pass. The
local JavaScript tests exercise both stereo channels, an unacknowledged
packet, exact FIFO recovery after a full ring, presentation credit, and
pause and suspend recovery. A separate Chrome fixture with controls applied
at fixed guest fields produces identical SHA-256 digests over 40 fields
of stereo PCM and packet lengths.

Chrome 157.0.8103.79 runs the ordinary Web player with its Sound and
Video Workers and AudioWorklet. Each run warms through field 12,900
and measures six twelve-second intervals of Machi Disc 1. The runs
match 3,984 fields in the forward order and 3,994 in the reverse,
with all 157 controller events reaching their requested presentation
fields. Every emulated field is drawn. Runtime errors and audio queue
drops are zero.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Overall playback, fields/s | 55.279 to 55.763 | 55.405 to 55.498 |
| Matched playback, fields/s | 55.272 to 55.730 | 55.416 to 55.489 |
| Four matched slow ranges, change | +1.67% | −1.52% |
| Matched field gap p99, ms | 47.7 to 46.4 | 47.1 to 47.8 |
| Matched field gap maximum, ms | 104.4 to 103.6 | 103.7 to 104.7 |
| Worklet underflow output sample frames | 241,713 to 215,919 | 232,687 to 230,127 |

The two playback orders reduce the missing audio by 10.67% and 1.10%.
Field gaps and individual intervals vary between runs. The candidate
still lacks 4.90–5.22 seconds of sound at 44.1 kHz per 72 seconds.
Sustained nominal gameplay and continuous audio remain open.

Related to #138 and #17. Private media and raw captures stay local.


## Adjust SH-2 DMA read code generation in Wasm (2026-11-23)

Rust emits `advance_dmac_block_read` as a separate function for the Wasm
target. The helper no longer appears as a separate named function after
Binaryen optimizes the module. The verified named build and release package
have identical bytes after removing custom sections. DMA admission, physical
reads and write acceptance retain the same ordering and guest clocks.

With the Sound and Video Workers active, an unpaced Machi Disc 1 comparison
matches 80 checkpoints over 3,200 fields. Each field contributes to the PCM and
pixel digests. Every checkpoint compares both CPUs, clocks, WRAM, and canonical
Sound transport bytes. The diagnostic throughput is 52.439 to 52.745 fields/s.
The 37-field CD-intensive interval takes 1,858.6 to 1,846.0 ms. A separate
profile-guided build using the three DMA functions as its training input did
not improve overall throughput and was held.

Chrome 157.0.8103.79 runs the ordinary paced player with the Sound and Video
Workers and AudioWorklet. Each arm warms through field 12,900 and measures six
12-second intervals. The comparisons match 4,006 fields in forward order and
4,011 in reverse order. All 157 controller events match their requested
presentation fields in both orders. Every emulated field is drawn. Runtime
errors and audio queue drops are zero.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Overall playback, fields/s | 55.546 to 55.770 | 55.668 to 55.721 |
| Matched playback, fields/s | 55.572 to 55.750 | 55.649 to 55.712 |
| Four matched slow ranges, change | +1.68% | +0.27% |
| Matched field gap p99, ms | 47.4 to 46.5 | 46.6 to 46.1 |
| Matched field gap maximum, ms | 104.8 to 103.3 | 104.8 to 106.3 |
| Worklet underflow output frames | 226,830 to 215,919 | 220,911 to 218,351 |

The audio shortages decrease by about 0.25 s and 0.06 s per 72 s. The forward
median queue delay for an admitted PCM packet is approximately 166 ms on both
packages. The game still has audible shortages and remains below nominal
real-time playback.

Formatting, 42 selected native DMA tests, and 24 Sound rendezvous tests pass.
The standard Wasm has SHA-256
`f979281be5937c6499cc7c69acdd67d81575d182ec8f90b9ab39bd409b68aa57`.
The Chrome `dmacBus`, `dmaStartup`, `cdStream`, and `videoTransactions` original
groups pass with that Wasm. Private media and raw captures stay local.

Related to #138 and #17. Sustained real-time gameplay, continuous audio, and
broader hardware conformance remain open.



## Aggregate BSC timing for private SH-2 cache refills (2026-11-23)

The cached executor stages a 16-byte WRAM refill inside an already admitted
observer window. The BSC planner uses the existing first-beat operation to
establish turnaround and row state, then calculates the duration of the
remaining beats together. Ordinary memory retains the configured wait and CS0
nibble cycles. SDRAM retains the burst length, active row and final precharge.
The literal planner handles held bus phases, refresh recovery, DRAM and
pseudo-static RAM.

An unpaced comparison in Chrome 157.0.8103.79 matches 80 checkpoints across
3,200 fields of Machi Disc 1. Both CPUs, guest clocks, WRAM, canonical Sound and
renderer transport match. Every field contributes to matching PCM and pixel
digests. Throughput rises from 51.361 to 51.740 fields/s across the comparison,
with gains in 64 of 80 windows. The CD-intensive interval takes 1,882.7 to
1,874.5 ms. The four late fields dominated by DMA take 342.7 to 347.1 ms.

The ordinary Chrome player uses the Sound and Video Workers and AudioWorklet.
Each arm warms through field 12,900 and measures six 12-second windows. All
four arms draw every emulated field, apply all 157 controller events and
report zero runtime errors and audio queue drops. Both orders share 4,011
matched presentation fields.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Overall playback, fields/s | 55.737 to 55.890 | 55.668 to 55.751 |
| Matched playback, fields/s | 55.741 to 55.869 | 55.657 to 55.744 |
| Four matched slow ranges, change | +1.52% | +0.50% |
| Worklet underflow output frames | 216,431 to 209,391 | 220,911 to 216,431 |
| Worklet underflow duration, seconds | 4.91 to 4.75 | 5.01 to 4.91 |

The 22 native BSC tests cover ordinary and SDRAM refill timings, all four
longword positions, earlier reads and idle states, open rows, precharge,
refresh recovery and held bus phases. The standard Wasm has SHA-256
`496127abe365a6ed58d2ebd28bae34c880c516a631146cfbe28b9badb40e603e`.
The Chrome `refreshArbitration`, `sh2Fetch`, `sh2BusPipeline` and
`abusTiming` original groups pass against that artifact.

Related to #138 and #17. Sustained real-time gameplay, continuous audio and
broader hardware conformance remain open.


## Avoid repeating VDP1 phase checks for a single clock (2026-11-23)

Every retained VDP1 draw or erase phase has a positive deadline. The single-clock
path uses that fact to advance one clock. Longer intervals still consult the
next deadline, including the final forced-stop edge. The private phase advance
checks the interval in debug builds and reuses the texture and table checks for
VRAM arbitration. The underlying texture, pixel and erase operations still run
at their existing times.

In Chrome 158.0.8127.36, an unpaced comparison covers the same 3,200 fields of
Machi Disc 1 across 80 windows. Both CPUs, guest clocks, WRAM, canonical Sound
and video transport match at every checkpoint. All fields contribute to the
matching PCM and pixel digests. Throughput rises from 52.353 to 52.889 fields/s,
and 76 of 80 windows improve. The CD-intensive 37 fields take 1,861.3 to
1,819.6 ms. The four late fields dominated by DMA take 341.8 to 336.8 ms.

The ordinary Chrome player runs with the Sound and Video Workers and a real
AudioWorklet at 44.1 kHz. Each arm warms through field 12,900 and measures six
12-second windows. All four arms draw every emulated field, apply all 157
controller events and report zero runtime errors and audio queue drops. The
matched results use 4,004 common presentation fields across both orders.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Overall playback, fields/s | 55.600 to 55.792 | 55.528 to 55.806 |
| Matched playback, fields/s | 55.591 to 55.781 | 55.555 to 55.793 |
| Matched p95 field time, ms | 34.4 to 33.6 | 34.4 to 33.4 |
| Matched p99 field time, ms | 47.2 to 45.9 | 47.4 to 46.3 |
| Four matched slow ranges, change | +1.75% | +2.82% |
| Worklet underflow output frames | 225,135 to 214,030 | 227,470 to 213,615 |
| Worklet underflow duration, seconds | 5.11 to 4.85 | 5.16 to 4.84 |

All 52 native VDP1-related tests pass. The standard Wasm has SHA-256
`57764c339d8f537805b8bb4d4b4967e3db4d11f066505bf303c33671cfb97fbc`.
The Chrome `videoTransactions`, `spriteShrink`, `dmaStartup` and `dmacBus`
original groups pass against that artifact. Private media and raw captures stay
local.

Related to #138 and #17. Sustained real-time gameplay, continuous audio and
broader hardware conformance remain open.


## Reuse the selected SCU interrupt output (2026-11-23)

The SCU stores the selected interrupt level and vector and updates them when
IMS or IST changes. The same factor ordering applies across the internal and
A-bus banks. Signal acknowledgement and DMA triggers still execute at their
existing edges. External vector acquisition still selects from the live status
for the accepted level.

An unpaced Machi Disc 1 comparison in Chrome 158.0.8127.36 matches all 3,200
fields and 80 checkpoints. Both SH-2s and their caches, clocks, WRAM,
renderer transport and canonical Sound state match. Every field contributes to
matching PCM and pixel digests. Diagnostic throughput increases from 52.215
to 52.978 fields/s, with gains in 76 of 80 windows. The 37 CD-intensive fields
take 1,826.9 to 1,787.8 ms. The final four DMA-intensive fields take 337.7 to
319.4 ms.

The ordinary Chrome player runs with the Sound and Video Workers and a real
44.1 kHz AudioWorklet. Each arm warms through field 12,900 and measures six
12-second windows. All four arms draw every emulated field, apply all 157
controller events at their requested presentation fields and report zero
runtime errors and audio queue drops. The matched comparisons use 4,000
common fields across both orders.

| Measure | Forward order | Reverse order |
| --- | ---: | ---: |
| Overall playback, fields/s | 55.490 to 55.918 | 55.792 to 56.013 |
| Matched playback, fields/s | 55.503 to 55.891 | 55.762 to 55.991 |
| Matched p95 field gap, ms | 35.2 to 32.8 | 33.7 to 32.7 |
| Matched p99 field gap, ms | 47.9 to 45.8 | 46.3 to 44.9 |
| Four matched slow ranges, change | +4.19% | +2.31% |
| Worklet underflow output frames | 230,383 to 207,471 | 215,023 to 202,095 |
| Worklet underflow duration, seconds | 5.22 to 4.70 | 4.88 to 4.58 |

All 88 SCU-related native tests pass, including the exhaustive priority test
across both banks. The Chrome `interruptEntry`, `dmaStartup`, `cdStream` and
`dmacBus` originals pass with the standard Wasm SHA-256
`c3cc1ed49e2f33f606fef5c6543c30f6575cb67c04409c91bf93121dccaeec0f`.
The JavaScript binding bytes match the baseline. Private media and raw
captures stay local.

Related to #138 and #17. Sustained real-time gameplay, continuous audio and
broader hardware conformance remain open.
