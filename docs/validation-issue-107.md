# Issue #107: browser execution cost and scheduling

Related to #107 and PR #108. Exact-final-head hosted PR validation remains a
pre-merge gate: the reviewed head timed out in hosted shared validation, as
recorded below. This worktree-only follow-up does not push or merge. No post-merge
check is required. No emulator, fixture, hardware deadline or expected result changes.

## Hosted baseline and critical path

Read the actual job logs of [failed main run 42160464316](https://github.com/takahirox/saturn-rust/actions/runs/42160464316)
at `4ffcb97c6f028def7da1085ebee21bce9ee6b959` and
[passing PR run 42157801981](https://github.com/takahirox/saturn-rust/actions/runs/42157801981)
at `2f8cd6086d5ce2eeaf6ac84d2abca0993969131e`. GitHub's compare API reports
no file differences between those revisions. These are two samples of the same
workload, not stable performance guarantees. Node was 24.22.2 on hosted Ubuntu
24.04; no hosted process CPU/RSS measurements were available.

| Browser worker elapsed (seconds) | Passing PR | Failed main |
| --- | ---: | ---: |
| Shared, 31/31 | 813.726 | 784.438 |
| Scenarios | 1,053.664 (43/43) | 1,199.608 (42/43, incomplete) |
| Sum of workers | 1,867.390 | 1,984.046 (incomplete) |

Job API start/completion intervals separately give 1,943/2,071 s summed browser
runner time (PR/main), including setup and runner regressions; all non-skipped
validation jobs sum to 3,512/3,482 s. The first validation job start through
aggregate completion spans 1,527/1,360 s, excluding initial queue delay but
including inter-job waits. Queue durations were not separately measured. Native alone
took 1,513/1,354 s and was the overall critical path in both samples. Thus this
browser optimization cannot by itself establish the <10-minute investigation
goal for the whole workflow. These runner intervals are not host CPU time.

The main scenario worker reached the unchanged compact-transport **1,200 s**
limit (`CI_TIMING` 1,200.237 s, `timeout=true`, exit code 13). `scspMvol` was
active, with only three completed evaluate calls; `largeMedia` never started.
The active group's failed teardown record does **not** prove an assertion failed.
The unsettled top-level-await warning occurred during this forced termination.
Native, Wasm and shared passed; aggregate `build` failed and site/deploy skipped.
The passing PR completed MVOL (131.086 s) and large media. It does not demonstrate
adequate main headroom: the completed costly main groups were roughly 30% slower
than their PR samples, while shared was slightly faster.

| Group seconds | Passing PR | Failed main |
| --- | ---: | ---: |
| cdRuntime | 167.715 | 219.721 |
| externalDiscovery | 145.283 | 188.253 |
| chd | 101.959 | 132.134 |
| mosaic | 95.955 | 128.050 |
| rotationHighResolution | 75.981 | 100.574 |
| videoTransactions | 68.309 | 88.552 |
| cdStream | 36.470 | 48.175 |
| scspMvol | 131.086 | interrupted |

Shared's main costs were hardware boundaries (234.998 s), display modes
(136.377 s), HLE player (82.505 s) and clock (70.833 s). Its 415.562 s observed
worker headroom motivates a limited transfer, while retaining this substantial
shared workload and its ordered UI dependencies.

The six largest completed main groups consumed 857.284 s. Instrumentation locates
most cost inside guest execution, not setup or assertions. In main, CD runtime
spent 216.551 s inside callbacks versus 218.589 s evaluate elapsed, discovery
178.235/185.353 s, CHD 128.911/130.923 s, mosaic 121.098/126.207 s,
high-resolution rotation 91.616/96.937 s, and video transactions 76.893/85.468 s.
Module navigation took about 28–33 ms each. These are overlapping wall-clock
counters, **not CPU time**, and must not be added as independent phases.
The [machine-readable record](ci-performance-issue-107.json) retains every group
and the costly modules' operation counters from both runs and local validation.

| Hosted module (main, except complete PR MVOL) | Evaluate callbacks | `run_steps` calls | Frame advances |
| --- | ---: | ---: | ---: |
| cdRuntime | 25 | 7,243,273 | 6 |
| externalDiscovery | 100 | 8,354,814 | 0 |
| chd | 32 | 2,028 | 0 |
| mosaic | 62 | 580 | 240 |
| rotationHighResolution | 50 | 816 | 48 |
| videoTransactions | 133 | 1,406 | 72 |
| scspMvol (PR) | 53 | 1,754 | 0 |

MVOL also made 1,800 PCM reads and 46 media mounts in the complete PR run. The
scenario runner already creates only one fresh context per module; timings do
not support removing isolation to save setup. No process CPU/RSS or exclusive
memory-pressure measurements were collected locally, and none are inferred from
wall time or transfer counts.

## Work reduction and retained defect detection

Two existing native integration tests independently cover the combinations
previously replayed in both native and browser. They use original reset-vector
guests, literal expected records/data, real device clocks, and CPU/RAM snapshots:

| Browser change | Independent retained native coverage | Browser coverage retained |
| --- | --- | --- |
| `externalDiscovery`: 24 fixed-clock runs → 12 | `startup_records_are_independent_of_step_chunks_and_passive_cd_inspection` in `tests/external_startup.rs`: all three layouts × 1/3/17/10000 chunks × inspected/uninspected | Every layout and every chunk; small chunks inspect FIFO/responses, bulk does not; compare complete snapshots and authored success oracles |
| `cdRuntime`: 8 fixed-clock runs → 4 | `runtime_chunks_and_passive_fifo_and_response_inspection_are_identical` in `tests/cd_runtime.rs`: both layouts × 1/3/17/10000 | Both layouts × 17/10000, inspected versus uninspected; compare full clocks/registers/logs/data and authored success oracles; Wasm 1/3 boundaries also remain in discovery |

This removes **22.4 million emulated steps of duplicate replays** (12 × 1 million
and 4 × 2.6 million), without shortening any surviving run. The original one
million / 2.6 million step boundaries remain exact. The fixed-state comparisons
still catch chunk-dependent clocks, DMA/IRQ records, CPU state, FIFO/response
inspection side effects and RAM changes. Native independently exercises all
removed combinations. Wasm still exercises tiny, odd and bulk calls through the
same production binding. No expectation is supplied by the emulator itself.

All successful/restart executions, all three discovery layouts, both runtime
layouts, 47 metadata controls, changed fetched instructions/extents/assets,
decode errors, omitted commands, cancellation/reset phases, SYSRES/backup,
inherited fixed-ROM rejection, full executable/IP/buffer bytes and canaries,
runtime Canvas and upload/privacy assertions remain. This is reduced duplicate
combination execution; it does not claim identical repetition counts.

Bounded profiling of other candidates:

- **CHD:** preserve all codecs, ISO/CUE/raw/subcode baselines, PCM, UI errors and
  reset/restart. Default and forced-cdlz fixtures have different SHA-256 hashes;
  equal decoded output alone does not justify skipping either decoder input.
- **Mosaic (initial pass):** kept all 60 guest executions, 240 frame advances, both density/field
  paths, source/enable mutations and repeated cold boots. Production Canvas and
  field history add browser coverage beyond native arithmetic.
- **High-resolution rotation (initial pass):** kept all 48 executions/advances and full
  640/704-wide Wasm/Canvas images. Input/output protocol work is secondary to
  guest execution; the existing compact transport already handles bulk arrays.
- **Video transactions:** keep all 132 BIOS resets, distinct VBE/HDTV, bus
  ownership, retained-word, cycle acquisition and omitted-write controls.
- **MVOL (initial pass):** kept all 46 executions, repeated cold resets, gain/source mutations,
  exact mute/resume progress and all five production AudioWorklet deliveries.
  The passing hosted sample spent 127.668 s inside callbacks versus 129.937 s
  evaluate elapsed; assertions totaled only 1.923 ms. Reducing assertion strictness
  or PCM transfer cannot address that cost safely.
- Keep instrumentation unchanged for comparable evidence. Per-call timing
  overhead on millions of small calls is real but not isolated by these counters;
  no CPU or instrumentation-only savings are inferred.

## Scheduling and runner integrity

The actual registry is **74 groups**, originally **31 shared / 43 scenarios**.
Move only `chd` and `cdStream`, after `hardwareBoundaries` following the unchanged
ordered Play/UI trace. Main's observed shared + CHD + streaming times total
**964.747 s**, with 235.253 s to the worker limit; the corresponding PR sum is
952.155 s. These sums motivate the assignment, not measured new hosted results.
CD work reduction separately reduces the remaining scenario load.

The new assignment is **33 shared / 41 scenarios**. Every independent module
still gets a fresh context/page/emulator after the trace context closes. The
registry and serial order are unchanged; targeted commands remain available.
Two browser workers, compact/raw limits (1,200/3,600 s), cancellation and forced
teardown, statuses, expected-result checks, report integrity, exact revision,
tested-asset digest and mandatory aggregate/publication gates are unchanged.
Runner regressions check inventory/assignment, reject the old successful 31/43
reports, and reject missing or failed CHD, streaming, MVOL and large-media groups.

## Executed local validation

Apple M4 Max / 64 GiB / macOS arm64, Node 26.12.1, Playwright 1.58.2,
stable Rust 1.99.0 and bindgen 0.2.129. The same production Wasm was used for
both complete comparison runs (SHA-256 in the machine-readable record). Chrome
157.0.8103.38 ran both comparisons with `BROWSER_CHANNEL=chrome npm run
test:browser`. Baseline workers loaded the original modules/manifests before
edits; optimized workers loaded the revised ones. Runs were sequential; no other
native/browser validation from this task ran alongside them. The host was not
reserved from unrelated activity. These are single samples, not a variance study
or a hosted-performance prediction.

| Complete comparison, seconds | Baseline 31/43 | Optimized 33/41 |
| --- | ---: | ---: |
| Shared worker | 384.043 | 452.488 |
| Scenario worker | 510.952 | 417.721 |
| Sum of browser worker elapsed | 894.995 | 870.209 |
| Complete suite elapsed, including verification/cleanup | 511.427 | 453.747 |
| Verified groups | 74 | 74 |

Observed summed worker time fell **24.786 s (2.77%)**; feedback elapsed fell
**57.680 s (11.28%)**. The changed groups alone saved 30.630 s; other group/setup
variation offset part of that. The elapsed improvement is mostly scheduling;
do not call it an 11.28% reduction in test work or CPU. Local execution has no
Actions runner allocation intervals; its worker sum is not billed runner time.
Revised hosted summed runner time and overall feedback remain unverified.

| Costly group seconds | Baseline | Optimized |
| --- | ---: | ---: |
| cdRuntime | 81.324 | 65.312 |
| externalDiscovery | 67.802 | 53.184 |
| chd | 49.018 | 46.794 |
| cdStream | 17.772 | 17.610 |
| mosaic | 44.528 | 45.565 |
| rotationHighResolution | 35.763 | 38.008 |
| videoTransactions | 31.746 | 31.315 |
| scspMvol | 64.313 | 66.517 |
| largeMedia | 5.851 | 5.807 |

CHD/streaming retain identical work despite moving shards. The two changed CD
modules reduce evaluate callbacks from 125 to 109 (25→21 and 100→88). Keep
all unchanged module variation separate from the mechanically removed guest
replays. Neither complete comparison reproduced the unsettled-await warning.
Observed optimized margins to 1,200 s were **747.512 / 782.279 s** locally;
these margins do not establish hosted headroom.

Commands already passed:

- `cargo fmt --check`, `cargo build --locked` and
  `cargo clippy --locked --all-targets -- -D warnings`.
- `sh scripts/check-fixtures.sh`: all **104** registered commands passed
  (4.356 s), including complete original CHD fixture reproduction checks.
- `sh scripts/build-web.sh`: production release Wasm build passed, followed by
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`.
- Both complete Chrome comparisons above, each with two independently verified
  success reports covering all 74 groups and the exact built asset digest.

Rust commands use the stable sysroot's `bin` first in `PATH`, plus
`/Users/takahiro/.cargo/bin` for bindgen. An initial build using the host's other
Rust installation failed because its Wasm target was absent; selecting the
already-installed stable toolchain resolved it. Initial Chromium setup also
stalled during Node ZIP extraction, and launching its incomplete installation
failed before testing. The pinned 145.0.7632.6 headless shell was then extracted
from that same downloaded archive with system `unzip` into a worktree-local
cache, and its real launch/version checked. No dependency, timeout or retry
policy was changed.

Pinned production-browser validation also passed, with no native validation
scheduled alongside it:

- `PLAYWRIGHT_BROWSERS_PATH=target/issue107-browsers node --test
  scripts/ci-runner.test.mjs`: **13 passed, 0 failed/skipped**, 13.188 s.
  After retaining the earlier runtime/discovery assignment negative control,
  the affected aggregate test was rerun with `--test-name-pattern='aggregate
  rejects'`: **1 passed**, 0.530 s. All other tests were unchanged.
- `PLAYWRIGHT_BROWSERS_PATH=target/issue107-browsers npm run test:browser`:
  **74/74 groups passed** in Chromium **145.0.7632.6**. Shared was **448.106 s**,
  scenarios **411.155 s**, summed workers **859.261 s**, suite **448.364 s**.
  Both complete reports passed the real aggregate verifier. Margins to the
  1,200 s limit were **751.894/788.845 s**, respectively. MVOL and large media
  completed, with no timeout or unsettled-await warning. This additional run
  checks the pinned browser; do not compare its different engine with Chrome
  157 as another optimization measurement.

Finally, `python3 scripts/ci-time.py native-tests cargo test --locked` passed
**1,193 tests, 0 failed/ignored**, across 106 test binaries plus doc tests, in
**519.236 s including compilation**. Native validation ran after the browser
measurements. The sum of actual harness durations was **475.460 s**; the
`NATIVE_TIMING` annotations repeat those same results and must not be counted
again. Both retained CD chunk/inspection matrix tests passed. No native test or
guest was changed to obtain the browser savings.

`git diff --check`, evidence JSON parsing, all three local runs' exact 74-name
inventory/status checks, and relative documentation link checks passed. The
small implementation changes only shard assignment/selection, two browser CD
combination loops, runner negative controls and this performance evidence.

## Hosted review follow-up: timeout remains

[PR run 42169608017](https://github.com/takahirox/saturn-rust/actions/runs/42169608017)
targets the reviewed PR #108 head `98b5a3fc6a952e614f7e385ce1e8fe55e583cc4f`.
Checkout tested merge revision `814212e65921fa421e1f103edf3d2bea66bcfa34`, combining
that head with main `4ffcb97c6f028def7da1085ebee21bce9ee6b959`. This is actual
hosted evidence, separate from the local comparisons above. Wasm build and
Clippy passed (27.342/11.412 s), as did the hosted runner regressions (28.934 s).
Native also passed fixtures, fmt, build, tests and Clippy. Its timed stages were
7.208/1.164/15.120/1,457.522/13.987 s, respectively; all **1,193 native tests**
passed, with 0 failed/ignored (excluding duplicate `NATIVE_TIMING` annotations).
The mandatory aggregate
`build` job then failed with `native=success wasm=success browser=failure`;
site/deploy skipped. The workflow concluded **failure**. The first validation
job start through aggregate completion spanned **1,508 s**, and all non-skipped
validation job intervals summed to **3,958 s**. Neither is CPU time, and neither
demonstrates the <10-minute overall feedback investigation goal.

| Hosted worker | Elapsed seconds | Passed groups | Margin to 1,200 s |
| --- | ---: | ---: | ---: |
| Shared | 1,199.575 (interrupted) | 32/33 | Timed out |
| Scenarios | 1,123.675 | 41/41 | 76.325 s (6.36%) |

Shared reached the unchanged supervisor limit during `cdStream`, after CHD
passed in 135.212 s. Streaming's 5.061 s failed teardown record does **not**
identify an assertion failure: the supervisor reported `timeout=true`, exit
code 13, and `CI_TIMING` recorded 1,200.273 s with exit code 1. No shared success
artifact was uploaded. The scenario worker completed MVOL (178.874 s) and large
media (14.635 s); its command passed in 1,124.435 s. Thus only **73/74** groups
completed successfully, and the successful scenario shard also lacks substantial
observed headroom. The review finding remains unresolved.

| Costly group seconds | Reviewed hosted head |
| --- | ---: |
| hardwareBoundaries | 313.319 |
| display-modes | 184.964 |
| cdRuntime | 179.555 |
| scspMvol | 178.874 |
| externalDiscovery | 143.722 |
| chd | 135.212 |
| mosaic | 126.713 |
| hle-player | 112.585 |
| rotationHighResolution | 101.156 |
| clock | 98.747 |
| videoTransactions | 89.352 |

Summed browser worker elapsed was **2,323.250 s, incomplete**. Summed browser
job start/completion intervals were **2,405 s**, including setup and regressions;
the two jobs spanned 1,255 s from their common start through shared completion.
These are distinct from overall workflow elapsed and CPU time. With only
76.750 s left in the combined 2,400 s worker budgets before completing streaming,
this sample does not support solving the remaining headroom problem by partition
alone. Shared's original workload alone consumed about 1,059 s, versus 784 s in main;
the local before/after savings cannot establish hosted savings under this
variation. The measured duplicate-work removal remains real, but the current
implementation has not met the hosted runtime requirement.

The downloaded scenario artifact independently matched all 41 current manifest
names and passed statuses, the checkout revision above, and the hosted site's
digest `bb6dd2c59ee77f2b1f85b2bc35e380c3e74948e7e1b470f2a90fa095a38da97a`.
The downloaded `web-build` ZIP also matched GitHub's SHA-256
`83b696ce7ac6a16e885dd1a1dcc2242ec4f62d8937210e6cd4760dcf44946d74`.
`verifyDirectory` rejected the available report directory with
`Missing/extra shard file`, as required. The worker timings, complete reported
group inventories (including the interrupted group), costly-operation counters
and Wasm call counters are retained under `hosted.review-*` in the
[measurement record](ci-performance-issue-107.json). The worker elapsed above
uses `BROWSER_TOTAL`; the successful scenario artifact records 1,123.785 s after
report-integrity work.

The fresh follow-up worktree initially lacked Playwright, so the focused runner
command could not start. After `npm ci --ignore-scripts` and restoring `web/pkg`
from the hosted artifact, `node --test --test-name-pattern='inventory|timed
commands|crashed|aggregate rejects|corrupted real|web build binds|CI gates'
scripts/ci-runner.test.mjs` passed **7 tests, 0 failed/skipped** (1.436 s).
No timeout, retry, assertion, test inventory or implementation change was made
in this follow-up, and no hosted run was retried.

## Second hosted result and review fix

[Run 42172508446](https://github.com/takahirox/saturn-rust/actions/runs/42172508446)
has now completed on published head `71d71160c3ef547e6bbf6682d6230b63c93df2ce`,
tested through merge revision `6daf26c10fb723cd3ef9566bc8a0cc8a35142e49`.
Native passed all 1,193 tests (1,259.932 s including compilation), fixtures,
fmt, build and Clippy. Wasm build/Clippy passed in 15.272/9.290 s and runner
regressions passed in 29.703 s. Scenarios passed all 41 groups in **1,116.208 s**,
including MVOL and large media: only **83.792 s** margin. Shared again timed out
at **1,199.595 s** during streaming, after hardware boundaries (315.620 s) and
CHD (133.632 s) passed. Streaming's interrupted 18.372 s teardown record is not
an identified assertion failure. Aggregate `build` failed; site/deploy skipped.
The worker sum is **2,315.803 s, incomplete**, not a completed workload or CPU
measurement. This second failure reinforces the need to reduce work on both
workers, rather than transfer streaming back onto the nearly full scenario shard.

The review fix reduces duplicate browser executions in four modules. It keeps
every distinct fixture/input, authored result, mutation and omitted-write control.
Each retained run uses its original stepping bound, reset boundary, frame count,
pixel/audio/IRQ expectations and assertion strictness. Existing native tests
independently retain the fuller combination/repetition matrices:

| Browser executions before → after | Independent native coverage and retained browser contracts |
| --- | --- |
| SMPC SF and dual-SH-2 issuer: 8 → 4 per guest | `resdisa_sf_guests_preserve_claim_order_and_real_nmi_disable_results` and `dual_sh2_firmware_preserves_command_issuer_and_permitted_slave_results` in `tests/smpc_transactions.rs` retain all chunk/inspection pairs. Wasm retains 1/3/127/10000, inspected small/odd versus uninspected bulk, every allowed/prohibited issuer, sticky diagnostics, exact progress and command-literal mutations. The missing-SF native test retains every chunk and repeated boots. |
| Reset observer: 72 → 18 | `cold_reset_cancels_guest_commands_beyond_former_deadlines_and_preserves_battery` retains all 72. Wasm retains every command × offset (0/1/7), single-step inspected and bulk uninspected runs, fresh machines, original pending-command boundary, 100,000 observer steps and the shared native progress oracle beyond former deadlines. |
| Byte audit: 16 → 6 | `original_byte_firmware_publishes_real_calendar_memory_and_handshake_results` in `tests/smpc_byte_contract.rs` retains all 16. Wasm keeps all four chunks, passive inspection at 1/3, repeated cold boots at 127/10000, the complete authored trace and exact 2,000,000-step progress. |
| Raster INTBACK: 8 → 4 | `raster_gated_intback_forms_publish_results_and_service_each_irq` retains all eight. Wasm retains every chunk, inspection, the 500,000-step horizon, authored results and all six IRQs. |
| MVOL: 46 → 32 | `guest_mvol_amplitudes_distinguish_pcm_dsp_runtime_cdda_and_combined_sources` in `tests/scsp_mvol.rs` repeats all 20 mode/gain pairs. Wasm keeps every pair; repeats every source at gain 13 and combined mute/unity; retains all source mutations, omitted-write control, full mute/resume traces and five production AudioWorklet checks. |
| Mosaic: 60 → 36 | `mosaic_guest_fields_pixels_controls_and_repeated_cold_boots` in `tests/mosaic.rs` repeats all 30 inputs. Wasm keeps all 30 plus baseline repeats for every mode/density, every source/enable/parameter control, both captured fields and full Wasm/Canvas images. |
| High-resolution rotation: 48 → 32 | `guest_high_resolution_rotation_full_images_mutations_and_cold_boots` in `tests/rotation_high_resolution.rs` repeats all 24 inputs. Wasm keeps all 24 plus baseline repeats for every renderer/width, every mutation/omitted enable and full 640/704-wide Wasm/Canvas images. |

SMPC alone removes **30.4 million fixed-horizon duplicate steps**, plus removed
producer prefixes and early-fault repetitions. Mosaic removes 96 duplicate frame
advances and high-resolution rotation removes 16. This supersedes the initial
decision above to retain every duplicate visual/audio cold boot; native's full
matrices were inspected before making this change. Production browser reset and
field-history coverage remains, with fewer repetitions. No unique negative
control moves out of Wasm. CD runtime/discovery, CHD, streaming, video transactions,
the ordered shared UI trace and all other groups are unchanged by this fix.

The registry and assignment remain **74 groups, 33 shared / 41 scenarios**. Each
independent module still owns a fresh context. Both workers lose substantive
work; no additional shard move, timeout increase, retry, extra worker or gate
relaxation is introduced. PR #108's Related Issues section was corrected to
`Related to #107`; the PR remains draft and the Issue remains open.

The fresh comparison used the same M4 Max/64 GiB host, Node 26.12.1,
Playwright 1.58.2 and pinned Chromium 145.0.7632.6. Both samples used the same
production Wasm (`c207f061835b8809ba325032fbee18c3f219469334c67c6fd9ec41ab810054a3`,
956,666 bytes), with no native/build/other browser work from this task alongside
them. They were sequential single samples; the host was not reserved from
unrelated activity. Both ran `PLAYWRIGHT_BROWSERS_PATH=target/issue107-browsers
NODE_OPTIONS=--import=./target/issue107-profile.mjs npm run test:browser`.
The ignored temporary preload logged per-callback elapsed times from
`BrowserTimings.operation` for hardware boundaries in both samples; it changed
no assertions, selection, bindings or reports. The ordinary command reproduces
the full coverage, counters and shard totals without that extra logging.

| Complete local comparison, seconds | Reviewed workload | Review fix |
| --- | ---: | ---: |
| Shared, 33/33 | 486.632 | 383.251 |
| Scenarios, 41/41 | 448.096 | 365.478 |
| Sum of workers | 934.728 | 748.729 |
| Suite including verification/cleanup | 486.923 | 383.538 |
| hardwareBoundaries | 144.302 | 48.578 |
| scspMvol | 72.993 | 45.834 |
| mosaic | 62.791 | 27.297 |
| rotationHighResolution | 36.475 | 24.604 |
| cdRuntime | 67.425 | 67.906 |
| externalDiscovery | 54.822 | 53.941 |
| chd | 53.629 | 47.727 |
| cdStream | 18.610 | 18.108 |
| videoTransactions | 32.666 | 32.899 |
| largeMedia | 6.053 | 5.339 |

All 74 names/statuses were independently compared against the current registry,
and both pairs of reports passed the real aggregate verifier. MVOL and large
media completed; neither complete run timed out or emitted an unsettled-await
warning. Observed local margins are **816.749 / 834.522 s**. They do not establish
hosted margins. Summed worker elapsed fell **185.999 s (19.90%)**, while suite
elapsed fell **103.385 s (21.23%)**. The changed groups account for 170.248 s of
the worker saving; unchanged-group variation accounts for 15.751 s. Even the
changed groups contain host variation, so these are observed samples, not a
causal CPU estimate. Local runs have no hosted runner allocation intervals.

The baseline hardware callback profile measured byte audit at 68.194 s, reset
observer at 26.836 s, SF at 14.518 s and issuer at 19.943 s. These are callback
wall times, not CPU. The driver counters confirm actual work was removed:

| Calls before → after | hardwareBoundaries | MVOL | Mosaic | High-resolution rotation |
| --- | ---: | ---: | ---: | ---: |
| Evaluate callbacks | 311 → 207 | 53 → 39 | 62 → 38 | 50 → 34 |
| `run_steps` | 18,912,367 → 6,131,753 | 1,754 → 1,222 | 580 → 348 | 816 → 544 |
| `reset_bios` (including initial mode setup) | 478 → 298 | 47 → 33 | 61 → 37 | 49 → 33 |
| Frame advances | — | — | 240 → 144 | 48 → 32 |

MVOL media mounts fell 46 → 32 and PCM reads 1,800 → 1,254; all five worklet
checks remain. No CPU/RSS measurements are inferred from these counters.

A bounded build experiment enabled release LTO and one codegen unit. Its
completed costly groups showed no consistent gain: video transactions
32.666 → 32.480 s, display modes 66.039 → 69.388 s and CD runtime
67.425 → 70.113 s. It was cancelled through the normal supervisor
(`cancelled=true`, `timeout=false`), with no full-suite success claim. Both build
flags were reverted and the original Wasm restored before the final comparison.
The production build script, emulator and fixtures have no changes from this
experiment.

Fresh follow-up checks passed:

- `sh scripts/check-fixtures.sh`: all 104 commands, 4.394 s.
- `sh scripts/build-web.sh`: 2.602 s; its generated bindings and Wasm exactly
  match the saved production build used in both complete browser comparisons.
- `PLAYWRIGHT_BROWSERS_PATH=target/issue107-browsers node --test
  scripts/ci-runner.test.mjs`: 13 passed, 0 failed/skipped, 15.108 s including
  the timing wrapper. Missing/failed groups, assertions, timeouts, cancellation,
  stale reports/revisions, corrupted artifacts and publication gates still fail.
- `cargo test --locked --test smpc_byte_contract --test smpc_transactions
  --test scsp_mvol --test mosaic --test rotation_high_resolution`: **24 passed**,
  0 failed/ignored, five binaries, 173.342 s including compilation. These are
  the unmodified native matrices used to justify the reduced browser replay
  combinations. They ran after both browser measurements. The full 1,193-test
  native suite also passed the published-head hosted run above; it was not
  repeated locally in this node.
- `cargo fmt --check`, `cargo build --locked`,
  `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`.
- `node --check` for all four changed modules, `git diff --check`, JSON parsing,
  exact registry/status comparisons for both complete samples, unchanged
  assertion call-site counts and relative documentation links. Existing historical
  measurement JSON is unchanged; the new data is under `review_fix` in the
  [measurement record](ci-performance-issue-107.json). Unchanged CD runtime,
  discovery, CHD, streaming and video-transaction Wasm call counts also match
  between the complete samples.

The fresh worktree initially lacked the pinned browser, so its first launch
failed before tests. After the Playwright download, its extraction was
interrupted and the same pinned archive was extracted with system `unzip` into
`target/issue107-browsers`; real launch/version were checked before measuring.
The Rust commands used the already-installed stable sysroot and bindgen in
`PATH`, as documented above. No hosted run was retried, and no allowance reset,
purchase, model/provider switch, push or merge was performed.

## Acceptance limits

Local success is not hosted success. A passed hosted PR workflow on the exact
final publication head (native, Wasm, both shards and aggregate `build`), with
substantial measured headroom in both shards, remains required before claiming
#107 complete. Both reviewed hosted runs failed shared validation and did not
establish substantial margin in scenarios. The review fix improves measured
local runtime; it has not yet established hosted headroom. Keep #107 open and
PR #108 draft until hosted validation of this work reduction meets that gate.
This follow-up is prohibited from pushing or merging; its local commit cannot
establish a passing workflow on a replacement publication head.
No live Pages inspection, human confirmation, private firmware or physical
hardware is required.
