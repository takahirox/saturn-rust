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

## Review-fix validation

Both agent-fixable review findings are corrected. RTE now schedules both stack
MA stages and starts target IF at their completion; SLEEP stops IF at its
immediate successor. The [manual-derived deadlines](sh2-fetch-pipeline.md#rte-and-sleep-boundary-regressions)
and [native regressions](../src/bus/fetch_pipeline_tests.rs) cover both CPUs,
synchronous and waited external RTE stacks, target-byte changes before/after
sampling, SLEEP alignment/waits and actual FRT IRQ wake. All three new vectors
pass with advancement chunks 1/3/17/10000 and passive inspection.

Before the production fixes, the new literal grant oracles failed: RTE's
synchronous-stack target granted at 3 instead of 5, and aligned SLEEP granted
an unsupported PC+4 fetch at 3. The fixed RTE target samples at 7, executing
the immediate 43 written at 6 and retaining it across a later write of 44.

The initial complete native run exposed an existing system-guest assumption:
SLEEP's unsupported fetch had fenced its preceding buffered store. The
[native system test](../tests/sh2_system.rs) now allows one sleeping hardware
step, bounded by eight clocks, to finish that write. Its full 19-instruction
PC/register/exception trace, stored result 35 and unchanged sleeping state
remain asserted. The [write-buffer rationale](sh2-fetch-pipeline.md#existing-fixture-reconciliation)
records the primary contract. No fixture bytes or FRT timing goldens changed.

Final checks on macOS arm64, Rust 1.99.0 stable and Chromium 145.0.7632.6:

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 1,036 tests across 95 harnesses, including 673 library tests; zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | Passed: all 94 reproduction commands |
| `sh scripts/build-web.sh` | Passed: rebuilt production Wasm with bindgen 0.2.129 |
| Complete `npm run test:browser` | Passed: 30 shared + 36 independent groups, 181.254 s parallel wall time; coverage and artifact reports verified |
| `node --test scripts/ci-runner.test.mjs` | Passed: all 12 regressions |
| Unstaged/staged whitespace checks | Passed |

An initial runner invocation preceded the web build and failed four tests because
`web/pkg` was absent. After the required build, all runner tests passed. No final
required check was skipped. The complete browser run preserved Star Collector,
Prism Courier and Hardware Lab assertions. This revision addresses the two
review findings; the remaining #17 scope stays open. No push or merge was
performed and no human result is claimed.

## Unconditional branch and TRAPA review-fix validation

The two subsequent agent-fixable findings are corrected. Unconditional branches
and RTE preserve their delay slot without second-successor IF during EX. TRAPA
uses the real fetch/arbiter path for both discarded successors before exception
entry clears the stream. The [stage table and grant derivation](sh2-fetch-pipeline.md#unconditional-branch-and-trapa-regressions)
cite Figures 7.82/7.92 and Renesas Rev.5 Figure 7.93. Conditional branches retain
their distinct discarded-fetch behavior.

Three new [native vectors](../src/bus/fetch_pipeline_tests.rs) cover both CPUs,
zero/six software waits, both entry alignments for BRA/BSR/RTE, chunks
1/3/17/10000 and passive inspection. TRAPA checks real completion sampling,
discard of a sampled illegal opcode, once-only stacking and no architectural
fetch/error for an unavailable overrun beyond CS3. Before production changes,
the first two literal grant oracles failed: BRA requested PC+4 at 3 instead of
its target at 4; TRAPA issued its first stack access at 3 instead of PC+4 IF.
All three new vectors pass after the fixes.

The first complete native/browser checks exposed two existing guest phase
assumptions. Hardware Lab read EFREG1 during its per-sample clear; the bitmap
fixture published its timed write one main clock before the first acquisition.
The [documented guest corrections](sh2-fetch-pipeline.md#unconditional-branch-and-trapa-regressions)
retain every expected result, mutation, H=844 record and pixel assertion.
Hardware Lab now ORs actual constant-register observations across a full sound
period, including its zero-coefficient control. The bitmap guest adds one
resident NOP; an isolated physical-grant probe confirmed publication changes
from 476531 to the equal acquisition edge 476532. Generated source/media/hashes
were rebuilt; all eight focused Hardware Lab/rotation tests passed. No production
sound or video behavior changed.

Final checks on macOS arm64 with Rust 1.99.0 stable:

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 1,039 tests across 95 harnesses, including 676 library tests; zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | Passed: all 94 reproduction commands |
| `sh scripts/build-web.sh` | Passed: rebuilt production Wasm with bindgen 0.2.129 and regenerated media |
| Complete `npm run test:browser` | Passed: 30 shared + 36 independent groups, 192.592 s parallel wall time; coverage and artifact reports verified |
| `node --test scripts/ci-runner.test.mjs` | Passed: all 12 regressions |
| Unstaged/staged whitespace checks | Passed |

The final full runs preserved Star Collector, Prism Courier and Hardware Lab
gameplay, graphics, sound, diagnostic computations and restart assertions. No
final required check was skipped. No push or merge was performed; no human
result is claimed. Related to #69 and #17; the remaining #17 scope stays open.

## Redirected MOV and LDC.L review-fix validation

Both subsequent agent-fixable findings are corrected. Delay-slot MOV now derives
its MA/IF split and retained overlap from the redirected target stream, covering
both branch/target alignments and the shared dependency split. LDC.L to SR/GBR/VBR
holds successor ID and younger IF through its initial EX and real operand MA;
younger IF resumes beside final EX. Operand waits and IF waits stretch their
respective slots once. The [primary-derived stage table](sh2-fetch-pipeline.md#redirected-mov-contention-and-ldcl-successor-stalls)
and [three native regressions](../src/bus/fetch_pipeline_tests.rs) document
the literal edges and both-CPU coverage, including zero/six software waits,
resident/P1/retained/synchronous routes, completion-byte mutations, once-only
effects, chunks 1/3/17/10000 and passive inspection.

Before the production fixes, the new branch oracle failed for BRA at 4n targeting
4n+2: only three instructions had retired at clock 6, rather than four including
SLEEP. The LDC.L oracle observed PC+4 IF granted at 3 instead of operand MA at 4.
After the fixes, aligned zero-wait P1 operand MA grants at 4, samples at 6,
and PC+4 IF grants at 6 and samples at 8. Six software waits move these edges
to 10/18/18/26. Operand/instruction writes before and after completion prove
separate retained samples. The crossed branch/target vectors retain the dependent
ADD's single stall and verify both MOV loads and stores.

No guest bytes, timing goldens or substantive assertions changed in this revision.
Related to #69 and #17; the remaining #17 scope stays open. No push or merge
was performed and no human result is claimed.

Final checks on macOS arm64 (Rust 1.94.0 for the complete native test run;
rustup stable 1.99.0 for the final build, Clippy and Wasm):

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 1,042 tests across 95 harnesses, including 679 library tests; zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | Passed: all 94 reproduction commands |
| `sh scripts/build-web.sh` | Passed: rebuilt production Wasm with bindgen 0.2.129 |
| Complete `npm run test:browser` | Passed: all 30 shared + 36 independent groups; 186.902 s parallel wall time, including report/coverage verification |
| `node --test scripts/ci-runner.test.mjs` | Passed: all 12 regressions |
| Unstaged/staged whitespace checks | Passed |

The first Wasm Clippy invocation selected Homebrew Rust without its Wasm target;
the first web build could not find bindgen on PATH. Using the documented rustup
compiler and Cargo-bin PATH resolved both environment failures; the final checks
above passed. No final required check was skipped. The full browser run retained
Star Collector, Prism Courier and Hardware Lab gameplay, graphics, sound,
diagnostic computations and restart assertions. No post-merge verification is
required; broad BIOS/commercial-game compatibility remains unverified.

## STC.L and redirected multiplier review-fix validation

Both subsequent agent-fixable findings are corrected. STC.L to memory holds
successor ID and younger IF through initial EX, then schedules MA before a
contending IF. The multiplier scoreboard and deferred IF boundary now derive
splits from the redirected successor stream. The [primary-derived deadlines](sh2-fetch-pipeline.md#stcl-successor-stalls-and-redirected-multiplier-contention)
and [three native regressions](../src/bus/fetch_pipeline_tests.rs) cover both
CPUs, STC.L SR/GBR/VBR, MUL.L/DMULU.L/DMULS.L, both branch/target alignments,
resident and retained paths, synchronous and buffered stores, zero/six software
waits, actual grant ownership, retained completion bytes, once-only effects and
chunks 1/3/17/10000. The byte-sampling vectors compare passive inspection on/off.

Before production changes, the new literal oracles failed: aligned P1 STC.L
requested PC+4 at 3 during EX; the crossed JMP/MUL.L case requested target+2
at 6 instead of 7. After the fixes, synchronous-stack STC.L grants PC+4 at 5
and samples at 7 (six waits: 11/19). A JMP at either branch alignment targeting
P1 4n+2 grants the first target IF at 3 and the later contending IF at 7
(six waits: 13). Completion-byte changes prove actual retained sampling.

The additional buffered-store table initially placed external drain one clock
after internal acceptance began. The existing write-buffer implementation admits
internally and starts external drain together at MA start. The table and its
stage derivation were corrected to that established phase convention; its IF
deadlines remain unchanged. An initial complete native invocation stopped on
that new table expectation. No guest bytes, timing goldens or existing assertions
changed in this revision.

Final checks on macOS arm64 (Rust 1.94.0 for the native build/tests; rustup
stable 1.99.0 for Clippy, final formatting and production Wasm):

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 1,045 tests across 95 harnesses, including 682 library tests; zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | Passed: all 94 reproduction commands |
| `sh scripts/build-web.sh` | Passed: rebuilt production Wasm with bindgen 0.2.129 |
| Complete `npm run test:browser` | Passed: 30 shared + 36 independent groups; 195.760 s parallel wall time, with report/coverage verification |
| `node --test scripts/ci-runner.test.mjs` | Passed: all 12 regressions |
| Unstaged/staged whitespace checks | Passed |

No final required check was skipped. The complete browser suite retained Star
Collector, Prism Courier and Hardware Lab gameplay, graphics, sound, diagnostic
computations and restart assertions. Related to #69 and #17; the remaining #17
scope stays open. No push or merge was performed and no human result is claimed.
No post-merge verification is required; broad BIOS/commercial-game compatibility
remains unverified.

## Byte-memory logic and TAS review-fix validation

The remaining agent-fixable finding is corrected. AND.B/OR.B/XOR.B/TST.B hold
successor ID and younger IF through initial EX and operand MA, resuming beside
second EX. TAS.B additionally holds them through second EX and places locked
write MA before a contending real IF. Actual waits stretch those stages once;
byte logic retains the final MA/successor-EX overlap. Architectural-only adapters
retain the same operations and flags.

The [primary-derived stage table and timing reconciliation](sh2-fetch-pipeline.md#byte-memory-logic-and-tas-successor-stalls)
link Hitachi Figures 7.75–7.76 and Renesas Rev.5 Figures 7.75–7.76 to three
[native regressions](../src/bus/fetch_pipeline_tests.rs):

- `byte_memory_stalls_order_operand_and_younger_if_samples_on_both_cpus`
- `byte_memory_resident_and_retained_slots_preserve_decode_stalls_on_both_cpus`
- `repeated_tas_grants_derive_the_cached_bsc_guest_period_on_both_cpus`

They cover all five instructions, both CPUs, both entry alignments,
resident/retained/synchronous/P1 routes, zero/six waits, actual grants and
completion-byte mutations, flags/readback/once-only effects, chunks 1/3/17/10000
and passive inspection. The new unchanged P1 oracle failed before production
changes: younger IF granted at 6 instead of operand MA at 7. After the fix,
operand MA grants/samples at 7/9, followed by logic IF at 9/11 or TAS IF at
12/14. Six waits give 19/27, 27/35 and 36/44 respectively. Resident test setup
explicitly releases completed host cache-warming bus history; an initial setup
omission left synthetic busy time and an extra turnaround state.

The first complete native/browser runs exposed the BSC TAS guest's old collapsed
cycle-count oracle. An independent stage derivation, checked against every grant
of sixteen repeated TAS operations, gives 104/104/120 loop states for IW3=0/1/2.
Including eight FRT setup/read states gives **14/14/16 ticks**, replacing
11/11/13; changing IW3=2 to 1 gives 14. Only those authored timing expectations
and the corresponding native/browser mutation literal changed. Guest bytes,
readback, data mutations, repeat boots and substantive assertions remain intact.
The stage contract records the unchanged guest hash and corrected TSV hash.

Final checks on macOS arm64 (Rust 1.94.0 for native build/tests; rustup stable
1.99.0 for native/Wasm Clippy and production Wasm):

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked --no-fail-fast` | Passed: 1,048 tests across 95 harnesses, including 685 library tests; zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | Passed: all 94 reproduction commands, including corrected BSC TSV reproduction |
| `sh scripts/build-web.sh` | Passed: rebuilt production Wasm with bindgen 0.2.129 |
| Complete `npm run test:browser` | Passed: 30 shared + 36 independent groups; 226.137 s parallel wall time, with report/coverage verification |
| `node --test scripts/ci-runner.test.mjs` | Passed: all 12 regressions |
| Unstaged/staged whitespace checks | Passed |

The initial runner invocation preceded generated `web/pkg` artifacts and failed
its artifact-dependent cases; after the production build the complete runner
suite passed, including a final run after the browser expectation update.
No final required check was skipped. The complete browser suite retained Star
Collector, Prism Courier and Hardware Lab gameplay, graphics, sound, diagnostic
computations and restart assertions. Related to #69 and #17; #17's remaining
hardware-completion scope stays open. No human result is claimed. No post-merge
verification is required; broad BIOS/commercial-game compatibility remains
unverified.

## PR memory-transfer review fix

Related to #69 and #17. LDS.L @Rm+,PR and STS.L PR,@-Rn now use the
ordinary load/store MA scheduler through `timing::mov_load_address` and
`mov_store_address`. This fixes an operand request during EX and restores
completion-byte sampling, IF/MA contention and retained successor overlap.
The existing implicit-PR dependency scoreboard remains in effect. The
[stage contract](sh2-fetch-pipeline.md#implementation-and-evidence) derives
literal edges from Hitachi Figures 7.86–7.87 and the established BSC/write-buffer
contract; these PR forms require no LDC.L/STC.L or multiplier-resource stalls.

Three [native regressions](../src/bus/fetch_pipeline_tests.rs) cover both PR
forms, CPUs, alignments, resident/P1 instruction routes, zero/six programmed
waits, chunks 1/3/17/10000 and passive inspection:

- `pr_memory_transfers_schedule_ma_and_retain_completion_samples_on_both_cpus`
  asserts resident operand grant at 3 and load completion at 5/11, mutates
  memory before/after completion, consumes PR with STS PR,R8, and checks
  once-only store publication and auto-update.
- `pr_memory_transfers_preserve_p1_ma_if_overlap_on_both_cpus` asserts P1
  operand grants at 5/17 and subsequent IF at load completion 7/25. Stores
  preserve the read-to-write gap and read fence, publishing at 8/26 for
  aligned entry or 7/25 for entry at 4n+2, followed by IF at that edge.
- `repeated_pr_loads_derive_guest_splits_and_implicit_dependency_overlap`
  checks literal retirement edges for repeated resident loads, separated
  consumers and independent replacements, without external waits hiding states.

Both new oracles failed before production changes: resident LDS.L granted at
2 instead of 3, and aligned P1 LDS.L did not grant the required IF at 7.
The unchanged load expectations pass after the fix. The store oracle was
reconciled with SH7604 §7.9: a preceding read requires a one-state gap before
writing, while a preceding write requires no turnaround before reading.

The first complete native/browser runs exposed TIMING's old separated-PR
expectation (3 ticks) and dependency-removed PR control (4 ticks). The repeated
stage oracle independently establishes four real IF/MA splits in eight triples
(24+4=28 states) and sixteen splits in sixteen independent pairs (32+16=48
states). Including the existing seven-state wrapper gives 4 and 6 ticks.
The builder, generated TSV and native/browser mutation literal now use those
expectations. Guest ROM bytes, arithmetic/readback, other controls and repeated
cold boots are preserved; hashes are in the conformance matrix. The original
full native run also included an obsolete store-oracle assertion while that
test was being reconciled; final checks use the corrected literal stage oracle.

Validation results below apply to this review fix. Native build/tests use
Homebrew Rust 1.94.0; Clippy and rebuilt production Wasm use rustup stable
1.99.0 and wasm-bindgen 0.2.129 on macOS arm64.

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked --no-fail-fast` | Passed: 1,051 tests across 95 harnesses, including 688 library tests; zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | Passed: all 94 reproduction commands |
| `sh scripts/build-web.sh` | Passed: rebuilt production Wasm with bindgen 0.2.129 |
| Complete `npm run test:browser` | Passed: 30 shared + 36 independent groups; 198.624 s parallel wall time, including report/coverage verification |
| `node --test scripts/ci-runner.test.mjs` | Passed: all 12 regressions |
| Unstaged/staged whitespace checks | Passed |

No final required check was skipped. The complete browser suite retains Star
Collector, Prism Courier and Hardware Lab gameplay, graphics, sound, diagnostics
and restart assertions. Related to #69 and #17; #17's remaining hardware scope
stays open. No push or merge was performed. No human result is claimed, and no
post-merge verification is required. BIOS/commercial-game compatibility remains
unverified.
