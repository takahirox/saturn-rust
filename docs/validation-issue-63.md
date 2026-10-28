# Issue #63 validation: ordinary MOV store stages

Related to #63 and #17. This implements the scoped ordinary-store increment;
#17 remains open. Starting worktree commit: `052e3c8d145e24c47a94570cf16aab5614a1432c`,
with inherited production source from selected main
`82ec9c509ac2e824a6de523c08e861d2978f8920`.

## Specification and implementation

The [stage/deadline table](sh2-store-pipeline.md) was written before production
scheduling changes. It cross-checks Hitachi Programming Manual §§7.2–7.5 and
§7.7.1/fig7.12 (pp.185–186), Renesas Software Manual Rev.5.00 §§7.1–7.2 and
§7.4.1/fig7.21 (p.438), and SH7604 Hardware Manual §§7.3.1/7.11.2. The contract
distinguishes manual requirements from deterministic emulator phases and gives
aligned/offset, waited, preceding-load and buffered/synchronous deadlines.
No source-Issue criterion or hardware rule was weakened. This execution forbids
GitHub comments, PRs, pushes and merges; only the local worktree is committed.

[CPU execution](../src/cpu.rs) decodes every ordinary store address through
[MOV timing](../src/cpu/timing.rs), sharing load MA qualification and successor
overlap. Register MOV, system-register stores, multiplier and TAS remain on their
existing paths. Source/address preparation uses the retained CPU snapshot, so
replay does not repeat register changes. Synchronous array writes occur at MA
completion; scheduled writes request at MA start. Invalid operands still occupy
MA before architectural retirement, branch-target publication and after-execution
break observation. The existing [journal/BSC transactions](../src/bus/transactions.rs)
retain scheduling and accepted payloads, independent of instruction retirement.
No external device or buffered publication duration was replaced by CPU clocks.

## Independent evidence

- [CPU vectors](../src/cpu/store_tests.rs): indirect, predecrement, indexed,
  register/GBR displacement; byte/word/long widths; effective addresses, source
  aliases and byte lanes; unchanged flags, once-only updates, aligned/offset and
  waited MA; delay-slot source/target retention and register-only exclusions.
- [Executing scheduler/bus vectors](../src/bus/store_tests.rs): both processors,
  cache data/address arrays, two-way RAM and purge, write-through/cache-through
  RAM, on-chip MMIO and video longwords; requests, local effects, acceptance,
  separate publication halves, retained payloads after successor register writes,
  six-Tw write-buffer backpressure and an external-read barrier. The backpressure
  kernel drains host cache-warming reservations before its E=64 anchor, satisfying
  H §7.9's preceding-read gap independently of measured results.
- Real peer CPU, peer on-chip DMAC and SCU requests assert grant priority,
  publication deadlines, copied data and DMA completion. The additional
  [A-to-B startup vector](../src/bus/dma_startup_tests.rs) executes an ordinary
  store: EX preparation does not qualify; MA's request qualifies before acceptance.
  SCU may initialize with a width-zero reservation, but cannot consume CD FIFO
  data until both CPU words publish. DMA data and completion IRQ retain order.
- Executing delay-slot/NMI, invalid-operand/odd-stack, manual/power/cold/SYSRES
  vectors cover pending replay and reset before admission versus accepted data.
  Manual reset drains accepted external work; power/system resets cancel it.
  Invalid-store suppression is separate from the existing H §4.8.3 deterministic
  byte-write policy for undefined unaligned exception stacking.
- Video boundary vectors cover advancement chunks 1/3/17/10000, with and without
  passive inspection, retaining old/first-half/final words and a fixed horizon.
  The native guest also repeats all controls across these advancement chunks.
- Original MIT [guest source](../tests/fixtures/sh2-store.s),
  [builder](../tests/fixtures/build_sh2_store.py),
  [bytes/hash manifest](../tests/fixtures/sh2-store.sha256),
  [literal records](../tests/fixtures/sh2-store.tsv),
  [native consumer](../tests/sh2_store.rs) and
  [browser consumer](../scripts/sh2-store.mjs) execute identical kernels on both
  CPUs. FRT/alignment/dependency, arithmetic/readback, changed-data, omitted-store
  and repeated cold-boot controls remain guest-generated. A master publishes data
  and flag; the slave computes and publishes the acknowledgement; both record it.
  No host manufactures completion. Expanded ROM SHA-256:
  `53745928a299cc18704cf100289ce652d11d980623089aa21ae8349a910bac38`.

The same guest bytes and oracle were copied into an isolated inherited worktree.
`cargo test --locked --test sh2_store` failed there (exit 101): records were
`5,3,5,3,5,3,7,3,5,7,5,3,12345679,600D` (hex for the last two), against authored
`5,3,7,3,7,3,9,3,7,7,7,3,12345679,600D`. The final implementation passes the
same original oracle and controls. These diagnostic captures did not define
expectations. The forwarding correction below supersedes the original dependent
kernel's timer expectation of 9 with 7; the guest source and ROM bytes are unchanged.

The existing peripheral-store/PR-wrapper timer records and rotation-fetch guest
are reconciled using the [independent derivation](sh2-store-pipeline.md#existing-guest-timing-reconciliation).
Rotation tests retain all pixel, counter, arithmetic and mutation assertions.
Star Collector, Prism Courier and Hardware Lab computations, devices, pixels,
audio, completion/restart and controls are unchanged.

## Executed validation

- `cargo fmt --check`: passed.
- `cargo build --locked`: passed.
- Complete final `cargo test --locked`: passed, 1002 tests across
  93 result groups, zero failures and zero ignored tests.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`: passed.
- All 91 registered fixture/media reproduction commands in
  `sh scripts/check-fixtures.sh`: passed.
- `sh scripts/build-web.sh`: passed, using the documented rustup PATH and
  wasm-bindgen 0.2.129.
- Complete extended `npm run test:browser`: passed under `/saturn-rust/`, all
  65 groups (30 shared, 35 isolated scenarios), zero required skips. Aggregate
  elapsed 150.636 seconds. This includes the new guest, corrected timer/rotation
  records, unchanged playable guests and production audio/Canvas checks.
- `node --test scripts/ci-runner.test.mjs`: all 12 tests passed, zero skips.
  Registry assertions include the additional store guest, with all existing
  required groups/builders retained.

Native checks use Homebrew rustc 1.94.0. Wasm checks/build use rustup rustc
1.99.0. Production Wasm SHA-256:
`0df258e13622a2b93fad576594eb8127a570a81d215a7efdfa1844dd7d93ba05`.
The build script recreates ignored Wasm output. Initial complete native/browser
runs exposed old peripheral-store/PR-wrapper and rotation-fetch expectations;
the final records retain the independent arithmetic and substantive checks.
Focused test-development failures were corrected before final validation.

## CCR timing review correction

The review regression reproduced an early synchronous CCR effect: with warmed
`MOV.B R15,@R15` (`0x2FF0`), R15=`0xFFFFFE92` and EX=32, CCR changed from
`0x01` to `0x82` at clock 33. CCR stores now use the synchronous MA classification
and apply control/purge at clock 34, following the existing stage table.
The executing vector in `src/bus/store_tests.rs` covers both CPUs and both
instruction alignments, repeated pending attempts at clocks 32/33, control and
cache invalidation at 34, and replay at 35 without repeating the purge or losing
a later refill. It fails against the pre-correction classifier at clock 33.

Correction validation passed: formatting, locked build, all 1003 native tests
(93 result groups, zero failures/ignored), native all-target and Wasm Clippy with
`-D warnings`, all 91 fixture/media reproduction commands, production web build,
all 65 browser groups (zero required skips), and all 12 browser-runner tests.
Browser aggregate elapsed 155.866 seconds; rebuilt production Wasm SHA-256:
`b8c7faf6a43b0e92169101af5b68a913f62eccab436e8aab4362dea3a031d21b`.

## Load-to-store forwarding review correction

Renesas Software Manual Rev.5.00 §7.2.2, printed p.419/Figure 7.14,
explicitly permits a memory load's WB and the following store's data MA in the
same cycle through internal-bus forwarding. The original table incorrectly
applied the address interlock to data-only dependencies. The corrected
[table and guest derivation](sh2-store-pipeline.md) distinguish data-only uses
from base/index uses, including aliases, and extend the derivation to waited
loads. No alternate SH7604 rule is asserted.

The CPU dependency mask now excludes ordinary-store data only after a memory
load. Address/index uses retain their EX interlock; multiplier/system-register
writeback retains its established schedule. The exhaustive CPU vector covers
all ordinary store forms/widths, all loaded and source/address registers,
aliases, and both retained/real IF cases. Both-CPU scheduler vectors now require
the aligned data-forwarded store at clock 35, with address/index stores still at
36. These vectors failed before the production correction (exit 101).

Sixteen data-only load/store pairs take N=48 and record FRT=7, matching
independent data. The native/browser guest controls now also load the address
pointer, preserving N=64/FRT=9 and the same readback, arithmetic and shared-data
handoff records. The corrected guest oracle failed before the production fix
(exit 101, original dependent record 9 versus expected 7). Native checks retain
chunks 1/3/17/10000 and repeated boots; browser checks retain both CPUs and
repeated boots. Fixture source/ROM bytes are unchanged; the builder, literal
oracle and oracle hash are updated reproducibly.

The first complete native run exposed the shared tracker's STS MACL-to-MMIO
interlock; restricting forwarding to memory loads preserves that existing
regression. Local browser setup required `npm ci`. The initial web build could
not locate the installed rustup linker's `libLLVM.dylib`; the final build uses
a temporary linker wrapper setting `DYLD_LIBRARY_PATH` to that same toolchain's
`lib` directory. No repository build settings or dependencies were changed.

Final forwarding-correction validation passed:

- `cargo fmt --check`, `cargo build --locked` and complete `cargo test --locked`:
  1004 native tests in 93 result groups, zero failures and zero ignored tests.
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`.
- All 91 fixture/media reproduction commands in `sh scripts/check-fixtures.sh`.
- `sh scripts/build-web.sh` and complete extended `npm run test:browser`:
  all 65 groups, zero required skips; aggregate elapsed 147.573 seconds.
- All 12 tests in `node --test scripts/ci-runner.test.mjs`, zero skips.
- Changed documentation file links and `git diff --check`.

Native checks use Homebrew rustc 1.94.0; Wasm checks/build use rustup rustc
1.99.0 and wasm-bindgen 0.2.129. Final production Wasm SHA-256:
`55c779dfb51318782903955d637c4c9ff9f89bdf17e0d1045dcf97451c04e204`.

## Remaining scope

Precise overlap evidence is for warmed instruction-cache execution. The existing
demand-driven cold/uncached fetch/refill path continues to execute; full
speculative IF/ID and unrelated instruction families remain mandatory work.
This completes only the ordinary-store increment. Remaining CPU/peripheral,
bus/reset/arbitration, SCU/DSP, video/audio, CD/media/HLE, SMPC and integrated
#17 acceptance remain mandatory. Hardware completion and genuine BIOS/commercial
compatibility remain unverified. No personal firmware, commercial media, physical
console, human testing/approval or live Pages inspection was required or used.
Required post-merge verification: none.
