# SH-2 multiplier timing contract

Related to #31 and #17. Authored before implementation or emulator measurements.

Primary sources are Hitachi, [SH-1/SH-2 Programming Manual](https://antime.kapsi.fi/sega/files/h12p0.pdf),
September 1996, §§7.2–7.5, table7.2 and §7.7.2; and Renesas,
[SH-1/SH-2/SH-DSP Software Manual](https://www.renesas.com/en/document/mah/sh-1sh-2sh-dsp-software-manual),
Rev.5, §7.2.3, printed pp.420–421. Only SH-2 rules apply.

## Independent timing derivation

Count EX intervals, not full instruction latency (§7.3). A single MA ends two
clocks after EX starts; the second MA ends three clocks after EX starts.
Multiplier access can share the last operating clock of an older multiply:
the extended MA ends with that clock (figs7.56/7.66). It does not require another
MA clock after the old mm ends. The following literal table excludes IF/MA
splits and memory waits. Each entry is **extra clocks for 0/1/2/3 intervening
single-clock independent instructions**. Signed/unsigned variants share timing.

| Producer | MULS.W / MULU.W | MUL.L / DMULS.L / DMULU.L | MAC.W / MAC.L | CLRMAC, LDS Rm,MAC, LDS.L @Rm+,MAC, STS MAC,Rn, STS.L MAC,@-Rn |
| --- | --- | --- | --- | --- |
| MULS.W / MULU.W | 1/0/0/0 | 0/0/0/0 | 0/0/0/0 | 1/0/0/0 |
| MUL.L / DMULS.L / DMULU.L | 3/2/1/0 | 2/1/0/0 | 2/1/0/0 | 3/2/1/0 |
| MAC.W | 1/0/0/0 | 0/0/0/0 | 0/0/0/0 | 1/0/0/0 |
| MAC.L | 3/2/1/0 | 2/1/0/0 | 2/1/0/0 | 3/2/1/0 |
| CLRMAC / LDS / STS (register or memory) | 0/0/0/0 | 0/0/0/0 | 0/0/0/0 | 0/0/0/0 |

MAC means either MACH or MACL. Their shared multiplier access serializes even
when the preceding instruction wrote the other accumulator half. Transfers
have one MA, occupy no mm tail, and have one baseline EX interval. Baseline
EX intervals for multiplication are 1 (word), 2 (double length), and 2 (MAC).
The inherited constant 3 for MAC includes a fetch split, not its no-contention
minimum: fig7.23 places the next EX two slots after MAC's EX. After the final
MA the short/long multiplier operates for 2/4 clocks, independently of slot
length. This gives a ready boundary EX+4/EX+7 and explains every table row.

References: MAC.W figs7.23–7.33 (printed pp.196–203), MAC.L figs7.34–7.44
(pp.204–213), word multiply figs7.53–7.62 (pp.222–229), double multiply
figs7.63–7.73 (pp.229–237), transfers including CLRMAC figs7.88–7.91
(pp.245–246). SH-1 figs7.12–7.22 and 7.45–7.52 are excluded.

## Fetch contention

§§7.4.1–7.4.3, figs7.6–7.8 distinguish IF (a real longword access) from
lowercase if (a retained second word). Even a cache hit has IF/MA contention.
An MA paired with IF splits the slot, adding the MA duration; pairing with if
allows overlap. For a four-stage instruction aligned at 4n, MA pairs with if;
at 4n+2 it pairs with IF. The two-MA pipeline's one-slot ID stall changes the
pairing: its first MA pairs with the fetch at PC+4 and its second with PC+6.
One of these is IF. When a multiplier interlock extends that MA, its entire
extended duration occupies the first half of the split slot (§7.4.1).

Figures explicitly illustrating IF contention include 7.25–7.26, 7.30–7.33,
7.36–7.37, 7.41–7.44, 7.57, 7.59–7.62, 7.67, 7.69–7.73. The other diagrams
must still be combined with §7.4; their generic IF labels do not certify
contention-free execution. Native architectural vectors isolate multiplier
contention; clocked fetch tests and the FRT guest additionally exercise alignment.

## Scope and lifecycle

MACH/MACL memory transfers target the **end** of their MA, including when
separation removes all extra mm stalls (figs7.63/7.71/7.73). Cache-array accesses
and resident cache reads complete synchronously, so their full wait precedes
the effect. Scheduled reads start at the ordinary MA start, after the EX/first-MA
lead-in; their bus cycle extends until both its memory latency and the older mm
have finished (§7.2.3, figs7.64/7.65/7.73). These waits overlap in full.
The adapter journals the access classification and earliest completion before
suspending. At bus grant, the final beat's duration is the maximum of its
ordinary duration and the remaining multiplier wait. Sampling still occurs at
completion; arbitration and longer bus waits can postpone it further. STS.L
retains its one-clock admission after the accumulator wait before supplying
data to the write buffer.
MAC.W/MAC.L start their independent first operand MA after EX, with synchronous
reads sampling at its nominal end (EX+2). A scheduled read starts at EX+1 and
samples after its actual latency. Any excess over one clock stretches that
first MA slot, including the successor's ID stall; an IF half of a split slot
still follows it before the second MA starts (§§7.2.3/7.4, figs7.23/7.34).
The second operand's completion and successor EX retain that elapsed time;
the multiplier tail begins after actual second-MA completion. Both completed
reads and clock observations are retained on replay. The
producer/consumer execution-state table above is unchanged.

For MAC/NOP/MOV with no older multiplier operation, cached code and no other
arbitration, these literal boundaries follow from those stage/slot rules:

| MAC EX / alignment | Each operand MA | First sample | Second sample | Third instruction EX |
| --- | --- | --- | --- | --- |
| 32 / 4n | One clock | 34 | 36 | 36 |
| 33 / 4n+2 | One clock | 35 | 36 | 37 |
| 32 / 4n | Two clocks | 35 | 38 | 38 |
| 33 / 4n+2 | Two clocks | 36 | 38 | 39 |

At 4n, IF splits the first MA slot; at 4n+2 it splits the second. A MAC.L
with the two-clock operands above becomes multiplier-ready at 42 (38+4),
independent of alignment. After DMULS.L EX=34, an adjacent MAC.L EX=37
instead reads its first operand during 38..40 and its second during 40..42;
the older mm ends at 42. A first-operand mutation at 39 must therefore be
observed, while changes after 40 must not replace that retained sample.

For the scheduler regression's DMULS.L EX at 34, final MA at 38 and final mm
at 42, LDS.L's MA starts at 38/39/40/41/42 with 0/1/2/3/4 separating NOPs.
Combining §7.2.3 with fig7.73 gives the following independently derived
completion clocks, assuming no additional arbitration. H §7.3's ordinary
32-bit reads take T1/T2 plus the selected WCR1 waits.

| Operand latency | 0 NOPs | 1 NOP | 2 NOPs | 3 NOPs | 4 NOPs |
| --- | --- | --- | --- | --- | --- |
| Synchronous cache read (nominal MA end retained) | 42 | 42 | 42 | 42 | 43 |
| Two clocks (peripheral or ordinary external read) | 42 | 42 | 42 | 43 | 44 |
| Three clocks (one external Tw) | 42 | 42 | 43 | 44 | 45 |
| Four clocks (two external Tw) | 42 | 43 | 44 | 45 | 46 |
| Eight clocks (six external Tw) | 46 | 47 | 48 | 49 | 50 |

An adjacent MAC.L's second MA likewise ends at 42 for a synchronous read or
two-clock peripheral read (figs7.64/7.65); the latter does not add a clock after mm.

MA completion and successor issue are distinct boundaries: table7.2 and
figs7.89/7.91 permit the successor's EX to overlap an aligned transfer's MA.
After a memory transfer retires, the CPU retains the final clock of that shared
MA/EX slot, including scheduled peripheral accesses. Section7.2.3 stretches
the whole slot for multi-clock operands: an aligned LDS.L starting EX at 32
with a two-clock operand read completes MA at 35; the NOP shares the stretched
MA slot and the third instruction starts EX at 35. MAC likewise
retains its successor EX boundary using two baseline intervals (figs7.23/7.34),
so delaying the second operand until MA completion preserves consecutive-MAC
overlap.
If its fetch completes without advancing the clock, the scheduler recovers the
overlapped clock, and any successor multiplier access uses that EX time. An
IF/MA split already occupies the interval and permits no such recovery; a waited
successor fetch or exception entry exhausts it. Operand waits stretch the shared
slot without consuming its overlap. Effects still
complete at MA, and replay retains the observed times. The four memory-transfer
guest kernels each measure sixteen aligned transfer/NOP pairs: 32 clocks,
FRT floor((8+32)/8)=5. The ordinary MOV MA lead-ins in the
wrapper are derived in [the load pipeline contract](sh2-load-pipeline.md).

Availability belongs to each SH-2 and advances in its actual PLL clock domain.
Fetch, operand and on-chip waits can exhaust an earlier mm tail. Replay must
retain sampled clock boundaries as well as completed memory effects. A reset
replaces CPU-local pipeline state. Exception entry is an intervening sequence,
not evidence of an undocumented multiplier cancellation rule: its eight clocks
exceed the longest retained tail. MACH/MACL are software-preserved registers;
exception entry does not add them to the architectural stack.

Full speculative IF/ID/EX/MA/WB completion, arbitrary cross-instruction fetch
scheduling and the remaining #17 conformance requirements are separate work.
