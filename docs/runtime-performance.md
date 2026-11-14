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
