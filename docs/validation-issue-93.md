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

The correction adds only the post-allocation occupancy check in `drive_frame`.
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

## Pre-merge handoff and limits

No GitHub Actions result is claimed. This node is explicitly prohibited from
pushing, opening a PR, merging or commenting; downstream must require successful
applicable PR jobs for the committed revision before merge. No post-merge
verification, physical Saturn, proprietary firmware/disc, human sign-off or live
Pages inspection is required by #93.

Full cross-chip arbitration, arbitrary asynchronous CDC/GFS calls, real Sega
BIOS/SDK ABI and commercial games are not verified. Remaining command/connector,
optical and broader #17 hardware requirements stay pending. The #81/#82 endpoint
evidence finding and draft/publication rule are unchanged. Successful local
checks establish this bounded increment, not umbrella hardware completion.
