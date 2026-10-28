# SH-2 ordinary MOV store pipeline

Related to #63 and #17. This contract was authored before production scheduling
changes. Expectations below are derived from the manuals, not emulator traces.

Primary references:

- Hitachi [SH-1/SH-2 Programming Manual](https://antime.kapsi.fi/sega/files/h12p0.pdf),
  September 3, 1996, §§7.2–7.5, figs7.4–7.9, and §7.7.1, fig7.12,
  printed pp.185–186: ordinary memory stores have IF/ID/EX/MA and no WB.
- Renesas [SH-1/SH-2/SH-DSP Software Manual Rev.5.00](https://www.renesas.com/en/document/mah/sh-1sh-2sh-dsp-software-manual),
  June 30, 2004, §§7.1–7.2 and §7.4.1, fig7.21, printed p.438,
  confirms these common SH-2 rules. DSP-only rules are excluded.
- Hitachi [SH7604 Hardware Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf),
  §§7.3.1 (pp.148–149), 7.11.1–2 (pp.210–211): fetches are longwords;
  CPU notification follows internal-bus acquisition, independently of external
  write completion. The BSC has one write-buffer entry. A subsequent external
  read waits for the preceding write; an internal peripheral access can overlap.

## Stage and deadline table

Set store EX start E=32. Address and source are prepared in EX, MA starts at
33. L is the CPU-facing MA duration, including contention/backpressure, not the
external device's write duration. On-chip memory/cache IFs fetch two words.
An aligned store pairs MA with retained `if`; a word-offset store pairs it with
real IF, splitting the slot with MA first (§7.4). Stores introduce no WB hazard.

| Store alignment | L | MA request | Acceptance/local completion | Successor EX | Third EX after NOP |
| --- | --- | --- | --- | --- | --- |
| 4n | 1 | 33 | 34 | 33 | 34 |
| 4n+2 | 1 | 33 | 34 | 34 | 35 |
| 4n | 2 | 33 | 35 | 33, stretched to 35 | 35 |
| 4n+2 | 2 | 33 | 35 | 35 | 36 |
| 4n | 4 | 33 | 37 | 33, stretched to 37 | 37 |
| 4n+2 | 4 | 33 | 37 | 37 | 38 |

| Destination, otherwise idle | Write request | Local mutation/acceptance | External publication |
| --- | --- | --- | --- |
| Cache RAM, data/address arrays, purge | MA boundary 33; synchronous effect at end | 34 | None |
| Write-through RAM | 33 | Cache update and buffer acceptance 34 | 33+D |
| Cache-through RAM | 33 | Buffer acceptance 34 | 33+D |
| On-chip MMIO, one-clock write | 33 | 34 | Device mutation 34 |
| B-bus video longword, default waits | 33 | Buffer acceptance 34 | High word 41, low word 49 |
| Occupied write buffer | 33 | After prior entry releases, one acceptance clock | Per actual grants/waits |

D is the programmed BSC/device cycle duration (ordinary 32-bit RAM with no Tw:
2 clocks; six Tw: 8 clocks). Video longwords retain their two independent
word publications; CPU MA does not replace those device deadlines. External
bus ownership and acceptance can overlap; acceptance is not a publication fence.

A preceding load at EX=32 has MA at 33..33+L. If it supplies the store's
address, R0 index, or data, store EX follows load completion at 33+L; a real
IF split already supplies this dependency split. An independent store shares
the load's MA slot, with its own MA in the next slot. For one-clock operands:

| Preceding load alignment | Store dependency | Store EX | Store MA | Store completion |
| --- | --- | --- | --- | --- |
| 4n | Independent | 33 | 34 | 35 |
| 4n | Address/data/index dependency | 34 | 35 | 36 |
| 4n+2 | Either | 34 | 35 | 36 |

Predecrement uses the old source even when source and address register alias;
only the address register changes, once. Invalid operand cycles still complete
the instruction's update and latch address error for dispatch at the next
eligible boundary. Delay slots finish the store before the branch target is
published architecturally. IRQ arrival cannot replace a partially replayed store.

## Emulator phase conventions

The instruction interpreter retains its pre-instruction CPU snapshot during
replay, so address/data preparation is stable without a second architectural
register update. Scheduled writes request at MA start; synchronous cache-array
writes mutate at MA end. Classification, clocks and waits are journaled.
Architectural retirement waits for acceptance, not external publication. Aligned
successors recover the final shared MA/EX clock; stretched EX is represented by
that slot's final clock, as in the existing load/multiplier model. Hardware
events precede replay at equal edges. These are deterministic emulator phase
choices, not additional physical timing requirements.

Manual reset drains accepted/in-flight external operands in longword units;
power/system reset cancels remaining phases. Reset before admission cannot
create a write. Precise new IF overlap evidence uses warmed instruction cache;
full speculative IF/ID and unrelated pipelines remain #17 work. This increment
does not establish hardware completion or BIOS/commercial compatibility.

## Guest oracle

The existing stopped/resumed FRT wrapper measures `floor((8+N)/8)`; the resume
store is at 4n+2 and completes before the first timer load. Sixteen aligned
store/NOP pairs take N=32. A leading NOP and sixteen offset pairs take N=49.
Thirty-two consecutive stores take N=48. A one-clock local load followed by a
dependent word-offset store takes four clocks per pair (N=64); an independent
store takes three (N=48). In each pair the store's own MA/IF split adds a clock
to the preceding load's dependency schedule. Peripheral writes have one-clock
CPU-facing MA, independently of
external buffering. Replacing stores with NOPs removes MA/IF splits while
preserving the instruction count. Data/readback and peer handoff records must
be computed by the executing guest.

## Existing guest timing reconciliation

The historical `sh2-timing` peripheral-write body contains 32 alternating aligned
and word-offset stores: 32 base states plus 16 IF splits gives N=48, so its
FRT record becomes 7. All peripheral readback, arithmetic and mutation checks
remain. The PR-preserving wrapper saves PR with an extra register instruction,
moving the resume store from 4n+2 to 4n. At resume completion clock zero the
following timer load can now share the store's final MA clock (representative
EX=-1). Its high-byte read completes at 2, low-byte read at 5, and body EX at 4;
the final timer latch adds 3. Thus these two wrappers use `floor((7+N)/8)`:
PR-use N=48 gives 6, separated N=24 gives 3, and the dependency-removed N=32
control gives 4. This is overlap of the resume store, not changed PR semantics.

The rotation-fetch guest retains its H=844 counter record, first old dot,
following new dots, all 320×224 pixels, readback/arithmetic and early/omitted-write
controls. Its hot poll/store block remains within one warmed cache line.
The store now needs its EX-to-MA clock. For tile/RBG1, replacing the previous
two-state BRA-to-successor plus slot store by NOP plus store compensates one
clock. Bitmap removes that intervening instruction altogether: the original
mode-dependent setup/delay places its counter sample in the later half of
HCNT's four-clock quantization interval, requiring one earlier store EX.
Normal-mode display acquisition is every four main clocks and precedes
equal-edge publication under the established [rotation convention](vdp2-rotation-fetch.md).
No pixel expectation or computation was relaxed. Fixture source, bytes and
hashes are reproduced by the existing builder.
