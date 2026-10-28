# Issue #75 validation: timed R–W decoding and transfers

Related to #75 and #17. The [primary-derived contract](cd-rw.md) records the
24-slot/23-pack distinction, bit layout, disc-deinterleaving boundary, GF(64)
parity matrices, scheduled acquisition/pre-roll/publication and lifecycle rules.
All fixtures are original MIT content under `tests/fixtures/LICENSE`. No personal
firmware, commercial media, human testing/approval, live Pages inspection or
post-merge verification was required. This node does not push, open a PR, merge,
comment, close an Issue or alter usage allowances.

## Implementation and independent evidence

`src/media.rs` validates one caller-supplied SRW1 representation and an explicit
CUE REM reference. The existing production Wasm file-table API passes these
selected bytes through that same parser; the UI accepts the companion extension.
Missing/ambiguous names, malformed headers/lengths/counts/symbols/FAD mappings,
truncated tables and duplicate attachments fail before media publication.
The browser additionally verifies that a malformed `mount_cue` leaves the guest
records intact; a failed reset retains its established stop-old-execution rule.

`src/cd_rw.rs` implements real P then Q parity processing, separate 75 Hz
acquisition and decoder deadlines, an eight-pack acquisition window and 24 decoded
physical slots. `src/cd.rs` connects pre-roll/PLAY, host flags and DATATRNS,
overwrite recovery, DataEnd and lifecycle cancellation. Frozen independent
codewords and Gaussian-elimination fixture construction do not share production
error-location logic. Tests cover all 1512 single-symbol position/magnitude
combinations and all 276 pairs of locations, P-valid/Q-invalid input and selected
uncorrectable header/payload patterns. Before/at/after checks span 100 decoder
publications, chunks1/3/17/10000 and passive inspection; separate tests check
both pre-roll acquisition deadlines and cancellation of its retained window.

`tests/cd_rw.rs` checks media limits and mapping, startup, 24-slot capacity versus
23-pack recovery, physical-slot overwrite during a partially consumed transfer,
partial/full/excess reads, the primary DataEnd count relations, DATATRNS competition, independent HIRQ
acknowledgment, enable/no-change/disable, pause/seek/scan, data/audio transitions,
unchanged-start PLAY, repeat, software reset, CDON/OFF, tray/replacement and
Machine reset at acquisition, decoding and transfer boundaries.

The generated SH-2 program installs actual SCDQ/A-bus and DMA-end handlers,
acknowledges the CD source, rearms SCU external interrupts, plays CD-DA and
retrieves four packs using alternating CPU and fixed-source SCU DMA transfers.
Only guest instructions write observed payloads, sums, sequence/error records and
DataEnd counts. Native and production browser execution compare all twelve words
of each pack to separately authored literals. Clean, changed, corrected-header/
payload, uncorrectable-header, uncorrectable-payload and omitted-enable controls
repeat cold boots. Native and browser stereo tails remain exactly `[1000,-2000]`.
The required `cdRw` browser scenario also loads the same CUE/ISO/audio/SRW1 through
the production local-file UI, rejects the missing companion and observes no
network request for selected media. Existing WebAudio assertions remain required.

## Inherited-main negative controls

Both controls use actual archived source at
`3b2f8038ec8769b446274f8166f7e2d9917695ac`, rather than modifying the implemented
codec into a stub. The same new guest/media run under inherited native Machine
and its independently rebuilt production Wasm. The old CUE parser ignores the
new REM; the unchanged `0x2001` remains WAIT. The SH-2 reaches completion600D
with zero retrieved packs. Native asserts four packs and fails exit101 with
`Ok(0)` versus `Ok(4)`. The browser required scenario likewise fails exit1 with
actual0 versus expected4. No accepted oracle, timeout or publication gate was
weakened to obtain these results.

Reproduction from this checkpoint (the two test commands intentionally fail):

```sh
mkdir -p target/inherited-rw
git archive 3b2f8038ec8769b446274f8166f7e2d9917695ac | tar -x -C target/inherited-rw
cp tests/fixtures/cd-rw-baseline.rs target/inherited-rw/tests/rw_negative.rs
cp tests/fixtures/cd-rw* target/inherited-rw/tests/fixtures/
cp scripts/cd-rw.mjs scripts/browser-groups.json scripts/browser-shards.json scripts/browser-smoke.mjs target/inherited-rw/scripts/
ln -s ../../node_modules target/inherited-rw/node_modules
(cd target/inherited-rw && CARGO_TARGET_DIR=../inherited-build cargo test --locked --test rw_negative -- --nocapture)
(cd target/inherited-rw && CARGO_TARGET_DIR=../inherited-build sh scripts/build-web.sh)
(cd target/inherited-rw && npm run test:browser -- --group=cdRw)
```

Use a Rust toolchain with the installed Wasm target and the pinned wasm-bindgen
CLI on PATH for the build. The negative native oracle is retained as original
source in `tests/fixtures/cd-rw-baseline.rs`; it is deliberately not registered as
a passing acceptance test of the current implementation.

## Executed checks

Local environment: macOS/aarch64, native Rust1.94.0, installed Rustup stable1.99.0
for Wasm, wasm-bindgen0.2.129, Node26.12.1, Playwright1.58.2,
Chromium145.0.7632.6. The first Wasm attempt selected Homebrew Rust without a
Wasm standard library; using the already-installed Rustup toolchain resolved it.
Initial CI-runner assertions correctly rejected stale 66/96 inventory counts;
registering the new required group/builder and updating the exact counts fixed
both failures. Guest bring-up failures were fixed before the runs below.
No remaining failing acceptance check is hidden by the negative-control results.

| Check | Executed result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | 1081 passed, zero failed/ignored, including 705 library tests and nine new CD integration tests |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | Passed, all 97 registered reproduction commands |
| `sh scripts/build-web.sh` | Passed, all three demos reproduced and production Wasm rebuilt |
| `node --test scripts/ci-runner.test.mjs` | 12 passed, zero failed/skipped |
| `npm run test:browser` | Passed all 67 groups in201940ms, two workers (shared179535ms; scenarios201648ms) |
| Inherited native/Wasm nonempty-pack controls | Both failed the intended zero-versus-four oracle |
| `git diff --check` | Passed |

The final required browser inventory has 30 shared groups plus 37 independent
scenarios. `scripts/ci-runner.test.mjs` asserts both exact inventories and includes
the new builder/group explicitly. Both shards still gate publication of the exact
retained Wasm/site digest; no test skip, fixture skip or weaker gate is added.

## Artifact hashes

Reproduce with `python3 tests/fixtures/build_cd_rw.py --check`.

| Artifact | SHA-256 |
| --- | --- |
| `cd-rw.iso` | `dea79b87c15ae35d25599cb8196c0a3d694f20f22b7d6f054c83c43de2662b7a` |
| `cd-rw.cue` | `a9219c988668b77ffc9eba7571ecedc4be7697e74dab6d9c0f2fc60f6fd27bf1` |
| `cd-rw-audio.bin` | `02b8eb38cee1979c221ecb2e35604461467697701f6395ff1c5b7e39165e93c4` |
| `cd-rw.srw` | `0e581988b08e1df84526d77d602a12b7d67903d9c09d650ee2c5d5b5acb0f61c` |
| `cd-rw-changed.srw` | `d55c7a3503853ce7e2be5b413c1768ce9589d30ed641d0b9bc4799fdd9ec6571` |
| `cd-rw-corrected.srw` | `3d5443145d1c424ce2f75ad0ad2d51f90a8de2e85a6928548c4804244a366f47` |
| `cd-rw-bad.srw` | `566483da7747b7b1c8e60df4202b7fc1f80cc40e601e98904fe550c530d8010a` |
| `cd-rw-header-bad.srw` | `8bbfa180f86bf95f2aa57a6721eb7ad9c43c68c362b33b7a70e7093fec3669dc` |
| `cd-rw.s` | `0a725f0da67d7221b9a0ad3b0d9bc3fc8c12e7029d527de86307ffd73f5dcd30` |
| `cd-rw-irq.s` | `f06c0e9aa77aaf248052c92669a685dcbcf94c158033da736b851ee8402b3b50` |
| `cd-rw-dma.s` | `57c5cf1023ff8f81d778e9ccbfbb8a7d52c91b37c09ae911743504a515f589d8` |
| Native inherited oracle source | `b018f5437cdff1bc9cd4e2912cb47acb392c9de82af34f0c6861d85d98a797b9` |
| Production Wasm | `6cd7d98138bb9d55c6f963018fe22c549d16d3c2c03c6c553cf8107ac619c0b4` |
| Inherited-main production Wasm | `f2f2c056cf9507972ead1cceca49505ab661c84e23ea2f2a8a2a70e7ce2da3ab` |

## Scope and remaining deviations

Exact publication phases, one-word partial-GET prefetch depth, positioned-resume
lookahead and virtual seek latency
are documented deterministic conventions where the primary manuals do not
specify oscillator phases. The sidecar explicitly begins after disc deinterleaving;
raw optical subchannel parsing and application rendering are outside this Issue.
All selected parity vectors, deadlines, ring/transfer/lifecycle and original
guest/browser controls are automated and use no proprietary data.

This is scoped progress toward #17, whose other CPU/peripheral,
bus/cache/reset/arbitration, SCU/DSP, video/audio/CD timing, media/HLE/SMPC and
integrated acceptance requirements remain mandatory. Genuine BIOS operation and
broad commercial-game compatibility remain unverified. A green local suite and
this checkpoint are not evidence of umbrella hardware completion. Required
post-merge verification: None.

## Review fix: filesystem interruption during R–W pre-roll

Related to #75 and #17. ChangeDirectory/ReadDirectory and AbortFile now call
`stop_rw` when they interrupt PLAY, including the prepared acquisition interval
before `rw_active` becomes true. This cancels input deadlines and discards the
lookahead window while retaining the decoded ring as before. Previously, input
could accumulate without a consuming decoder, panicking at the eight-pack bound
in checked builds and growing without bound in release builds.

The original committed mixed-media fixture drives a new command-level regression
for all three commands. Each interrupts immediately or one clock before, at or
after either acquisition deadline. ReadDirectory first acquires the root so its
command is accepted. Continued advancement checks empty-pack WAIT, empty DATATRNS
and no stale DRDY; a fresh PLAY must transfer the independently expected first
pack. The regression failed at the reported eight-pack assertion before the fix
and passed afterward. No fixture bytes or acceptance gates changed.

Review-fix checks passed: `cargo fmt --check`, `cargo build --locked`, native
and Wasm Clippy with warnings denied, `cargo test --locked` (1083 passed,
zero failed/ignored, including all 11 CD R–W integration tests), all 97 fixture
reproduction commands,
`sh scripts/build-web.sh`, all 12 CI-runner tests and all 67 browser groups
(201694 ms, two workers). An initial CI-runner execution started before the
Wasm assets were built and timed out waiting for Ready; the rerun after the
completed build passed. The rebuilt production Wasm SHA-256 is
`17baeb14da12ca36392620b102c02066b97db21217fea17aa6c8bd2cf7e9b0e5`.
