# SH-2 instruction fetch and decode

Related to #69 and #17. This stage contract precedes production changes.
Expectations are authored from the references below, independently of traces.

Primary references:

- [Hitachi Programming Manual](https://antime.kapsi.fi/sega/files/h12p0.pdf),
  §§7.1–7.5, printed pp.168–176, Figures 7.1, 7.4, 7.6–7.9:
  distinct IF/ID/EX stages, longest-stage slot stretching, MA before bus-using
  IF, and one-state retained-halfword `if`. Figure 7.7 distinguishes entry
  at 4n from entry at 4n+2. Figures 7.78–7.82 and 7.92–7.97 govern redirects;
  Figures 7.92 and 7.94 are on printed pp.247–248.
- [Renesas Software Manual Rev.5.00](https://www.renesas.com/en/document/mah/sh-1sh-2sh-dsp-software-manual),
  §§7.1–7.4, printed pp.409 onward, Figures 7.1–7.9: the common/SH-2 rules
  retain these constraints. Section 7.2.2, Figure 7.14 permits forwarding
  from load WB to store data MA; it does not remove address dependencies.
  SH-DSP stages and its fourth contention class do not apply to SH7604.
- [SH7604 Hardware Manual](https://antime.kapsi.fi/sega/files/sh7604.pdf),
  §§4.3–4.7, 6.3.2, 7.3.1, 7.11.2 and 8: exception eligibility, UBC,
  aligned external longword fetches, preceding-write fencing and cache policy.

## Stage and cancellation table

| Situation | Required behavior |
| --- | --- |
| Cold entry | IF, then ID, then EX occupy separate slots. ID costs one state. |
| Steady register stream | ID of the successor and IF of the second successor overlap current EX (Figure 7.1). |
| Aligned fetch | Retain sampled companion word; its later `if` occupies a state without a bus request (Figure 7.7). |
| Entry at 4n+2 | External fetch remains aligned longword; discard preceding word. Next IF needs another longword (SH7604 §7.3.1). |
| Cache hit | Sample cache through the CPU cache path; a real IF still occupies a slot. |
| Cold miss/line crossing | Finish the existing ordered refill with actual grants. Younger stages cannot escape its stretched slot. |
| Cache disabled/P1 | Fetch through BSC, including CS0's fixed word bus and programmed waits. No backing-memory shortcut. |
| IF versus MA | MA wins; bus-using IF follows it. A retained `if` shares MA. Do not add the same dependency/split twice (Figures 7.6/7.8/7.9). |
| Waited slot | IF/ID/EX availability advances at the longest participating stage's completion (Figure 7.4). |
| Untaken conditional | Keep sequential IF/ID. |
| Taken conditional | Discard both following instructions, including a sequential-address target (Figure 7.78). |
| Delayed conditional branch | Preserve the slot and fetch/discard its successor; target fetch follows branch EX when taken (Figure 7.80). |
| Unconditional branch/call/return | Preserve the slot with its ID stalled during branch EX; no second-successor IF. Target fetch follows branch EX (Figure 7.82). |
| RTE | Preserve its slot with restored SR; no second-successor IF. Target IF follows completion of both MA stages, including waits; the slot's ID stalls for three slots (Figure 7.92). |
| TRAPA | Fetch/discard both following instructions, including second-successor IF during EX, before clearing the stream for exception entry (Renesas Rev.5 Figure 7.93, printed p.497). Use the same side-effect-free lookahead policy as other discarded IF. |
| Exception/IRQ/NMI/UBC | Keep existing acceptance/deferral boundaries. Clear younger decoded bytes; fetch observations are not execution breaks (§6.3.2). |
| SLEEP/wake | Fetch only SLEEP's immediate successor, never its second successor during SLEEP EX (Figure 7.94). Discard the sleeping stream before handler entry. |
| Reset/SSH reset/PLL transition | Use the established reset domains: reset/SSH reset cancels the affected CPU; PLL retains the master stream and resets the slave. Manual reset retains the accepted physical longword drain, not its instruction. |
| Discarded fault | Never dispatch an architectural exception for discarded bytes. Invalid fetch addresses are checked only for the instruction selected to execute. |

## Deterministic implementation conventions

The interpreter publishes architectural state at a completed boundary. A stretched
EX is represented by the slot's last clock, matching the existing MOV/multiplier
convention. Real fetch bytes must be journaled during the slot even when the
older instruction has not retired. Cache hits may be sampled at slot start;
external reads sample only at their granted completion. Hardware completion
precedes interpreter replay at equal edges. These are emulator phase conventions,
not additional silicon requirements.

Lookahead must be bounded. Ordinary ROM/RAM/cache routes may supply overrun bytes;
speculative MMIO reads are not authorized by these diagrams. An unavailable
lookahead is a deferred instruction fetch, not permission to read a device or
raise a discarded exception. Software coherency still requires the documented
cache maintenance and control-flow synchronization.

## Initial scheduling oracle

With no other owner, P1 high RAM, a 32-bit bus and no software waits, each real
IF takes two states. Entry at 4n has IF at 0..2 and ID plus retained `if` at
2..3. Current EX overlaps the IF of PC+4 at 3..5; its representative EX clock
is 4. At clock 4 no instruction has retired and PC+4 owns the actual external
fetch. At clock 5 the first instruction can publish; the companion successor
needs no new external read. With six Tw, replace each two-state real IF by eight
states. The first PC+4 request is then at 9 and completes at 17. Entry at 4n+2
requires a second real IF during ID instead of the retained state.

This Issue remains an increment toward #17. BIOS and commercial-game
compatibility are unverified.

## Implementation and evidence

`cpu::fetch::Fetch` retains a decoded entry, a fetched successor and an ID
readiness edge per CPU. The existing CPU replay journal retains pending physical
beats and completed bytes while an older instruction is unpublished. No new
backing-memory reader or latency-only fetch route exists. `Fetch::advance` issues
IF two instructions ahead; actual completion stretches the EX/ID/IF slot.
Ordinary MOV MA precedes a contending real IF; retained `if` recovers the existing
MA/EX overlap. Long multiply and MAC defer advancement through their documented
successor ID stall. The existing multiplier and dependency scoreboards remain
responsible for their own resource interlocks.

`Cpu::execution_clock` reports the representative EX clock to Machine instead
of applying execution states to the earlier demand-fetch boundary. This avoids
charging elapsed fetch time twice. Architectural-only Memory adapters retain
synchronous stepping. Tests which deliberately start at a resident EX boundary
explicitly prime IF/ID and warm the complete instruction stream, including the
two-instruction lookahead. Cold-entry tests never use that precondition.

The native [stage vectors](../src/bus/fetch_pipeline_tests.rs) assert literal
grants, retirement edges, ordered refill beats, retained sampled bytes, CS0's
word phases, waited slots, MA/dependency interactions, redirects, NMI deferral,
watchdog reset, PLL retention, and competing CPU/DMAC/SCU/refresh ownership.
Advancement chunks 1/3/17/10000 and passive inspection produce identical traces.
Existing address-error, UBC, load, store, multiplier and reset-domain suites
remain required. A PLL change preserves the master's journal and frontend;
it resets the slave domain. Clearing the master's frontend alone would make
replay inconsistent with its retained transactions.

The original MIT [guest builder](../tests/fixtures/build_sh2_fetch.py) installs
identical kernel bytes into RAM through guest word stores. Both processors run
14 register/MOV kernels and measure their own FRT. The master sleeps before the
slave's measured work, then receives the guest-computed shared-memory response
through NMI. The oracle `ticks` enumerates slots from the manuals, independently
of production helpers. Native [acceptance](../tests/sh2_fetch.rs) and
[production Wasm](../scripts/sh2-fetch.mjs) apply data, arithmetic, omitted-load
and programmed-wait controls and repeat cold boots.

| Kernel route | Aligned ADD | Offset ADD | Aligned MOV/NOP | Offset MOV/NOP |
| --- | --- | --- | --- | --- |
| Resident cache | 5 | 4 | 5 | 6 |
| P1 high RAM, no Tw | 7 | 7 | 7 | 9 |
| P1 high RAM, two Tw | 11 | — | — | 13 |
| P1 CS0, fixed word bus | 11 | — | — | 13 |
| Cold high-RAM instruction cache | 9 | — | — | 11 |

Values are FRT ticks, not per-instruction costs. The aligned wrapper contributes
eight clocks; the offset wrapper contributes seven. For an uncached real IF,
replace its one-state cache slot by its actual two-state high-RAM or four-state
CS0 transfer, plus programmed waits. Cold high-RAM lines cost four two-state
longwords. MA wins before real IF, while retained `if` shares MA. This fully
specifies the builder's arithmetic; production traces never supply its values.

The same unchanged oracle fails on inherited `ba886f6`/main `7902b06`: aligned P1
MOV/NOP gives 9 instead of 7 ticks; waited aligned ADD and CS0 aligned ADD give
12 instead of 11; cold aligned ADD gives 8 instead of 9 and cold offset MOV/NOP
10 instead of 11. Guest readback/arithmetic is still correct, isolating timing.

### RTE and SLEEP boundary regressions

Figure 7.92 places target IF after RTE's second MA, rather than immediately
after its EX as for an ordinary delayed branch. Each stack read uses the existing
MA scheduling/classification hook; synchronous reads occupy one slot and real
transactions stretch it. The resulting completion edge supplies `Fetch::redirect`'s
target IF boundary. With resident RTE IF at 0, ID at 1 and EX at 2, the literal
oracle is:

| Stack route | First MA | Second MA | P1 target IF grant/completion |
| --- | --- | --- | --- |
| Synchronous cache array | 3–4 | 4–5 | 5 / 7 |
| P1 high RAM, no Tw | 3–5 | 5–7 | 7 / 9 |
| P1 high RAM, six Tw | 3–11 | 11–19 | 19 / 27 |

The native `rte_target_if_follows_both_ma_slots_and_retains_completion_bytes_on_both_cpus`
vector changes the target immediate before and after completion. Both CPUs must
execute 43, preserving the delay slot, restored SR and updated stack pointer.
The inherited code granted the synchronous-stack target at 3 instead of 5.

Figure 7.94 stops IF at SLEEP's immediate successor. `Cpu::execute` therefore
skips second-successor advancement for SLEEP; `Fetch::select` still supplies its
immediate successor. The native `sleep_fetches_only_its_immediate_successor_with_waits_on_both_cpus`
vector requires one aligned fetch for entry at 4n, or two for entry at 4n+2,
with zero and six Tw. The inherited aligned case granted an extra PC+4 fetch
at 3. `sleep_irq_wake_discards_the_prefetched_successor_on_both_cpus` then uses
an actual FRT capture IRQ to wake each CPU and execute a handler without retiring
the old stream. All three vectors compare advancement chunks 1/3/17/10000 with
passive inspection and assert actual grants, rather than inferred cycle totals.

## Existing fixture reconciliation

These changes retain substantive guest computations, mutation controls and
pixel assertions:

- The system exception ROM retains its complete 19-instruction PC/register/
  exception trace and stored result 35. Its last MOV can remain in the write
  buffer when SLEEP publishes: SH7604 §7.11.2 separates CPU acceptance from
  external completion, and Figure 7.94 supplies no second-successor IF to fence
  that write. The native test now advances one sleeping hardware step, bounded
  by the high-RAM write's two states plus six default Tw, before asserting the
  stored result and unchanged sleeping CPU state. No guest bytes change.
- The uncached BSC eight-read window formerly summed an EX state after each of
  eight real IFs. Figure 7.4 overlaps those states: `72+8*Tw` becomes
  `64+8*Tw`, or `8+Tw` FRT ticks. Narrow-data beats still add `2+Tw` each.
- Figure 7.82 fetches a delayed branch target before its slot's MA. In the
  self-modifying P1 guest, the target's old immediate 7 is therefore retained
  before the slot stores immediate 42. The historical demand-fetch expected
  42 is replaced by 7. The non-branch retained-word and later refetch controls
  remain 7/42, and changing the stored immediate still changes the later fetch.
  Synchronous architectural-only fetch tests retain their original behavior.
- The DMA UBC notification in the uncached diagnostic falls at the following
  eligible instruction boundary, 0x133E rather than 0x133C: the concurrent IF
  slot is completed before the pending break is accepted. Instruction, delay
  slot and data break PCs, BRCR flags and all guest state assertions remain.
- The CD copy guest explicitly programs zero CPU software waits with a longword
  BSC write. This keeps its command/reservation sequence inside the independently
  timed 32-sector copy job. All COPY NG/capacity/concurrency assertions remain.
- The HLE raster guest preloads EXTEN/VCNT addresses in unused R13/R14 before
  synchronization. Removing two literal-load MA operations per handler keeps
  two HBlank entries inside the same blank with the new frontend. Its computed
  arithmetic, counter, mask, same-line and restart assertions remain.
- The rotation guest retains H=844, every original pixel, readback and mutation
  assertion. The aligned hot path is four poll instructions followed immediately
  by store EX/MA (with one additional bitmap NOP as explained below). It uses
  one terminal DT/untaken-BF pair (two clocks), with no prefix NOPs.
  Bitmap warms both IF lookahead lines before line-15
  synchronization; tile/RBG1 retain their entry phase. Taken DT/BF iterations
  cost four clocks, so removing the historical extra iteration(s) compensates
  the frontend transition without changing the measured/pixel oracle.
- The VDP1 effects guest enables instruction cache for its three-instruction
  completion poll. A resident poll bounds timer rounding around the independently
  required 128 Gouraud and 640 RMW clocks; the existing timer bounds and all
  pixels/IRQ/readbacks remain.

Unrelated EX microsequences, full bus/chip conformance and the rest of #17 remain
open. No BIOS or commercial-game compatibility is established by this increment.

### Unconditional branch and TRAPA regressions

Figures 7.82/7.92 hold the unconditional branch/RTE delay slot without a
second-successor IF during EX. `Fetch::hold_delay_slot` retains that slot;
conditional branches still use Figure 7.80's discarded IF. The native
`unconditional_branch_and_rte_hold_the_slot_without_sequential_if_on_both_cpus`
vector requires these literal target grants from cold P1 high RAM:

| Entry | Tw | BRA/BSR target grant | RTE target grant, synchronous stack |
| --- | --- | --- | --- |
| 4n | 0 | 4 | 6 |
| 4n+2 | 0 | 5 | 7 |
| 4n | 6 | 10 | 12 |
| 4n+2 | 6 | 17 | 19 |

Real IF takes 2+Tw states; successor ID/retained `if` takes one, or another
real IF at 4n+2. Branch EX takes one slot; RTE adds two synchronous MA slots.
These sums determine the table independently of production traces. The old
aligned zero-wait BRA instead requested PC+4 at 3 and its target at 5.

Renesas Rev.5 Figure 7.93 (printed p.497) requires TRAPA to fetch/discard both
successors. Its EX now uses ordinary `Fetch::advance` before exception entry
clears the stream. The existing EX stretch calculation counts the shared slot
once. `trapa_samples_and_discards_second_successor_before_exception_accesses_on_both_cpus`
requires PC+4 grants at 3/9 and completions at 5/17 for zero/six Tw. The physical
journal must contain the illegal opcode written immediately before completion;
neither it nor either successor may execute. Stack accesses follow the completed
IF exactly once. `trapa_discards_an_unavailable_overrun_without_fetching_or_raising_an_address_error`
checks a deferred overrun beyond CS3, retaining the guard that excludes MMIO.
Both CPUs, advancement chunks 1/3/17/10000 and passive inspection are covered.

Removing unsupported IF also exposed two guest phase assumptions. No timing
golden, result, mutation control or pixel assertion changes:

- Hardware Lab's single EFREG1 read could land between the sample clear and
  DSP instruction 27's constant store. Its guest now ORs 128 real reads of that
  constant. At a minimum five main clocks per iteration this spans 640 clocks,
  exceeding one 44.1-kHz period (26,874,100/44,100 < 610). Read spacing is shorter
  than the 100 remaining DSP instructions' stable window. OR of the constant
  and cleared zero retains the original result, including the zero-gain control.
- The bitmap hot path retains the H=844 sample, then executes CMP, untaken BF,
  one resident NOP, and store EX/MA. The NOP adds one state (Figure 7.1), shifting
  the eight-state physical write from 476523–476531 to 476524–476532. Line 16
  starts at 476532; acquisition precedes equal-edge publication, preserving the
  old first dot and new following dots. These observed edges validate the stage
  derivation; the unchanged authored pixel oracle remains the expectation.
