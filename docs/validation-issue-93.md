# Issue #93 validation

Related to #93 and #17. The [focused hardware contract](cd-streaming.md) defines
the primary-derived overlap/ownership matrix, original guest and remaining
limits. This is a bounded CD streaming increment, not Saturn hardware completion.

## Final inherited-main negative control

Before the production correction, an initial guest/device probe failed on the
missing buffer-full notification. After fixture development, the final unchanged
guest, both media and independent native oracles were run again with
`src/cd.rs` from `bf76a3ffe26872a72157e9ddfaf6c84be045faf6`. Other production
files match that inherited revision. A `try/finally` restored the corrected file;
no expected result was weakened or converted into an expected failure.

```text
cargo test --locked --test cd_stream firmware_guest -- --nocapture

base layout:  pc 06021270, phase 3, HIRQ sample 0FE5, status E093
              trace end 260746E0, completed fast sum 0017E8AC
moved layout: pc 06021270, phase 3, HIRQ sample 0FE5, status E093
              trace end 26074820, completed fast sum 001FE160
expected success 600D; actual 57491(E093), expected 24589(600D)
0 passed; 2 failed; execution 1.76s; command 3.222s; exit 101

cargo test --locked --test cd filling_pool_publishes -- --nocapture

BFUL at second accepted sector, count 2: actual 4, expected 12
0 passed; 1 failed; execution 0.01s; command 0.387s; exit 101
```

The guest already booted through original firmware/AIP, discovered each asset,
completed its fast streaming pass and calculated the independently expected
full-file sum. It then retained 198 sectors in a separate partition and observed
two new drive acquisitions. CSCT was set but BFUL absent. The negative device
control isolates the missing event at 354022 reference clocks after Play
acceptance. A finite two-sector range never attempts another acquisition, so
the notification stays missing. ST-162 §4.2, printed38/PDF26, requires the
full-buffer pause/BFUL transition, independently of any command polling policy.

The initial correction added the post-allocation occupancy check in `drive_frame`.
The same final guest and test oracle pass with it. To reproduce in an independent
worktree at `bf76a3f`, copy `tests/cd_stream.rs`, the four `cd-stream` fixture
outputs and the added device tests from this checkpoint, then run the two
commands above. The selected GitWeave ancestor `2f1aae1` inherits the same
production implementation; its administrative checkpoint commits do not change
this control.

## Artifacts and independent oracles

| Artifact | SHA-256 |
| --- | --- |
| Unchanged external-startup.hex | ddacd88bfe52625514c49d35ea103f34df563fb113c8dd2747f8f367f7eecf6f |
| cd-stream.s | b1038a788aeb27f28635ed304290391882b2f5061252d1d04ffd5bf024a10b08 |
| cd-stream.bin | 848fcb7fc71ee7ae647626958257d25606d0ceb41edd1e15af4a31f9510024fc |
| cd-stream.iso | 0525aeba7025ade5decbd2bffe92577e1364ea19a603df9025b766786dfdf00f |
| cd-stream-moved.iso | 4f24f0f60b2af043cf353f813a1ecc73632bd3677cab9f58d5bd7772d3396b32 |

`python3 tests/fixtures/build_cd_stream.py --check` reconstructs every output
from original MIT source. Both 128-sector discs carry the same 12300-byte
application, root/data layouts 20/64 and 27/88, lengths 12291/16387 and distinct
payload formulas. Native/JS oracles independently construct complete expected
bytes, canaries, prefix sums, final colors, FAD identities and DMA addresses.
No core-generated screenshot, FIFO snapshot or success flag supplies expected
content. Changing actual asset byte 0 changes both transferred arrays and the
guest-derived full-frame color; cold reset reexecutes the selected medium.

Six new native guest tests and two additional device tests retain all 1142 prior
Rust tests. One new browser scenario retains all 72 old groups; one new fixture
builder retains all 101 old commands. All 13 runner tests retain independent
assertions, including the tested-artifact digest and two-shard publication
barrier. No timeout, retry policy, published asset allowlist or workflow gate is
relaxed. Existing synchronous runtime, BIOS-less HLE, original games, Hardware
Lab, audio, graphics, input and backup checks remain registered.

The focused browser scenario passed 665 assertions in 16.223s (16.835s including
setup), using production Wasm and Chromium 145.0.7632.6 under `/saturn-rust/`.
It covers both layouts, pre-PEND Canvas/CPU progress, changed bytes, reset while
the moved layout has a slow FIFO, complete final pixels, and a fixed 2000000-step
snapshot comparing chunks 10000 and17 plus passive inspection. Native compares
chunks 1/3/17/10000 and complete clock/CPU/RAM/framebuffer state. Device tests
apply the same chunks at literal before/at/after drive events, masked BFUL,
partial GetDelete, competing FIFO WAIT, immediate soft reset and rearm.

The guest aborts a real pending directory/drive acquisition before its first
sector, checks EFLS, rejects an unavailable FID and starts fresh Play. Native
controls include cold repeat, SYSRES with retained backup sentinel, reset with
an accepted slow FIFO and absent media. Existing unit/device/SCU suites retain
pending selector cancellation, medium-change, no ghost DMA writes, byte/word
publication, physical source retention, IRQ masking/acknowledgment, retry/error,
empty GET, reserved identity and command-rejection tests. This is focused
coverage, not a claim to exhaust all cross-chip interleavings.

## Complete checks

All required local commands passed. The two browser shards verified the same
production site before their successful aggregate result.

| Command | Result / elapsed seconds |
| --- | --- |
| `cargo fmt --check` | Passed / 0.375 |
| `cargo build --locked` | Passed / 1.201 |
| `cargo test --locked` | 1150 passed, zero failed/ignored / 410.635 |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed / 0.968 |
| `sh scripts/check-fixtures.sh` | All 102 commands passed / 4.919 |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed / 10.326 |
| `sh scripts/build-web.sh` | Passed / 1.916 |
| `node --test scripts/ci-runner.test.mjs` | All 13 passed, zero skipped / 13.535 |
| Complete `npm run test:browser` | All 73 groups passed / 444.094 |

The native run has 102 harnesses including doc tests; summed harness time is
375.420 seconds. The six new guest tests took 8.750 seconds in that complete
run. Compared with the #91 record's 378.883-second full native invocation,
this invocation took 31.752 seconds longer, including compilation and concurrent
browser/tooling work. The summed harness difference is 1.540 seconds; neither
comparison isolates the new tests' CPU cost. These are local shared-host
measurements, not hosted CI performance claims.

Production Wasm SHA-256:
`da75ee2e44b6da6aa284e472acd9c583471addd505d4f5932f9c44e8280238c0`.
The complete browser aggregate passed all 73 groups: 30 shared groups in
243.981 seconds, 43 scenarios in 443.635 seconds, and 443.868 seconds measured
by the supervisor (444.094 seconds including npm overhead). The new streaming
group took 17.010 seconds in that full invocation. Compared with the #91
record's 447.347-second full browser invocation, wall time decreased by
3.253 seconds despite the added coverage; shared-host variance prevents
attributing that change to this patch. No group was retried, dropped or skipped.
The source/media allowlist and tested-artifact barrier remain unchanged.
The tested-site SHA-256 is
`346855d8aac579063ff9ad346857db373ff8e210aa76a483c67433b1a7fcd898`.
The aggregate validates both shard manifests against that directory digest;
`web/pkg/saturn_rust_bg.wasm` matches the Wasm hash above.

After the complete runs, final review strengthened the reset probe to require
the slow stream's BFUL sample, actual GET FIFO consumption and an uncompleted
DMA IRQ target, excluding the earlier reservoir PUT. It also added explicit
native/Wasm IRQ-count equality and a data-derived cached-kernel result oracle.
All 50 affected native CD/guest tests passed again in 9.804 seconds, final
formatting/Clippy passed in 0.384/0.160 seconds, and the final browser streaming
group passed all 673 assertions in 16.718 seconds (17.322 seconds including setup, 17.769 seconds
including npm overhead). No production, generated guest/media or built Wasm
bytes changed during these assertion refinements. The final strengthened
oracles were also rerun against inherited main, giving the negative results
above. Relative documentation file links, JavaScript syntax and Git whitespace
were checked.

## CI shard timeout correction

Review found that [GitHub Actions run 41378197649](https://github.com/takahirox/saturn-rust/actions/runs/41378197649)
failed the required `build` gate: `scenarios` exhausted its 1,200-second worker
budget during `scspMvol` and never reached `largeMedia`. `cdStream` itself passed
in 46.341 seconds. Native, Wasm and `shared` jobs succeeded, but that does not
make the failed aggregate acceptable.

The correction moves only the independent `cdRuntime` and `externalDiscovery`
modules from `scenarios` to `shared`, after the complete original UI trace.
Their hosted times were 214.346 and 183.158 seconds, versus 516.087 seconds for
the original shared trace. Each moved module uses the same fresh-context runner
as before; the shared trace's context is measured and closed before either starts.
The split is now 32 shared groups and 41 scenarios. All 73 groups, independent
oracles, individual `--group` commands and original `--serial` order are retained.
The two-worker cap, 20-minute limit, failure propagation and artifact/publication
gates are unchanged; no retries or test-clock reductions were added.

The focused inventory/aggregate/publication checks passed, followed by all 13
runner regressions (14.540 seconds, none skipped). The production web build
passed in 6.082 seconds and generated the same Wasm SHA-256 recorded above.
JavaScript syntax, Rust formatting and Git whitespace checks passed.

The complete `npm run test:browser` command passed all 73 groups and the
unchanged artifact/revision aggregate in 388.176 seconds (387.957 seconds in
the supervisor). Successful report times were 387.719 seconds for 32 shared
groups and 306.772 seconds for 41 scenario groups. `cdRuntime` and
`externalDiscovery` passed in 77.775 and 65.018 seconds on shared; `cdStream`,
`scspMvol` and `largeMedia` passed on scenarios in 17.062, 61.704 and 5.340
seconds. No groups were skipped or retried. The aggregate verified the same
site digest recorded above. Compared with the earlier 444.094-second local
full-suite record, elapsed time decreased by 55.918 seconds; these shared-host
runs are not a controlled performance comparison or revised hosted-CI evidence.

The first local build selected Homebrew Rust without Wasm target libraries;
setting `RUSTC` to Rustup stable's compiler fixed the environment. Initial browser
launches exposed an incomplete cached Chromium framework and then an absent
headless shell. Downloading and extracting the exact pinned Chromium and shell
revision 1208 (145.0.7632.6) under ignored `target/issue-93-shards/browsers`
resolved these setup failures. No dependency or browser revision changed.

This correction changes CI orchestration and documentation only. The prior
complete native/fixture/Clippy results remain recorded above; those unaffected
suites were not repeated for this correction. No successful revised GitHub
scenario or aggregate check is claimed: this node must not push or merge.
After publication, require successful `browser (shared)`, `browser (scenarios)`
and required `build` checks for the revised PR head before approval.

## Review correction: full-buffer pause across an audio transition

The review regression retains 199 sectors in partition 2 and plays a one-sector
data track followed by two audio sectors. Against the unchanged reviewed
implementation (`25a8df7`), the new
`full_buffer_pause_spans_data_to_audio_and_resumes_without_consuming_samples`
device test failed: after PAUSE/FAD 151 and CSCT/BFUL=12, `audio_sample()` returned
`[4660, -1234]` instead of `[0, 0]`. The command exited with status 101; one test failed in 0.01 seconds
of test execution. This reproduces the review's hardware-visible sample leak.

ST-162-062094 §4.2, PDF26/printed38, specifies the full-buffer PAUSE/BFUL and
automatic continuation from the retained position when space is available.
The correction gates audio sampling on the armed pause, preserving sample/FAD
position, and performs the existing space/resume check before the audio-track
return in `drive_frame`. The directory workspace exception remains intact.
No new unconditional pool restriction on audio playback or timing policy is added.

The regression uses chunks 1/3/17/10000 with passive CR4/FIFO inspection. It checks
the literal first-sector deadline, silence and retained FAD while full, silence
between freeing a slot and the next drive deadline, PLAY/FAD 151 after recovery,
sticky BFUL and explicit acknowledgment, all 1176 distinct stereo frames,
FAD 152 after frame 588, PEND/FAD 153 after frame 1176, and retained data bytes.
It refills the pool after recovery to distinguish an armed pause from ordinary
audio playback with a full pool. The focused corrected test passed all four
stepping variants in 0.04 seconds. Original guest/media fixtures are unchanged.

All applicable local checks were repeated for this production correction:

| Command | Result / elapsed seconds |
| --- | --- |
| `cargo fmt --check` | Passed / 0.405 |
| `cargo build --locked` | Passed / 1.205 |
| `cargo test --locked` | 1151 passed, zero failed/ignored / 411.942 |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed / 2.739 |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed / 5.163 |
| `sh scripts/check-fixtures.sh` | All 102 commands passed / 3.791 |
| `sh scripts/build-web.sh` | Passed / 5.654 |
| `node --test scripts/ci-runner.test.mjs` | All 13 passed, zero skipped / 14.632 |
| Complete `npm run test:browser` | All 73 groups passed / 392.306 |

The native run retained all 102 harnesses, with summed execution time 377.160
seconds. The browser aggregate verified both complete shards against the same
production site: 32 shared groups in 391.761 seconds and 41 scenario groups
in 303.870 seconds, supervisor 392.077 seconds. The existing streaming group
passed in 17.352 seconds. Relative to the preceding recorded native invocation
(410.635 seconds) and rebalanced browser invocation (388.176 seconds), elapsed
times increased by 1.307 and 4.130 seconds. These concurrent shared-host runs do
not isolate the added regression's cost; no coverage, retry or timeout changed.

Production Wasm SHA-256:
`ac53c1a8dac597dc8f085e5b2df1ae01c4aa7d244e8ed1b58220b96fe93c85ad`.
Tested-site SHA-256:
`b858db20ee9a4db444a08ae8a5a0fa8e879a881b2fd73b433e6580dd522a00a5`.
Initial runner attempts failed because the cached pinned browser installation
was incomplete and its matching headless shell was absent. Downloading the
unchanged pinned Chromium/headless-shell 145.0.7632.6 archives and extracting
with `ditto` into ignored `target/issue-93-audio-review/browsers` resolved setup.
The successful runs used that directory and `BROWSER_CHANNEL=chromium`.
Relative documentation links and Git whitespace checks passed. Required hosted
PR CI remains pending; this correction is committed locally without pushing.

## Review correction: R–W decoding during buffer-full PAUSE

The new device regression
`buffer_full_pause_defers_rw_publication_until_resume_and_preserves_pack_order`
fails against the reviewed production implementation (`e495450`). It enables
R–W decoding, retains 199 PUT sectors in partition 2, and plays one data sector
followed by two audio sectors with an original SRW1 companion. After 200000
clocks, status is PAUSE/FAD 151 and retrieval returns WAIT (`8100`). After
another 1000000 clocks, status remains PAUSE/FAD 151, but retrieval incorrectly
returns `4100` instead of `8100`. The unchanged oracle failed in 0.02 seconds
of test execution, with exit 101.

ST-162-062094 §4.3, PDF28/printed40, restricts R–W decoding to CD-DA PLAY.
`sync_rw` now includes the armed buffer-full pause in its activity predicate;
the retained `playing` flag alone cannot start acquisition or publication.
Freeing a slot still resumes at the existing drive deadline. The regression
checks WAIT and an empty FIFO throughout the pause, WAIT immediately after
freeing a slot, PLAY/FAD 151 after automatic recovery, all eight distinct valid
packs in literal source order across both audio sectors, full DataEnd counts,
and no extra pack. It repeats with chunks 1/3/17/10000 and passive inspection
off/on. The corrected focused test passed in 0.10 seconds. Existing guest/media
fixtures and browser assertions are unchanged.

All applicable local checks passed again for this correction:

| Command | Result / elapsed seconds |
| --- | --- |
| `cargo fmt --check` | Passed / 0.454 |
| `cargo build --locked` | Passed / 1.531 |
| `cargo test --locked` | 1152 passed, zero failed/ignored / 438.754 |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed / 4.813 |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed / 7.411 |
| `sh scripts/check-fixtures.sh` | All 102 commands passed / 4.347 |
| `sh scripts/build-web.sh` | Passed / 11.376 |
| `node --test scripts/ci-runner.test.mjs` | All 13 passed, zero skipped / 12.817 |
| Complete `npm run test:browser` | All 73 groups passed / 394.643 |

The native run retained all 102 harnesses, with summed execution time 394.870
seconds. The browser aggregate verified 32 shared groups in 394.178 seconds and
41 scenario groups in 304.802 seconds against the same production site; the
supervisor took 394.415 seconds. `cdStream` and `cdRw` passed in 19.964 and 4.338
seconds. Relative to the preceding full-buffer audio correction's complete
runs, native/browser elapsed times increased by 26.812/2.337 seconds. These
concurrent shared-host measurements do not isolate the new device test's cost.
No existing coverage, retry policy or timeout changed.

Production Wasm SHA-256:
`51e899a990435d76cba1908906228d2aadc4ded02e11fefa0652c6b8add73291`.
Tested-site SHA-256:
`017a41e9005199aba0c1bbd47bcd271506468ccf176fef7f76b61042e99d3198`.
The first runner attempt failed because the matching headless shell was absent.
The pinned Playwright download completed but extraction stalled; stopping only
this task's installer processes and extracting that same archive with `ditto`
under ignored `target/issue-93-rw-review/browsers` resolved setup. Successful
checks used `PLAYWRIGHT_BROWSERS_PATH` pointing there and Chromium headless shell
145.0.7632.6. No browser or dependency revision changed. Relative documentation
links and Git whitespace checks passed. Hosted checks for the subsequently
published PR head remain required; this node does not push or merge.

## Corrected development failures and environment

The first root-read attempt did not clear firmware PEND before starting a new
range; it was corrected to clear the owned completion factor. The initial FRT
read used a word access; SH7604 §11.3 requires high/low byte accesses. Adding a
cache-warmed DMA kernel exposed a shared IRQ counter read through a cached
address; the guest now reads the uncached alias used by the firmware handler.
The first kernel call also passed GO through R0, which the fixture's call helper
uses for its target; GO now uses R4. These were original guest mistakes fixed
before the final inherited-main control, not emulator failures.

The first intermediate video oracle examined only one field boundary, which
could return the field begun before the color change. Two boundaries expose
computed raster pixels while PEND remains clear; native and production Canvas
assert that condition explicitly. An initial browser assertion used the Rust
enum name instead of the binding's established `User-provided BIOS` string.
Runner inventory checks correctly rejected the additional builder while their
expected count still said 101; the exact count is now 102 and explicitly includes
the new builder, without removing any existing entry.

Tooling uses Rustup stable Rust 1.99.0, wasm-bindgen 0.2.129, Node 26.12.1,
Playwright 1.58.2 and its pinned Chromium 145.0.7632.6 on macOS arm64. npm packages
were initially absent. Playwright's ZIP extraction stalled on this host; only
this task's installer processes were stopped, and macOS `ditto` extracted the
same pinned archives into ignored `target/issue-93/browsers`. Both full Chromium
and matching headless-shell are available there; the runner uses supported
`BROWSER_CHANNEL=chromium` and `PLAYWRIGHT_BROWSERS_PATH` overrides. No browser
revision, assertion or timeout changed. Manuals, PDF text tools, temporary logs
and downloaded executables stay under ignored `target/`.

## Hosted validation for the reviewed head

[GitHub Actions run 41384873482](https://github.com/takahirox/saturn-rust/actions/runs/41384873482)
completed successfully on 2026-11-02. It validates PR #94 head
`3c6fd1bb341d60b5da34487c824b969fd9c7cacb` through GitHub's tested merge commit
`a98b57aaa6c04af084a2b995242ad4e44404c604`, combining that head with inherited
main `bf76a3ffe26872a72157e9ddfaf6c84be045faf6`. The worktree files matched the
reviewed head before this validation-record update.

| Hosted job | Completed evidence / command elapsed seconds |
| --- | --- |
| [native](https://github.com/takahirox/saturn-rust/actions/runs/41384873482/job/125083627164) | Fixtures passed / 5.797; `cargo fmt --check` / 2.956; `cargo build --locked` / 8.731; `cargo test --locked`: 1152 passed across 102 harnesses, zero failed/ignored / 873.302; all-target Clippy with `-D warnings` / 5.499 |
| [wasm](https://github.com/takahirox/saturn-rust/actions/runs/41384873482/job/125083627355) | Production web build / 20.522; Wasm Clippy with `--locked` and `-D warnings` / 8.489; exact web-build artifact uploaded |
| [browser (shared)](https://github.com/takahirox/saturn-rust/actions/runs/41384873482/job/125083810209) | All 13 runner regressions passed, zero skipped / 21.842; all 32 browser groups passed / 820.319 |
| [browser (scenarios)](https://github.com/takahirox/saturn-rust/actions/runs/41384873482/job/125083810149) | All 41 browser groups passed / 828.773; `cdStream` / 47.085; `cdRw` / 10.955; `scspMvol` / 168.589; `largeMedia` / 14.076 |
| [build](https://github.com/takahirox/saturn-rust/actions/runs/41384873482/job/125087602795) | Required native/Wasm/browser results all `success`; exact artifact/revision aggregate passed for all 73 groups at 04:02:39 UTC |

The aggregate log reports `scenarios=828169ms/41 groups` and
`shared=819816ms/32 groups`. Those are report durations; whole-command timings
in the table use outer `CI_TIMING` durations, and named scenario timings use
`BROWSER_TIMING`. Both reports passed the existing revision,
site digest and complete-group checks against the downloaded production build.
The web-build ZIP SHA-256 is
`e21bfcffcc0d5ebee044ffdefb90530bf3eef07baa17f2338c618c726ffdfa99`;
this is the uploaded archive digest, not the site-directory or Wasm digest.
Hosted fixture verification reproduced all four `cd-stream` hashes recorded
above. No retry, coverage reduction, timeout change or production fix was needed
to resolve the review's pending-hosted-validation finding.

The runner regression log deliberately includes an intentional worker failure;
all 13 runner tests and both real shards succeeded. `site` and `deploy` were
skipped as specified for a pull-request run. Earlier pending-hosted statements
above describe the state when those local corrections were committed; this run
supplies the completed hosted evidence for the reviewed head.

## Pre-merge handoff and limits

Required hosted validation passed for the reviewed head identified above. This
evidence update changes documentation only; it does not claim hosted validation
for a subsequent published revision. Downstream must require successful
applicable PR jobs for the revision being merged. This node does not push,
merge or comment. No post-merge verification, physical Saturn, proprietary
firmware/disc, human sign-off or live Pages inspection is required by #93.

Full cross-chip arbitration, arbitrary asynchronous CDC/GFS calls, real Sega
BIOS/SDK ABI and commercial games are not verified. Remaining command/connector,
optical and broader #17 hardware requirements stay pending. The #81/#82 endpoint
evidence finding and draft/publication rule are unchanged. Successful local
checks establish this bounded increment, not umbrella hardware completion.
