# Web emulator optimization research

Related to [#138](https://github.com/takahirox/saturn-rust/issues/138) and #17.
Research date: 2026-11-13. Local source inspected: `2d6b17f`.

## Recommended direction

Prioritize reducing CPU execution and synchronization overhead, then optimize
measured display kernels. Evaluate a WebAssembly-generating SH-2 JIT as a larger
experiment if reusable interpreted blocks cannot remove enough work. GPU
rendering is a credible later option, particularly for VDP2 composition, but
requires evidence that rendering accounts for enough of the slow game scene.

The existing implementation already includes many standard emulator techniques.
Adding another opcode lookup table, enabling release mode, or moving execution
into a Worker would not by itself address the remaining throughput gap. Each
candidate below is an experiment whose adoption depends on production-Web
measurements and preserved guest behavior. Implementation results are recorded
in the [runtime performance investigation](runtime-performance.md).

## Performance target and available evidence

An earlier detailed [runtime record](runtime-performance.md#reuse-coherent-normal-tile-address-runs)
reports 14.142 fields/s initially and 13.854 fields/s in a fresh repeat for one
game scene, with Canvas presentation and PCM draining. Those are historical
measurements with the documented compiler/browser builds, not a new benchmark of
`2d6b17f`. They imply about 70.7–72.2 ms per field and a 4.24–4.33 times throughput
gap to 60 fields/s. Revised compiler measurements are linked from that record.

Define success as advancing guest time at the selected Saturn video rate while
presenting normally and delivering continuous audio. A game may intentionally
change its picture at less than 60 Hz. Repeated pictures are compatible with
correct real-time emulation; a 60 Hz browser callback does not demonstrate it.
PAL and interlace need their actual field cadence rather than a forced 60 Hz.

For a representative 14 fields/s baseline, reaching 60 requires eliminating
about 76.7% of total elapsed work. By Amdahl's law, removing an entire subsystem
that costs 30% yields at most 1.43 times throughput. Making half the runtime four
times faster yields only 1.6 times overall throughput. These are illustrative
bounds, not measured subsystem shares.

An earlier [game profile](runtime-performance.md#publish-the-first-private-peer-issue-at-a-ready-master-endpoint-2026-11-12)
attributed 51.2% of samples to cached-burst execution including callees, and 22.3%
to that function itself. Subsequent changes improved that path, sound snapshots
and tile acquisition, so a fresh game profile must determine today's priorities.
Inclusive samples must not be added to their children's samples.

## Techniques used by Saturn emulators

### Yabause block recompilation and idle detection

Yabause's documented SH-2 recompiler caches translated blocks, propagates
constants, allocates registers, links blocks and invalidates translations after
code writes. It handles delay slots explicitly. Its documented CPU switching
quantum is roughly 170–180 guest cycles; that scheduling approximation cannot be
imported into this project's bus and pipeline timing model without independent
evidence. The useful transfer is amortizing decode, dispatch and state traffic
within a proven execution interval. [Yabause SH-2 recompiler design](https://wiki.yabause.org/index.php5?title=SH2_dynamic_recompiler).

The separate idle-aware interpreter analyzes loops and checks whether execution
can remain idle. This supports recognizing stable polling behavior as a real
Saturn optimization, while this project's event deadlines and side-effectful
register reads require stronger guards than merely observing a repeated PC.
[Yabause idle interpreter source](https://github.com/Yabause/yabause/blob/7e38821dbac265490f115e163c523a939acda759/yabause/src/sh2idle.c).

### Kronos cached interpretation and compute rendering

Kronos stores decoded opcode handlers in address-indexed tables for each SH-2.
Its decode path installs the handler and calls it, reducing repeated instruction
classification. This is a portable alternative to native code generation, but
indirect handler calls can have different costs in Wasm. Our opcode tables and
prepared instructions already cover part of the same opportunity.
[Kronos cached interpreter source](https://github.com/FCare/Kronos/blob/e21fe2f983165eb80800d1916d59234680ebaa45/yabause/src/core/sh2/sh2_kronos/src/sh2int.c).

Kronos's renderer report explains using compute shaders to reproduce VDP1's
quad/line behavior instead of approximating it with triangles; it also describes
rotation-background compute rendering. The report identifies its default SH-2
core as a cached interpreter. Thus a fast Saturn emulator need not depend only
on a native SH-2 JIT. The OpenGL 4.3 backend would need a browser-specific design,
such as WebGPU compute. [Kronos renderer report, 2020](https://www.libretro.com/index.php/kronos-2-1-2-progress-report-sega-saturn-emulator/).

### Yaba Sanshiro GPU pipelines

The May 2026 VDP1 article describes screen-pixel coverage and texture lookup in
compute shaders. Its reported device/scene measurements are about 78 fps average
for compute versus 180 for the preceding tessellation path. Compute improved
coverage fidelity while costing more; it is not automatically a faster rendering
algorithm. [Yaba Sanshiro VDP1 compute design](https://www.yabasanshiro.com/blog/2026-05-09_vdp1_emulation_with_compute_shader).

The May 2026 VDP2 design renders layers into separate buffers containing color,
priority and calculation attributes, then selects and composites the required
layers per pixel. This maps more closely to Saturn's priority and color rules
than ordinary GPU alpha blending. The author reports 312 versus 338 fps average
for new/old methods on AYN Thor running Sonic R with the limiter removed. These
device-specific results demonstrate feasibility, not a speed forecast for this
Rust/Wasm implementation. [Yaba Sanshiro VDP2 design and measurements](https://www.yabasanshiro.com/blog/2026-05-31_new_vdp2_emulation).

Its earlier cache article reports that some games depend on different data in
the two SH-2 caches and on cache-hit timing. Disabling guest cache emulation
therefore changes observable behavior. Retain that behavior while caching the
host's derived lookup or decode results. [Yaba Sanshiro SH-2 cache report, 2022](https://www.yabasanshiro.com/blog/2022-03-15_sh2-cache).

These Yaba Sanshiro findings come from developer articles. The inspected
Yabause recompiler design is a separate implementation, not evidence of the
current Yaba Sanshiro CPU backend.

### Mednafen and the Beetle Saturn fork

The inspected Beetle Saturn fork has a next-event scheduler, a direct memory
mapping table, and an optional rendering thread with an explicit command queue.
These suggest reducing repeated event discovery, specializing ordinary memory
access, and parallelizing immutable render work. They do not justify widening
Saturn synchronization intervals indiscriminately. Source-level findings here
apply to this fork and revision, not every upstream Mednafen release.
[Beetle scheduling and memory mapping](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/ss.c),
[Beetle render queue and color cache](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/vdp2_render.c).

Mednafen recommends preloading CD images to avoid storage stalls. This addresses
I/O latency rather than steady CPU throughput. Our browser already loads local
media into memory; decompression or decoding caches are worth investigating only
if game profiles show significant media work. [Mednafen Saturn documentation](https://mednafen.github.io/documentation/ss.html).

## General emulator techniques and Web applicability

| Technique and primary evidence | Application to this repository | Assessment |
| --- | --- | --- |
| State-specialized blocks and direct chaining: [QEMU translator internals](https://www.qemu.org/docs/master/devel/tcg.html) | Retain decoded plans and reusable admission facts across compatible SH-2 blocks. Exit before events, MMIO, faults, cache changes or interrupt eligibility changes. | High-priority experiment after profiling; avoid repeated whole-burst setup. |
| Superinstructions: [Vmgen manual](https://www.complang.tuwien.ac.at/anton/vmgen/html-docs/Superinstructions.html) | Fuse measured common SH-2 sequences into one host dispatch, retaining each instruction's retirement and event boundaries. | Useful intermediate step before a full JIT; existing scalar blocks already amortize some work. |
| Guest-to-Wasm JIT and hotness thresholds: [v86 design](https://github.com/copy/v86/blob/315b2b0b758703ef0e6cc7b4088d6068f83f3c20/docs/how-it-works.md) | Generate Wasm for hot SH-2 blocks and share runtime memory/helpers with the interpreter. | Credible larger experiment. Measure compilation latency, module/cache growth and side-exit costs. |
| Guarded memory fast paths: [v86 design](https://github.com/copy/v86/blob/315b2b0b758703ef0e6cc7b4088d6068f83f3c20/docs/how-it-works.md) and Beetle mapping above | Specialize known RAM regions and cache hits, retaining permissions, aliasing, wait states and transactional writes. | Extend existing paths where profiles show repeated address/region dispatch. |
| Compact hot data and result types: [Rust Clippy's large-error guidance](https://clippy.rust-lang.org/master/index.html#result_large_err) | Separate large, uncommon diagnostic payloads from results returned on every memory access. | Measure an indirect SMPC fault payload while preserving every diagnostic field and error/replay behavior; smaller types alone do not establish a speedup. |
| Idle-loop recognition: Yabause source above | Reuse proofs for stable polling loops until the first event that can affect a read, interrupt, cache or peer CPU. | Already present in private-peer polling; expand only for measured uncovered loops. |
| SIMD and regular data layouts: [WebAssembly SIMD documentation](https://emscripten.org/docs/porting/simd.html) | Vectorize independent pixel decode, color conversion and composition runs; consider audio only where dependencies allow. | Medium priority, contingent on hot kernels. Preserve integer widths, saturation and rounding. |
| Parallel rendering: Beetle render queue above; [browser threading requirements](https://emscripten.org/docs/porting/pthreads.html) | Produce immutable display packets in emulated order, then decode/compose them in a Worker. | Later experiment. Shared Wasm memory needs deployment isolation and synchronization support. |
| Profile-guided optimization: [rustc PGO documentation](https://doc.rust-lang.org/rustc/profile-guided-optimization.html) | Train compiler decisions using representative workloads after proving a Wasm profiling pipeline works. | Lower priority. Native training results do not demonstrate a Wasm gain. |

QEMU's chaining avoids repeated main-loop dispatch and preserves state recovery
at exits. Its native code patching and host fault handlers cannot be transplanted
directly into browser Wasm. Transfer the block/state organization while retaining
Saturn-specific timing. [QEMU translator internals](https://www.qemu.org/docs/master/devel/tcg.html).

v86 provides a working browser JIT precedent, but documents structured-control-flow
constraints, module-generation cost and memory overhead. Its design uses hotness
to limit compilation and software checks for special memory accesses. Native
ARM/x86 code generators cannot execute directly in the browser; generated Wasm
needs its own backend. Compiling the emulator itself to Wasm still leaves guest
instruction decoding and dispatch in the interpreter.
[v86 design](https://github.com/copy/v86/blob/315b2b0b758703ef0e6cc7b4088d6068f83f3c20/docs/how-it-works.md).

Rust's large-error guidance explains that an uncommon error variant still sets
the minimum size of a frequently returned `Result`, and demonstrates boxing its
large payload. This is a general implementation technique, rather than a finding
about a specific Saturn emulator. Its default lint threshold is not a performance
boundary; evaluate actual target layouts, error allocation and production-Web
timing before adopting it. [Clippy result-size guidance](https://clippy.rust-lang.org/master/index.html#result_large_err).

Moving the entire emulator into one Worker can improve input/UI responsiveness,
but does not parallelize its core work. Splitting Master and Slave SH-2 into
independent host threads would introduce synchronization on their shared buses
and memory. Start with independent rendering work rather than assume that two
guest CPUs can run freely on two host cores. Shared-memory browser threading
also depends on cross-origin isolation; verify the actual hosting arrangement
before selecting that design. Emscripten's deployment constraints are relevant
to the browser APIs, although this project uses Rust/wasm-bindgen.
[Browser threading requirements](https://emscripten.org/docs/porting/pthreads.html).

## Existing optimizations and remaining opportunities

| Area | Existing implementation | Next useful experiment |
| --- | --- | --- |
| SH-2 decode and execution | [Opcode classification](../src/bus/cached.rs), [scalar decoder](../src/cpu/scalar.rs), [scalar blocks and prepared operations](../src/cpu.rs) | Reusable blocks containing loads, stores and branches; measured superinstructions; then hot-block Wasm compilation. Another generic decoder table alone has weak justification. |
| CPU/device synchronization | [Cached bursts](../src/bus/cached.rs), [polling proofs](../src/bus/private_poll.rs), [poll periods](../src/bus/poll_cycle.rs) | Count burst lengths, exit reasons, replay retries and admission costs. Reuse established facts until their dependencies change. |
| Cache and memory | [Host cache lookup](../src/cache.rs), [ordered shadow versions](../src/cache/shadow.rs) | Reduce lookup/forwarding and publication work without changing hardware cache contents, LRU, aliases or bus grants. |
| Sound rollback | [Boxed snapshots and RAM undo logs](../src/sound/checkpoint.rs), guarded checkpoint elision in cached execution | If still hot, reduce saved mutable state or reuse storage. Preserve live DSP/PCM work, control writes and RESET boundaries. |
| Video acquisition and composition | [Coherent tile address runs](../src/vdp2/fetch.rs), [limited priority composition](../src/vdp2.rs) | Reuse decoded data only for identical captured operands; SIMD contiguous runs. Later, move composition of captured data to GPU/Worker. |
| Sound DSP | [Execution-time program reads and zero-step paths](../src/sound/dsp.rs) | If hot, predecode stable program fields with write invalidation. Keep coefficients, delayed state, RAM and arbitration live. |
| Build and delivery | [Fat LTO and one codegen unit](../Cargo.toml), [inlining and Binaryen optimization](../scripts/build-web.sh) | Measure alternatives only against the production artifact. Existing release and optimizer settings are already active. |
| Browser presentation | [Frame pacing and Canvas upload](../web/app.js), [Wasm marshaling](../src/wasm.rs) | Attribute framebuffer allocation/copy and Canvas time separately; consider reusable output memory when material. |

The inspected generated framebuffer binding copies its returned array, and the
app constructs a new clamped array and ImageData for each field. Reducing copies
is plausible, but at 320×224 one RGBA image is only 286,720 bytes. Profile this
cost before treating it as the explanation for approximately 70 ms fields.
Any retained Wasm view must be refreshed after memory growth.

The runtime history already records unsuccessful broader predecode, layout,
inlining and snapshot experiments. Reuse that evidence when choosing trials;
reviving a rejected idea needs a changed workload, design or compiler rationale.
Its historical two-instruction average scalar-block length is not a current
limit on broader blocks containing memory and control operations.
[Prior VM experiments](runtime-performance.md#post-checkpoint-vm-optimization-evaluation-2026-11-10).

## First implementation experiments

1. **Measure the current game path.** Freeze a standard Web build and profile
   the reported slow scene after deterministic boot/input. Attribute execution,
   bus scheduling, video acquisition/composition, sound, media and JS delivery.
   Collect burst lengths and exit reasons separately from uninstrumented timing.
   This selects the first change and establishes the actual remaining gap.
2. **Amortize hot execution setup.** If CPU/bookkeeping remains dominant, try
   reusable decoded blocks or a small measured superinstruction family that
   includes common memory/control operations. Reuse the existing transaction
   machinery and precise exits. Measure setup/rollback costs as well as dispatch.
3. **Optimize the largest independent kernel.** If video or sound dominates,
   select one captured-data reuse, layout, predecode or SIMD experiment. A cached
   palette result must include the captured palette/version identity; a static
   background does not permit skipping raster acquisition or timed VRAM reads.
4. **Evaluate an SH-2-to-Wasm prototype.** If dispatch still costs enough to
   justify compilation, start with one supported hot block class, hotness-based
   admission and an interpreter fallback. Compare warm throughput, cold compile
   pauses, code-cache size and generated-code exits before expanding coverage.
5. **Evaluate GPU rendering or rendering Workers.** Proceed when measured
   independent render work can repay packet construction, upload, synchronization
   and any CPU readback. VDP2 composition is a narrower initial boundary than
   replacing all VDP1/VDP2 timing and memory behavior.

A block cache must identify the instructions each CPU can actually observe.
Invalidating solely on a physical RAM write is insufficient: SH-2 caches do not
snoop, and retained prefetch/decode words can outlive that write. Cache-control
changes, DMA, aliases, refills and existing retained instructions must all keep
their established behavior. The current cache/refill tests are essential for
both interpreted blocks and a future JIT.

Do not select frame skipping, reduced guest clocks, disabled sound, removed cache
semantics, game-specific BIOS bypasses or ordinary triangle/alpha approximations
as evidence of meeting this real-time target. They change the workload or output
whose performance the user wants improved.

## Iteration and acceptance

Use the existing native and Web benchmark tools for their supported workloads.
[benchmark-runtime.mjs](../scripts/benchmark-runtime.mjs) accepts an original guest
or optional BIOS and local disc media. Supply CUE tracks explicitly with
repeatable `--track` arguments and record a deterministic controller timeline
before treating a result as a reproducible game benchmark. Include the production
audio consumer when claiming continuous audible playback; draining PCM alone measures a narrower workload.

For every iteration:

1. Record source revision, artifact hash, compiler/optimizer, browser and device,
   boot mode, original guest or private local input identifier, input timeline,
   warmup and measured field range. Keep private data and identifying paths out
   of public results. Exercise startup, steady gameplay and a heavier transition.
2. Alternate baseline/candidate order over matching windows with competing
   builds/tests stopped. Report total-fields/total-time throughput, p50/p95/p99
   field times, maximum stalls, delivered audio and underflows. Use enough fields
   for tail statistics; a short window cannot establish stable p99 latency.
3. Compare CPU, caches, clocks, IRQs, RAM, framebuffer and PCM at meaningful
   boundaries. Exposed browser snapshots supplement original guests with
   independently predicted outputs and the existing full-state native oracles.
   Retain tests proving that optimized paths execute and that broken guards fail.
4. Repeat apparent gains in fresh browser instances. Retain a change only when
   its reproducible benefit justifies its complexity and measured workload
   tradeoffs; report regressions explicitly. Do not multiply gains from different sessions into a combined claim.
5. Run affected checks, then the applicable repository native/Wasm/browser suite
   for the selected implementation. Keep #138 open until defined representative
   workloads sustain their real video rate with presentation and audio; keep
   hardware-completion limits under #17 separate. Normal review and exact-head
   CI remain required for implementation PRs.

Private BIOS and game files are available locally for the requested supplemental
measurements. Original licensed guests remain the reproducible correctness and
CI basis. This research does not establish general BIOS/game compatibility.
Required post-merge verification: none.

## Executed-core follow-up

The follow-up from main `87a18a9` builds and runs the pinned Beetle Saturn
revision above on the same host and authorized local game, with JIT disabled.
See the [measurement record](runtime-performance.md#follow-up-from-main-87a18a9)
for measurements and limits. This is executable reference evidence rather than
an inferred performance claim from source inspection.

Further inspected mechanisms in that revision are:

- An event-bound interpreter dispatches the master and resumes the slave at
  memory/issue boundaries. It does not construct our per-burst speculative
  CPU/cache/protocol journal. The cached minimum event deadline and specialized
  memory handlers keep common execution in a short loop.
- Data-only and full instruction-cache emulation select separate interpreter
  entry points. Results with the default data-only mode must not stand in for
  a full-cache comparison. [Execution loop and cache selection](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/ss.c).
- A two-level instruction-class table deduplicates 16-entry rows. Four-way
  cache tags are stored together and compared with SSE2 or NEON where available.
  Those native intrinsics need a separately measured Wasm implementation.
  [Decode and memory implementation](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/sh7095.inc).
- The master can fuse compare/conditional-branch pairs, load/compare/branch
  triples and NOP delay slots. These macros are guarded by `_LOW_ACCURACY_` and
  excluded from the slave's resumable interpreter. Their state/clock formulas
  do not by themselves prove equivalence to our observer boundaries.
  [Fusion implementation](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/sh7095_ops.inc).

The transferable priority is to amortize common execution and synchronization
without repeatedly reconstructing admission and rollback state. The native/Web
measurement of our own implementation helps distinguish that work from host
backend cost. Renderer threading, reduced cache fidelity, fused dispatch and
compiler differences remain explicit comparison variables; none establishes a
portable Web speedup on its own.

The branch-admission/protocol-snapshot prototype is not adopted: after correcting
the benchmark's initial preparation asymmetry, its apparent gain changes sign
when initialization/execution order is reversed in a fresh browser. The final
change retains the measurement correction and native game benchmark support;
its emulator core remains identical to main `87a18a9`. The executed reference
and native profiles justify further structural investigation, not an invented
speedup or a claim that the Web 60 fields/s target is met.

## Detailed Beetle audit, 2026-11-14

This audit compares main `2790789` with the same pinned Beetle revision,
`30f4c12072d265c9e553dcb29386059114945dea`. It investigates how to reduce
equivalent work, rather than treating its approximately 189 native fields/s as
a forecast for this Web implementation. No emulator change is adopted here.

### Execution without speculative restart

The full-cache master loop runs one instruction, catches the slave up, and
compares the effective timestamp with a retained global event deadline. Memory
operations can also hand control to the slave before completing a shared-bus
access. The slave records a numeric resume location and live operands when it
reaches the supplied bound. On its next call, a switch returns directly to that
location. The inspected non-debug resume table has 392 entries. This is a
switch-based continuation, not a requirement for a native JIT or computed goto.
[Master loop](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/ss.c),
[resume implementation](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/sh7095s_rsu.inc),
[resume points and saved operands](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/sh7095.inc).

Our ordinary path clones the CPU and re-enters the interpreter through a bus
journal; the cached path amortizes that cost with bounded private execution,
cache versions, retirement records and publication/recovery. Replacing only a
rare ordinary fallback will not remove the cached path's dominant bookkeeping.
The larger experiment is a resumable execution path coupled to exact bus/device
boundaries, so common execution can publish effects directly without needing
the same speculative batch setup and reconstruction.
[Our ordinary boundary](../src/lib.rs), [journals](../src/bus/transactions.rs),
[cached execution](../src/bus/cached.rs).

Start with one memory-instruction family and retain the existing engine as an
independent fallback/oracle. A resume record must retain the accepted bus phase,
sampled value, register-write stage, IF/delay-slot state and exception/interrupt
eligibility. Do not repeat an accepted MMIO access or commit a younger register
effect before its real boundary. This is the highest architectural opportunity,
but a substantial implementation project rather than a small dispatch patch.

### Predecode lifetime, fast memory and cache layout

Beetle retains the instruction's raw word and decoded operation class together
in `Pipe_ID`; dispatch consumes that class rather than repeating a literal
decoder. Its class lookup has a two-level 4,864-byte representation instead of
a flat 65,536-byte table. The useful idea is the lifetime of decoded information,
not an assumption that table compression is always faster.
[Decode and dispatch](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/sh7095.inc).

We already have scalar and timing tables, prepared operations and retained IF
words. A distinct experiment would retain a compact execution class with a
ready fetched word and reuse it across preparation/dispatch. Check the actual
`Cpu`/fetch-value layout before adding fields: larger rollback copies can cancel
the saved classification work. Bind plans to CPU-visible cached/retained words,
not merely the current contents of RAM at a PC.

Beetle's external-memory table maps 64 KiB address regions to native base
offsets, while specialized handlers retain cache, bus and on-chip behavior.
Tags and data use separate arrays, with four aligned tags in each row; native
SSE2/NEON compares the four ways together. The two-way mode and invalid-tag
convention remain part of lookup. A Web implementation would use checked linear
memory offsets and optionally Wasm SIMD, not copy native pointers or intrinsics.
[Mapping](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/ss.c),
[cache representation](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/sh7095.h).

One narrow opportunity in our implementation is more specific than a wholesale
memory-map rewrite: once `CachedLines` contains any shadow version, its lookup
scans the selected set even when that particular set has no shadow version.
The base cache's recent-hit hint is then unused for unchanged sets. Prototype
falling back to the base lookup when that set's version head is zero; retain
the current shadow lookup for changed sets. Verify duplicate-tag priority,
two-way mode, purges, retained code and store forwarding against the current
cache oracle. This is a small, testable experiment, not a demonstrated gain.
[Base lookup](../src/cache.rs), [shadow lookup](../src/cache/shadow.rs).

### Drawing: incremental arithmetic and specialized kernels

VDP1 uses error accumulators for edges, texture position and Gouraud shading.
Its inner stepping advances with additions/comparisons rather than recomputing
a division for every dot. It also selects specialized drawing functions from
the command's modes. Our `texture_coordinates` computes scaling/vertical
interpolation divisions per dot and is used by acquisition and drawing.
[Beetle stepping](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/vdp1_common.h),
[our sampling](../src/vdp1.rs).

A practical first prototype is a line-local quotient/remainder accumulator for
our existing sampling formulas. Derive it from our current semantics, including
reduction centers, expansion ties, reversed traversal, clipping, high-speed
shrink and live control changes. Do not substitute Beetle's sampling formulas
as a correctness oracle. Exhaustive small-line comparisons and the independent
literal vectors in [VDP1 sampling](vdp1-sampling.md) can test arithmetic reuse
without changing drawing timing or VRAM access order.

VDP2 keeps renderer-side memory updated by write commands, converts changed
CRAM entries into a 2,048-entry color cache, and selects normal-background
functions by bitmap/color/transparency/priority/color-calculation modes. The
normal-background dispatch has 240 combinations, moving many decisions out of
the pixel loop. The 4-bit cell helper can decode/gather eight pixels together
with NEON when the palette range permits it. Single-thread mode consumes the
same commands synchronously; a render thread is not required for these gains.
[Renderer implementation](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/vdp2_render.c).

Our display acquisition deliberately records per-dot/path operands and VRAM
timing, which differs from simply reading the renderer's current memory. Keep
that acquisition behavior. Reuse derived address/format plans across unchanged
fetch groups, specialize a few observed hot formats, and cache decoded palette
values with the correct palette mode and color-calculation MSB. Preserve the
existing captured-word lifetime across mid-line writes. We already cache linear
tile runs, so another cache of only the same tile address is not a new design.
Generating the entire native dispatch matrix could inflate Wasm compilation and
instruction-cache costs; start with measured hot modes and a general fallback.
[Acquisition](../src/vdp2/fetch.rs), [sampling/composition](../src/vdp2.rs).

### Sound: distinguish faster organization from coarser observation

`RunSCSP` calls a complete sample renderer and advances its next-sample counter
by 256. The sample function runs DSP and voice loops together. Sound-CPU RAM
instruction fetch has a direct RAM path; its SCSP catch-up call is commented
out, while ordinary data accesses service the sample deadline. These are
concrete scheduling differences from our distributed sound model, not just
different spellings of identical hot loops.
[Sound scheduling/fetch](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/sound.c),
[sample renderer](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/scsp.inc).

Our path advances 128 DSP slots per sample, distributes voice work, and models
RAM grants, completion, resets and MIDI/interrupt edges. Its sound checkpoint
also saves synthesis/CPU state and records undo data. Copying Beetle's whole
sample schedule would change those observations. The transferable experiments
are direct RAM data routing after proving the existing grant/completion, batching
slots only inside a proven observer-free interval, and saving only changed
synthesis/CPU components. Preserve real DSP/sample work rather than suppress it.
The earlier 17.68% inclusive checkpoint profile contains sound execution; it is
not a claim that eliminating copies alone removes 17.68% of runtime.
[Our scheduling](../src/sound.rs), [slots](../src/sound/pcm.rs),
[checkpoint](../src/sound/checkpoint.rs).

There is also a more local opportunity: Beetle decodes dirty MPRO words into
step descriptors and performs a liveness pass, preserving successors needed to
complete delayed RAM operations. Our DSP decodes the four program words on
each live step; its existing zero-program/zero-step optimizations are narrower.
Start with predecoding only, preserving every current ALU effect and fault at
its original execution edge. Invalidate the written row immediately, including
partial-word writes. Keep coefficients, addresses, RAM and delayed state live.
Do not copy whole-sample invalidation or dead-step elimination without proving
behavior when the next MPRO step changes during a sample.

The descriptor cache must not make every sound checkpoint larger. For example,
an immutable shared descriptor array with copy-on-write on MPRO modification
can preserve rollback while avoiding a second full per-checkpoint array copy.
This is a design candidate, not a measured implementation. General dead-step
elimination additionally needs correct live-out state for future program
changes, slot/port timing and execution-time faults.
[MPRO decode/liveness](https://github.com/libretro/beetle-saturn-libretro/blob/30f4c12072d265c9e553dcb29386059114945dea/mednafen/ss/scsp.inc),
[our live decoder](../src/sound/dsp.rs).

### Reference ablations and work counts

Four counterfactual builds remove one mechanism at a time from the conservative
reference configuration: full instruction-cache emulation, synchronous software
rendering, JIT off, dispatch fusions off and zero interleave quantum. Production
Rust/Wasm code is unchanged. Native runs use the same host, private input and
controller timeline; boot 3600 fields before six continuous 240-field windows.
Each comparison runs separate processes in A/B/B/A order, with A the unchanged
reference and B the counterfactual. A fresh private save-directory copy is used
per process. Audio, video and serialized-state checksums agree at every matching
window, including the padded 6,323,164-byte save-state checksum. Hash agreement
is supporting evidence, not a proof of complete hardware equivalence.

| Mechanism removed | Reference fields/s | Counterfactual fields/s | Counterfactual change |
| --- | ---: | ---: | ---: |
| Explicit four-way SIMD tag path, retaining the scalar fallback | 192.926 | 188.220 | −2.44% |
| Cached event-minimum update, recomputing the same minimum every update | 196.162 | 200.158 | +2.04% |
| Compressed decode table, using the equivalent flat table | 198.952 | 206.127 | +3.61% |
| Direct sound-RAM instruction fetch, using the existing general read helper | 196.804 | 188.091 | −4.43% |

These are exploratory native ablations, not predicted Rust/Web gains. The
compiler may still optimize/vectorize the scalar fallback; this is a source-path
comparison, not an instruction-by-instruction ISA isolation. The
event-minimum pair shows substantial run-to-run drift; no benefit from removing
that cache is established. A smaller decode table is not an automatic win in
this workload. Two reversed-order B/A/A/B repeats give:

| Mechanism removed | Repeated reference fields/s | Repeated counterfactual fields/s | Counterfactual change |
| --- | ---: | ---: | ---: |
| SIMD tag match | 196.660 | 201.012 | +2.21% |
| Direct sound-RAM instruction fetch | 193.272 | 195.291 | +1.04% |

The signs reverse relative to the first runs. Consequently these native trials
do **not** establish reproducible speedups for either small fast path. All
matching audio, video and serialized-state checksums still agree. The source
mechanisms remain candidates for targeted Rust/Web experiments, not adopted
gains. The flat-table and event-minimum observations are exploratory and not
independently confirmed. None of these few-percent observations explains the
approximately eightfold native gap; copying small source optimizations blindly
is not supported by this study.

An instrumented build separately counts fresh master/slave instruction entries,
samples, minimum reductions, cache probes and sound-RAM instruction fetches.
The instrumented endpoints match the uninstrumented reference, including its
serialized-state checksums; its elapsed times are excluded. Raw inputs, save
states, images, patches and binaries remain under ignored local build output.

For the exact common interval from field 3680 to 3760, counting our existing
native benchmark endpoints and the separately instrumented reference gives:

| Work per field | Our core | Beetle reference |
| --- | ---: | ---: |
| Master SH-2 instructions/normal instruction entries | 336,421.29 | 344,043.96 |
| Slave SH-2 instructions/normal instruction entries | 192,516.01 | 84,225.75 |
| Generated stereo sample frames | 737.1375 | 737.1375 |

Our counters report completed instructions; the reference counts fresh normal
instruction entries, excluding standby/DMA/exception pseudo-dispatch classes
and resumed entries. Faults and interval-boundary instructions can differ.
These are diagnostic work counts, not a cross-core retirement oracle. The
master workload is of similar magnitude, while the slave does substantially
different work. Both reach the same visible selection scene, but their CPU/bus
timing models and internal states are not equivalent. This does not authorize
skipping slave instructions or assert a defect in either core.

The reference additionally performs about 376,292 cache probes and 21,700
event-minimum reductions per field in this interval. This argues for inspecting
the cost of each common boundary, not assuming a fast core avoids synchronization
entirely. Its sound-CPU instruction-fetch count is not directly comparable with
our completed-instruction counter. Matching generated sample rates also does
not equate the distributed DSP/voice observation work described above.

### Implementation order and acceptance

Use three stages rather than promise one large multiplier:

1. Narrow experiments: preserve the base cache hint for unchanged shadow sets;
   replace repeated VDP1 divisions with exact incremental arithmetic; retain
   decoded execution classes for the lifetime of fetched words.
2. Kernel experiments: reuse captured-group plans and specialize hot VDP2 modes;
   predecode live DSP words without increasing checkpoint copies; investigate
   observer-bounded sound slot batching and narrower state saving.
3. Architectural experiment: a resumable memory-instruction path coupled to
   exact shared-bus/event boundaries, expanding only after independent original
   guests establish effects, clocks, IF/MA order and interrupt/reset behavior.

For each, count removed work first, then measure uninstrumented production Wasm
with both initialization orders and an identical-artifact control. Require the
existing native/Wasm/browser oracles and negative controls for the changed
boundary. Keep general fallbacks. Reference timing/visibility approximations
are not permission to weaken this project's established observations. The
approximately 19 fields/s Web scene still needs roughly threefold throughput;
no measured Web speedup or real-time completion is claimed by this research.

## Implementation experiments following the Beetle audit

The implementation increment uses main `5ff3de3` as its baseline. It selects
two bounded changes from the audit, preserving the existing event/bus model:

- Ordinary scalar retirement reuses the classification already present in the
  instruction's timing descriptor. It adds no CPU/fetch fields or decode table.
- VDP1 retains an exact texture quotient/remainder cursor per raster line.
  Acquisition and drawing reuse its coordinate. Reversed pre-clipping uses a
  signed increment with Euclidean remainder; reduction centers and expansion
  ties remain the existing formulas. V is constant within these raster rows.
  HSS parity still reads live draw control at each query. Transparent, clipped
  and mesh dots advance the cursor; completed end-code rows discard it.

Frozen production Wasm SHA-256 fingerprints:

| Artifact | SHA-256 |
| --- | --- |
| Baseline | `778be960b1243209bdf90a16219dd3c63e9713c72cd290ab63ccfeaaf8c86532` |
| Final classification/texture candidate | `0920c35a9f27c2d06e5a4da816c3dda5a024b75c8ff263284f3487c803fb685e` |

The VDP1 numeric comparison retains the original division kernel only in tests.
It checks all 63 nonzero character widths, small and extreme spans, forward and
reversed traversal, clipped starting phases, both flips and both live parities.
Existing literal Figure 6.5 rows, physical-word capture, contention, pre-clipping,
stop/reset and pixel-publication guests remain the independent boundary checks.
This changes host arithmetic, not emulated clocks or sampling rules.

### Measured selection and rejected implementations

All trials use production release Wasm, Rust 1.100.0, wasm-bindgen 0.2.129,
Binaryen 127 and local headless Chrome 157.0.8103.52. Corresponding intervals
must match every benchmark CPU/counter/PCM/pixel endpoint. Tests and compilation
stop before throughput measurements. All runtimes finish preparation before
either timed window, as corrected in PR #140.

Short original-workload screens used five warmup fields and four continuous
30-field windows. They are diagnostic: extending warmup to 120 fields with six
60-field windows removes an apparent cache win in the simple store loop
(+2.64% becomes −0.01%). The longer-warmup identical-artifact controls measure
−0.03%, −0.22% and −0.12% in the loop, busy-sound/video and normal-bitmap scenes.
Use the longer method for selection rather than claim the short-run difference.

The cache candidate improves a source-authored refill loop by 1.62% in its
initial long-warmup comparison, but changes −0.43% when initialization is
reversed; its identical-artifact control changes +0.21%. No reproducible cache
speedup is established, so that implementation is not retained.
The CPU classification reuse improves
the long-warmup loop by 0.97%; loading the candidate first in a fresh browser
also improves it by 1.96%. These component observations are not additive
predictions for the game. Final component repeats, final game comparisons,
artifact fingerprints and exact-head CI are recorded in
[Issue #138](https://github.com/takahirox/saturn-rust/issues/138).

Several plausible implementations are deliberately absent from the selected
production source:

| Trial | Observation and selection |
| --- | --- |
| Base-cache lookup for unchanged shadow sets | The refill-loop effect reverses sign under reversed initialization; no reproducible gain was established. |
| Predecoded SCSP instructions in an immutable shared program | Numeric, live-write, fault and snapshot checks passed; the short busy-sound/video screen slowed 1.65%, while normal bitmaps were effectively flat. No consistent gain was established. |
| Share only raw MPRO with copy-on-write | The short busy scene changed −0.59%, normal bitmaps +0.85%; no consistent workload benefit justified the extra ownership machinery. |
| Hoist VDP2 bitmap row/wrap/format arithmetic | Existing literal-fetch comparisons passed; the normal-bitmap screen changed −0.06%, providing no measured improvement. |
| Incremental screen coordinates and Gouraud values in addition to texture coordinates | Exhaustive signed interpolation and existing drawing tests passed, but a long-warmup shaded scene slowed 3.44% versus texture stepping alone (51.352 → 49.583 fields/s); the ordinary busy scene was flat (52.049 → 52.006). The extra per-dot cursor state is not retained. |

The last result is a concrete limit on transferring Beetle's raster organization:
reducing divisions does not ensure a faster persistent per-dot implementation.
These timings do not establish the exact machine-code cause. A future larger
observer-bounded kernel could organize its local state differently, but is not
an adopted speedup here. Likewise, a full resumable CPU/bus redesign remains
separate from this increment's reuse of existing decoded metadata.

The first cache/texture-only game trial improved 18.907 → 19.294 fields/s
(+2.04%); a five-mechanism exploratory bundle improved 19.581 → 19.973 (+2.00%).
Every matching endpoint agreed. Neither is the selected final artifact: the
first still had a production division fallback, and the second included the
subsequently rejected DSP/VDP2 trials. Final adoption requires both game
initialization orders and an identical-artifact control on the selected source,
with Canvas and audio draining enabled. The defined scene remains far below
real-time throughput; #138's broader gameplay and audio-underflow criteria stay
open. Private media, raw traces, captures and experimental builds remain local.

## Resumable kernel experiment (#145)

The user authorized trying a new execution architecture after the production
optimizations above. The first vertical slice is `kernel-experiment`, an opt-in
Cargo feature. Default native/Wasm builds continue to use `Machine`; there is
no product engine selector. The rebuilt default production Wasm is byte-identical
to the preceding selected artifact (`0920c35a…85e`); experimental declarations
are appended to keep existing diagnostic source positions stable. This increment
does not replace the game core or complete #138/#17.

`src/cpu/resumable.rs` owns a CPU's architectural state and a fetch, operand or
retirement continuation. Normal execution never clones the CPU, retries an
instruction from its beginning or rolls back a completed device access. Only
validated arithmetic, MOV decoding and branch semantics are reused from the
existing interpreter. `src/kernel.rs` owns both CPUs, RAM, pending requests,
one active transfer, replies and the earliest-event loop. Fixed CPU/bus slots
avoid per-instruction allocation; optional diagnostic traces allocate only
when explicitly enabled. Setup, reset and benchmark verification may copy state.

The first board intentionally implements a **serial synthetic test contract**:
fetch transfer, optional operand transfer, then the instruction's base execution
states. It has 64 KiB of RAM and a synthetic side-effecting register, not a
Saturn address map. Transfer latency is configurable, including zero. Requests
are granted round-robin, with master first initially. Reads sample and writes
publish at completion. External writes/resets occur before a completion at the
same timestamp. Granted transfers drain once across CPU reset, while an epoch
rejects stale delivery and queued requests are canceled. `run_until(T)` executes
the half-open interval ending at T, preserving arbitrary split-run equivalence.

This contract establishes ownership and continuation behavior; it does **not**
model SH-2 IF/ID/MA overlap, load interlocks, instruction caches, BSC wait rules,
write buffers, interrupts, exception entry, DMA or Saturn peripherals. Ordinary
register operations, MOV addressing, branches/delay slots, MOVA and SLEEP are
supported; other instructions or invalid addresses stop with a retained explicit
error. Even a complete synthetic guest is not evidence of Saturn timing or
media compatibility. A fetched opcode is retained from its completion-time
bytes; there is no RAM-PC instruction cache that could hide later writes.

### Correctness and measurements

Original numeric tests check completion-time sampling, retained read/opcode
values, exactly-once side effects, reset during granted reads/writes, cancellation
of queued requests, a peer's store/read order and arbitrary one-clock splits.
Additional original guests compare architectural state and RAM with `Cpu::step`
at retirements, covering ordinary MOV widths/addressing and source/destination
aliases, branch targets/links and PC-relative delay slots. Shared arithmetic
helpers are not independent arithmetic or hardware-timing proof.

The optional native example and `scripts/benchmark-kernel.mjs` compare two
original six-instruction loops: register/branch work, and load/increment/store
work on separate data words for the two CPUs. Each variant retires exactly
5,000,000 instructions per CPU and drains before comparison. The comparison is
the existing **ISA interpreter with immediate plain RAM**, not the production
`Machine` or an equivalent timed Saturn board. It isolates the cost of this
new scheduler versus the simpler architectural interpreter; it does not
measure removal of production replay or establish a game speedup.

Rust 1.100.0 release, wasm-bindgen 0.2.129, Binaryen 127 and local headless Chrome
157.0.8103.52 were used. Both paths are warmed before either measurement, setup
and verification are outside timing, and six rounds alternate execution order.
The following are medians for 10,000,000 total retired instructions:

| Host/workload | Continuation kernel | Immediate ISA reference | Kernel million instructions/s |
| --- | ---: | ---: | ---: |
| Native, register loop | 120.002 ms | 46.224 ms | 83.33 |
| Native, memory loop | 172.511 ms | 62.588 ms | 57.97 |
| Wasm main thread, register loop | 182.600 ms | 64.200 ms | 54.76 |
| Wasm main thread, memory loop | 231.150 ms | 80.200 ms | 43.26 |
| Wasm Worker, register loop | 182.700 ms | 64.150 ms | 54.73 |
| Wasm Worker, memory loop | 231.400 ms | 79.800 ms | 43.22 |

All state/RAM comparisons pass. Endpoint digests also agree between native,
browser main thread and Worker across all rounds. A shorter fresh-browser run
measured 54.27/42.11 million instructions/s in the Worker. The longer Worker run
completed with `crossOriginIsolated=false`, one ordinary Wasm instance and 337
main-thread timer ticks. This establishes a functioning independent Worker,
not a measured production UI-latency or audio-underflow result. Moving to a
Worker alone does not increase execution throughput in these observations.

The experimental Wasm SHA-256 is
`94e84d88b3379595fc4283875a16e70e09b3612ba24693d42e0c2b2e4496228b`.
Raw local records remain under ignored `target/kernel-138/`.

The kernel takes about 2.8–2.9 times the immediate reference's time in Wasm.
Eliminating replay is therefore insufficient by itself: explicit bus and
retirement event dispatch still has a substantial cost, even on simple loops.
Keep this as an experimental foundation, not an adopted production speedup.
Do not convert these synthetic clocks or instruction rates into game FPS.
The existing game's roughly 20 fields/s result remains the relevant game
measurement; this increment provides no new game FPS or 60-fps claim.

### Reproduction and integration order

Run the original native experiment with:

```sh
cargo run --locked --release --features kernel-experiment --example kernel_benchmark -- 5000000
```

For the browser harness, build a separate feature-enabled package (leave the
default `scripts/build-web.sh` package untouched):

```sh
RUSTFLAGS='-C llvm-args=--inline-threshold=1000' cargo build --locked --release --target wasm32-unknown-unknown --features kernel-experiment
wasm-bindgen --target web --out-dir target/kernel-138/pkg --no-typescript target/wasm32-unknown-unknown/release/saturn_rust.wasm
wasm-opt target/kernel-138/pkg/saturn_rust_bg.wasm -O4 --enable-sign-ext --enable-mutable-globals --enable-nontrapping-float-to-int --enable-bulk-memory --enable-reference-types --enable-multivalue -o target/kernel-138/pkg/optimized.wasm
mv target/kernel-138/pkg/optimized.wasm target/kernel-138/pkg/saturn_rust_bg.wasm
BROWSER_CHANNEL=chrome node scripts/benchmark-kernel.mjs --pkg target/kernel-138/pkg --count 5000000 --runs 6
```

Next, replace the synthetic contract with the real SH-2 fetch/cache/pipeline and
shared-bus boundaries. Add register-only execution spans bounded by the earliest
possible peer/device effect, using decoded CPU-visible fetched bytes. Measure
event exits and host work removed, not merely larger block sizes. Preserve
the existing literal timing, contention, interrupt/reset and self-modifying-code
oracles before connecting original board guests. Attach existing peripheral
implementations through direct timed events; wrapping the old snapshot/replay
`Machine` would not complete this redesign. Full instruction and exception
coverage is required before authorized local game comparisons become meaningful.

The browser destination remains one Emulation Worker owning both SH-2s,
68000 and devices in one Wasm instance. The UI presents completed output; the
existing AudioWorklet consumes bounded PCM, with real consumption/underrun
measurements. Ordinary transferable buffers remain a viable fallback; shared
buffers are optional. Start with CPU video kernels and GPU presentation, then
evaluate safe rendering spans. Consider a hot-block Wasm compiler only after
the interpreter/event profile justifies its compilation and invalidation costs.
Adoption still requires production Web game throughput, tail latency, audio
and broader gameplay evidence under #138, without frame skipping or changed
guest timing. These integrations are remaining work, not implemented here.

A separate ten-second native mixed-path sample also locates substantial host
work in continuation/event dispatch: 3,361 self samples in `Executor::poll` and
1,483 in the inlined kernel loop, versus 710 in shared scalar execution across
both kernel and reference paths. These are diagnostic self-sample counts, not
Wasm percentages or additive inclusive timings. They support investigating
event-exit frequency; raw process metadata/stacks remain local.

## Owned cache read transport (2026-11-15)

Related to #138 and #17. The next integration step adds `kernel::ReadPort` under
the existing test/`kernel-experiment` boundary. It owns an actual SH7604 `Cache`,
the retained instruction halfword and one pending read. It has no `Clone`
implementation or CPU-instruction replay log. This component is separate from
`Kernel`'s still-synthetic CPU execution schedule; CPU IF/ID/MA integration is
not complete and this is not a new game engine or a measured game speedup.

The supported surface is the canonical 1 MiB low/high WRAM windows and their
cache-through aliases. Aligned byte/word/longword data reads and aligned-word
instruction fetches use the existing cache's enable/allocation/two-way controls,
victim selection, tags and LRU rules. A miss emits four real longword requests
in the existing wrapped order, with the requested word last. A private 16-byte
continuation collects completion values; the line becomes visible only after
all four complete. Installation calls the original cache implementation with
these already completed local bytes, without repeating physical bus reads.
Retained instruction halves issue no extra cache read or LRU touch.

Owner/sequence tickets bind each completion. Reset or duplicate replies cannot
install an old line or complete a newer request. The real-board adapter also
rejects changed transfer addresses, widths, instruction flags and grouping
before touching the bus. The transport assigns no artificial latency: its
adapter uses the production arbiter/BSC path and physical read completion.
WRAM exercises width/timing; active endian conversion is outside this port. The existing physical-beat journal is retained only for an
unfinished logical byte/word/longword unit and cleared after that unit succeeds;
CPU decoding and earlier completed refill longwords are not replayed.

### Evidence and limits

All 18 kernel tests pass, including seven new port cases. The shared original
fixture checks 900 reads across both CPU identities, cache controls, physical
8/16/32-bit buses, BCR1 area-2 endian-flag controls and chunk
limits, then two simultaneous CPU cache fills and an instruction-fetch/operand
stream that shares one transaction boundary. Each value and each CPU's
completion clock matches the existing production memory path. Additional cases
compare full cache contents/replacement state, partial-line visibility, retained
halves, bypass/purge behavior, stale replies and malformed transfers.

The fixture returns `[904, 3497650377]` on native, browser main thread and Worker.
The existing original CPU experiments also preserve their architectural digests
in both browser contexts. Worker progress occurs without cross-origin isolation.
These are component/experimental checks, not production-game timing acceptance.
Native all-target and Wasm Clippy with all features and warnings denied pass.
The native example uses `CARGO_PROFILE_RELEASE_STRIP=none` locally because the
installed `rust-objcopy` cannot load its LLVM library; its conformance output
succeeds. Native microbenchmark times from that check are not adopted as gains.

The default production JS/Wasm remain byte-identical to main `86e55e3`; Wasm
SHA-256 is `464639fa99636abb0a2682c09a6cad87f01d94f6abd5d8f6af71d73fe2f09548`.
An initial placement of the adapter preserved executable sections but changed
panic-source line constants. Moving the gated adapter into its own module,
with its declaration appended after existing source, restores full identity.
The feature-enabled browser artifact used for the final local check is
`3ddcd29778eaf6925af15e6508aebea84556e3ea5331e803f94a2fa73595bbf8`.
PR #150 passed exact-head CI `43626150297` and final author self-review, then
merged as `3115cd2`. No independent approval is asserted.

Writes/buffered acceptance, BIOS and non-WRAM devices, cache arrays, CPU pipeline
and exception/interrupt behavior, DMA and coordinated board-reset integration
remain pending. Range/alignment failures here are prototype host errors, not
synthesized SH-2 exceptions. Port ticket cancellation is tested; it does not
establish complete board reset-drain behavior. Existing granted physical work
remains the board's responsibility.

The next step is to connect real IF/ID/MA continuations and write-buffer
acceptance, then integrate the peripheral timeline without whole-instruction
CPU copying/replay. The default game remains around 21 fields/s. Sustained Web
60, broader gameplay/transitions and production audio-underflow acceptance stay
open, as do #138/#17. No required post-merge verification is added.


## Actual-board CPU continuations and private spans (2026-11-15)

Related to #138 and #17. Building on PR #150 (main `3115cd2`), the opt-in
`TimedExecutor` now connects IF/ID/EX/MA to the owned cache/read transport and
production arbiter/BSC. It latches scalar, ordinary MOV-load and branch effects
once; waits resume their phase without CPU cloning or instruction decode replay.
Branch target IF, retained halves, delay-slot loads, cache refills and load
WB/EX dependencies retain the established stage conventions in
[the fetch contract](sh2-fetch-pipeline.md) and
[the MOV-load contract](sh2-load-pipeline.md).

`TimedExperiment` owns both CPUs and one actual board. Private cache-only work
can advance together before the next observable event. A miss stops that
continuation; board time catches up before issuing any physical request.
Admission excludes active transfers/buffered writes, interfering DMA/on-chip
states and non-reference/pending PLL ticks, and stays strictly before existing
on-chip/interrupt/raster/SCU/VDP1, sound, CD and SMPC horizons. Ordinary catch-up
still runs device edges. Private retirement epochs publish in one count per CPU;
a resident scalar path avoids moving a latch through multiple phase tags.
This does not add a speculative CPU snapshot, rollback journal or artificial
bus latency. Range/alignment/unsupported failures remain terminal prototype
host errors; they do not implement architectural exception entry.

### Correctness and reproducible component comparison

All 893 library tests pass, including six new native cases. The shared fixture
compares 4,360 instruction
retirements against the production replay interpreter: full CPU state, completion
clock and next-issue clock match across both CPU identities, allocation controls,
physical widths/waits, P1 aliases and both halfword alignments. It covers competing
CPUs and taken/untaken conditional, delayed, relative, indirect, call and return
branches. The digest is `[4360, 829066179]` on native, browser main and Worker.
Additional cases preserve inherited literal cold/P1 NOP edges, early refill bytes,
the requested-word-last sample, decoded successors and terminal failure behavior.
Removing retained MA/EX overlap deliberately fails the shared oracle; restoring
the exact source passes it.

Original actual-board ALU/load loops also match the **normal production Machine**,
including its cache batches and fallback, for single/both CPUs and cache/P1
paths. Full architectural state, main/reference clocks, field count, generated
sample count and delivered PCM agree. Two successive fields and 37-clock host
splits pass; a separate streaming-load/second-code-line case verifies yielding
at misses. Private fixtures/media are not needed or published.

For the read-only loop results below, use revision `70ce4b1` (PR #151).
Build that feature revision and run its matching script:

```sh
BROWSER_CHANNEL=chrome node scripts/benchmark-timed-kernel.mjs \
  --pkg path/to/feature-pkg --fields 2 --runs 6
# Fresh browser, reverse the execution-context order:
BROWSER_CHANNEL=chrome node scripts/benchmark-timed-kernel.mjs \
  --pkg path/to/feature-pkg --fields 2 --runs 6 --worker-first
```

An independent, unmeasured normal-Machine control fixes each workload's final
main clock. Both measured variants start initialized and cold, warm in a
separate instance, alternate order, and settle zero-time CPU issue at that final
edge. This handles the normal frame runner returning before/after issue or a
clock after a field edge; no guest clocks are rounded or ignored. PCM/state
checking is outside timing. Six fresh rounds per workload/context compare the
same interval; these are repeated original fixtures, not twelve continuous game
fields. No builds, tests or profiles overlap preparation or throughput.

### Measured behavior and remaining limits

Rust 1.100.0, wasm-bindgen 0.2.129, Binaryen 127, Chrome 157.0.8103.52; feature
Wasm SHA-256 `28a50260c5f223d3fb3d4bf5f550cc5bf8c70d4ff229c4747dd80eb3839fe474`.
Each ratio below is normal-Machine total time divided by continuation total time
for six Worker rounds; values below one are regressions.

| Original loop | CPUs | Fetch/data route | Main-first run | Fresh Worker-first run |
| --- | --- | --- | ---: | ---: |
| ALU | Master | Cache | 0.913× | 0.919× |
| ALU | Master | P1 bypass | 1.363× | 1.382× |
| ALU | Both | Cache | 7.647× | 7.484× |
| ALU | Both | P1 bypass | 1.564× | 1.564× |
| MOV load | Master | Cache | 0.805× | 0.784× |
| MOV load | Master | P1 bypass | 1.277× | 1.283× |
| MOV load | Both | Cache | 5.635× | 5.609× |
| MOV load | Both | P1 bypass | 1.497× | 1.512× |

The initial unbatched continuation improves both-CPU cache ALU/load loops only
1.40×/1.65× while being 6–8× slower in the single-CPU cache cases. Repeated board
exits were still dominant. The first private-span version raises both-CPU cache
ratios to about 4.45×/4.72×; its single-CPU ALU Worker sample also contains three
slower late rounds. The final resident-scalar path above improves that tradeoff
but does not eliminate all single-CPU regressions. These are actual-board
**component** comparisons against the production Machine, not game speedups.
Worker/main contexts preserve all digests; main timers progress without shared
memory or cross-origin isolation. Native/Wasm all-feature Clippy passes.
Default production JS/Wasm remain byte-identical to the adopted game baseline
(Wasm `464639fa99636abb0a2682c09a6cad87f01d94f6abd5d8f6af71d73fe2f09548`).
PR #151 passed exact-head CI `43629080418` and final author self-review, then
merged as `724840f`. No independent approval is asserted.

Writes/buffer acceptance, BIOS and devices, system/MDU instructions, interrupt/
exception/UBC acceptance, DMA and coordinated reset integration remain pending.
The default app still uses `Machine`, and the synthetic `Kernel` experiment is
retained as a distinct earlier test contract. This new component cannot boot the
private game yet. The actual game remains about 21 fields/s; sustained Web 60,
broader gameplay/transitions and production audio-underflow acceptance remain
unmet. #138/#17 and the active goal stay open. Required post-merge checks: none.


### Correction: endian coverage of the owned WRAM fixtures

The earlier read-port wording overstated active endian coverage. The
[SH7604 manual](https://antime.kapsi.fi/sega/files/sh7604.pdf), BCR1 bit12 and
§7.3.2 (printed pp.136/150), restricts conversion to CS2. Canonical WRAM is
outside CS2. The existing flag-on/off WRAM matrix checks noninterference of that
control; it does **not** exercise active little-endian transfers. The historical
904-read digest and timing results remain valid under that narrower scope.
A new store-fixture attempt incorrectly byte-swapped WRAM code when enabling
this flag, failed on the resulting unrelated opcode, and was corrected to keep
WRAM code/data big-endian. Active CS2 endian integration remains outside the
owned port's supported surface. No production BSC behavior is changed.


## Owned buffered WRAM stores (2026-11-15)

The next opt-in increment latches ordinary MOV store addresses, widths, source
payloads and predecrement updates before any buffer grant. The actual BSC's
one-entry write-buffer protocol accepts the store and owns its physical drain.
Only successful acceptance updates the issuing owned cache; a miss does not
allocate a cache line, and cache-through stores do not update that cache.
Architectural auto-update/retirement retains the original MA and load/store
slot conventions. A private span stops before a store requests acceptance. Independent cache
work can overlap an accepted WRAM data drain: it cannot observe backing RAM,
and admitted devices cannot read WRAM while DMA/UBC and observer fences hold.
Every byte still publishes at its actual board edge during catch-up. Non-WRAM
writers, locks, pending requests and interfering devices prevent admission.

The shared original matrix covers 2,560 programs / 5,120 stores across both
CPUs, cache enabled/disabled, physical 8/16/32-bit buses, waits, aliases, entry
alignment and sixteen ordinary MOV store forms. It compares full CPU states,
completion/next-issue clocks, and every changed RAM word at each clock and for
64 clocks after final retirement. The latter includes an outstanding buffer's
remaining byte publication. Self-predecrement sources, cache forwarding and
backpressure are included. BCR1 area-2 flag controls only check WRAM
noninterference, as corrected above; active endian conversion is not covered.
Native digest: `[5120, 2630414917]`. Removing accepted-cache publication fails
the expected fixture; exact source restoration passes the owned-CPU suite.
Native all-target and Wasm all-feature Clippy with warnings denied pass. The
final source also passes literal 8/16/24/32-clock high-WRAM byte publication,
non-WRAM writer rejection, queued/active UBC fences, byte-bus field endpoints
and the inherited self-modifying branch-slot target-byte expectation.

The timed original memory guest now includes MOV stores, matching the earlier
synthetic experiment's instruction sequence. Its result is a **load/store**
workload and must not be compared as the same work as PR #151's read-only loop.
Two fields and 37-clock host splits preserve the normal Machine's full CPU,
clock, field, PCM and backing-RAM endpoints in native tests. Native, main thread
and Worker match `[5120, 2630414917]` and the earlier `[4360, 829066179]` CPU
digest. Two fresh Chrome runs reverse the context order, each using six
alternating two-field rounds per workload and normal-Machine controls. Worker
main timers progress without cross-origin isolation. Builds/tests/profiles do
not overlap preparation or throughput; checks are outside timing.

Feature Wasm SHA-256:
`3c549e2c9d44c88a8edca6b0517ce2473da2a6bdfe1bc450dc617a7a1a230e8c`.
Toolchain remains Rust 1.100.0, wasm-bindgen 0.2.129, Binaryen 127 and Chrome
157.0.8103.52. Ratios are normal-Machine total time / continuation total time;
below one is a regression.

| Original loop | CPUs | Route | Main-first run | Fresh Worker-first run |
| --- | --- | --- | ---: | ---: |
| ALU | Master | Cache | 0.813× | 0.820× |
| ALU | Master | P1 | 1.312× | 1.308× |
| ALU | Both | Cache | 6.529× | 6.509× |
| ALU | Both | P1 | 1.502× | 1.501× |
| Load/store | Master | Cache | 0.423× | 0.423× |
| Load/store | Master | P1 | 1.239× | 1.228× |
| Load/store | Both | Cache | 1.856× | 1.852× |
| Load/store | Both | P1 | 1.417× | 1.421× |

Before allowing WRAM-drain overlap, both-CPU cached load/store ratios are
1.698×/1.696×; the final ratios above are 1.856×/1.852×. Candidate time drops
about 8–9% between those source variants; the normal-Machine-relative improvement
is about 9%. Single-CPU cached load/store remains substantially slower than the
normal Machine. These limits are retained; the read-only 5.6× result from PR
#151 cannot be applied to a workload with stores. No result is a game FPS claim.
All 897 native library tests, native/Wasm all-feature Clippy, formatting and
JS syntax checks pass. The final feature rebuild matches the measured artifact
byte-for-byte; default production JS/Wasm remain byte-identical to the adopted
baseline (`464639fa99636abb0a2682c09a6cad87f01d94f6abd5d8f6af71d73fe2f09548`).
Full exact-head CI and final review remain required before merge. BIOS/devices,
remaining instructions, IRQ/exception/reset integration and actual Web game
real-time acceptance remain open. The default game remains around 21 fields/s.
Required post-merge verification: none. #138/#17 and the active goal stay open.


## WRAM CPU/bus domain with bounded device catch-up (2026-11-15)

PR #152 passed complete exact-head CI `43632815198` and author self-review,
then merged as `52f225c`. No independent approval is asserted. Its default
production artifact remains the adopted game baseline.

The next opt-in increment runs the two owned CPUs and actual WRAM physical bus
within a bounded domain. Cache misses, buffer acceptance, byte publication,
shared arbitration and BSC busy/read-to-write gaps still occur at their real
main-clock edges. Independent device domains catch up once at the end of the
segment. Admission stops strictly before interrupt/on-chip/raster/SCU/VDP1,
sound, CD and SMPC observer deadlines. Non-reference PLL, pending clock work,
DMA faults/activity, reset, locks, non-WRAM transfers, replay journals and
exception/interrupt entry prevent admission. This does not enlarge guest quanta
across observations or defer visible WRAM publication.

BSC bus aging runs with the physical domain. Catch-up preserves only its final
busy/gap cell so the existing device advance does not age that cell twice;
other on-chip counters still advance. A literal zero-Tw read/write case checks
two transfer clocks plus one required idle clock, and whether elapsed physical
idle time already paid that gap. Removing restoration fails this test; restoring
the exact source passes. Debug builds also assert that the bounded interval
crosses no BSC row event. New admission tests reject DMA faults, reset, PLL
changes, queued B-bus reads and on-chip DMA.

The shared original active-device differential runs eight fields: single/both
SH-2 CPUs, live M68K NOP or SCSP software-interrupt set/clear programs, patterned
NBG0 RGB bitmap, cyclic VDP1 polygon drawing and nonzero looping PCM. It compares
full SH-2/M68K states, clocks/fields, backing RAM, delivered PCM, nonblack pixels,
VDP1 draw-buffer bytes, SCSP pending state and recognized master interrupt.
Native, browser main and Worker match `[8, 3646496445]`. SH-2 interrupt acceptance
remains masked in this fixture; it does not validate interrupt entry or game
compatibility. Inherited read, instruction and store digests remain respectively
`[904, 3497650377]`, `[4360, 829066179]` and `[5120, 2630414917]`.

All 900 native library tests and native all-target / Wasm all-feature Clippy
with warnings denied pass. The benchmark method remains six alternating
original two-field rounds, independently fixed end clocks, checking outside
timing and fresh reversed context order. No build/test/profile overlaps
preparation or throughput. Toolchain remains Rust 1.100.0, wasm-bindgen 0.2.129,
Binaryen 127 and Chrome 157.0.8103.52. Final feature Wasm SHA-256:
`301134870a947b3fd4cba61ea7091a96f94bbc7cd48459fd8e1147764a9393d0`.

Worker ratios below are normal-Machine total time / continuation total time;
values below one are regressions. These are component results, not game FPS.

| Original loop | CPUs | Route | Main-first run | Fresh Worker-first run |
| --- | --- | --- | ---: | ---: |
| ALU | Master | Cache | 0.800× | 0.811× |
| ALU | Master | P1 | 2.510× | 2.548× |
| ALU | Both | Cache | 6.626× | 6.548× |
| ALU | Both | P1 | 2.294× | 2.328× |
| Load/store | Master | Cache | 0.657× | 0.635× |
| Load/store | Master | P1 | 2.213× | 2.194× |
| Load/store | Both | Cache | 2.627× | 2.599× |
| Load/store | Both | P1 | 2.196× | 2.176× |

Both-CPU cached load/store takes 105.40 / 103.47 ms per two fields, versus
146.98 / 145.40 ms in the preceding store-overlap experiment (about 28–29% less
candidate time across these separately measured versions). Single-CPU cached
loops remain slower than the normal Machine, so no universal speedup is claimed.
Both fresh browser contexts preserve all shared digests, and main timers
progress during Worker execution without cross-origin isolation.

The final feature rebuild is byte-identical to the measured artifact. Default
production JS/Wasm remain byte-identical to the adopted baseline
(`464639fa99636abb0a2682c09a6cad87f01d94f6abd5d8f6af71d73fe2f09548`).

BIOS, remaining ISA, non-WRAM device accesses, interrupt/exception and coordinated
reset integration remain pending. The default app still uses Machine, and the
actual game remains around 21 fields/s. Sustained Web 60, broader gameplay and
production audio-underflow acceptance remain unmet. #138/#17 and the active goal
remain open. PR #153 passed full exact-head CI `43637051450` and author
self-review, then merged as `ad11e0c`. No independent approval is asserted.
Required post-merge verification: none.


### Rejected initial production-Machine domain placements

A follow-up connects the bounded WRAM domain to the existing complete Machine's
prepared scalar/branch/load/store paths, retaining ordinary instruction, device,
IRQ/exception and reset fallback. It avoids a cache-copy/CPU-state handoff while
allowing BIOS/game execution immediately. The first two placements are rejected:

| Trial | Original CPU loop | Actual selected game scene |
| --- | --- | --- |
| Domain before existing cached batches | 72.379 → 65.421 fields/s | 21.486 → 20.370 fields/s |
| Existing cached batches first | 71.971 → 71.954 fields/s | 21.458 → 21.188 fields/s (screening only) |

The first game comparison boots both variants independently to field 3600,
then alternates two successive 80-field windows with Canvas/PCM. The same
feature Wasm (`ea80e88ad294f642f4c816118cf5e3f1bd22bf10fa39b83699a1887b74e057ea`)
selects ordinary versus alternate frame methods; exposed CPU/clock/PCM/pixel
endpoints agree. The original loop uses three continuous 60-field windows after
120-field preparation. Putting the old cached batches first removes the
original-loop regression, but does not establish a game gain.

The second game row uses an ignored **screening** harness: restore the same
instance's memory/exported globals/tables, then execute ordinary/alternate
methods over the same 80-field interval six times in alternating order after
an unmeasured pair. It never restores state into a different instance/artifact.
The original guest first validates identical end states across restores/methods;
the game also preserves exposed endpoints. Feature Wasm:
`72aee443760d4886875f4e2015745576bab0e53e092567dc66082984451f27fd`.
This repeated-interval result is not continuous gameplay/adoption evidence.
Private states, media and the harness stay ignored/local; all device time still
executes. No build/test/profile overlaps preparation or throughput.

Neither placement is adopted in the default app. A subsequent investigation
retains WB/EX scalar interlocks and counts admitted clocks, completed operation
classes and reasons for returning to the ordinary Machine, separately from
uninstrumented timing. The actual default game remains around 21 fields/s and #138 stays open.

The retained-WB scalar variant also fails screening: 21.277 → 20.914 fields/s
(about 1.7% slower). Its six alternating
same-instance 80-field windows preserve all exposed endpoints, as does an extra
counted window excluded from throughput. Native tests compare the complete CPU,
including private pipeline/MDU/fetch fields, across single/both CPUs, word/byte
physical buses, original MUL.L/STS fallback and 300 cuts; all 901 library tests
and all-target/all-feature Clippy pass. Feature Wasm:
`faa0894b6a24963a3d7684483d2c92e190846abc61618281768899d2b8c4ce99`.
These checks establish correctness of the prototype surface, not a reason to
adopt a regression.

A further diagnostic-only build separates bus admission failures from short
observer horizons. In the selected 80-field interval, 531,718 attempts execute
only 59,422 of 35,936,320 clocks in the new domain (0.165%). There are 90,595
Machine-state rejections (90,588 have a pending master boundary; only seven a
slave boundary). Another 392,987 attempts fail bus/deadline admission: 95,761
fail cached-wait admission, 104,256 have a nonlocal active transfer, 32,390 a
nonlocal internal transfer, six a nonlocal buffered write and 48 a replay
journal; 160,526 have a horizon shorter than two usable clocks. Short-bound
counts overlap: master on-chip 94,478, slave on-chip 78, IRQ 23,091, raster
40,655, VDP1 814, CD 259, SMPC 85 and sound 1,799. MCIEB is zero at the diagnostic
endpoint. Thus neither WRAM range alone nor sound alone explains the rejection
rate. Master pending CPU work and on-chip event boundaries must be integrated;
adding supported ALU instructions alone barely changes the covered interval.
All diagnostic methods preserve the same exposed guest endpoints. No diagnostic
timing is included in throughput.

### Rejected CPU/on-chip domain integration variants

A separate prototype permits real WRAM/on-chip physical transfers and full CPU
replay fallback. On-chip events progress in the CPU domain; independent device
catch-up skips that already advanced domain. A later variant ages only physical
BSC busy/gap state per bus step and catches up other on-chip counters at their
absolute deadlines, before MMIO grant/completion, before arbitrary legacy issue
(including SLEEP), and on domain exit. It does not snapshot/restore on-chip state.
BSC counter advancement explicitly excludes bus aging already charged at the
physical edge. Single-CPU frame execution retains the normal cached path.

Three native differentials compare complete CPU/private pipeline/MDU/fetch state,
clocks, readiness, RAM/PCM and FRT counters: word/byte buses and MUL.L/STS fallback,
FRT/DIVU accesses (quotient 14), and entering from a pending master replay boundary
with a cold literal load of 42. All pass; the lazy variant passes all 903 native
library tests and native/Wasm all-feature Clippy. These are original guests.

An intended removal of the old pending-CPU entry guard was initially missed by
a source replacement; the explicit pending-master test detects this missing
coverage and the later variant removes the guard. The initial full-on-chip
variant therefore cannot establish acceleration of pre-existing pending
boundaries. All prototype source versions remain preserved locally.

| Rejected game screening variant | Normal fields/s | Alternate fields/s | Covered main clocks / 35,936,320 |
| --- | ---: | ---: | ---: |
| Full on-chip update at every local step, cached batches first | 21.371 | 20.214 | 728,233 (2.03%) |
| On-chip updates at events/accesses, pending entry fixed, cached batches first | 21.409 | 19.667 | 1,011,486 (2.81%) |
| Same lazy local domain before cached batches in dual-CPU execution | 21.384 | 6.892 | 34,615,975 (96.33%) |

Each row uses six alternating same-instance 80-field screening windows after an
unmeasured pair, with Canvas/PCM and matching exposed endpoints; counted windows
run afterwards and also match. These are repeated-interval screens, not new
continuous adoption evidence. No builds/tests/profiles overlap preparation or
throughput. The first full-on-chip variant also regresses the original single-CPU
loop from about 72 to 53 fields/s; retaining its old path restores about 71 fields/s
with no demonstrated gain. Different artifacts must be compared using each row's
own normal control.

Feature Wasm SHA-256 values, in table order:

- `6d6b95048ad5c448a872147a823124856cec3cf2f523f499926993762c78e70f`
- `1fe5572356dd14d83563b9b393a7551fa7f8689c38a3a682dbee3f481ee00f2c`
- `be55d6ff5bf967588af7708cb995c292c7e195d3c1a44e1bd73c6c0ed9490933`

Putting the new domain first reaches most guest clocks but executes 49,883,631
local loop turns and 42,056,900 nonzero on-chip counter updates per 80 fields,
retiring 40,661,164 total CPU instructions. The substantially worse throughput
rules out adopting this fine-grained integration as the new production engine.
Component-only cache/WRAM speedups did not predict game behavior. Further design
work must reduce host dispatch/admission/device transitions across instruction
blocks and proven polling intervals; covering more instructions alone is not
sufficient. No variant changes the default game runner or completes #138.

A matched-symbol Chrome profile is collected in a separate extra window after
all throughput and counted runs; its exposed endpoint also matches the normal
control. The function map comes from a byte-identical optimized rebuild of the
last artifact. In 11,706.384 ms / 7,762 weighted samples, self time includes
`kernel_local_allowed` 10.568%, `kernel_flush_local` 4.961%,
`kernel_advance_local` 4.952%, bus allocation 4.451%, interrupt sampling 3.870%
and local-deadline queries 2.696%. The exported wrapper has 11.147% self time,
which includes inlined runner work; it must not be interpreted as JS marshaling
cost. CPU step/replay, instruction execution and real device work remain too.
Self samples are diagnostic estimates, not exact accounting or a speedup.

The failed prototype sources are preserved locally and reverted from the working
implementation. The default engine remains unchanged. A next block-level
candidate can reuse the existing private-poll template's per-clock and partial
publications: these already reconstruct a proven slave polling microstate at an
arbitrary admitted endpoint. This avoids interpreting both CPUs at every clock.
The master can then use owned, preflighted instruction blocks inside the same
observer proof. This is a design direction, not an implemented or measured gain.


### Rejected in-place master-prefix screens

The next prototype reuses the existing arbitrary-phase private-slave polling
proof, mutably borrows the real master cache, and executes preflighted
scalar/load/branch instructions and buffered WRAM stores in the actual CPU.
Unsupported operations, misses and observer limits exit before that instruction
changes state. Independent sound advances only through actual master retirements;
there is no future sound snapshot/replay or per-instruction full CPU rollback.
The normal frame API remains unchanged.

Original native differentials cover 72 combinations of store/read loops, physical
bus widths, instruction limits, entry halfword offsets and live sound programs.
They compare full CPU/private pipeline/MDU state, cache contents/LRU, RAM, clocks,
readiness and PCM against the existing optimized path and an independent one-clock
replay. New versus existing optimized execution also agrees strictly on both
CPUs' retained fetch words and instruction epochs. Pure replay has an inherited
slave speculative-fetch/epoch difference at some pending branch phases, reproduced
with the new executor disabled; that difference is excluded only from the replay
comparison. Master fields, architectural/private CPU state, future execution,
cache contents/LRU, observation flags and PCM remain checked.

A shared eight-case original fixture includes real stores and nonzero live audio
without test-only fetch priming. Native, browser main thread and Worker agree:
`[8, 1587117688]` for the first variant and `[8, 408818393]` for the second.
The digest includes intermediate cut points, which change when an existing-path
fallback executes a larger prefix; each variant retains its own independent
state/output comparisons. The first variant passes all 901 library tests before
the shared fixture is added, and final-source native/Wasm all-feature Clippy.

| Rejected same-interval game screen | Normal fields/s | Alternate fields/s | Owned clocks / 35,936,320 |
| --- | ---: | ---: | ---: |
| Owned prefix before both existing polling paths | 21.382 | 20.468 | 3,112,332 (8.66%) |
| Retired-peer path first; reuse matched private proof on fallback | 21.494 | 20.685 | 2,861,635 (7.96%) |

Both use six alternating 80-field windows over fields 3600–3680 after an
unmeasured pair, Canvas presentation and PCM draining. The same instance's
memory/exported globals/tables are restored outside throughput timing. All exposed
SH-2/M68K, clock, PCM and pixel endpoints match, including extra counted windows.
These are screening measurements, not continuous gameplay or adoption evidence.
No builds, tests or profiles overlap preparation or throughput. The original
single-CPU guest retains its old path and remains effectively flat in each
variant (about 73 and 71 fields/s respectively).

The first screen makes 1,215,379 attempts, commits 180,209 owned batches and
retires 2,470,031 owned instructions. The second makes 1,141,207 attempts,
commits 168,902 batches and retires 2,280,393 instructions. Unsupported/short
prefixes return to the old path 516,124 and 459,275 times respectively.
Limiting replacement to a small fraction of clocks while adding admission work
fails to improve the game. Neither variant is adopted.

Feature Wasm SHA-256 values, in table order:

- `6d52acea1284b8d6a263e6e77803da6038b9dff3a425fcc8f9105808e572451f`
- `7bc12a7d655a29d35bc9e8bc0d1d3c7dfdbe87005b52adb889189f59ee7fd3e9`

A separate second-variant profile runs after all throughput/count windows and
preserves the same exposed endpoint. Its exact artifact's function map resolves
3,927.608 ms / 2,616 weighted samples. Existing `cached_batch` retains 21.373%
self time, sound advancement 8.848%, display acquisition 7.752% and VDP1
advancement 4.298%. These diagnostic sample shares are not a speedup or disjoint
inclusive costs. The next admission experiment retains existing pending-store
completion/acceptance times while allowing independent cache work to overlap.
The default game remains around 21 fields/s; #138 and #17 remain open.


The pending-store overlap variant also fails screening: 21.300 → 19.750 fields/s
(−7.28%). It covers 12,340,264 / 35,936,320 main clocks (34.34%) and retires
9,833,169 owned master instructions in 423,156 batches, including 248,927 batches
entered with an active buffered transfer. It calls sound catch-up 1,058,191 times
and stages 470,879 stores. Increasing coverage does not demonstrate a net gain.
The six alternating windows, extra counted window and separate later profile all
preserve exposed CPU/clock/PCM/pixel endpoints. Its 72-case native differential
explicitly requires successful owned execution while a buffered write is active;
the shared native/main-thread/Worker digest is `[8, 3213625458]`.
Feature Wasm: `bc9dee8a074b5413aceb22f316b21da2e24fead692f561362606f14b282d7c4a`.

In the counted 80-field window, the existing retired-peer path accounts for
5,000,035 master instructions, the new private-proof wrapper (including its
existing-path fallback) for 10,400,668, and remaining outer fallback execution
for 11,512,933. These are runner-path work counts, not disjoint function-profile
percentages. The separate exact-map profile samples 4,099.492 ms / 2,730 weighted
samples: existing `cached_batch` has 15.287% self time, sound advancement 8.875%
and display acquisition 7.750%; the exported alternate wrapper has 5.527%,
including inlined runner work. No JS-marshaling attribution follows from that
wrapper sample. The failed sources and artifacts remain local; the runtime
changes are reverted. The next diagnostic collects frequent guest dispatcher
locations separately from throughput to identify reusable instruction blocks.
No new default-game improvement or real-time acceptance is claimed.


A separate diagnostic of the unchanged runner counts 558,943 committed cached
batches and 26,600,308 master instructions over fields 3600–3680. Only 125,712
instructions are folded by the existing cycle proof; scalar blocks account for
7,448,529. The frequent regions contain ordinary function prologues/epilogues,
word loads, masks and conditional returns. Read-only local constant inspection
identifies the hottest routine's operand as cacheable high WRAM, not a device
status register. A follow-up diagnostic finds 1,036,123 cache hits and 84 misses
among its 1,036,207 dispatcher load attempts. Across the top 256 dispatcher
locations there are 5,868,804 load hits, 2,925 ordinary-memory misses and 5,262
other routes. These are attempt counts and a frequency-limited subset, not all
retired accesses or a globally measured miss ratio. Both diagnostics match all
exposed uninstrumented endpoints; their elapsed times are excluded from
performance evidence. CCR is 1 at the counted endpoint.

The first diagnostic artifact is
`3c99e45334734710809fbf934574e3d5eee123fef4e43e0eb4bcf2d0359ecb35`;
the load-classification follow-up is
`1650d9549c061072012061fe0432a975ec6e05f37f0a3e188ee6579f83d781c4`.
The original active-board native/main-thread/Worker digest remains
`[8, 3646496445]`. Raw guest locations, instruction words and operand addresses
stay local. Diagnostic instrumentation is removed after preserving the evidence.

This motivates evaluating unchanged-shadow-set lookup on the actual store-heavy
game workload. The earlier refill-loop screen above did not establish a gain;
it did not evaluate this actual game. The proposed path uses the real cache's
existing lookup only when that particular set has no staged version. Modified
sets retain the complete version chain, including replacement, aliases and
rollback. Adoption still requires actual game comparisons and repeats.


### Select unchanged-set lookup using actual game comparisons

The selected implementation checks whether the addressed set has a staged
version. Unmodified sets reuse the real cache's existing tag/way lookup;
modified sets retain the version chain. A prior unrelated stack write therefore
no longer sends every subsequent instruction/data lookup through the shadow
set scan. No allocation, guest phase or cache-state field is added. Truncation
restores the previous set head, and final shadow commitment still invalidates
the affected real lookup. The now-unused whole-shadow emptiness helper is removed.

The adopted baseline is the existing production package, SHA-256
`464639fa99636abb0a2682c09a6cad87f01d94f6abd5d8f6af71d73fe2f09548`;
intervening gated prototypes/rejection documentation preserve that default engine.
Selected production Wasm:
`0c205c8707a61e3d434cde157b91bb950631616a8c69ba42ec96991e1f920e54`.
Rust 1.100.0, wasm-bindgen 0.2.129, Binaryen 127 and Chrome 157.0.8103.52
match the preceding comparisons. This source builds without the experiment
feature.

| Actual continuous game comparison | Normal fields/s | Selected fields/s | Change |
| --- | ---: | ---: | ---: |
| Baseline initialized/prepared first | 21.405 | 21.523 | +0.55% |
| Fresh browser, selected initialized/prepared first | 20.921 | 21.349 | +2.05% |

Both comparisons independently boot both instances to field 3600 before either
is measured, then alternate six successive 80-field windows through field 4080
with the same authorized media/input, Canvas and PCM draining. All 12 paired
windows favor the selected package, including one near-flat first-comparison
window (+0.005%). All corresponding exposed CPU/clock/PCM/pixel endpoints agree.
The different gain magnitudes limit precision; the result is a modest,
workload-specific improvement. Candidate window p95 is 47.3–48.0 ms and p99
47.7–49.7 ms across the two runs. No local builds/tests/profiles overlap
preparation or throughput.

Original workloads use 120 warmup fields and six continuous 60-field windows.
Each row below is one alternating comparison, with matching exposed endpoints;
these tradeoff observations are not fresh-order repeats or universal gains.

| Original workload | Normal fields/s | Selected fields/s | Change |
| --- | ---: | ---: | ---: |
| CPU/store loop | 71.799 | 71.344 | −0.63% |
| Active sound/video | 52.649 | 52.589 | −0.11% |
| Normal bitmaps | 110.630 | 110.619 | −0.01% |

Selection prioritizes the actual game gain and records the small original-loop
tradeoff. The earlier artificial refill-loop rejection is preserved above;
it did not establish a general cache win. Existing tests cover literal cache
ways/LRU, duplicate tags, two-way/disabled modes, ordered writes/refills, rollback
and warmed real lookups across shadow eviction. The selected source passes all
900 all-feature library tests, the focused 8 cache and 52 cached-execution tests,
formatting, native all-target/all-feature and Wasm all-feature Clippy with
warnings denied, and the production build. Complete exact-head CI and review
remain required before merge.

This is a partial #138 increment, also related to #17. Sustained game 60 fields/s,
broader gameplay/transitions and production audio-underflow evidence remain
unmet. Host audio-queue drops are zero in these runs; that is not an AudioWorklet
underflow measurement. No required post-merge verification is added. The next
structural investigation is a persistent master-loop proof spanning ordinary
observer windows, while replaying every buffered WRAM write on its real bus
schedule. Long CPU/cache repetitions have not yet been demonstrated by this
frequency-only diagnostic.

### Guarded master-loop phase reuse (#138 continuation)

The implementation retains the independent Rust core. It learns repeated
resident CPU work and reuses verified instruction results across the existing
hardware observer windows. The ordinary board scheduler still executes sound,
display acquisition, refresh, interrupts and physical WRAM transactions.

The initial game diagnostic found a recurring 200-instruction, 264-main-clock
period. Complete CPU/private-pipeline state, next-ready timing and retained IF
matched, while some cache data and LRU bits differed. A complete ordinary-issue
trace then captured 246 logical reads and 13 buffered WRAM stores, including two
write-buffer stalls, with no physical Master read or unsupported access.
Endpoint repetition alone was insufficient evidence for execution reuse.

Automatic retention requires at least 32 retired instructions and usable compiled
guards. Two distant branch sites can execute a four-instruction cycle; retaining
that cycle would displace the existing short-cycle folder. The original sparse
branch control keeps the existing folder, and removing the minimum-size guard
makes its negative control fail with 596,465 new-engine instructions instead of
zero. The exact source is restored after that deliberate mutation.

The runtime proof has the following boundaries:

- Training runs on a private CPU/cache/SH7604 copy. The live Machine stays at its
  original clock and field endpoint. Physical RAM is not cloned; a usable trace
  must obtain its reads from resident ordinary-memory cache lines.
- Each execution attempt matches the complete CPU/pipeline and retained IF under
  clock translation. Instruction/local-clock counters are rebased; actual
  exception diagnostics and an already completed multiplier deadline are kept.
  Both the current and recorded entry require an idle multiplier, and the trace
  must preserve its completion deadline.
- Cache guards check the bytes read before the period overwrites them, plus the
  required tag/hit identities. Guards apply through the selected instruction
  prefix. Cache ways may move: actual way bindings translate the recorded LRU
  touches, starting from the current replacement state. Duplicate tags and
  self-modifying instruction inputs remain conservative rejection cases.
- Every selected WRAM store goes through the existing BSC planning and real
  commit path. Its nominal request, actual acceptance and owning instruction are
  retained. A changed acceptance time in either direction rejects the attempt.
  Selection uses instruction ownership, including stores sharing a clock with
  the preceding retirement.
- Existing IRQ, DMA, raster, BSC, UBC, reset and device horizons still bound the
  attempt. The existing private-Slave proof publishes the matching peer phase.
  A shortened Sound prefix truncates staged stores/cache versions and publishes
  its exact CPU phase directly.

Retirement times come from the retained phase table without a temporary vector
per attempt. For a pending private peer, every positive retirement is eligible.
When the maximum reference-clock retirement gap is below nine, the existing
no-checkpoint Sound proof also applies: synthesis and the 68000 still execute,
while the large rollback copy is omitted. Sparse gaps and other peer schedules
keep the checkpoint path.

#### Executed prototypes and rejected results

These screening comparisons use one instrumented package, restoring that same
instance's memory, globals and tables between variants. They repeat the same
80-field interval and establish neither continuous throughput nor production
adoption. Preparation precedes timing; profiles and work counts are separate.

| Screening variant | Control fields/s | Candidate fields/s | Decision |
| --- | ---: | ---: | --- |
| Replay every memory operation, attempt at every instruction | 21.272 | 17.854 | Reject |
| Attempt only at batch entry | 21.065 | 20.846 | Reject |
| Preserve expired multiplier history across IRQs | 21.307 | 21.354 | Effect too small to establish |
| Whole-suffix guards with fixed cache-way identities | 21.027 | 19.699 | Reject |
| Prefix guards with actual LRU-way bindings | 21.432 | 21.956 | Advance to further validation |
| Allocation-free times and direct shortened-prefix publication | 21.313 | 22.163 | Advance to normal-API validation |

An earlier attempt aborted on an invalid assumption that rebuilding a shortened
prefix would always reach the requested count within one retained suffix. It
provided no throughput result. The selected-prefix implementation removes that
assumption. Whole-suffix validation also rejected work beyond the actual window;
fixed way identities rejected otherwise valid cache placements. Neither rejection
was a reason to weaken cache visibility or guest timing.

#### Normal-API evidence before production-package selection

Independent instances boot normally from reset with the same media/controller
schedule. Both finish preparation through field 3600 before either is measured.
Six alternating continuous 80-field windows reach field 4080, including Canvas
presentation and PCM draining. Automatic training runs inside the candidate;
there is no manual capture, heap restoration or extra live-Machine advance.

| Comparison | Control fields/s | Candidate fields/s | Change |
| --- | ---: | ---: | ---: |
| Same package, automatic loop engine disabled/enabled | 20.851 | 21.593 | +3.56% |
| Loop engine already enabled, Sound checkpoint proof added | 21.987 | 22.153 | +0.75% |
| Fresh browser, checkpoint candidate initialized/prepared first | 22.131 | 22.352 | +1.00% |

All corresponding exposed endpoints and 64-bit fingerprints of complete private
CPUs, effective cache data/tags/LRU, retained IF, instruction epochs, full WRAM
and next-ready times agree. The checkpoint increment improves all 12 paired
windows. Its candidate p95 ranges are 45.6–46.3 ms and 45.3–46.0 ms; p99 ranges
are 46.1–47.2 ms and 45.8–47.4 ms. The initial automatic-engine comparison has
candidate p95 47.0–47.5 ms and p99 47.6–48.7 ms. Host/run variation prevents
combining absolute rates from different comparisons as one measured speedup.

These packages use the diagnostic feature. The engine-disabled/enabled package
is Wasm `0a47e8f971a556d3e8d470c2b63acbdc853803bee388febc3db8c6e7e05e1c95`;
the checkpoint candidate is
`a8b79fce759f17c6237e00df589c506253b95bb990c34bd33c92bab8db5bb8aa`.
Rust 1.100.0, wasm-bindgen 0.2.129, Binaryen 127 and Chrome 157.0.8103.52 match
the recorded build and browser setup. No local builds/tests/profiles overlap
preparation or throughput.

A separate profile after all timing, fields 4080–4160, covers 15,303,112 template
instructions and 34,446 direct shortened-prefix publications. Its 25,839 samples
span 3,970.342 ms. Self time is 20.660% in `Bus::cached_batch`, 8.427% in Sound
advance, 8.270% in display acquisition and 4.052% in VDP1 advance. The 70.597%
inclusive cached-batch figure contains these nested device paths; it is not a
pure CPU percentage. Inlined work also prevents treating wrapper self time as
JavaScript marshalling cost.

#### Production integration and acceptance

The normal package enables automatic phase learning and reuse. Diagnostic Wasm
exports remain behind `kernel-experiment`. Ordinary memory operations skip trace
recording unless a private capture is active. A native full-suite run passes
1,372 all-feature tests in 111 result groups, with native/Wasm Clippy passing.

Original portable controls cover a constant loop with nonzero WRAM writes,
changing CPU state, changing cache inputs and adjacent stores with real buffer
waiting. They additionally exercise ordinary NMI/MUL/RTE, a live multiplier
before completion, cache address/data-array way relocation, changed input/BSC
waits, automatic training and live 68000 SCSP writes selecting shorter prefixes.
Native comparisons include full CPUs, caches, WRAM, clocks, Sound, PCM and BSC
state. Native, Chrome main-thread and Worker results agree at `[4, 829299155]`.
The existing timed-kernel runner supports `--verify-only` for these original
controls without timed rounds; the native example prints the same loop digest.

The final **default production** package is compared with main `b04287a`'s
adopted unchanged-shadow-set package, using the unmodified production runtime
benchmark. A fresh-browser repeat reverses initialization, preparation and first
timing order. Both use the same field 3600–4080 continuous Canvas/PCM workload.
All 12 paired windows improve, and corresponding exposed CPU/Slave, clock, PCM
and pixel endpoints agree. The forward change is +4.68% (21.537 → 22.545
fields/s); the fresh reverse is +5.12% (21.387 → 22.481). Candidate p95 ranges
are 45.0–45.7 ms and 45.1–45.7 ms; p99 ranges are 45.6–46.9 ms and
45.6–46.8 ms. Host queue drops are zero; production AudioWorklet underflows are
not measured.

The baseline Wasm is
`0c205c8707a61e3d434cde157b91bb950631616a8c69ba42ec96991e1f920e54`.
The selected production Wasm is
`65c1d31bbcef21463cba7ee89c6588e9cbbe4ac31c534ebbe7710c48734e1177`;
its feature-enabled verification companion is
`9539038dbd2bcd499add8503e167de6b878af90fb172c85086945406178a7bb8`.
The companion passes the same main-thread/Worker controls after the minimum-size
guard is restored.

Original workloads use 120 warmup fields and six alternating continuous 60-field
windows with the same production packages and Canvas/PCM handling:

| Original workload | Baseline fields/s | Selected fields/s | Change |
| --- | ---: | ---: | ---: |
| Short CPU/store loop | 69.870 | 67.806 | −2.96% |
| Busy sound/video | 52.340 | 52.337 | −0.006% |
| Normal bitmaps | 110.735 | 110.484 | −0.23% |

All corresponding exposed endpoints agree. The short CPU/store workload is a
measured regression in all six windows; it has no long reusable CPU cycle. The
existing short-cycle folder remains available. Automatic discovery and the
larger production execution path may contribute; their individual costs have not
been isolated on this workload. The game gain is retained with
this explicit tradeoff; this increment does not improve every workload. Exact-head
native/Wasm/browser CI is required before merge and remains pending at publication.

This remains partial work related to #138 and #17. Sustained 60 fields/s,
representative broader gameplay/transitions, production audio underflows and
hardware-completion acceptance remain open. Private inputs and raw diagnostics
stay local. No required post-merge verification is added.
