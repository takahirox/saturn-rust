# Ordered renderer Worker experiment

Related to #138 and #17. This opt-in `kernel-experiment` vertical slice keeps
our original Rust core and moves VDP2 acquisition, composition and scanout into
an independent Wasm instance. It does not replace the core with an external
emulator. The normal player and default Machine execution remain unchanged.
Real-time gameplay and AudioWorklet acceptance remain open.

## Timing and ownership

The producer retains both SH-2 CPUs, CPU-facing VDP2 RAM/registers/status/grant
masks, raster interrupts, BSC/DMA, VDP1 drawing/erase and sound execution. Its
video journal owns every transferred payload. The consumer never reads future
producer RAM, reusable producer storage or a live front framebuffer.

The ordered stream contains logical raster spans, VRAM/CRAM/register writes,
flushes, front-buffer changes, and PAL/PLL/reset transitions. Front changes that
complete during a span precede its final raster event. HBlank scanout precedes
display erase; VBlankOut installs the swapped front surface before acquisition.
Every existing video flush remains ordered, including a CPU write to the current
non-display framebuffer. Cached board catch-up records logical raster progress
independently of the already advanced board clock.

Adjacent action-free spans can coalesce. Other commands retain FIFO order. The
producer drains at field boundaries and at 4096 queued commands. The `SRV1`
transport has little-endian metadata and unchanged guest-byte-order payloads;
chunks are bounded to 16 MiB. The decoder validates the complete chunk and its
raster endpoints before applying it. No native pointers cross Wasm instances.

The browser client copies a borrowed Wasm view into an owned transferable buffer.
The Worker copies rendered pixels into owned output before transferring them.
At most two fields are in flight by default, with a separate 64 MiB byte budget.
The producer waits for capacity; the client verifies sequence numbers, field
numbers, chunk receipts and pixel length. No SharedArrayBuffer or cross-origin
isolation is required. Errors reject pending work, including failed Wasm startup.

## API boundary

`advance_video_field()` returns at exactly one raster field boundary, even when
the CPU is sleeping or waiting. The old `advance_frame()` behavior is retained.
Removing pure rendering events can change the host instruction-step return cut;
comparisons therefore use the exact-field API on both sides and compare complete
CPU/device state at the same emulated time.

Enable transport only at the pristine reset boundary. Await the client's ready
promise, activate its borrowed-byte import, then enable transport on the producer.
For each field, await capacity, activate the client, advance the producer and
submit the field marker. Keep up to two receipts in flight and consume every
receipt in sequence. Drain outstanding receipts before comparing endpoints or
ending an interval. The external producer deliberately rejects `framebuffer()`;
the receipt owns the authoritative completed image.

The callback supports one active producer in each JavaScript realm. Activate the
corresponding client before executing a producer. This prototype does not yet
wire the Worker into `web/app.js`, migrate the whole producer off the UI thread,
or establish normal playback latency or audio-underflow behavior.

## Reproduction

Build the optional Wasm feature using the pinned Rust, wasm-bindgen and Binaryen
versions from [setup](setup.md). Use the same build flags and `wasm-opt -O4`
configuration as `scripts/build-web.sh`, adding `--features kernel-experiment` to
the Cargo build. Keep its generated JS and Wasm together in an isolated directory.

```sh
node scripts/verify-video-worker.mjs --pkg /path/to/feature-package
node scripts/benchmark-video-worker.mjs \
  --baseline /path/to/feature-package --candidate /path/to/feature-package \
  --skip 120 --frames 60 --runs 6 --continuous --draw
```

Repeat the benchmark with `--candidate-first`. Both sides intentionally load the
same feature artifact; the baseline uses the ordinary renderer and the candidate
uses the real Worker. Both use the exact-field API. Private BIOS/media and input
options follow `scripts/benchmark-runtime.mjs`; selected media is read through
browser-local File objects, never served over HTTP. Do not publish private files,
captures or raw profiles.

The benchmark includes every frame's Canvas presentation, PCM draining and final
queue drain. The final image hash is outside timing. Its per-iteration quantiles
are producer-loop timings with pipelined receipts, not end-to-end field latency.
A separate all-frame correctness experiment hashes the ordered image sequence.
Neither measurement establishes sustained player/AudioWorklet acceptance.

## Executed validation

The frozen experimental source is based on main
`fceea5b79f827e02c26c82870bad367021acfac3`. The feature Wasm SHA-256 is
`84803351ab66f57bb0a9e79fdb60b987af4cf3f7ba2a9a6463453b3bb88a979d`.
Rust 1.100.0, wasm-bindgen 0.2.129, Binaryen 127 and headless Chrome
157.0.8103.52 are used. Later publication changes to error handling are validated
separately; do not assume byte identity with a newly built package.

- All 928 native library tests pass, together with native all-target/all-feature
  and Wasm Clippy. Native originals compare literal acquisition, delayed palette
  writes, front erase/swap/pixel completion, both cached catch-up clock modes,
  PLL/PAL/reset, RBG/sprite/interlace guests and independent expected pixels.
- Consumer and real-producer negative controls fail when prefix flush, span
  forwarding, HBlank erase or VBlank front-surface ordering is removed. Exact
  restored originals pass. The transport golden-byte and invalid trailing
  payload/endpoint tests verify byte order and whole-chunk validation.
- The public browser Worker verifier passes four RBG/sprite/interlace originals
  twice across cold resets, including two complete extra scanouts after each
  guest marker (36 total fields). All CPU/device/PCM and every-frame bytes agree.
  It also rejects reordered commands and a failed Wasm startup; all receipts
  drain with at most two pending fields and about 1.33 MB peak pending bytes.
  Cross-origin isolation is false.
- Existing original browser main/Worker conformance remains `[4, 2337474100]`.
  Original CPU, busy and normal workloads each pass six paired windows; every
  full 22-hash device endpoint, CPU state, PCM and ordered image sequence agrees.
- The authorized game selection scene, fields 3600–4080 with six continuous
  80-field windows and deterministic controller input, also matches every
  endpoint and every-frame image sequence. The all-frame-checking diagnostic
  measures 25.9481 versus 32.8311 fields/s (+26.53%). Hashing overhead is included;
  this is not default-player or adoption throughput.
- With final-image hashing outside timing, the first fresh-browser comparison
  measures 27.6823 versus 35.1764 fields/s (+27.07%). Every paired window improves
  and all full-device/CPU/PCM/final-image endpoints agree. A fresh browser with
  initialization/execution order reversed measures 28.5930 versus 36.6049
  fields/s (+28.02%); all six windows improve and every endpoint agrees again.
  These twelve paired windows establish an optional Worker-path gain, not a
  normal-player result.

A separate 80-field producer profile (2,226 ms sampled, 1,470 samples) attributes
approximately 33.75% to cached CPU/bus planning, 24.04% to sound, 11.77% to VDP1
and 30.44% to remaining execution/presentation. No VDP2 work remains in this
producer sample. These are approximate sampled groups, not whole-system or
Worker-inclusive CPU percentages; instrumented timing is excluded from gains.

The current main production results remain 28.6444 fields/s in selection and
31.5182 fields/s in the separately defined story scene. These optional exact-field
experiments do not replace those production numbers. The Worker slice alone does
not demonstrate 60 fields/s, broader story compatibility, normal audio delivery,
or hardware completion. No post-merge verification is required for this slice.
