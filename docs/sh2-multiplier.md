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
separation removes all extra mm stalls (figs7.63/7.71/7.73). A scheduled bus
transfer can supply the final clock of that wait. Cache-array accesses and
resident cache reads complete synchronously in the adapter, so their full wait
must precede the effect. The adapter journals this latency observation before
suspending; it does not add a second clock to an already scheduled transfer.
This completion rule for LDS.L/STS.L does not change MAC operand overlap or the
producer/consumer execution-state table above.

MA completion and successor issue are distinct boundaries: table7.2 and
figs7.89/7.91 permit the successor's EX to overlap an aligned transfer's MA.
After a synchronous transfer retires, the CPU retains that successor EX time.
If its fetch completes without advancing the clock, the scheduler recovers the
overlapped clock, and any successor multiplier access uses that EX time. An
IF/MA split already occupies the interval and permits no such recovery; a waited
fetch or exception entry exhausts it. Effects still complete at MA, and replay
retains the observed times. The four memory-transfer guest kernels each measure
sixteen aligned transfer/NOP pairs: 32 clocks, FRT floor((6+32)/8)=4.

Availability belongs to each SH-2 and advances in its actual PLL clock domain.
Fetch, operand and on-chip waits can exhaust an earlier mm tail. Replay must
retain sampled clock boundaries as well as completed memory effects. A reset
replaces CPU-local pipeline state. Exception entry is an intervening sequence,
not evidence of an undocumented multiplier cancellation rule: its eight clocks
exceed the longest retained tail. MACH/MACL are software-preserved registers;
exception entry does not add them to the architectural stack.

Full speculative IF/ID/EX/MA/WB completion, arbitrary cross-instruction fetch
scheduling and the remaining #17 conformance requirements are separate work.
