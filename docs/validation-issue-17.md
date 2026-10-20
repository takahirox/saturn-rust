# Issue #17 dual-SH-2/FRT checkpoint validation

**Result: partial implementation. Related to #17. No umbrella acceptance
criterion is complete; #17 must remain open.** Issues #11/#13/#15 are historical,
administratively closed trackers and must not be reopened.

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
  81 native tests, none ignored. Existing address-error/system fixtures, backup,
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

## Remaining scope and continuation

This is a functional CPU-control/FRT checkpoint. The eight-reference-clock
interleave and legacy visible/blank turn budgets do **not** satisfy hardware-time
scheduling, contention, scanline timing or cycle accuracy. FRT pin synchronization,
access waits, clock-switch edges, external clock wiring and output-pin routing
remain missing. BCR1 CPU identity, cache/coherence, manual reset, other INTC sources
and the other SH7604 peripherals remain missing. SMPC still lacks all other
commands, RTC, INTBACK, system/clock/sound controls and analog input.

All broad CPU, bus/scheduler/backup, SCU/DSP, VDP1/advanced VDP2, MC68EC000/SCSP/
DSP/CD-DA/WebAudio, runtime CD/BIN/CUE, full BIOS/HLE services and two new guest
demo criteria remain open. This diagnostic ROM is neither required new demo and
does not substitute for them. The [conformance matrix](hardware-conformance.md)
records implementation/test links and deviations. Continue under #17 without
closing keywords, reopening old trackers or imposing human-only gates.
