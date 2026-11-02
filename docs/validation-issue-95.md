# Issue #95 validation

Implemented the standard local-media input increment for Issue #95. All checks
below ran before the checkpoint commit. No GitHub mutation, push, PR, merge,
proprietary media, personal firmware, physical-console test or live Pages check
was used. Required post-merge verification: **None**. Hardware completion remains
tracked by #17; mounting a container is not retail-game compatibility evidence.

## Result and input contract

The production native Rust/Wasm reader accepts self-contained MAME CHD v5
single-session CD-ROM with cdlz/cdzl/cdfl/none and the supported Mode1/raw
Mode1/raw Mode2/audio layouts. It shares `Media`, `SectorReader`, HLE boot and
external BIOS mounting with existing ISO/CUE media. There is no new boot rule,
per-title data, external converter or expanded medium written at runtime.

The picker accepts one CHD, one cooked ISO, or a CUE with its referenced BINARY
tracks selected together. A raw BIN alone is rejected. No ordinary disc needs
SRW1, a private manifest, extracted executable or extra BIOS-like asset. HLE
requires no firmware; BIOS mode requires locally supplied firmware and never
falls back to HLE. See [the complete format/resource contract](media.md).

## Independent media and oracles

[build_chd.py](../tests/fixtures/build_chd.py) generated **14** small MAME
`chdman createcd` 0.289 containers from original MIT source media. The adjacent
fixture LICENSE and homebrew licenses apply. The
[inventory](../tests/fixtures/chd/inventory.json) records compressed SHA-256,
logical sizes and independently reconstructed logical CD-unit SHA-1.

| Inventory | Native and production Wasm evidence |
| --- | --- |
| Data-only CD stream, default combination and each cdlz/cdzl/cdfl/none | Every first-track byte through the original guest's FIFO loop, all 408 TOC bytes, BIOS mode retained, no HLE installed |
| Mixed Hardware Lab, same five codec selections | Every data byte/TOC, complete stereo PCM equality; HLE guest SCU DMA data, IRQ/slave/DSP results and nonzero stereo |
| Changed CD-stream payload | Changed source byte reaches the guest FIFO without a cached/original-data substitution |
| Gaps/padding | Stored audio INDEX 00/01, unstored pregap, postgap, non-four-frame data track, raw Mode2; exact native sector/index/TOC equality and browser guest TOC/data/audio |
| Raw Mode1/XA/plain Mode2/audio | Complete native raw/cooked sector equality, including ECC reconstruction; browser first-track FIFO bytes, complete TOC and stereo |
| Opaque standard RW_RAW subcode | Standalone CHD mounts and preserves data/audio; native Read R–W remains WAIT instead of manufacturing reconstructed packs |

Native [CHD tests](../tests/chd.rs) compare every logical raw/cooked sector,
all track numbers/modes/CONTROL/start/INDEX/end/leadout fields, complete TOC and
every boot-sector byte against source ISO/CUE/BIN. The original Hardware Lab
guest computes expected DMA bytes and results `[1,1,84,35,42,393984]`, completion
`0x600D` and DSP result `0x900`; CHD preserves its complete stereo output.
The original synthetic BIOS probe reads the entire first track and TOC through
real guest commands/FIFO reads, not host media inspection.

The [Chromium group](../scripts/chd.mjs) runs that same probe on the production
Wasm at `/saturn-rust/`, checks the same complete byte/TOC/PCM oracles, and
exercises the real picker, HLE, retained-medium Reset/Restart and cancellation.
Parent CHDs, cdzs, oversized hunks, malformed FLAC channel/bit depth,
corrupt/truncated input and a valid-map-CRC
self-cycle produce errors without stale guest execution. A lone BIN is rejected
and observed requests are GET-only with no bodies. The existing shared UI trace
also retains its mode/pending-read cancellation and ISO/CUE missing-track tests.

Native rejection coverage adds 512 deterministic header/map/codec-payload bit
mutations across the four codecs without panics, truncated sizes, version/unit/
hunk limits, parent SHA-1, cyclic metadata, corrupt map/tree and CRC/SHA failures. CD FLAC checks reject mono/non-16-bit frames,
including a later frame in the same hunk, before upstream stereo decoding.
A separately constructed negative fixture has a valid Huffman map CRC and a
self-referential hunk, verifying rejection before upstream recursive decoding.
This is bounded regression coverage, not a claim of exhaustive fuzzing.

## Final executed checks

Host: macOS arm64; rustup stable **rustc 1.99.0**, wasm-bindgen **0.2.129**,
Node **26.12.1**, Playwright **1.58.2**. The final complete browser run used pinned
Chromium headless shell **145.0.7632.6**. An earlier complete 74-group run also
passed with local Chrome **156.0.8078.74** before the subcode-boundary correction;
only the final pinned-Chromium run is final-artifact evidence.

Rust commands selected the documented rustup toolchain with
`PATH="$(rustup run stable rustc --print sysroot)/bin:$HOME/.cargo/bin:$PATH"`.
Final browser commands used
`PLAYWRIGHT_BROWSERS_PATH="$PWD/target/chd-browsers"` for the local pinned cache.
No test coverage was skipped or assertion weakened.

| Command | Final result |
| --- | --- |
| `cargo fmt --check` | Pass; 0.421 s |
| `cargo build --locked` | Pass; 1.457 s |
| `cargo test --locked` | **1160 passed**, zero failed/ignored, 103 harnesses including doc tests; 448.174 s |
| `cargo clippy --locked --all-targets -- -D warnings` | Pass; 0.899 s |
| `sh scripts/build-web.sh` | Pass; original demos reproduced; final Wasm compile 1.80 s |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Pass; also repeated successfully after native checks |
| `sh scripts/check-fixtures.sh` | All **103** registered commands pass; 3.792 s |
| `node --test scripts/ci-runner.test.mjs` | **13 passed**, zero failed/skipped; 16.484 s |
| `npm run test:browser` | All **74 groups pass**, 32 shared + 42 scenarios; aggregate artifact/coverage verification passes; 392.962 s |
| Git whitespace and changed-document relative links | Pass |

The final browser shards measured 392.710 s shared and 345.951 s scenarios.
The CHD group measured **45.561 s**. The complete existing Star Collector, Prism
Courier, Hardware Lab, ISO/CUE/BIN, optional SRW1, CD runtime, sound and hardware
coverage remains registered. CI inventory regressions enforce the new 74-group /
103-builder totals and retain both shard and tested-artifact publication gates.
The hosted GitHub workflow was not run because this node must not push or open
a PR. Local times include overlapping native/browser work on one host; they are
not hosted runner CPU or queue-time measurements.

## Memory, performance and corrected attempts

Final bindgen Wasm is **927994 bytes**, compared with **581375 bytes** for the
selected upstream source built separately with the same compiler/bindgen:
**+346619 bytes (59.6%)**. The decoder is pure Rust, with no C acceleration,
external executable, CDN or backend dependency in the runtime.

The reader retains compressed bytes, bounded map/target tables, one hunk cache
and bounded codec scratch. A load performs a logical decompression/hash pass; CD FLAC also has a
bounded frame-decoding pass to validate stereo/16-bit/count constraints before
upstream reconstruction. Large CHDs therefore cost admission CPU proportional
to logical size without retaining an expanded image. See [resource bounds](media.md#resource-and-failure-behavior).
The complete native media equivalence case, timed separately with
`/usr/bin/time -l target/debug/deps/chd-… --exact mame_cd_codecs_match_every_sector_track_toc_and_boot_byte`,
measured 0.08 s real, 0.07 s user and 5636096 bytes maximum RSS (3457408-byte peak
memory footprint). This is a small-fixture measurement, not a full-disc memory
or latency claim. Guest execution dominates the substantive browser group.

Initial local attempts and their resolutions:

- Homebrew Rust lacked the Wasm target for that compiler; selecting the documented
  rustup stable compiler/target fixed the build.
- Rust 1.99 Clippy reported two new constant-chunk iterator warnings; changed to
  `as_chunks`/`as_chunks_mut`. Final native/Wasm Clippy passes without allows.
- The first local test used an out-of-range `run_steps(300000)` count; it now uses
  supported bounded calls. The guest/model was not changed.
- Playwright's Node 26 installer stalled after downloading archives; the existing
  Node 24 binary could not load its simdjson library. The downloaded official,
  pinned headless-shell ZIP passed `unzip -t` and was unpacked into this worktree's
  ignored test cache. The unchanged runner and complete suite then passed.
- Final decoder review found that assertion avoidance alone does not enforce
  CD FLAC stereo PCM. Added complete frame channel/bit-depth/count validation
  and native/browser rejection controls; all final suites were rerun.
- Review found that raw optical subcode is not the SRW1 reconstructed-symbol
  boundary. The direct mapping was removed before final validation; the native
  no-fabricated-pack check and complete final Chromium run cover the correction.

## Documented limits

CHD versions 1–4, parent/differencing images, non-CD or multi-session metadata,
other codecs/track types and unlike stored pregap types fail explicitly.
CHTR/CHT2 cannot recover CUE FLAGS or INDEX 02–99 that were not encoded.
Optical R–W deinterleaving remains unsupported; standard subcode images mount
with no reconstructed R–W packs and never require an invented companion. Existing
explicit SRW1 diagnostics remain available. Retail BIOS/game/hardware completion
is separate from this verified media-format support.
