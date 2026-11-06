# SMPC SF/COMREG lifecycle and ordering audit (#105)

## Outcome and limits

The source audit and original guests demonstrate no rejection of the tested,
documented SF-before-COMREG sequences and no loss of an idle, accepted SF claim.
There is no demonstrated hardware-contract defect requiring a runtime change in
this increment. `MissingSf` remains an explicit host protocol diagnostic. It is
**not evidence that Saturn hardware electrically rejects COMREG with SF=0**.
Source comments now make that distinction explicit.

This is the bounded no-defect outcome allowed by #105. The unavailable private
BIOS's preceding instructions, SF writes and reason for SF=0 remain unknown;
neither that firmware nor authentic BIOS/menu compatibility was tested or fixed.
No proprietary firmware, hash, trace, PC exception, substitute menu, dependency or
new testing subsystem is introduced. Broader hardware completion remains #17;
the #81 SCU evidence finding is unaffected. No post-merge verification is needed.

## Primary contract

Sources inspected on 2026-11-06:

- [ST-169-R1-072694](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-169-R1-072694.pdf),
  original PDF: §1.2/fig1.3, printed pp.5–6; §2.2 pp.13–20;
  §2.3/OREG31 p.21; RESENAB/RESDISA pp.33–34; INTBACK pp.36–44 and
  §3.2. The parameterless RESDISA table is in **§2.3**, classified Type B
  by table2.4, rather than the non-resetable command section §2.4.
- [Japanese command-issuance transcription](https://docs.exodusemulator.com/Archives/SSDDV25/segahtml/hard/smpc/hon/p02_20_j.htm):
  direct fetch failed, but the search index supplied the issuance, timing and
  restrictions text. Its account of SF management to prevent double issue
  agrees with the original PDF. It supplies no additional electrical gate rule.
- [SH7604 hardware manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf),
  original PDF: §§2.3/7.3 (areas/operand cycles), §7.11.2 pp.210–211
  (CPU completion, actual cache-through address and single-level write buffer),
  §8 (write-through cache and generated longword refill cycles).
- [Sega technical bulletins](https://manualzilla.com/doc/5914290/saturn-technical-bulletins),
  *Using Dual CPUs on the SEGA SATURN* §§4.5/7.1: clock changes reset slave;
  the example polls SF, writes 1, then SSHOFF/SSHON. The SMPC status correction
  lists RESD=1 as the default. These reinforce software sequencing and reset
  state; neither describes the electrical effect of missing SF.

The manual requires software to check SF bit0=0, write byte 01 to SF, optionally
write IREG parameters, then write the COMREG command byte. SMPC clears SF at
completion. SF upper read bits are undefined; this model deterministically
returns zero there. Only set is specified for SH-2 SF writes, so other byte
values retain `InvalidSf`. COMREG is byte/W; OREG31 is the command report,
published when processing starts. It is not a COMREG readback register.

The SF discussion describes software synchronization against double issue.
It does not specify command-discard circuitry, an SF=0 bus error, or guaranteed
acceptance with SF=0. Neither unconditional acceptance nor hardware rejection
follows from that silence. Retaining a bounded failure for an out-of-protocol
sequence is an emulator policy, not a verified electrical behavior or an SH-2
architectural exception. The guest negative control proves that policy only.

The diagnostic's `busy` means an internal pending command or INTBACK wait. It
does not simply echo SF: a claimed SF can be 1 while internal busy is false and
there is no deadline. Likewise SF=0/busy=false/command=0 is coherent at cold
initialization (and command 00 can be the last completed command). The command
field is the last accepted code, not proof of current execution. The manual
does not provide a complete power-on value table for every interface latch;
zeroed command/result latches are this model's deterministic initialization.
Disc absence has no separate SF initialization rule in these sources or code.
RESDISA disables reset-button NMI, defaults disabled at power-on, has no IREG
parameters or SMPC completion IRQ, and reports 1A in OREG31. The existing
807-reference-clock deadline represents the listed 30 μs, rounded up at
26.8741 MHz. Internal task collisions and exact physical latency are not modeled.

## Complete SF lifecycle in the source

All SF assignments are in [smpc.rs](../src/smpc.rs). Snapshot/read/error paths
in [diagnostics.rs](../src/smpc/diagnostics.rs) do not mutate command state.

| Entry/path | SF, command and deadline behavior |
| --- | --- |
| `Default` | SF=false, command=0, pending=None, result=0; reset-button NMI disabled. |
| `write_from`, SF byte 01 | SF=true only. No deadline, command change or implicit COMREG write. This remains true through elapsed clocks, non-acquisition raster edges and unrelated port/input changes. |
| Accepted COMREG | Requires idle internal state and claimed SF; latches command/OREG31 and deadline or INTBACK VBlank wait. Parameterless 1A ignores IREG contents. Master/slave restrictions remain table2.5's five commands; 1A is permitted on either CPU. |
| `advance_clocks` | With no pending command, returns without clearing SF. Before deadline it decrements remaining clocks only. At completion it removes pending, clears SF and performs command effects. Ordinary completion retains last command/OREG31. |
| INTBACK status completion | Restores SF=true if peripheral continuation remains; SF=false for status-only. Sets result/IRQ independently of SCU acknowledgement. |
| Continue/VBlank-out | IREG0 continuation changes await flags and starts the next peripheral phase at VBlank-out; it does not reinitialize SF. |
| Peripheral completion | Clears SF; packet/IRQ become ready. Later raster edges do not clear a newly claimed idle SF. |
| `end_peripherals` | BREAK during await/collection or VBlank-in collection timeout cancels pending/await flags and clears SF, retaining command/report. Its call sites require the active INTBACK phase; an unrelated idle claim is untouched. |
| `reset_volatile` | Explicit Machine cold reset/boot selection calls Bus reset, replacing volatile state and queued bus work. SF/command/results return to zero. Region/battery/backup retention is independent. |
| `system_reset` | Only timed SYSRES completion uses this path. Reinitializes SMPC; SF=false, command=0, OREG31=0D. Retains documented model battery/input domains and installed firmware/media. |
| NMI, reset-button input, NMIREQ | CPU exception/input paths do not reinitialize SMPC or its SF. RESENAB/RESDISA change the reset-button NMI enable at command completion. |
| Clock changes, SSHON/SSHOFF, SH-2 watchdog reset | Clock completion clears the completing command's SF, resets sound/slave and requests master NMI. It preserves the master transaction journal. CPU-only resets/drains and slave off/on do not call SMPC reset. |
| Failed reads/writes and Debug observation | Return snapshots/errors without clearing SF, changing a deadline or publishing results. Wide operands fail before register decomposition. |

A new SF write **while an earlier command is still busy** is not an independent
reservation protected from that earlier completion; the documented sequence
requires waiting first. This does not demonstrate lost SF in a valid sequence.
The audit found no unrelated SMPC reinitialization on instruction execution,
cache activity, ordinary device advancement, no-disc detection or UI rendering.

## SH-2 and bus ordering

[CpuBus](../src/bus.rs) checks complete SMPC widths before cache/BSC
decomposition. `Bus::physical` maps P1 20100063/2010001F to 00100063/0010001F,
retaining the caller's address for diagnostics. SMPC's odd byte lane reaches
`Smpc::write_from(issuer, lane, byte)`; even lanes are disconnected. `MOV.B`
uses one byte regardless of configured external bus width. A cached P0 data
read can generate prohibited longword refills; P1 reads use the actual byte
address even with CCR enabled. SF/COMREG writes are write-through, not retained
only in cache.

[Transaction scheduling](../src/bus/transactions.rs) distinguishes CPU write
acceptance from external completion. `buffered_external_write` captures each
transfer's address/width/value. `accept_buffered_write` refuses another external
write while that processor's buffer is occupied. `grant_buffered_write` drains
the front beat; `complete_bus_transaction` uses the owning processor when calling
`write_from`, and removes the buffer only after its last beat. A second COMREG
store therefore cannot pass the preceding SF store on that CPU. Internal/on-chip
or cache-hit work can progress meanwhile; competing CPU/DMAC/SCU/refresh owners
can delay bus grants without dropping, widening or reordering those two stores.
Read snapshots use the actual `main_clocks` at device access, not CPU retirement.
SMPC command deadlines advance in the reference-clock domain, distinct from
PLL-selected main clocks.

CPU-only reset drains preserve already accepted physical transfers and cancel
unaccepted instructions; whole-machine cold reset replaces the queues. This is
intentional reset ordering, not evidence that ordinary execution loses an SF
write. Existing reset-bus, write-buffer, issuer, cache-refill and dual-CPU guests
remain complementary coverage. No special treatment of PC 0000025C is present.

## Original runnable evidence

The existing [builder](../tests/fixtures/build_smpc_transactions.py) now generates
three additional original MIT 512 KiB reset-vector ROMs (34 total), assembly
listings and literal expected result records. No SDK/cross compiler is needed.

| Guest/control | Evidence |
| --- | --- |
| `smpc-sf-direct` | Cold SF/report=0; documented raster window; RESENAB then INTBACK proves RESD=0; claims SF and writes 1A through P1 byte stores; samples SF=1/report=1A, polls to SF=0/report=1A, then INTBACK proves RESD=40. |
| `smpc-sf-activity` | Same sequence with CCR purge/enable, instruction fetch/refill, cached RAM fill/hit/write-through, uncached RAM stores/reads and a 512-iteration arithmetic/branch loop between SF and COMREG. Both RAM reads produce 12345678 and SF remains 1 before command issue. |
| `smpc-sf-missing` | P1 COMREG=1A with SF=0 yields sticky `MissingSf`, byte width/value/master provenance, command/report=0 and no pending deadline or success marker. This is an emulator-policy control. |
| Command operand mutation | Change the unique 1A literal to 19 without removing the claim or success marker. Final actual RESD becomes 0; the original expected result fails. Native and Wasm both check this mutation. |
| Cold reset of an idle claim | Native/Wasm reach the guest-written 55 reservation mailbox before COMREG, cold reset, verify cleared state/mailbox, then execute the entire original sequence again. No cancelled store leaks into the new boot. |
| Unit deadline/lifecycle fences | After RESENAB, a reset-button edge requests NMI at RESDISA deadline−1; at deadline it is disabled, SF clears and report stays 1A. A later SF claim remains intact after another 10000 clocks; inherited fences also check deadline+1. Idle claims survive 30000 clocks/raster/input/errors; cold/SYSRES discard them explicitly. Chunks 1/3/127/10000 agree. |
| Inherited boundaries | Existing timed INTBACK continue/BREAK/timeout, pending SETTIME/SETSMEM/INTBACK cold resets past former deadlines, SYSRES, NMI/clock, issuer, widths and invalid parameter guests are retained and rerun. |

[Native guest assertions](../tests/smpc_transactions.rs) and
[production Wasm assertions](../scripts/hardware-boundaries.mjs) compare the same
literal result files and [shared progress records](../tests/fixtures/smpc-progress.tsv)
at 250000 turns: direct 2777186 clocks/65446 instructions; activity 2082131
clocks/152304 instructions; zero exceptions for both. These captured model counts
are regression evidence, not independently specified Saturn cycle timings.
Chunks 1/3/127/10000, passive Debug observation and reset preserve final CPU/device
state. [Play UI](../scripts/play-ui.mjs) uploads each local guest with no disc,
uses real Start and Restart, checks synchronous cleared CPU/result/fault state,
then checks the same RESD records or MissingSf fault. It preserves the existing
offline/no-upload/storage and HLE/media/control regression assertions.

## Acceptance and executed validation

| #105 criterion | Assessment |
| --- | --- |
| Source-first lifecycle audit | Complete; every SF assignment and reset caller is classified above. |
| SH-2/bus ordering audit | Complete for the implemented byte path; buffering/issuer/clock paths inspected and cache-active synthetic sequences tested. |
| Document hardware contract | Complete; software obligation is distinct from unmeasured SF=0 electrical behavior and internal busy. |
| Original runnable regressions | Complete; direct/missing/activity, real RESD effects, deadline and reset/abort boundaries covered by new and inherited guests. |
| Evidence-led correction | Bounded no-defect outcome; runtime acceptance/diagnostics/timing preserved. No fabricated fail-before claim. |
| Native/production browser | Complete; matching guest results/progress, fault state and real no-disc Start/Restart pass. |
| Regression/validation | Complete locally as recorded below; no hosted publication or post-merge action performed. |

Validation environment: stable Rust 1.99.0 with its compiler/Cargo first in PATH,
locked dependencies and wasm-bindgen 0.2.129. Browser version is the pinned
Chromium 145.0.7632.6. Timing wrappers report local wall time with overlapping
checks, not a serial performance baseline. Commands use the existing
`python3 scripts/ci-time.py <label> <command>` timing wrapper where a duration
is listed.

| Command | Result / wall time |
| --- | --- |
| `npm ci --ignore-scripts` | Pass; locked dependencies, no changes to lockfiles. |
| `cargo fmt --check` | Pass. |
| `cargo build --locked` | Pass / 4.394 s. |
| `cargo test --locked` | **1193 pass**, zero failed/ignored across 107 harnesses including doc tests / 517.604 s. Includes existing external-BIOS, SMPC, HLE, CD/media, backup and dual-CPU guests. |
| `cargo test --locked --lib smpc::tests` | **24/24 pass**; 0.04 s harness time, including both new lifecycle/deadline tests. |
| `cargo test --locked --test smpc_transactions resdisa` | Final strengthened result/operand assertions: **2/2 pass** / 9.869 s including compilation. |
| `cargo clippy --locked --all-targets -- -D warnings` | Pass / 5.008 s; after final test refinements pass / 1.509 s. |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Pass / 5.476 s. |
| `sh scripts/check-fixtures.sh` | All **104 registered commands pass** / 6.913 s. |
| `python3 tests/fixtures/build_smpc_transactions.py --check` | All **34 ROMs**, assembly and literal records reproduce; repeated after final fixture edits. |
| `sh scripts/build-web.sh` | Production release Wasm and matching bindings pass / 9.470 s. |
| `node --test scripts/ci-runner.test.mjs` | **13/13 pass**, zero failures/skips / 14.171 s. |
| `node scripts/browser-suite.mjs --group=hardwareBoundaries` | Production Wasm original/new guests pass / 116.432 s; final full-table command-mutation control also passes / 115.581 s. |
| `npm run test:browser` | **74/74 groups pass**, zero failures/skips / 512.493 s wrapper, 512.261 s suite. Shared: 31/31, including BIOS-only Start/Restart and hardware boundaries; scenarios: 43/43. Both tested-artifact/coverage reports verified by the existing runner. |
| JS syntax, Markdown local links and Git whitespace | Pass. |

Browser commands set `PLAYWRIGHT_BROWSERS_PATH="$PWD/target/issue105-browser"`;
the suite additionally uses `BROWSER_CHANNEL=chromium`. The initial focused
browser launch failed because pinned executable 1208 was absent. The standard
installer stalled after 100% download and was stopped. As in the #103 setup
record, the pinned full Chromium archive and matching headless shell from the
installer's official CDN were verified with ZIP integrity checks and extracted
locally with executable modes. The runner uses the matching headless shell;
the suite uses full Chromium. No dependency version, assertion or timeout was
changed. An initial native test helper also exceeded the existing 10000-turn
`run_steps` cap; it was corrected to issue bounded chunks, after which both tests
pass. These setup/helper failures are excluded from the passing results.

The last test refinement strengthens the changed-command control from checking
RESD/success to checking the entire changed result table. Focused final checks
cover that refinement; the full regression run also passes the original control.
No production runtime semantics changed after the production web build.
