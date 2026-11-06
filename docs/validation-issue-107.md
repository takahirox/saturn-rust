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
- **Mosaic:** keep all 60 guest executions, 240 frame advances, both density/field
  paths, source/enable mutations and repeated cold boots. Production Canvas and
  field history add browser coverage beyond native arithmetic.
- **High-resolution rotation:** keep all 48 executions/advances and full
  640/704-wide Wasm/Canvas images. Input/output protocol work is secondary to
  guest execution; the existing compact transport already handles bulk arrays.
- **Video transactions:** keep all 132 BIOS resets, distinct VBE/HDTV, bus
  ownership, retained-word, cycle acquisition and omitted-write controls.
- **MVOL:** keep all 46 executions, repeated cold resets, gain/source mutations,
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

## Acceptance limits

Local success is not hosted success. A passed hosted PR workflow on the exact
final publication head (native, Wasm, both shards and aggregate `build`), with
substantial measured headroom in both shards, remains required before claiming
#107 complete. The reviewed hosted run failed shared validation and did not
establish substantial margin in scenarios. Keep #107 open and PR #108 draft
until further optimization and successful exact-head hosted validation address
these measured failures. This follow-up is prohibited from pushing or merging;
its evidence commit cannot establish a passing workflow on a replacement head.
No live Pages inspection, human confirmation, private firmware or physical
hardware is required.
