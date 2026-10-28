# Issue #69 validation

Related to #69 and #17. This work adds bounded instruction fetch/decode scheduling
to both SH-2s. The [stage and cancellation contract](sh2-fetch-pipeline.md) was
written before production changes. Its primary references, literal deadlines,
independent guest oracle and fixture reconciliations are the acceptance basis.

The inherited native P1 oracle retired two instructions by clock 4, where the
new oracle requires zero and an active PC+4 fetch. After implementation the
same oracle passes. The unchanged original guest was also run against an isolated
archive of inherited checkpoint `ba886f6` (production main `7902b06`): it fails
with aligned P1 MOV timing 9 versus 7 FRT ticks, waited/CS0 ADD 12 versus 11,
and cold-cache 8/10 versus 9/11. Guest arithmetic still matches. Expected values
come from the builder's manual-derived slot enumeration, never production traces.

Implementation/test links are in the [conformance entry](hardware-conformance.md#scheduled-sh-2-ifid-issue-69).
Native tests exercise actual fetch grants and retained bytes alongside EX/MA,
ordered refills, CS0 words, both CPUs, redirects, NMI, resets, competing bus
masters, advancement chunks and inspection. The original MIT guest computes
28 timing/arithmetic records per CPU, then shared-memory/NMI handoff records.
Its four mutation families and repeated cold boots run natively and through
rebuilt production Wasm under `/saturn-rust/`.

## Checks

All final checks passed on macOS arm64 with Rust 1.99.0 stable and Chromium
145.0.7632.6. The documented toolchain PATH was used for native/Wasm checks.

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 1,033 tests, zero failed/ignored, across 95 harnesses including doc tests; 670 library tests |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | Passed: all 94 registered reproduction commands, including IF/ID source/bytes/oracle/hashes |
| `sh scripts/build-web.sh` | Passed: rebuilt production Wasm with matching bindgen 0.2.129 |
| Complete `npm run test:browser` | Passed: 30 shared + 36 independent groups, 193.257 s parallel wall time; both artifact/coverage reports verified |
| `node --test scripts/ci-runner.test.mjs` | Passed: all 12 runner/publication-barrier regressions |
| `git diff --check`, staged whitespace check | Passed |

The initial web build could not find the installed bindgen CLI on the default
PATH. Repeating with the repository's documented stable-toolchain/CLI PATH
succeeded. No installation, allowance reset, purchase or model/provider switch
was needed. No final required check was skipped.

The complete browser suite preserved Star Collector and Prism Courier gameplay,
graphics/input/restart, Hardware Lab diagnostics and sound, and production
Canvas/AudioWorklet delivery. Existing rotation tests retain their H=844,
first-dot retention, all following pixels and early/omitted-write controls.
The [fixture reconciliation](sh2-fetch-pipeline.md#existing-fixture-reconciliation)
records timing changes and primary-derived reasons; no substantive assertion
was removed to obtain a passing suite.

No push, PR, merge, Issue comment or Issue closure was performed. No post-merge
verification is required. Issue #17 remains open with its unmet scope; this
checkpoint does not establish BIOS or commercial-game compatibility.
