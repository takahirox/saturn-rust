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
