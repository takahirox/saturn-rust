# Issue #17 implementation validation

**Result: partial implementation. Related to #17. No umbrella acceptance
criterion is complete; #17 must remain open.** Issues #11/#13/#15 are historical,
administratively closed trackers and must not be reopened.

## PR #18 review fixes: SH7604 and cache (2026-10-21)

Started from review checkpoint `1cd6385b801d7b5462a915794687a13df3eb9102`,
which preserves PR head `a286e489f06c4044368b03a2e947f5bbef673bd7`.
Upstream main remains `dbd981b030a46daa6923072ce240680dd2e58ebf`, verified with
`git ls-remote origin refs/heads/main`. Changes are local; no push or merge.

Implementation added in this revision:

- Misaligned exception stacking now completes the original exception and reaches
  vector 9, inhibiting recursive stacking errors as required by SH7604 §4.8.3.
  Undefined stacking data use explicitly documented big-endian bytes at the exact
  SP. Native vectors cover offsets 1/2/3; the shared guest repairs its stack.
- CPU-local WDT interval/reset modes, keyed writes, divisors, observed-one/clear
  protocol, IRQ vector/priority, reset-cause retention and 512-clock internal reset
  hold (H §§12.2–12.3). Reset vectors use physical 8/12 for manual reset and
  preserve shared RAM and battery backup. Browser Step/Run remain usable while
  reset completion is pending.
- CPU-local SCI registers, internal-clock transmit timing, status, receive errors,
  external-character adapter, interrupt sources, and real RXI/TXI DMA handshakes
  (H §§13.2–13.3). No implicit loopback or invented incoming data.
- CPU-local signed 32/32 and 64/32 DIVU, dependent-access interlocks, completion
  timing, DVCR/VCRDIV and overflow IRQ (H §§10.2–10.4). **The six-cycle overflow
  intermediate is not implemented:** overflow output is deterministic saturation
  plus input-high preservation, including OVFIE=1. This remains an explicit gap.
- On-chip DMAC channels 0/1, transfer widths 1/2/4/16, counts/address updates,
  auto/DREQ/SCI triggers, fixed/round-robin channel selection, end IRQs, flag
  acknowledgement, NMI cancellation and CPU vector 10 for address errors
  (H §§9.2–9.3). Single-address DACK mode is rejected explicitly. Burst/cycle-steal
  CPU arbitration and bus-cycle timing are not implemented.
- IPRA/IPRB and VCRA–D/VCRWDT internal source arbitration, using programmed
  priority and the manual table 5.4 tie order. Peripheral flags remain guest
  acknowledged. External ICR/IRL, refresh and UBC sources remain missing.
- Independent 4 KiB write-through caches, CCR ID/OD/CE/CP/TW, pseudo-LRU,
  associative purge, address/data arrays, two-way RAM and TAS external reads
  (H §8, tables 8.2–8.4, figs.8.10/8.11). Slave/DMA writes do not snoop master
  cache; the guest must purge it. Hardware fill timing/contention remain absent.

The original MIT-licensed [sh7604.s](../tests/fixtures/sh7604.s), built by
[build_sh7604.py](../tests/fixtures/build_sh7604.py), executes the same guest code
natively and in Chromium Wasm. Its expanded 524288-byte synthetic BIOS SHA-256 is:

```text
a45486b44108ca2db9abd4df0d05fe030c75f01ba73fbc4e64ab64eae9964880
```

Guest instructions calculate signed quotient/remainder, initialize/copy four
RAM words with DMA, acknowledge DMA/WDT interrupts, recover from a DMA address
error at SR.I=15, transmit through SCI, prove stale-cache/purge behavior, trigger
manual reset, observe CCR reset/cause retention, and reach the address-error
handler after misaligned TRAPA stacking. The sixteen golden mailbox words at
06003000 are independent test constants:

```text
FFFFFFF2 FFFFFFFE 55555555 00000001 00000001 00000001 FFFFFF84 00000001
FFFFFFFF 00000001 0000600D 00000000 00000000 12345678 00000000 00000001
```

They encode q32/r32, q64/r64, DMA IRQ, WDT IRQ, SCI status, manual-reset count,
reset cause, stacking-error handler, completion, pre-DMA cache value, stale value,
purged value, reset CCR and DMA address-error handler. After reset there are two
exceptions (TRAPA then address error), saved PC=00001C00, SP=060FF000 and no
pending address error. The four DMA destination words are 12345678–1234567B.
Cold reset repeats the result. This ROM is a peripheral diagnostic, **not** either
required new demonstration disc.

Checks performed on this revision:

- Formatting, locked native build and all **100 native tests** pass; none ignored.
- Native all-target Clippy and Wasm Clippy with warnings denied pass.
- Star Collector, dual-SH-2 and new SH7604 fixture reproducibility checks pass.
  CI now verifies the new fixture too; the existing ISO and dual guest are unchanged.
- `sh scripts/build-web.sh` passes with the existing rustup stable toolchain and
  wasm-bindgen-cli 0.2.129. An initial attempt selected Homebrew Rust without its
  Wasm target; selecting the installed rustup toolchain fixed that environment
  mismatch without source/dependency changes.
- `npm ci --ignore-scripts`, `npx playwright install chromium`, and the complete
  `npm run test:browser` pass in Chromium 145.0.7632.6 under `/saturn-rust/`.
  The new guest assertions include repeat execution and usable controls during
  watchdog reset, alongside all existing game/input/backup/privacy checks.
- JavaScript syntax and Git whitespace checks pass.

The full operator objective is **not achieved**. CPU/SH7604 edge cases, BSC/UBC/
standby, full mapping/backup services and hardware-time scheduling remain pending,
as do MC68EC000, SCU/DSP, VDP1/advanced VDP2, SCSP/DSP/CD-DA/WebAudio, runtime
CD/BIN/CUE, complete BIOS/HLE and SMPC/analog behavior and both required new
licensed demo discs. Passing these tests does not satisfy those missing criteria.
No human-only gate, proprietary file, new Issue or historical Issue reopening is
needed to continue implementation. See the [conformance matrix](hardware-conformance.md).

## Earlier dual-SH-2/FRT validation (historical)

Read live [Issue #17](https://github.com/takahirox/saturn-rust/issues/17) through
the GitHub connector. `git ls-remote origin refs/heads/main` returned
`dbd981b030a46daa6923072ce240680dd2e58ebf` on 2026-10-21, the preserved PR #16
checkpoint. The assigned worktree started at
`850d3c1f9112d056281cfd967e3730320be4e1c8`; no technical diagnosis was supplied.
No push, Issue change/comment, PR creation or merge is part of this node.

## Implemented scope and primary references

- `Machine` owns an independent slave `Cpu`, initially held in reset. SSHON
  starts its common power-on vectors and SSHOFF resets the slave CPU/FRT,
  preserving shared RAM, master CPU and backup. Sources: SMPC User's Manual
  ST-169 table 1.1 and §2.3 pp.23/24; Dual CPU User's Guide ST-202 §1.1;
  SH7604 Hardware Manual §4.2.2 and table 4.3.
- Both CPUs execute the original interpreter against shared external memory and
  independent FRT/INTC state through `Bus::for_cpu`. CPU-specific errors stop
  the entire machine until reset; a failed slave exception entry is observable
  in the browser and cannot be bypassed by calling Run again.
- Original FRT implementation covers FRC/OCRA/OCRB/ICR, TIER/FTCSR/TCR/TOCR,
  shared TEMP, internal divisors, compare/counter-clear/overflow flags, computed
  output levels and observed-one/write-zero acknowledgment. IPRB FRT priority,
  VCRC/VCRD and level-sensitive IRQ dispatch are integrated. Sources: SH7604
  §§5.3.2/5.3.6/5.3.7 and §§11.1–11.5, tables 11.2/11.3.
- Word strobes at 21000000/21800000 signal slave/master FRT capture, as in
  ST-202 §1.2 and §§3/4. P0 aliases also use the shared external bus. No canned
  mailbox/result writes or CPU/game identification hooks exist in the host.
- SMPC byte SF/COMREG/OREG31 sequencing implements SSHON=02, SSHOFF=03 and
  NMIREQ=18 with a minimum 807-reference-clock delay. NMIREQ latches master NMI
  through instruction deferral and wakes SLEEP at SR.I=15. Source: ST-169 §1.2
  pp.5/6 and §2.3 pp.21/23/24/32 (30 µs minimum). Other commands fail explicitly.
- The browser keeps bounded execution available for a sleeping master with a
  possible hardware wake source, and reports slave faults as machine faults.
  Existing input release, file privacy and Star Collector behavior are preserved.

## Original licensed guest and independently expected results

[dual-sh2.s](../tests/fixtures/dual-sh2.s) is original guest assembly, covered by
[the adjacent MIT license](../tests/fixtures/LICENSE). The original project
assembler accepts a section base plus LDC/SLEEP/RTE; its existing Star Collector
output is unchanged. [build_dual_sh2.py](../tests/fixtures/build_dual_sh2.py) emits
[dual-sh2.hex](../tests/fixtures/dual-sh2.hex), checked for reproducibility in CI.
The expanded, zero-filled 524288-byte synthetic BIOS has SHA-256:

```text
d9eda155b46f6a8ce63c6533853075e0f7c5e057fe4483c06770d4becad7a616
```

Both CPUs begin at 00000200. Guest startup writes a shared flag, so the second
processor selects its own initialization without a host shortcut or BSC stub.
The master retains R14=42 and SP=06100000; the slave executes with R14=17 and
SP=060FE000. Guest instructions configure CPU-local FRTs, exchange an input of
35 and independently compute 35+7=42. Capture IRQs wake both CPUs and guest
handlers acknowledge their own flags and return with RTE. SSHOFF/SSHON causes
fresh slave execution while shared boot/IRQ counters accumulate. Finally the
slave is reset and a timed NMIREQ wakes the sleeping master at SR.I=15.

The golden mailbox at 06002000 contains fourteen longwords:

```text
00000001 00000002 00000023 0000002A 00000002 00001234 00000002
00005678 00000002 0000002A 0000600D 0000034E 000000F0 00000001
```

These are respectively startup flag, slave boots, input, result, master IRQs,
master captured count, slave IRQs, slave captured count, restart phase, retained
master marker, completion marker, NMI saved PC/SR and NMI count. The captures are
seeded independently at 1234/5678 and use external-clock selection, avoiding a
claim that fixed interpreter turns measure actual SH-2 instruction time.
Native tests and Chromium/Wasm compare these same expected values and check
re-execution after cold reset. A separate negative test corrupts only the slave
stack literal; exception vector 66 entry must fail on the slave, remain sticky
and disable browser execution. No host writes guest state after boot.

## Checks run

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  83 native tests after the timer review correction below, none ignored.
  Existing address-error/system fixtures, backup,
  both pads, parser, pixels and Star Collector completion/restart remain covered.
- `cargo clippy --locked --all-targets -- -D warnings`: pass.
- `python3 homebrew/star-collector/build.py --check`: pass, packaged ISO unchanged.
- `python3 tests/fixtures/build_dual_sh2.py --check`: pass, hash above reproduced.
- `sh scripts/build-web.sh`: pass with matching wasm-bindgen-cli 0.2.129.
- `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`: pass.
- `npm ci --ignore-scripts`, `npx playwright install chromium`, and
  `npm run test:browser`: pass in Chromium 145.0.7632.6 under `/saturn-rust/`,
  including the dual-CPU guest, visible/sticky slave fault, existing guest traces,
  all prior UI/gameplay/backup/input tests and local-file privacy checks.
- `node --check scripts/browser-smoke.mjs` and `git diff --check`: pass.

Native validation used Homebrew Rust 1.94.0. Initial Wasm attempts selected the
Homebrew compiler without a Wasm standard library, despite Rustup having the
target installed. Selecting the existing Rustup stable toolchain directory on
PATH fixed this environment mismatch: Rust 1.99.0, wasm32-unknown-unknown, no
source workaround or dependency change. No deployment/live Pages, personal
firmware, physical device, external workflow investigation or human sign-off was
needed or performed. Genuine Sega BIOS/retail compatibility is **not independently
confirmed**.

## Timer compare review correction

The review found that FRT compared the incremented count, making OCRA=3/CCLRA=1
clear one count edge early and OCRA=0 wait until wraparound. Compare flags,
output levels and counter clear now use the value held before the count edge,
per SH7604 §§11.4.2/11.4.3/11.4.6, figs. 11.6/11.7/11.11. OVF still denotes
FFFF→0000 (§§11.2.5/11.4.7, fig. 11.12), including simultaneous FFFF compares;
clearing from other values does not assert overflow or compare against zero.

Independent unit vectors cover the four-edge OCRA=3 period, compare=0 on the
first and repeated edges, simultaneous A/B matches and overflow boundaries.
The source-authored MIT-licensed [timer guest](../tests/fixtures/frt-compare.hex)
sets all timer/INTC registers through SH-2 instructions. Native and browser Wasm
tests check compare=0/3 IRQ timing, SLEEP wake, the stacked PC/SR, guest flag
acknowledgment and RTE. These verify the existing functional clock quantum,
not physical instruction timing. Both new unit tests and the guest test fail
when only the previous `increment` implementation is restored; restoring the
fix passes all 83 native tests.

After this correction, formatting, locked build/test, native and Wasm Clippy,
web build, both existing guest-media reproducibility checks, JavaScript syntax,
and the extended Chromium 145.0.7632.6 browser suite all pass. The existing
dual-CPU fixture and Star Collector ISO remain unchanged. No umbrella criterion
is completed by this correction.

## Remaining scope and continuation

The earlier CPU-control/FRT revision used the eight-reference-clock
interleave and legacy visible/blank turn budgets. They still do **not** satisfy hardware-time
scheduling, contention, scanline timing or cycle accuracy. FRT pin synchronization,
access waits, clock-switch edges, external clock wiring and output-pin routing
remain missing. The review-fix section above supersedes the earlier missing-cache,
manual-reset and non-FRT peripheral statements. BCR1 identity, BSC/UBC/standby
and complete hardware timing remain missing. SMPC still lacks all other
commands, RTC, INTBACK, system/clock/sound controls and analog input.

All broad CPU, bus/scheduler/backup, SCU/DSP, VDP1/advanced VDP2, MC68EC000/SCSP/
DSP/CD-DA/WebAudio, runtime CD/BIN/CUE, full BIOS/HLE services and two new guest
demo criteria remain open. This diagnostic ROM is neither required new demo and
does not substitute for them. The [conformance matrix](hardware-conformance.md)
records implementation/test links and deviations. Continue under #17 without
closing keywords, reopening old trackers or imposing human-only gates.
