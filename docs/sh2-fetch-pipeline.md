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
| PR memory transfers | LDS.L @Rm+,PR uses ordinary load EX/MA/WB; STS.L PR,@-Rn uses ordinary store EX/MA (Figures 7.86–7.87, printed p.244). Apply ordinary IF/MA contention and waited-slot overlap, retaining the implicit-PR WB dependency. These forms do not use the LDC.L/STC.L ID stalls or the MACH/MACL resource interlock. |
| Byte-memory logic / TAS | Hold successor ID and younger IF for two / three slots respectively (Figures 7.75–7.76). Logic resumes IF beside second EX; TAS resumes beside final MA, with MA first for real IF. |
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

LDS.L PR and STS.L PR enter the same operand scheduler through
`timing::mov_load_address` / `mov_store_address` (Figures 7.86–7.87).
The [PR transfer vectors](../src/bus/fetch_pipeline_tests.rs) assert literal
grants and completion sampling on both CPUs and alignments, with zero/six Tw,
chunks 1/3/17/10000 and passive inspection. Resident entry has IF/ID at
0..1/1..2, EX at 2, and operand grant at 3; external load completion is 5/11.
Changing the operand one state before completion must update PR, while changing
it after completion must not. The successor STS PR,R8 exercises implicit PR
writeback dependency handling. Stores publish once and retain predecrement.

For P1 entry, aligned EX overlaps IF at 3..5 (9..17 with six Tw),
putting operand grant at 5/17. Entry at 4n+2 instead overlaps ID with real IF
at 2..4 (8..16), then has EX at 4/16 and the same operand grant at 5/17.
Loads sample at 7/25; the next real IF grants at that edge. STS.L also obeys
SH7604 §7.9's minimum one-state read-to-write gap: aligned entry publishes at
8/26, while 4n+2's empty EX bus state satisfies the gap and gives 7/25.
The next IF waits for the write-buffer drain and grants at publication; a
preceding write introduces no BSC turnaround. These edges use the existing
bus phase conventions described above, rather than extra PR-specific stalls.

The existing TIMING guest starts its PR kernels' bodies at 4n+2. In eight
LDS.L/NOP/STS PR triples, six-byte spacing alternates the LDS.L alignment;
four real IF/MA splits add to 24 base states, giving 28. With the established
seven-state FRT wrapper, `floor((7+28)/8)=4` ticks replaces the earlier 3.
Sixteen LDS.L/STS PR pairs all load at 4n+2, so all sixteen require real IF
splits. Removing the PR dependency by replacing STS PR with MOV #0 retains
those splits: 32+16=48 states, `floor((7+48)/8)=6` ticks replaces 4.
Figure 7.9's WB dependency shares the IF split and adds no second delay.
`repeated_pr_loads_derive_guest_splits_and_implicit_dependency_overlap` checks
every retirement edge of the triples and both dependent/independent pairs,
with synchronous resident operands and no external grant to hide a state.
Guest bytes, computed data, operand/control mutations and cold resets remain
unchanged; only these two manual-derived expectations are reconciled.

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

### Redirected MOV contention and LDC.L successor stalls

Hitachi Figures 7.6/7.7 and 7.82 determine the delay-slot MOV's MA/IF
relationship from the **target stream**. For resident instructions, branch
EX starts at 2, target IF at 3, delay-slot MOV EX at 4, and synchronous MA
at 5..6. The IF beside MA is target+4: target 4n needs real IF and splits
the slot; target 4n+2 uses retained `if` and permits independent target EX
at 5. Thus target ADD followed by SLEEP reaches SLEEP at 7 or 6 respectively,
for either branch alignment. A dependent ADD splits once for either target
alignment and reaches SLEEP at 7. The same IF rule applies to MOV stores.
The native `delay_slot_mov_uses_target_alignment_for_contention_and_dependency_on_both_cpus`
vector crosses both branch/target alignments, loads/stores and dependencies.

Hitachi Figure 7.84 (printed p.243) and Renesas Rev.5 Figure 7.84 (printed
p.491) stall successor ID for the initial EX and MA slots of LDC.L to SR,
GBR or VBR. Third-instruction IF advances beside the final EX, after actual
operand completion; it cannot advance during either stalled slot. The Rev.5
diagram labels the final stage WB while its prose calls it EX; both editions
agree on the two stalled successor slots. No SH-DSP-only transfer is included.

| Aligned P1 high-RAM LDC.L | Initial IF | ID | Initial EX | Operand MA | Final EX / PC+4 IF | Successor EX |
| --- | --- | --- | --- | --- | --- | --- |
| Zero software waits | 0..2 | 2..3 | 3..4 | 4..6 | 6..8 | 8 |
| Six software waits | 0..8 | 8..9 | 9..10 | 10..18 | 18..26 | 26 |

These are slot derivations, not recorded traces: external high-RAM transfers
take 2+Tw states; initial EX and ID each take one; the final EX shares the
real IF's stretched slot. A synchronous operand instead takes one MA state.
An operand's extra latency stretches MA once, and a real IF's extra latency
stretches the final EX once. Retained `if` or resident IF takes one state.
The native `ldc_long_stalls_younger_if_until_operand_completion_and_retains_samples_on_both_cpus`
vector asserts literal grants for both CPUs and all three LDC.L forms with
zero/six Tw. It changes operand and instruction bytes immediately before/after
completion, requiring the separately sampled values, one postincrement and
once-only retirement. Both vectors cover chunks 1/3/17/10000 and passive
inspection. The existing equal-edge host-mutation convention applies.
`ldc_long_stall_edges_cover_resident_retained_and_synchronous_routes_on_both_cpus`
also checks literal SLEEP edges with resident instructions, P1 entry at 4n+2,
cache-array operands and external operand waits, retaining the same two stalls.

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

### STC.L successor stalls and redirected multiplier contention

Hitachi Figure 7.85 (printed pp.243–244) and Renesas Rev.5 Figure 7.85
(printed p.491) stall successor ID during STC.L's initial EX. This applies to
SR/GBR/VBR; the SH-DSP-only registers are excluded. Third-instruction IF resumes
beside MA, with MA before a bus-using IF (Figures 7.6–7.7). A retained `if`
does not add a bus request. Actual operand completion, write-buffer read fencing
and IF waits stretch that slot; total-cycle adjustments cannot replace these
ordered requests.

For aligned P1 entry and a synchronous cache-array stack, the independent edges
are:

| Software waits | Initial IF | ID | Initial EX (younger IF held) | MA | PC+4 IF |
| --- | --- | --- | --- | --- | --- |
| Zero | 0..2 | 2..3 | 3..4 | 4..5 | 5..7 |
| Six | 0..8 | 8..9 | 9..10 | 10..11 | 11..19 |

For an external high-RAM stack, the existing buffered-store contract admits
internally in one state while starting external drain at MA start (4 or 10).
The drain takes 2+Tw states; the BSC write-to-read turnaround then precedes the
fenced IF, which grants at 7 or 19 and samples at 9 or 27. These are the existing
write-buffer phase conventions, not an additional silicon STC.L rule.
The `stc_long_holds_younger_if_through_ex_and_retains_completion_bytes_on_both_cpus`
and `stc_long_ma_if_contention_preserves_retained_and_buffered_routes_on_both_cpus`
[native vectors](../src/bus/fetch_pipeline_tests.rs) check these literal grants,
both entry alignments, resident/retained paths, SR/GBR/VBR, completion-byte
mutations and once-only predecrement/store effects.

Figures 7.6–7.7, 7.63 and 7.82 also require multiplier MA/IF pairing to follow
the redirected successor stream. After resident JMP EX at 2, a zero-wait P1
target IF occupies 3..5 and delay-slot MUL.L/DMUL EX starts at 5. For target
4n+2, the first MA is 6..7 and target+2 needs real IF at 7..9. For target 4n,
that halfword is retained beside first MA; target ADD EX at 8 requests target+4
at 8..10. Six software waits stretch the initial target IF to 3..11, moving
those later IF edges to 13..21 or 14..22. Moving JMP between 4n and 4n+2
cannot change either target stream's schedule. Both the multiplier scoreboard's
split and its deferred IF boundary use this successor alignment.

The `delay_slot_multiplier_uses_target_alignment_for_grants_and_samples_on_both_cpus`
[native vector](../src/bus/fetch_pipeline_tests.rs) crosses both branch and target
alignments for MUL.L/DMULU.L/DMULS.L. It checks literal grants and bytes changed
before/after completion. All three new vectors exercise both CPUs and advancement
chunks 1/3/17/10000; the sampling vectors also compare passive inspection on/off.

### Byte-memory logic and TAS successor stalls

Hitachi Figures 7.75–7.76 (printed p.238), cross-checked against Renesas
Rev.5 Figures 7.75–7.76 (printed pp.484–485), require the following slots.
The common rules apply to SH7604; there is no applicable SH-DSP correction.

| Instruction | Initial EX | Read MA | Second EX | Final MA | Successor ID / third-instruction IF |
| --- | --- | --- | --- | --- | --- |
| AND.B / OR.B / XOR.B / TST.B | Hold | Hold | Resume | Write for modifying forms | Resume beside second EX, after two stalled slots |
| TAS.B | Hold | Hold | Hold | Locked write | Resume beside final MA, after three stalled slots; MA precedes bus-using IF |

Each MA and EX slot occupies at least one state, with actual operand/IF waits
stretching that slot once (Figure 7.4). TST.B preserves memory and consumes the
same two ID stalls. Byte logic's final write can share the successor's EX;
retain that overlap using the existing completed-MA convention. TAS retains its
existing locked external read/write and address checks. A retained `if` shares
TAS's final MA without another external request; a real IF follows the write.
The emulator publishes completion before equal-edge replay as elsewhere above.

For isolated aligned cold P1 entry, zero waits give initial IF 0..2, ID 2..3,
EX 3..4, read MA 4..6 and second EX starting at 6. Byte logic grants PC+4 IF
at 6 and samples at 8. TAS instead spends second EX 6..7, locked write MA
7..9, then grants PC+4 IF at 9 and samples at 11. With six software waits,
initial IF is 0..8, ID 8..9, EX 9..10, read MA 10..18. Byte logic's IF is
18..26; TAS has second EX 18..19, write MA 19..27 and IF 27..35.
These are stage-derived deadlines, independent of production traces.

The [native vectors](../src/bus/fetch_pipeline_tests.rs)
`byte_memory_stalls_order_operand_and_younger_if_samples_on_both_cpus` and
`byte_memory_resident_and_retained_slots_preserve_decode_stalls_on_both_cpus`
cover all five instructions, both CPUs, zero/six waits, chunks 1/3/17/10000 and
passive inspection on/off. The former executes the reported `LDC R15,GBR; NOP`
setup, then the byte operation, NOP, MOV and SLEEP. Its operand grants/samples
are 7/9 (six waits: 19/27); younger IF grants/samples are 9/11 for logic and
12/14 for TAS (six waits: 27/35 and 36/44). Writes immediately before/after
completion require the actual separately retained operand and instruction bytes.
AND/OR/XOR/TAS readback, TST/TAS flags, retirement count and SLEEP check once-only
effects. The inherited implementation instead granted younger IF at 6 before
operand MA; the unchanged new oracle failed there.

Resident instruction vectors use both entry alignments, synchronous cache-array
operands for logic and external P1 operands for all five instructions. Completed
host cache warming releases its synthetic external-bus history before execution.
With the same setup, byte EX starts at 4 and read MA starts at 5. Synchronous
logic has second EX 6..7 and final MA 7..8 beside successor EX; MOV EX starts at
8 and SLEEP at 9. External logic reads 5..7 (six waits: 5..13), moving SLEEP
to 10 (16). TAS reads 5..7, executes 7..8 and writes 8..10 (six waits:
5..13, 13..14 and 14..22). At aligned entry, real IF then takes one cache
slot 10..11 (22..23), followed by NOP, MOV and SLEEP at 11/12/13
(23/24/25). At entry 4n+2, retained `if` shares final MA, so these three
instructions start one state earlier. Actual external grants assert operand
ownership and once-only writes; no latency-only fetch can satisfy the P1 vectors.

The cached BSC turnaround guest's old 11/11/13 FRT expectation used
`max(4, four external states) + max(1,IW3)` per TAS and omitted the distinct EX
slots and MA/IF split. Figure 7.76 instead gives EX(1), read MA(2), EX(1),
write MA(2), then a cache IF(1) for alternating aligned instructions; retained
`if` shares final MA for the other half. Sixteen TAS span eight seven-state
and eight six-state iterations, or 104 states. SH7604 Figure 7.47's read/write
turnaround uses the empty second-EX state: IW3=0/1 adds no state, IW3=2 adds
one state per pair, yielding 104/104/120 states. The unchanged FRT setup/read
cost adds eight states: 112/112/128, hence **14/14/16 ticks** at divide-by-eight.
Changing IW3=2 to 1 yields 14. The independent
`repeated_tas_grants_derive_the_cached_bsc_guest_period_on_both_cpus` vector
checks every read/write grant and the 104/104/120-state period, both entry
alignments and CPUs, and all four chunk sizes. It does not read the guest golden
or derive expectations from production traces.

Only [the authored TSV](../tests/fixtures/bsc-idle.tsv) and its
[builder's derivation](../tests/fixtures/build_bsc_idle.py), plus the corresponding
native/browser IW mutation literals, change. Guest bytes remain unchanged
(`bsc-idle.hex` SHA-256 `520d1c5c7ecbf193e2026b84b9a02ebde9bae333008a5cb5b02cdd29581abce7`).
The corrected TSV SHA-256 is
`5bc1b9f5cd635b58b76248a522a57029bf6e8c6455af033a3ec4e4560f505b49`.
Readback, data mutation, both-CPU, repeat-boot and substantive assertions remain.
