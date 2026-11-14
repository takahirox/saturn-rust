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
