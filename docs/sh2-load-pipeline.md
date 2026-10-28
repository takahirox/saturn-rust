# SH-2 ordinary MOV load pipeline

Related to #55 and #17. This stage table was authored before production changes.

Primary contract: Hitachi [SH-1/SH-2 Programming Manual](https://antime.kapsi.fi/sega/files/h12p0.pdf),
September 3, 1996, §§7.2–7.5 (printed pp.169–176), figs7.4–7.9,
and §7.7.1, fig7.11 (pp.184–185). Apply the SH-2 rules; the SH-1
multiplier diagrams are irrelevant. Figure7.11's MA label is rendered “MB”
in this edition; its accompanying five-stage description identifies MA.
No timing correction is inferred from that typographic discrepancy.
A cross-check with Renesas [SH-1/SH-2/SH-DSP Software Manual](https://www.renesas.com/en/document/mah/sh-1sh-2sh-dsp-software-manual),
Rev.5.00, June 30, 2004, §7.1.3, §7.2.1–2 and §7.4.1/fig7.20
(printed pp.411, 413–418, 437), confirms the SH-1/SH-2 slot, dependency
exceptions and ordinary load MA/WB sequence. Figure7.20 labels MA correctly.
DSP-specific operations and multiplier corrections are not applied to MOV.

## Independent clock table

Let load EX start at 32. EX ends at 33, MA starts at 33, and an L-clock
operand completes at 33+L. WB follows MA. At 4n MA shares retained if;
at 4n+2 real IF splits the slot, with MA first. A dependent successor
also splits MA/EX. These are the same slot split, not cumulative penalties.
Same-destination loads and MAC are the documented dependency exceptions.

| Alignment | L | MA acquisition | Operand completion / WB start | Independent successor EX | Dependent successor EX | Third EX after independent NOP |
| --- | --- | --- | --- | --- | --- | --- |
| 4n | 1 | 33 | 34 | 33 | 34 | 34 |
| 4n+2 | 1 | 33 | 34 | 34 | 34 | 35 |
| 4n | 2 | 33 | 35 | 33 (stretched through 35) | 35 | 35 |
| 4n+2 | 2 | 33 | 35 | 35 | 35 | 36 |
| 4n | 4 | 33 | 37 | 33 (stretched through 37) | 37 | 37 |
| 4n+2 | 4 | 33 | 37 | 37 | 37 | 38 |

## Implementation conventions and scope

The transaction interpreter exposes completed architectural instructions, rather
than partial EX state. It samples synchronous cache/cache-array operands at MA
completion, and admits scheduled operands at MA start. The journal retains
completed reads, scheduling decisions and clock observations. An independent
successor of an aligned load recovers the last shared-slot clock after load
retirement. Its nominal EX at the start of a stretched slot is represented by
that slot's last clock, matching the existing multiplier scheduler convention.
A dependency consumes that overlap; an existing IF split already covers it.
Thus the literal table describes stage starts, while native scheduler assertions
also identify the representative clock used for stretched EX.

Equal-edge mutations are deterministic emulator ordering: hardware transactions
complete before CPU replay at an edge; a direct host write after advancement to
that edge follows scheduled sampling. Synchronous reads sample during replay.
This ordering is not a claim about analog setup/hold timing.

The required precise scope is resident instruction-cache execution. The existing
fetch/refill path remains demand driven; it does not speculatively issue future
IF cycles alongside older instructions. Cold/uncached fetch waits exhaust any
retained overlap. Arbitrary branch/refill IF/ID scheduling and other instruction families remain
mandatory #17 work. Ordinary stores now have a separate
[stage/deadline contract](sh2-store-pipeline.md). Genuine BIOS and
commercial-game compatibility remain unverified.

## Existing timing fixtures

The corrected wrapper in `build_sh2_timing.kernel` resumes FRT at clock zero.
The first aligned high-byte load has EX=0, MA=1..3; the following word-offset
low-byte load has representative EX=2, MA=3..5. The body starts at 5.
The final high-byte load samples three clocks after body completion. Hence
FRT records `floor((8+N)/8)`, replacing the earlier six-clock wrapper.
This changes timer expectations while preserving guest computation and controls.

| Fixture/body | Independently counted N / timer expression |
| --- | --- |
| 16 aligned load/NOP pairs, one-clock operand | 32; FRT=5 |
| Leading NOP and 16 word-offset load/NOP pairs | 49; FRT=7 |
| 16 aligned load/dependent ADD pairs | 48; FRT=7 |
| 32 alternating aligned/offset one-clock loads | 48; FRT=7 |
| Eight load/NOP/ADD triples | 24+4 IF splits=28; FRT=4 |
| 32 two-clock peripheral loads | 64+16 IF splits=80; FRT=11 |
| 16 aligned peripheral load/NOP pairs | 48; FRT=7 |
| 16 aligned peripheral load/dependent ADD pairs | 64; FRT=9 |

The multiplier bodies are unchanged. Their shared wrapper also changes from
six to eight clocks, including mutation expectations. The earlier multiplier
wait-aging vector now drains the older mm during MOV EX/MA as well as the read;
its following STS therefore has one execution state for either operand route.
The external-wait scheduler vector now starts MA at 33 after EX=32, rather than
issuing data at 32; it still asserts independent peer progress and cold reset.

For A-bus kernels, sixteen alternating loads add eight IF splits to the
bus-derived bodies. Revised N is 40/88/200/88/60/55; with the eight-clock
wrapper the authored ticks are 6/12/26/12/8/7. The changed normal-wait operand
adds sixteen clocks, giving 14 ticks. Bus durations and data assertions remain.

For the uncached BSC software-wait fixture, enumerating the retained CS0 fetches,
eight load lead-ins and peripheral phases gives 72+8*Tw clocks, hence 9+Tw
ticks. A narrow bus adds `(2+Tw)` per additional physical data beat per operand.
The width/endian and changed-wait controls retain all transfer assertions.

The memory-interface write kernel has its initial high-byte MA at 1..3,
low-byte MA at 3..5 and EXTU at 5. The first ADD starts at 6; its following
store admission ends at 8. The eighth admission ends at
`8+7*max(2, external_write_cycle)`. The final peripheral latch completes three
clocks later, giving `floor((11+7*max(2, external_write_cycle))/8)`.
Read-interface records keep their existing protocol-derived values.

The CS0 kernel contains sixteen ROM and sixteen low-RAM loads. For the
selected wait fields each grouped external read takes L=4/5/7 clocks. The
alternating alignments add sixteen IF splits. FRT is
`floor((8+32*L+16)/8)` = 19/23/31. Replacing the ROM reads by NOPs gives
`floor((8+16+16*L+8)/8)` = 12/14/18, retaining the original omitted-read control.

The rotation image guest keeps its original H=844, first-dot retention and
all following-dot replacement assertions. A DT/taken-BF iteration costs four
clocks; normal HCNT advances two units per iteration. Tile/RBG1 use one fewer
iteration and two explicit NOPs; bitmap uses four fewer iterations. A delayed
BRA to the instruction following the store adds one clock over the old NOP
without expanding the hot poll/store path across another instruction-cache
line. Its store executes in the branch slot. The source builder records these
phase choices, regenerates source/bytes/hashes, and preserves the early-write,
omitted-write, changed-color and chunk controls.


Issue #69 adds [bounded scheduled IF/ID](sh2-fetch-pipeline.md) for resident,
cold and uncached streams. Earlier demand-fetch limitations above describe the
historical increment. Existing MA, dependency, forwarding and multiplier
resource rules continue to apply; the new contract documents fixture corrections.
