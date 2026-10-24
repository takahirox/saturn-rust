# Issue #55 validation: ordinary MOV load stages

Related to #55 and #17. This implements ordinary MOV memory-to-GPR scheduling;
#17 remains open and hardware completion remains incomplete. Starting repository
commit: `4d8a8f81dd64e6b610c9930a7b808a97d8a5c450`, whose production source is the
selected `e3b17e1e22f68232b6dfb68c1687b736ec490380` snapshot.

## Specification and implementation

The [stage/clock table](sh2-load-pipeline.md) was written before production code
changes. It identifies the September 3, 1996 Hitachi Programming Manual,
applicable sections/figures, SH-2 scope and the figure7.11 MA label typo, cross-checked against Renesas Rev.5.00 §§7.1–7.2 and fig7.20. No
acceptance criterion or hardware rule was relaxed; no external Issue correction
was necessary. The execution instruction prohibited GitHub comments/PRs/pushes,
so this node only changes and commits its worktree.

[CPU scheduling](../src/cpu.rs) decodes all ordinary load addresses through
[the timing module](../src/cpu/timing.rs) before executing the existing operand
semantics. Scheduled reads start at MA; synchronous reads wait until its end.
The existing journal retains classification, earliest completion, clock samples
and read effects. Shared successor EX retention now belongs to the CPU-local
load pipeline and is also used by [multiplier phases](../src/cpu/multiplier.rs).
An existing IF split covers the ordinary load dependency split. Equal-edge
ordering and stretched-slot representative EX clocks are explicit model
conventions in the contract, not extra hardware claims.

## Independent coverage

- [CPU vectors](../src/cpu/load_tests.rs): every addressing form and width,
  postincrement and indexed aliases, sign extension, both instruction alignments,
  delayed-branch PC-relative addressing, register MOV/MOVA/MOVT exclusions.
- [Machine vectors](../src/load_scheduler_tests.rs): both SH-2s, independent,
  source-dependent and destination-only dependent successors, cache/cache RAM,
  peripheral and external operands, 0/1/2/6 Tw, before/at/after mutations,
  once-only auto-update, power/manual/cold/SYSRES cancellation (including a same-edge completed sample),
  MAC address forwarding and NMI arrival during a pending read.
- [Bus vectors](../src/bus/load_tests.rs): literal refill grants at 33/35/37/39,
  requested sample at 41, retained earlier words, horizon 50 across advancement
  chunks 1/3/17/10000 with and without passive inspection; existing peer, DMAC
  and SCU priorities and delayed CPU completion; FIFO effects survive repeated
  CPU replay and reset before architectural publication.
- Existing exhaustive decoding, address-error, IRQ, TAS, buffered-write, refresh,
  device-ownership and CPU/system reset suites retain their substantive checks.
- Original MIT [guest](../tests/fixtures/sh2-load.s),
  [builder](../tests/fixtures/build_sh2_load.py),
  [byte/hash records](../tests/fixtures/sh2-load.sha256),
  [literal oracle](../tests/fixtures/sh2-load.tsv),
  [native consumer](../tests/sh2_load.rs) and
  [browser consumer](../scripts/sh2-load.mjs): identical bytes execute on both
  processors, producing FRT and arithmetic/readback records. Controls change an
  authored operand, remove a dependency or move a kernel by one word. Each
  variant repeats cold boot. Expanded ROM SHA-256:
  `4a29e441e678d12ede5d6b7a4d4fb8f20845dade4898f925299098099395d32e`.
  No host writes results.

The guest was run against inherited production code in an isolated temporary
worktree. `cargo test --locked --test sh2_load` failed (exit 101), reporting
aligned/offset independent ticks 4/4 against the authored 5/7, repeat ticks 4
against 7 and dependent peripheral ticks 8 against 9. The final implementation
passes the same bytes/oracle. No expectation was derived from these captures.

Affected historical FRT oracles are corrected using the independent arithmetic
in [the contract](sh2-load-pipeline.md#existing-timing-fixtures). The rotation
fixture is retimed to retain its existing pixel and H-counter assertions.
Star Collector, Prism Courier and Hardware Lab tests are unchanged.

## Executed checks

- `cargo fmt --check`: passed.
- `cargo build --locked`: passed.
- `cargo test --locked`: passed, 980 tests across 91 result groups, zero
  failures and zero ignored tests. Focused final replay/grant vectors also pass.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`:
  passed using rustup stable and its installed Wasm target.
- All 90 fixture/media `--check` commands registered in
  [existing CI](../.github/workflows/pages.yml): passed, including the new guest.
- `sh scripts/build-web.sh`: passed with the documented rustup PATH selection
  and wasm-bindgen 0.2.129. An initial invocation selected the Homebrew compiler
  without its Wasm target; selecting the installed rustup target resolved it.
- Complete extended `npm run test:browser`: passed under `/saturn-rust/` in
  Chromium 145.0.7632.6, with no required skips. This includes the new MOV guest,
  all corrected timer fixtures, and unchanged Star Collector, Prism Courier and
  Hardware Lab assertions, audio/worklet checks, controls and repeated boots.
- JavaScript syntax, documentation file links and `git diff --check`: passed.

Native checks used Homebrew rustc 1.94.0; Wasm checks/build used rustup rustc
1.99.0. Production `web/pkg/saturn_rust_bg.wasm` SHA-256:
`b7350653df9cf918c8026a1639502c7815a030eb6f98795604c26d2c9e91fcba`.
A final rebuild produced the identical Wasm hash used by the passing browser
run. The generated Wasm is recreated by the build script, rather than committed.

## Remaining scope

Precise new pipeline evidence is for warmed instruction-cache execution.
Uncached fetch/refill guests still execute through the existing demand-driven
path, which does not model speculative future IF/ID transactions. Ordinary
store pipeline completion and other families remain separate mandatory work.
Full CPU/peripheral semantics, bus/reset/arbitration conformance, SCU/DSP,
video/audio, CD/media/HLE, SMPC and comprehensive integration acceptance remain
mandatory under #17. Genuine BIOS and commercial-game compatibility remain
unverified. No personal firmware, commercial media, physical console, human
approval or live Pages inspection was required or used.
