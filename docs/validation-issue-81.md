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
