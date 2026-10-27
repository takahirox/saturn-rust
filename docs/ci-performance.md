# CI validation performance — Issue #61

This continuation preserves PR #60's lossless bulk transport, independent native
validation, all 64 browser groups, 988 Rust tests, 90 fixture builders and all
three original licensed discs. It changes orchestration, test compilation and timing/reporting;
no emulator, guest, fixture, clock budget, expected result, runtime web asset
or lockfile changes. The native test compiler moves from optimization level 1
to 3 with debug assertions and overflow checks still explicitly enabled.
Related to #17; hardware completion remains tracked there. Issue #61 must remain open until its final PR workflow and hosted
performance verification have actually run. This node may commit locally but
is explicitly prohibited from pushing or creating a PR. The workflow has no
manual-dispatch trigger.

## Current compact-transport diagnosis

Read-only inspection of the successful, complete [main run 40484306388](https://github.com/takahirox/saturn-rust/actions/runs/40484306388)
confirmed revision `7b4a60a35ca4934c2246fa3128c31e2da60a78f5` and the actual
09:54:03–10:10:10 UTC workflow interval on 2026-10-27. This is the **post-#60**
baseline, not the earlier raw-transport baseline below.

| Executed hosted stage | Seconds |
| --- | ---: |
| Entire workflow, including publication | 967 |
| Native job | 527 |
| Native check step | 506 |
| Native test command, including compilation | 497.670 |
| Sum of native harness durations (92 harnesses, 988 passed) | 459.800 |
| Build/browser job | 939 |
| Build client step, including tool installation | 129 |
| Pinned bindgen installation | 104.479 |
| Wasm build / Wasm Clippy | 12.910 / 11.581 |
| Browser validation step, including setup/regressions | 791 |
| npm / Chromium installation / runner regressions | 3.426 / 33.993 / 12.108 |
| Complete 64-group browser command | 740.913 |

Within that browser run, evaluate wall time was 682.261 s and in-browser callback
time 649.617 s. Polling took 12.634 s, page navigation 0.397 s and instrumented
assertion methods 2.494 s. On the instrumented integration page alone,
`run_steps` consumed 445.655 s (34,676 calls), and frame advancement 82.450 s
(484 calls); resets, media mounts, pixel/PCM reads were small. These inclusive
counters overlap and exclude guest work on other pages: **do not add them as
independent phases**. Guest execution now dominates the critical path. Browser
launch, protocol overhead, publication and fixture checking are not its main
cost. Complete per-group and per-Rust-binary timings are retained in the
[machine-readable evidence](ci-performance-results.json).

The slowest hosted browser groups were SCSP MVOL (105.488 s), display modes
(96.693 s), HLE player (66.796 s), high-resolution rotation (60.360 s), Star
Collector/UI backup (49.338 s), Prism Courier (47.753 s) and HLE dual CPU
(39.604 s). Native display modes (66.510 s), MVOL (64.750 s), HLE dual CPU
(46.780 s), HLE player (42.590 s) and Prism Courier (34.830 s) dominate the
native workload. The preceding [PR run 40482246109](https://github.com/takahirox/saturn-rust/actions/runs/40482246109)
also passed: 902 s overall, 606 s native job, 587 s native check step and 773 s
browser validation. This observed native variance is another reason to measure
that path rather than assuming browser sharding alone guarantees <10 minutes.
The public run logs contain no per-process CPU/RSS samples;
local resource measurements below must not be represented as hosted measurements.

## Implemented split and publication barrier

The existing runner already separates 34 scenario modules from the shared UI
trace. Their hosted group times total **364.183 s**; the other 30 groups total
**376.077 s**. These are group partitions of one executed run, **not measured
parallel runtimes**. They motivate two balanced shards without moving or
concurrently invoking callbacks against the original shared page.

- `shared` retains the complete original UI/media/input/backup/game trace in its
  original order. It skips only the explicitly registered scenario calls.
- `scenarios` invokes every registered module, with a fresh context/page/emulator
  per module. It opens Debug/settings/BIOS mode before fixture execution, retains
  all authored resets and mutations, and closes the context before the next.
  Large-media tests retain their original private contexts and standard-size
  inputs. No two callbacks use the same page concurrently.
- The default full command launches those two independent Node/Chromium/server
  process trees and verifies the combined inventory. `--serial` runs all 64 groups
  in the original single-process order. Individual modules and either shard have
  deterministic reproduction commands; dependent UI groups remain together.
- `wasm` uploads one immutable `web-build`. Each CI browser job downloads it,
  validates its allowlist and runs its shard. A report is written only on complete
  success after cleanup and an unchanged-artifact check. It contains the Git SHA,
  every group status and a SHA-256 over every file path, length and byte. This hash
  binds the artifact; it **does not replace any pixel/PCM/data assertion**.
- The retained required `build` status waits for native, Wasm and **both**
  browser jobs. Its `always()` first step explicitly fails on any failed,
  skipped or cancelled dependency, preventing a skipped final gate from looking
  like a green required check. It then checks the exact two reports,
  original-to-shard group coverage, successful statuses, revision and tested-content digest against the downloaded artifact.
  Missing, cancelled or failed required jobs cannot pass this barrier.
  Publication jobs retain their default-success dependency gates and have no
  `always()` override. Main-only `site` waits for all validation and publishes that
  same immutable artifact. Current-main protection, serialized main publication,
  superseded-PR cancellation and PR deployment restrictions remain intact.

The matrix is capped at two browser jobs, one Chromium per job. Native can
continue concurrently on its own runner, for at most three validation runners
per revision. Locally the complete command caps itself at two browser process
trees even on larger machines. No retry, ignored rejection, hidden skip or hard
performance threshold was added. Each compact-transport worker has a 20-minute
failure timeout (60 minutes for the slower raw fallback); signals and timeouts terminate its process group and fail the supervisor.
`BROWSER_GROUP_START` identifies an active group before a crash/timeout, and
per-group/per-shard durations and failure status remain in logs.

The pinned bindgen CLI now has one exact-version, OS/architecture/Ubuntu-specific
cache entry, with no fallback keys. The existing build script still checks its
exact version on every build and fails on a missing or invalid executable. A
cold cache runs the same locked install; no test/build/check is conditional on
the cache. The actual baseline spent 104.479 s compiling this unchanged tool.
Cache hit rate and restore/save overhead have **not** been measured on GitHub;
that potential saving is separate from the executed sharding speedup below.

## Executed local comparison

The local host is Apple M4 Max / 16 logical CPUs / 64 GiB, macOS arm64,
Node 24.12.0, Playwright 1.58.2 / Chromium 145.0.7632.6, stable Rust 1.99.0 and
wasm-bindgen 0.2.129. Both serial and sharded runs use the same compact-transport
Wasm build and complete original workload. After exploratory runs finished,
`scripts/measure-browser.py` alternates full serial and full sharded runs, with
no other native/browser guest-execution validation workloads scheduled alongside them. It retains raw logs,
executed wall time, child user/system CPU and sampled process-tree RSS.
Sampling uses `ps` every 0.5 s, may miss short memory peaks, counts shared mappings
more than once, and includes small sampling overhead in child CPU figures.
This host was not reserved from unrelated system activity.

| Complete browser comparison | Serial | Two isolated shards |
| --- | ---: | ---: |
| Run 1 | 277.862 s | 145.847 s |
| Run 2 | 278.578 s | 144.403 s |
| Run 3 | 281.154 s | 145.684 s |
| **Median** | **278.578 s** | **145.684 s** |

Full-suite median wall time fell **47.70% (1.91×)**. Every invocation passed
all 64 original groups. The slowest parallel run (145.847 s) was still 47.51%
faster than the fastest serial run (277.862 s). Shared-shard medians were
141.477 s; scenario-shard medians were 144.623 s, demonstrating the measured
partition remains balanced after initialization/isolation overhead.

Median aggregate child CPU was 300.741 s serial versus 306.184 s sharded
(+1.81%), while elapsed time nearly halved. Sampled total process-tree peak RSS
ranged 1.912–1.970 GiB serial and 1.885–2.638 GiB sharded. These are local summed
RSS samples, not a hosted memory reservation or an exclusive-memory measure.
The modest aggregate CPU increase is consistent with retained guest work plus
fresh-context overhead, rather than shortened emulated clocks. Hosted job-minute
cost also includes duplicated browser provisioning and additional handoffs.

An initial original-runner baseline also passed all 64 groups in 280.991 s;
it overlapped a native baseline and a scenario probe, so it is excluded from the
controlled medians. The native baseline passed 988 tests with zero failures or
ignored tests across 92 harnesses (189.710 s harness sum, 234.713 s command wall,
9.280 s compilation). An initial scenario probe exposed a coverage-accounting
bug: restoring a setup timer incorrectly added a 35th group. The complete
inventory check rejected it. Setup restoration was fixed before the comparisons;
the failing probe is not counted as a performance success.

A second optimization raises the native test compiler from level 1 to **3**,
retaining `debug-assertions = true` and `overflow-checks = true`. It changes no
hardware code or expected data. In concurrent complete-validation runs using
cold independent Cargo targets, both profiles passed all 988 tests:

| Executed native measurement | Level 1 | Level 3 |
| --- | ---: | ---: |
| Test compilation | 8.900 s | 11.760 s |
| Sum of 92 harness durations | 190.600 s | 159.630 s |
| Entire `cargo test --locked` command | 235.235 s | 207.192 s |
| Native path, all fixtures/build/fmt/tests/Clippy | 243.633 s | 215.328 s |
| Concurrent native + sharded browser validation | 244.140 s | 215.838 s |

The 2.860-second compile increase is outweighed by reduced test execution.
Display modes fell 29.440→24.280 s, HLE dual CPU 19.120→16.040 s and MVOL
28.230→23.860 s. The complete native path improved 11.62%; a separate level-3
native path also passed in 212.733 s. The latter's browser build failed (see
below), so it is **not** a successful end-to-end pipeline result.

A matched **executed complete-validation control** then used the final
application/fixture sources and identical timing/regression tooling, with
`CARGO_PROFILE_TEST_OPT_LEVEL=1` and full-browser `--serial` to reproduce the
original execution settings. Both it and the optimized run used independent
source directories and cold Cargo targets:

| Executed concurrent full-validation path | Original settings | Optimized settings |
| --- | ---: | ---: |
| Native (all mandatory commands) | 241.663 s | 215.328 s |
| Web build + Wasm Clippy + runner tests + all browser groups | 303.533 s | 174.993 s |
| **Actual end-to-end local wall time** | **304.044 s** | **215.838 s** |

The complete local critical path improved **29.01% (1.41×)**. Both runs passed
all 988 native tests, 90 fixture builders and 64 browser groups. These are
executed concurrent runs, not sums of isolated stage medians. The optimized
browser path is now shorter than native; native remains the local critical
path. The profile/serial flags, exact commands and all stage times are saved
with the measurements. This controlled local comparison is not a new GitHub
Actions measurement or a prediction that hosted runners share its timings.

All pipeline measurements exclude tool provisioning, checkout, GitHub queueing,
artifact transfers and publication. Native and browser commands ran concurrently
with separate cold Cargo targets; the final control/optimized runs also use
independent source directories, as CI does. Exact commands, stage durations,
per-binary/group timings and sampled memory are retained in `issue61` in the
measurement JSON. The timed pipelines used the same eleven runner regressions;
the final additional build-path regression brings the final tree to **twelve**,
all passing in a separate full runner invocation (11.978 s). Timing figures are
not adjusted or extrapolated to include that later regression. Final review also
retained the existing `build` check as the explicit dependency/status barrier;
its success/failure/cancellation/skip guards and workflow naming were verified by
the final regression run after the timing runs. The nominal dependency graph and
guest execution schedule are unchanged by that final gate correction.

Two failed exploratory pipeline attempts are recorded and excluded from
successful timings. First, simultaneous fixture verification and web generation
in one source directory raced a Prism Courier ISO rewrite. Separate source
checkouts remove the race. Next, isolation exposed that `build-web.sh` used
`CARGO_TARGET_DIR` for Cargo but hardcoded the default `target/` for bindgen.
The helper now reads bindgen's input from the same configured directory. A
regression supplies a deliberately stale default artifact, a fresh custom target
with spaces and a stale CLI version, checking correct selection and rejection.
The earlier level-1 compiled/default Wasm files were independently byte-identical
(SHA-256 `7874e3a9b5f8ea06df32642c3fe58dbc7f34a67aa5d05f1d9552c4c2264ba979`), so
that earlier timing did not change the executed binary. The final complete run
uses the corrected path and freshly generated bindings. Initial local setup
also selected Homebrew Cargo without rustup's Wasm target; selecting the stable
rustup compiler/Cargo pair corrected that before comparison runs.

## Coverage, result parity and regressions

The unchanged [original inventory](../scripts/browser-groups.json) and the
[complete original-to-shard mapping](../scripts/browser-shards.json) enumerate
all 64 groups exactly once. Executable regressions compare the module-call
registry and shard manifest and reject duplicate, missing or unknown groups.
The source audit preserves **all 3,417 original nonblank runner lines in order**,
normalizing only timing setup, the conditional module dispatch and the final
status log. All **598 original direct assertion sites** remain (599 now, adding
fresh-page error checking). Every scenario module, all 90 registered fixture
commands/arguments, native tests, production code and runtime asset sources are
byte-identical to the checked revision. Only Cargo test optimization changes;
debug assertions and overflow checks remain on. The evidence records source hashes.

Repeated full serial and isolated/sharded runs execute the same authored expected
register/clock/IRQ/DMA/exception, Canvas and PCM assertions, including every
operand mutation, omitted-operation negative control, repeated boot, privacy,
reset/error, game and real AudioWorklet check. A new real-Wasm regression also
compares **all returned data** from the original video-write guest (six runs,
both CPUs' DMAC/FRT records and every returned Canvas/framebuffer pixel), then
compares every sound sample, peripheral clock, retired instruction/register,
exception and guest record across ten 10,000-step sound trace checkpoints in a
previously used shared context versus fresh contexts. No sampled/hash substitute
is used for these comparisons. Native emulator source is unchanged; both compiler
profiles pass all 988 tests against the same original clock/state/PCM/pixel oracles with no ignored tests.

Runner regressions deliberately mutate a real VDP2 guest result and require both
the shard and supervising command to fail, remove a stale success report, and
print the failed group. Additional tests cover rejected promises, crashed workers,
timeouts, corrupted real Wasm assets, missing/duplicate/failed groups/shards,
stale revisions/digests, the real aggregate CLI and publication dependencies.
The actual retained-status shell guard is executed against success and every
failure/cancelled/skipped result for each of its three dependencies.
The original full-command injected assertion failure and raw/compact bulk-array
parity/security tests remain.

## Final verification and remaining acceptance

All mandatory final-source checks passed:

- `cargo fmt --check`, `cargo build --locked`, and `cargo test --locked`
  (**988 passed, zero failed/ignored**, 92 harnesses including docs).
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`.
- All 90 commands in `sh scripts/check-fixtures.sh`.
- `sh scripts/build-web.sh` with the pinned CLI and corrected custom-target path.
- Complete `npm run test:browser` (all 64 groups), three controlled full serial
  and three controlled full sharded runs, plus the complete validation pipelines.
- `node --test scripts/ci-runner.test.mjs`: **12 passed, zero failed/skipped**.
- Actionlint 1.7.12, JavaScript syntax, source/fixture parity and Git whitespace
  checks. No emulator, guest, native test, lockfile or public asset source diff.

The performance commands and fallback are documented in [setup](setup.md#github-pages).
To reproduce the native-profile control without changing the checked source,
use `CARGO_PROFILE_TEST_OPT_LEVEL=1 python3 scripts/ci-time.py native-tests cargo test --locked`;
unset the override for the committed level-3 profile. Use separate source
checkouts for concurrent complete native/web paths and separate empty Cargo
target directories for cold compile comparisons.

**No new hosted run or hosted speedup is claimed.** The post-#60 hosted baseline
is real; the local results above are real; a hosted <10-minute result is pending.
The measured partition removes about half the browser critical path locally.
Applying the observed partition to the historical hosted stages suggests a
browser path around 129 + 376 + setup/handoff seconds instead of 939 seconds,
with the original 527-second native job a competing path before the test-profile
optimization. Local native gains must not be assumed to equal hosted gains.
This is an explicit **projection**, not an executed workflow or a CI performance threshold.
The 8-minute stretch goal cannot be established by this split alone: native
at level 1 already took 8m47s (10m06s in the preceding PR). The native profile
change addresses that path locally; its hosted gain is unmeasured. Bindgen
installation (104.479 s),
native guests and workflow scheduling/artifact handoffs remain concrete costs.

Two browser jobs duplicate Node/Chromium setup and add artifact/result handoffs;
aggregate guest work is retained and fresh-context initialization adds a small
cost. More concurrent runners trade extra setup/runner minutes for lower latency.
The exact pinned-tool cache adds one official Actions cache step; no Rust/npm
dependency, paid service or external backend was added. Its potential warm-cache
saving is the historical 104.479-second install minus unmeasured cache transfer
cost, not an executed latency result. Cold-cache CI retains that cost. Native
execution and scheduling remain the next bottlenecks if the actual PR workflow
misses the target; do not weaken tests or claim a sum of stages as a hosted run.

The PR workflow must run against the final checkpoint and all required jobs must
pass before merge. This node's no-push/no-PR instruction prevents that operation;
it is a remaining automated acceptance criterion, not a human, firmware, owner
investigation, live Pages or hardware approval gate. There is no required
post-merge verification. Keep #61 and #17 open.

The earlier #59 record below describes its historical runner and measurements;
its statements about one browser/no subsets/two jobs apply to that version.

---

# Previous optimization — Issue #59 (historical record)

The optimization changes test transport and CI orchestration. It changes no
Saturn emulation code, guest instructions, cycle budgets, expected values,
timeouts, fixture hashes or runtime web assets. Related to #17; hardware
completion remains tracked there and is not established by this performance work.

## Diagnosis and design

The checked source revision is `5c5deca76f97af23af45fb0fb3941f43156dbb45`;
its application/test source matches main `402f9e8c1a2819a1e2f52723c7674f133f470dde`.
GitHub's actual job timestamps for [run 40147912040](https://github.com/takahirox/saturn-rust/actions/runs/40147912040)
show native checks taking 427 seconds, browser build 81 seconds, browser
validation/setup 1,445 seconds, and deployment 10 seconds (about 33m16s overall).
The [PR #58 run](https://github.com/takahirox/saturn-rust/actions/runs/40093236785)
shows 554 seconds native and 1,847 seconds browser/setup. Deployment is small;
fixture reproducibility was five seconds in the main run.

Per-group measurements found large costs in high-resolution rotation, video
transactions, SCSP MVOL and rotation fetch. The common transport pattern was
passing a 524,288-element ROM array repeatedly and returning full image arrays.
Playwright represents each array element as a protocol value. A local 20-call
transport-only probe (512 KiB input, 640×224 RGBA output) took 33.851 seconds with
numeric arrays versus 0.451 seconds with byte strings, while browser work stayed
near 12 milliseconds per call. This identified serialization as a measurable
cause rather than inferring it from the total workflow duration.

The probe motivated a single-string representation; JSON also retains signed
PCM and finite non-byte numbers without a byte-only codec. The implementation
uses **lossless JSON strings for bulk finite numeric arrays**
instead of adding workers or changing emulated work. JSON round trips every
finite number exactly; negative zero, nonfinite values, undefined, BigInt and
sparse arrays retain the ordinary protocol representation. Callbacks receive
ordinary arrays, and all existing Node assertions receive the reconstructed
complete arrays, including every Canvas pixel and PCM sample. No hashes or
sampled comparisons replace assertions. The helper serializes trusted repository
callbacks using the same source-transfer model as Playwright; it is not shipped
in `web/` and changes no production CSP or networking behavior.

The secondary improvement runs native validation independently from Wasm/browser
validation. Native and browser jobs each use their own hosted runner. Pages
upload depends on **both** jobs, consumes the browser job's exact tested artifact,
and preserves main serialization and the latest-main check. PRs cannot publish.
No cache, retry policy, sharding framework or dependency was added.

## Reproduce and interpret measurements

The local host was macOS arm64, Apple M4 Max, 64 GiB RAM, Node 24.12.0,
Playwright 1.58.2 / Chromium 145.0.7632.6, rustup stable Rust 1.99.0, and exact
wasm-bindgen 0.2.129. The initial exploratory native run also used Homebrew Rust
1.94.0; the final native and Wasm checks used rustup stable. The first attempted
Wasm build with Homebrew failed to find rustup's installed target; selecting the
rustup compiler/Cargo together fixed the environment. No test was skipped.

Build once with `sh scripts/build-web.sh`. Install Chromium as described in
[setup](setup.md#automated-browser-smoke-test). For controlled future comparisons:

```sh
mkdir -p target/ci-profile
for attempt in 1 2 3; do
  for mode in raw compact; do
    BROWSER_TRANSPORT="$mode" npm run test:browser > "target/ci-profile/$mode-$attempt.log" 2>&1
  done
done
python3 scripts/ci-time.py native-tests cargo test --locked
```

Every invocation executes the complete suite. Performance is reported, never a
flaky threshold or a reason to drop work. The first recorded baseline added only
group timers to the checked source. Later raw runs additionally measure evaluate
wall/browser time, Wasm calls and polling; they preserve the original numeric
array representation. Compact runs change only representation plus instrumentation.
The experimental runs overlapped on this same host (up to four independent suite
processes, plus short regression probes); some overlapped native validation.
They demonstrate the large transport improvement but are not isolated-host
throughput or GitHub-hosted latency measurements. The committed
[measurement data](ci-performance-results.json) retains per-run group totals and
operation measurements, including this limitation. The first baseline's 98
segments aggregate to the same 64 unique groups; later reports aggregate them
before printing the count.

| Complete browser run | Raw baseline | Compact default |
| --- | ---: | ---: |
| 1 | 798.670 s | 281.408 s |
| 2 | 809.296 s | 283.159 s |
| 3 | 808.879 s | 279.423 s |
| **Median** | **808.879 s (13m29s)** | **281.408 s (4m41s)** |

Full-browser median wall time decreased **65.21%**, a 2.87× speedup. All six
invocations passed all 64 unique groups. Selected group medians:

| Group | Raw | Compact |
| --- | ---: | ---: |
| High-resolution rotation | 156.871 s | 22.714 s |
| Video transactions | 91.159 s | 13.099 s |
| SCSP MVOL | 83.801 s | 39.584 s |
| Rotation fetch | 70.648 s | 8.454 s |
| Display modes | 37.310 s | 36.667 s |
| HLE player | 24.708 s | 24.678 s |

Instrumented raw run 2 and compact run 3 each executed **1,204 evaluate calls,
209 polling waits and 12 page navigations**. Evaluate wall time fell from
781.151 to 255.011 seconds while in-browser callback time stayed close
(248.321 versus 242.449 seconds). Polling remained 4.964 versus 4.902 seconds;
navigation remained 0.487 versus 0.438 seconds. The compact run's 17,647 timed
assertion calls took 1.326 seconds. The unchanged guest counts/assertions and
similar browser execution times distinguish protocol savings from weakened work.

A separate final local run executed both mandatory validation paths concurrently
with separate Cargo target directories (matching the jobs' independent build
state). It included all native commands/fixtures, web build, Wasm Clippy, the six
runner regressions and another complete default browser run. **All commands
passed in 301.803 seconds (5m02s)**: the native path took 250.581 seconds and the
browser path 301.801 seconds, including a 292.989-second browser invocation.
The exact commands and command durations are retained in the measurement data.
Tool installation, GitHub scheduling and artifact publication are excluded.

For context, combining the measured 250.581-second native path, 0.503-second
web build/Wasm Clippy and 808.879-second raw-browser median in the old serial
order gives **1,059.963 seconds (17m40s)**. This is a stage-sum projection,
**not an executed end-to-end baseline** or a GitHub speedup claim. The actual
local combined validation result and browser medians establish a material
critical-path improvement; the new hosted workflow's total remains unmeasured
under this node's no-push/no-PR constraint.

Guest execution remains expensive after transport is fixed. Display-mode guests,
SCSP MVOL, HLE player, gameplay and dual-CPU guests still execute all their
hardware clocks. The native test profile remains optimization level 1 with debug
assertions and overflow checks enabled. Further emulator or fixture changes need
separate hardware evidence; they are unnecessary for this transport improvement.

## Coverage and isolation audit

[The executable inventory](../scripts/browser-groups.json) lists 64 groups.
Every full invocation must visit all registered groups and fail on any missing
entry or thrown assertion. Each timed module must invoke its real scenario.
Original shared-page groups retain their exact order, reset behavior and local
state. Independent pages/contexts, including standard-size media and HLE player,
retain their original lifecycle. The default remains one Chromium per suite;
there is no newly shared writable browser state or concurrent scenario execution.
JSON packing can create a string alongside an existing array, but does not add
concurrent emulator instances or change the large-media memory bounds.

The source audit against `5c5deca` verified all **3,307 original nonblank lines**
of the integrated runner remain in order (allowing its strict-assert import
alias for timing). All **598 direct assertion call sites**, every imported
scenario module, every loop/cycle count, operand mutation and negative control
remain. All imported scenario files are byte-identical to baseline. Moving the
fixture checks preserved the exact ordered **90 commands and arguments** in
[the registry](../scripts/check-fixtures.sh). The measurement data includes source
and module hashes so this audit is reviewable independently of timing claims.

The groups retain SH-2/68k/DSP/bus/device records, Wasm and full Canvas output,
AudioWorklet, all three original licensed homebrew/diagnostic discs, standard-size
ISO/CUE media, responsiveness, invalid inputs/media, reset/fault behavior,
input privacy, local backup isolation, offline behavior, no local-byte requests,
`web/` allowlisting and the `/saturn-rust/` project prefix. No proprietary assets,
browser data upload, external CDN or runtime backend was introduced.

Repeated full raw/compact runs exercise identical authored guest/Canvas/audio
and input assertions in the same order. Runner regression tests additionally
compare every element of 512 KiB input/output and signed PCM arrays across
raw/compact/compact/raw fresh contexts, check imports and independent state,
and require callback exceptions, rejected promises and closed pages to reject.
A disposable child injects a deliberately failing real browser assertion into the
complete runner: it exits 1, prints failure timings and never reports success.
A missing inventory group and a failing timed subprocess also fail. No retries
or catch-and-ignore paths are used for scenario failures.

## Validation and CI limits

All required checks passed on the final implementation:

- `cargo fmt --check` and `cargo build --locked`.
- `cargo test --locked`: **988 passed, zero failed/ignored**, 92 test harnesses
  including doc tests. Stable-Rust harness time was 192.77 seconds; cold test
  compilation contributed additional command time. Display modes (29.73 s),
  SCSP MVOL (28.78 s) and HLE dual CPU (18.99 s) led the native binary timings.
- `cargo clippy --locked --all-targets -- -D warnings`.
- The full 90-command `sh scripts/check-fixtures.sh` registry (3.708 seconds in
  the separate timed run; also rerun with final native validation).
- `sh scripts/build-web.sh` using the pinned bindgen CLI.
- `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`.
- Complete `npm run test:browser`, three raw baselines and three compact runs;
  the combined local pipeline adds a fourth final compact validation.
- Six `node --test scripts/ci-runner.test.mjs` regressions, including real
  Chromium equivalence, deliberate full-runner assertion failure and CI gates.
- Actionlint 1.7.12, YAML parsing, source/fixture parity audit and Git whitespace
  checks. Native/Wasm source, lockfiles and committed media have no diff.

The final PR GitHub Actions workflow was not executed remotely: this node is
explicitly prohibited from pushing or creating a PR, and the workflow has no
manual-dispatch trigger. Read-only inspection supplied the historical run
measurements. Local commands, real Chromium regressions, inventory checks and
Actionlint validate the final orchestration; no hosted performance result or
post-merge verification is claimed or required. There is no human, personal
firmware, live Pages or physical-console acceptance gate.

For debugging/rollback, run `BROWSER_TRANSPORT=raw npm run test:browser`; it runs
all groups with the original representation. Timings are always enabled and
print failures as well as successes. See [setup](setup.md#github-pages) for stage
and Wasm timing interpretation and the deployment dependency graph. CI uses at
most two concurrent validation runners per revision; publishing jobs follow the
validation barrier. Never roll back by removing mandatory commands or allowing
Pages to publish a rebuilt/untested directory.
