# CI validation performance — Issue #59

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
