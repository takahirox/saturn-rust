# Issue #81 validation: scheduled SH-2 interrupt entry

Related to #81 and #17. This checkpoint implements the interrupt microsequence
and records remaining SCU evidence gaps; it does not establish hardware completion.
Do not close #17. Do not treat green checks as resolving unverified endpoint
behavior or as evidence of genuine BIOS or commercial-game compatibility.

The [phase contract](sh2-interrupt-entry.md) was recorded before implementation.
It links the exact primary revisions and separates specified phases from board
conventions. H Rev. 4 Table 5.8, Figures 5.7/5.10 require recognition before
acceptance, intervening internal states and an early NMI request clear. Master
external acquisition uses the accepted level with CS inactive and software waits
inapplicable. ST-210 item 7's IST write prohibition remains enforced.

## Independent evidence

- [Ideal-memory phase oracle](../src/cpu/interrupt_tests.rs): literal 10/13-state
  response, ordered stack/vector accesses, saved PC/SR, NMI clear and handler IF.
- [Native Saturn vectors](../src/bus/interrupt_tests.rs): both adapters, master
  NMI/SCI/autovector/external IRL, slave SCI/IRL, literal physical CS0 word beats,
  stack waits, accepted-level competitors, same-level convention, exact clear
  phase, replay, retained completed writes, peer CPU/DMA/refresh and resets.
  Chunk sizes 1/3/17/10000 and passive inspection off/on agree.
- [Original MIT guest builder](../tests/fixtures/build_interrupt_entry.py), adjacent
  [license](../tests/fixtures/LICENSE), `.s`/`.hex`/`.sha256` artifacts. Native
  [Machine](../tests/interrupt_entry.rs) and [production browser Wasm](../scripts/interrupt-entry.mjs)
  execute identical guest bytes with two cold boots for each of five controls.
  The guest uses private single-state cache-RAM entry accesses and a real FRT
  compare. From request to its first handler FRC latch: recognition 5 + entry 8
  + handler IF 1 + ID 1 + load EX 1 + peripheral read 2 = 18 states. Compare K
  signals on leaving K, so the guest must read K+1+floor(18/8)=K+3. K=32/48
  therefore gives 35/51, independently of emulator trace output. It also records
  saved SR=50, return PC=C000050C, accepted mask=60/A0 and post-RTE value=7/11.
  A real master SCU DMA copies 13579BDF/2468ACE0 and invokes vector 4B. Omitting
  FRT TIER prevents both stages; omitting DMA start prevents SCU entry/slave boot.
  The host writes no running guest records, flags or completion state.

## Inherited-main negative oracle

A temporary detached worktree at `018eacb3aefa1318338511a5623e29b2ef2d44ff`
received only `interrupt_entry_does_not_issue_a_stack_write_at_acceptance`.
`cargo test --locked --lib interrupt_entry_does_not_issue_a_stack_write_at_acceptance`
failed (0 passed, 1 failed): **Figure 5.7 requires D,E,E before the SR stack
access**. The oracle checks both ordinary and buffered stack requests, so it
also detects inherited write-buffer acceptance. It passes with this implementation.
The production worktree was never reverted to perform the negative check.

## Existing expectation changes

- `nmi_during_pending_if_waits_for_older_execution_and_discards_younger_decoded_words`:
  NMI recognition takes two states (Table 5.8). Boundaries are t5/t6/t7; arrivals
  at t4/t5 therefore allow one/two successors before acceptance. Previously every
  arrival was assumed to preempt at t5. Saved PC and arithmetic expectations now
  follow those literal boundaries.
- `delay_slot_store_finishes_ma_before_target_and_nmi_entry`: arrival t35 is
  recognized at t37; the target SLEEP executes at t36, so saved PC is target+2
  and three instructions have retired. The delayed store still completes once.
- `slave_raster_pending_exception_is_canceled_by_smpc_and_cold_reset`: setup now
  allows five recognition states after VBlank assertion before issuing entry.
  Reset still cancels the unfinished entry before its first stack data phase.
- Direct source-acknowledgement vectors now exercise the journaled vector-selection
  operation; the pending-slave assertion projects the retained level/vector from
  the new source-identity record. No assertion was removed.

## Required checks

Executed locally on macOS/arm64 with Node 26.12.1, Playwright 1.58.2 /
Chromium 145.0.7632.6, native Homebrew Rust 1.94.0 and installed rustup stable
Rust 1.99.0 for Wasm. The matching wasm-bindgen 0.2.129 CLI was installed under
`/tmp/saturn-issue81-tools`. No dependencies or lockfiles were changed.

| Check | Observed result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 1,117 tests, zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| Wasm warnings-denied Clippy | Passed |
| `sh scripts/check-fixtures.sh` | Passed; all 99 builders, including the new guest |
| `sh scripts/build-web.sh` | Passed with installed Wasm compiler and matching CLI |
| `node --test scripts/ci-runner.test.mjs` | Passed: 12 tests, zero skipped |
| `npm run test:browser` | Passed: shared 30 + scenarios 39 = all 69 groups; 247.531 s |
| Focused `cargo test --locked --lib interrupt_tests` | Passed: 14 tests |
| `git diff --check`, new-document relative links | Passed |

Reproduce the web build in this shell with
`RUSTC="$(rustup which rustc)" PATH=/tmp/saturn-issue81-tools/bin:$PATH sh scripts/build-web.sh`.
For Wasm Clippy use
`PATH="$(dirname "$(rustup which rustc)")":$PATH cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`.
The initial web build exposed the Homebrew compiler's missing Wasm standard
library; selecting the already installed rustup compiler resolved it. An earlier
browser aggregate correctly rejected changed site assets when the final UBC
source-identity correction was rebuilt during that run. The final aggregate
uses immutable final build output; no failed run is counted as passing.

## Remaining acceptance limits

The cited ST-097/ST-210 documents establish source/level/vector mappings and
prohibit IST writes. H establishes accepted-level output and permits clearing
on external acquisition. They do not affirmatively establish SCU same-level
arbitration, its exact source-clear edge or acknowledgement without IVECF in
SH-2 autovector mode. Ascending-vector selection, completion-edge consumption
and accepted-source autovector acknowledgement are explicit deterministic
conventions. The Issue #81 requirement for affirmative SCU-specific evidence
therefore remains unmet; leave #81 open rather than claiming this checkpoint
fully resolves it. No Issue edits or comments were made under this node's
local-files/commit-only authorization.

Noise pulses, physical external WAIT and slave external-vector wiring are not
established by these tests. All remaining #17 CPU/peripheral/cache/bus/reset/DMA,
SCU/DSP, video/audio/CD, BIOS/HLE/SMPC and integrated requirements still apply.
Required post-merge verification: none.

## Review follow-up: declined IRQ context (P1)

`Machine::issue_cpu` now supplies source context only to the IRQ acceptance
attempt and removes it before falling back to instruction execution. DMA/NMI
entry also runs without the unrelated IRQ context. External acquisition retains
the accepted level and original architectural vector that initiated the transaction
instead of reconstructing its replay operation from the boundary's IRQ candidate.

The new `masked_scu_external_vector_request_cannot_supply_a_native_ubc_vector`
and `masked_scu_autovector_request_is_not_acknowledged_by_a_native_ubc` tests in
[the native phase vectors](../src/bus/interrupt_tests.rs) both failed before the
fix: external mode replay expected `InterruptVector(15,12)` but recorded
`InterruptVector(15,66)`; autovector mode cleared the unrelated HBlank status bit.
Both now pass with stack waits, chunks 1/3/17/10000 and passive inspection off/on.
They check UBC vector 12, saved PC/SR, first handler fetch, no IVECF transaction,
UBC acknowledgement and preservation of the masked HBlank request.

### Review-fix validation

Re-executed in the assigned worktree with the native and Wasm toolchains listed
above. `npm ci` supplied the locked Playwright dependency without changing
manifests or lockfiles.

| Check | Observed result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 1,119 tests, zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| Wasm warnings-denied Clippy (command above) | Passed |
| `sh scripts/check-fixtures.sh` | Passed; all 99 builders |
| `sh scripts/build-web.sh` (toolchain command above) | Passed |
| `node --test scripts/ci-runner.test.mjs` | Passed: 12 tests, zero skipped |
| `npm run test:browser` | Passed: all 69 groups (30 shared + 39 scenarios), 244.236 s |
| `cargo test --locked --lib interrupt_tests` | Passed: 16 tests |
| `git diff --check`, changed-document relative links | Passed |

The first CI-runner invocation lacked the local Playwright dependency. After
`npm ci`, another invocation overlapped web asset generation and timed out waiting
for browser initialization (11 passed, 1 failed). The final invocation ran against
the completed web build and passed all 12 tests. Neither earlier invocation is
counted as passing. The complete browser aggregate used the final immutable build.

### SCU evidence investigation (P2 remains unresolved)

Rechecked ST-097-R5 §2.2/Table 2.1 (printed p.27) and §3.5/Table 3.8
(pp.57–60), ST-210 item 7 (p.4), and H §§5.2.3/5.6 (pp.83–85/101–102).
They do not establish the three disputed SCU endpoint behaviors listed above.
The additional [ST-162-R1 System Program manual](https://antime.kapsi.fi/sega/files/ST-162-R1-092994.pdf),
pp.2/4/9, describes software interrupt dispatch priorities and mask-service IST
writes, not SCU hardware tie arbitration or its automatic acknowledgement edge.
Its IST-writing software description cannot override ST-210's prohibition.
[Technical Bulletin #15](https://antime.kapsi.fi/sega/files/Sattechs.pdf), item 7,
also prohibits those writes without specifying that edge. No affirmative primary
evidence supporting a replacement behavior or a specification reconciliation was
found. P2 remains an unmet mandatory acceptance criterion; #81 must remain open.

## Review follow-up: interrupt detection inside an idle batch (P1)

`Machine::step_hardware_boundary` now bounds advancement by the bus's existing
`main_deadline`, including device events that can assert a new interrupt. The
machine returns to CPU scheduling at that edge, before choosing an interval
using the new request's recognition deadline. Previously the bus could detect
the request internally and advance through recognition completion before the
machine issued the CPU again.

`production_scheduler_accepts_interrupt_detected_during_idle_batch_on_time` in
[the native phase vectors](../src/bus/interrupt_tests.rs) reproduces the finding
through the production scheduler, without the trace helper's extra issue calls.
SCI is disabled; FRT compare A=0 and OCIA generate a request at clocks 1–7.
Both CPUs and passive inspection off/on give 28 cases. H Table 5.8 requires
acceptance at R+5, and Figure 5.7 plus the two-state WRAM access gives first SR
push completion at R+10. The test also checks saved PC/SR. Before the fix, the
clock-1 case failed with acceptance **8 instead of 6**; all cases pass after it.

P2 remains unresolved. Rechecking ST-097-R5 §§2.2/3.5 and ST-210 item 7 did
not establish the disputed endpoint semantics. The additional
[ST-135-R4 Program Library User's Guide 3](https://antime.kapsi.fi/sega/files/ST-135-R4-092295.pdf),
Interrupt Management Library §2.4, printed p.63, describes software status-reset
and A-Bus acknowledge functions. It does not specify SCU same-level hardware
arbitration, automatic clearing at acquisition completion or SH-2 autovector
acknowledgement. That software API is not affirmative evidence for those
behaviors. No SCU behavior was changed on that basis, and no acceptance criterion
is waived. #81 and #17 must remain open.

### Idle-batch fix validation

Re-executed in the assigned worktree using the same native/Wasm toolchains and
matching wasm-bindgen CLI listed above. `npm ci` restored the locked browser
dependency; manifests, lockfiles and generated fixture assets are unchanged.

| Check | Observed result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 1,120 tests, zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| Wasm warnings-denied Clippy (command above) | Passed |
| `sh scripts/check-fixtures.sh` | Passed; all 99 builders |
| `sh scripts/build-web.sh` (toolchain command above) | Passed |
| `node --test scripts/ci-runner.test.mjs` | Passed: 12 tests, zero skipped |
| `npm run test:browser` | Passed: all 69 groups (30 shared + 39 scenarios), 252.797 s |
| Production-scheduler regression | Failed before the fix, passed after it |
| `git diff --check`, changed-document relative links | Passed |

The expected pre-fix regression failure is the only failed check in this pass.
Required post-merge verification: none.

## Review follow-up: reference-clock interrupt detection (P1/P3)

`Bus::advance_cpu_clocks` now yields at CD, SMPC and sound deadlines as well as
main-clock events. A shared `reference_deadline` bounds reference ticks after
PLL conversion. For non-reference PLL rates, the bulk path stops before the
fixed-device edge and the existing single-main-tick path processes that edge,
including zero/two-main-tick cases. This avoids rounding reference deadlines
into main clocks or advancing through a newly established recognition deadline.

`production_scheduler_accepts_reference_clock_interrupts_on_time` in
[the production-scheduler vectors](../src/bus/interrupt_tests.rs) exercises actual
INTBACK completion, CD Get Status/CMOK with A-Bus acknowledgement enabled, and
SCSP MIDI output completion. It covers all four NTSC/PAL PLL frequencies,
reference phases 0/14, requests 1–7 reference clocks away, autovector/external
modes and passive inspection off/on: 672 cases. Expected detection clocks use
literal frequencies and the documented cross-domain ordering convention;
acceptance at R+5 and SR push completion at R+10 follow H Table 5.8/Figure 5.7
and the two-state WRAM access. Saved PC/SR are checked independently. Before the
fix, the SMPC clock-1 case reproduced acceptance at **8 instead of 6**. All 672
cases pass with the fix.

The initial full native run exposed two reset tests that assumed one scheduler
call per vector word. [Those tests](../src/reset_tests.rs) now continue across
intermediate reference-device events, checking the reset CPU state at each
return. The literal 8/16/24/32 word-completion clocks, separately sampled vector
values, atomic register publication and cancellation checks remain intact;
cancellation is exercised at each actual completed word. No hardware timing
expectation was changed.

The next full run reached [the SH-2 system guest](../tests/sh2_system.rs) and
found the same one-sleeping-turn assumption for a pending buffered write.
The test retains the original **at most eight clocks** first-turn bound, then
polls final value 35 over bounded additional sleeping turns with CPU-state checks.
A public `step()` can return past a physical completion edge; bounding all later
observation clocks by eight also failed and is not a hardware latency oracle.
The unchanged [write-buffer vectors](../src/bus/write_buffer_tests.rs) still
check the physical write at every clock through its literal t8 completion and
the dependent read through t16. Its focused rerun passes. The final native rerun
uses `--no-fail-fast` so every test binary executes even if another failure appears.

PR #82's Related Issues now says `Related to #81`, with an explicit partial-work
statement instead of a closing reference. Its draft state and published head
were unchanged; this node does not push or merge.

### SCU evidence finding (P2 still unresolved)

Rechecked [ST-097-R5](https://antime.kapsi.fi/sega/files/ST-097-R5-072694.pdf)
§2.2/Table 2.1 (printed p.27) and §3.5/Table 3.8 (pp.57–60), and
[ST-210](https://antime.kapsi.fi/sega/files/ST-210-110194.pdf) item 7 (p.4).
Also checked [ST-103-R1 Saturn Overview](https://antime.kapsi.fi/sega/files/ST-103-R1-040194.pdf)
§3.2 (pp.13–15): it describes the SCU's interrupt-controller role and system
configuration, without specifying same-level selection, an acquisition-completion
clear edge or autovector acknowledgement. No affirmative primary evidence for
those three behaviors, or for a replacement technical contract, was found.
The existing deterministic conventions remain identified as unverified; no
criterion is waived and no Issue correction is asserted. P2 remains mandatory
and unmet. Leave #81/#17 open and PR #82 in draft.

### Reference-clock fix validation

Executed locally with the same native/Wasm toolchains and matching wasm-bindgen
CLI recorded above. `npm ci --ignore-scripts` restored locked browser dependencies;
no dependency, lockfile, fixture or generated web asset changes are committed.
The complete browser run used the final runtime build; the later changes affect
only native test polling and documentation.

| Check | Observed result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked --no-fail-fast` | Passed: 1,121 tests, zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| Wasm warnings-denied Clippy (command above) | Passed |
| `sh scripts/check-fixtures.sh` | Passed; all 99 builders |
| `sh scripts/build-web.sh` (toolchain command above) | Passed |
| `node --test scripts/ci-runner.test.mjs` | Passed: 12 tests, zero skipped |
| `npm run test:browser` | Passed: all 69 groups (30 shared + 39 scenarios) |
| Production reference-clock regression | Failed before the fix (8 vs 6); all 672 cases pass after it |
| `git diff --check`, changed-document relative links | Passed |

The preceding native failures and host-polling corrections are recorded above;
only the final complete run is counted as passing. P2 remains unresolved despite
these checks. Required post-merge verification: none.

## Review follow-up: premature Issue-closing reference

The next review found that PR #82's published body again contained `Closes #81`,
despite the partial-work statement above. Replaced that reference with
`Related to #81` and removed the paragraph describing the closing reference as
contingent. GitHub readback confirms the partial-work statement, draft state and
unchanged head `fa434b219245eb4f8d9c150138534d28ed9bd988`. Subsequent publication
must preserve the plain reference while mandatory acceptance remains unmet.
No push, merge or source Issue change was performed.

Rechecked ST-097-R5 §2.2/Table 2.1 and §3.5/Table 3.8 and ST-210 item 7.
Also inspected the [ST-202-R1 Dual CPU User's Guide](https://antime.kapsi.fi/sega/files/ST-202-R1-120994.pdf),
§§1.0/4.4–6.2 (printed pp.1/3–5). It specifies shared external devices, slave
initialization and FRT communication, but does not establish SCU same-level
arbitration, source-clear timing or autovector acknowledgement. No affirmative
evidence for a behavior change or technical reconciliation was found. The SCU
finding remains unresolved; this metadata correction does not satisfy it.

Validation for this documentation/PR-body correction: `git diff --check`, local
Markdown links and PR metadata readback passed. Runtime files are unchanged;
the native/Wasm/browser suites were not rerun. Their latest observed results
remain in the preceding validation table. Keep #81/#17 open and PR #82 in draft.
