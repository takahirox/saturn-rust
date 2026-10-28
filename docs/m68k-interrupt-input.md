# Retained MC68EC000 interrupt input

Related to #71 and #17. This increment covers input propagation and replay/reset
lifetime; it does not complete CPU timing or the hardware umbrella.

## Primary contract and reconciliation

[Motorola MC68000UM](https://www.nxp.com/docs/en/reference-manual/MC68000UM.pdf),
§3.5 (printed p.3-6) and §6.3.2 (pp.6-12–6-13), requires requests to remain
asserted until acknowledgement. Input requests become pending during execution;
exception entry occurs between instructions. A lower-to-seven transition qualifies
even with I=7. Seven held continuously can also qualify when software lowers I.
The saved PC identifies the next instruction; interrupt entry saves the old SR
and raises I to the acknowledged level. Our mandatory vectors hold each new
request through its IACK; they do not require servicing a withdrawn request.

[Sega SCSP manual §4.2.12](https://www.infochunk.com/saturn/segahtml_en/hard/scsp/hon/p04_2c.htm),
figs.4.55–4.58/table4.31, corresponds to
[ST-077-R2-052594](https://antime.kapsi.fi/sega/files/ST-077-R2-052594.pdf),
§4.2, printed pp.95–100, figs.4.62–4.65/table4.38 (PDF pp.107–112).
SCIPD/MCIPD bit5 is independently software-settable; SCIRE/MCIRE independently
clear their own route. SCIEB gates sound requests, and SCILV0/1/2 program the
three level bits. IACK does not clear SCIPD. Timer B/C, MIDI output and sample
requests share level selector bit7.

The later Sega text clarifies the conflicting R2 English SCIEB/MCIEB prose:
zero **disables** an enable, rather than being an ineffective write. Figure 4.56
labels MCIEB and MCIRE write-only, despite the prose's `(R)`
labels. This increment uses writes for main-route control. The existing model's
MCIEB readback is not established by those primary diagrams and remains a
separate register-conformance deviation. R2 p.100's timer example repeats
SCILV0 for the 100426/100428 addresses; fig.4.65 and the later fig.4.58 identify
SCILV1/2. The pending/enable/clear/level writes used here require no production
correction in this increment. The seven-level route is supported; Sega's reservation of level7
for development tools (§1.2, interrupt discussion) is a software-use convention,
not a different MC68EC000 input contract. No task acceptance criterion is relaxed.

## Scheduler convention and lifetime

[`Sound::advance_domain`](../src/sound.rs) samples SCSP output on entry, immediately
after serial advancement, after each device/CPU event and after residual MIDI
advancement. Thus main-CPU MMIO,
sound-CPU MMIO (including MIDI reads), timers, internal DMA and serial events
reach retained input while internal computation or memory work is pending.
This keeps the existing event ordering and oscillator phase documented in
[CPU sound-memory allocation](scsp-cpu-memory.md).

[`M68k::begin_replay/finish_replay`](../src/m68k.rs) and the
[transaction scheduler](../src/sound/transactions.rs) separate two lifetimes:

- Boundary input is frozen with the instruction/exception snapshot. Its edge
  latch moves out of live input and is consumed by successful IACK replay once.
- New input updates the live level and a single saturating level7 latch. On
  completion, live input replaces the replayed level and is ORed with any
  unconsumed snapshot latch. It cannot change an in-flight replay's bus sequence.

There is no interrupt FIFO and no new transaction journal. Completed reads,
writes and IACKs retain the existing journal results. SNDOFF cancels both the
snapshot and live input; SNDON begins a new reset sequence and samples the
then-current output. Cold reset replaces both domains. CPU RESET output retains
its existing external-device lifetime.

The model has no electrical IPL synchronizer delay. Sampling follows emulator
events; equal-edge ordering is a convention, not a measured input aperture.
The guest's low interval is 82 reference clocks (about 34 sound-CPU clocks),
with both transitions inside one DIVU; native vectors use an 80-reference-clock
low interval. No oracle depends on a narrow pulse or a withdrawn request.

## Independent oracles

The [fixture generator](../tests/fixtures/build_m68k_irq.py) records the expected
order before production changes: first request/IACK, first handler at I=7,
clear/low/reassert inside DIVU, completed quotient/remainder, then second IACK
and nested handler. Its original MIT content uses the adjacent
[license](../tests/fixtures/LICENSE); source, [SH-2 listing](../tests/fixtures/m68k-irq.s)
and [bytes](../tests/fixtures/m68k-irq.hex) are reproducible with
`python3 tests/fixtures/build_m68k_irq.py --check`.
Expanded 512 KiB ROM SHA-256:
`acd9a4d91d683f7b031df7165e1bbd7359e119d98436e7c758ca35e90c438754`.
The generator contains the original 68k word program with opcode/mnemonic
comments; no licensed firmware, SDK samples or commercial media are included.

The 68k initializes D0=7, D1=2 and its record pointer. Every handler computes
its count and publishes stacked SR (word), PC (long), D0 (long), count (long):

| Entry | SR | PC | D0 | Count |
| --- | --- | --- | --- | --- |
| First | 2700 | 0000021E | 00000007 | 1 |
| Second | 2700 | 00000322 | 00010003 | 2 |

DIVU 9/2 independently changes D0 to 00010004 and the first record's D0 to 9.
Omitting either the intervening clear or the second assertion gives exactly one
entry, an empty second record and the computed result at 944. With rearming, the
second interrupt precedes the store at322, so 944 remains zero. The SH-2 generates
all transitions through production MMIO. The host only observes guest records.

[`tests/m68k_irq.rs`](../tests/m68k_irq.rs) observes completed SCIPD transitions
at unchanged CPU PC320, instruction count14, D0=7, exception count1. It also
compares all guest controls across chunks 1/3/17/10000, inspection on/off and two
cold boots. [`src/sound/irq_tests.rs`](../src/sound/irq_tests.rs) independently
checks completion before IACK, separate IACK/stack cycles, saved PC/SR, held7,
software-lowered I, pending/enable/level rearming, timer output, contended stack
work, exactly-once MIDI reads, reset cancellation and sound/main independence.
A separate serial-completion/equal-CPU-deadline vector checks the emulator
ordering convention; it is not an electrical timing requirement.
[`src/reset_tests.rs`](../src/reset_tests.rs) adds real delayed SMPC
SNDOFF/SNDON cancellation of a rearmed input during DIVU.

[`scripts/m68k-irq.mjs`](../scripts/m68k-irq.mjs) runs the identical ROM and
controls in production Wasm through the required `m68kReset` group, mounted at
`/saturn-rust/`. Full results and inherited-behavior controls are in the
[validation record](validation-issue-71.md).

Full prefetch microsequences, data-dependent DIV timing, electrical input
synchronization, complete bus arbitration, synthesis and the other #17 hardware
requirements remain pending. Genuine BIOS execution and broad commercial-game
compatibility remain unverified.
