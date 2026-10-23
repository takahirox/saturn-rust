# SH-2 reads through the SCU B-bus memory bridge

Related to #37 and #17. This contract covers scheduled execution on both SH-2s.

## Primary basis and limits

[SCU Final Specifications ST-210](https://antime.kapsi.fi/sega/files/ST-210-110194.pdf),
§3 item32, printed p.9, corrects the distinction between internal CPU read size
and the external longword unit, including byte and word operands. Its write rule
preserves the requested size.
[SCU ST-097-R5](https://antime.kapsi.fi/sega/files/ST-097-R5-072694.pdf),
§2.1 and §3.2 figs3.6–3.8, distinguishes the SCU's 32-bit unit from the devices'
16-bit transfers. Those figures illustrate DMA writes; they establish interface
widths, rather than an exact CPU-read waveform or a measured turnaround time.
[SCSP ST-077-R2](https://antime.kapsi.fi/sega/files/ST-077-R2-052594.pdf),
§3.1 printed pp.18–19, requires main-interface halfword accesses and buffered
completion. §3.2 pp.20–21/fig3.2 orders sound-memory owners.
[VDP2 §3.3](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p03_31.htm)
allocates display/CPU access slots. VDP1 local-memory ownership follows
[ST-013-R3 §2.1](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p02_10.htm),
as in the existing VDP1 acquisition tests.

These references establish transfer granularity and ownership constraints.
Ascending-address read phases, one retained CPU group, and reuse of the existing
BSC envelope for each device phase are deterministic model conventions. They do
not establish complete board pin timing, overlapped bridge requests, arbitrary
nonstandard BSC configurations, or physical-console timing equivalence.

## Transaction table

Let `A` be the architectural operand address, `L=A&~3`, and `B0..B3` the bytes
sampled at addresses `L..L+3`. Alignment is checked **before** conversion.
The normal Saturn big-endian mapping is:

| Operand | Valid alignment/lanes | CPU-facing external unit | Device phases, in order | Operand result and completion |
| --- | --- | --- | --- | --- |
| Byte | Any A, lane0/1/2/3 | Aligned longword at L | Word at L; word at L+2 | B[A&3], after both words |
| Word | A even, lane0/2 | Aligned longword at L | Word at L; word at L+2 | B0:B1 or B2:B3, after both words |
| Longword | A divisible by4 | Aligned longword at A | Word at A; word at A+2 | B0:B1:B2:B3, after both words |
| Cache fill | Existing 16-byte line policy | Four longword units in existing wrap order | Two separately completed words per unit | Commit line only after all eight words |
| Cache hit | Existing CPU-local tag/array policy | None | None | Existing cached bytes |

MOV.B/MOV.W sign extension remains in the interpreter; the adapter returns raw
operand lanes. Existing CS2 little-endian compatibility reverses the extracted
operand bytes, leaving byte addresses intact. CPU-local UBC sees the requested
operand; a peer's external UBC sees the assembled aligned longword after its
second completion, never separate downstream halfwords. Host/debugger `Bus`
reads remain instantaneous inspections. Unscheduled `for_cpu` accesses retain
their existing functional/BSC diagnostic behavior; production Machine execution
uses the scheduled adapter.

## Scoped endpoints and software restrictions

| Memory endpoint (physical address) | Preserved geometry |
| --- | --- |
| Sound RAM 05A00000–05AFFFFF | Existing MEM4MB layout, including unpopulated upper addresses; these are not an extra RAM mirror |
| VDP1 VRAM 05C00000–05C7FFFF | Existing 512KiB VRAM |
| VDP1 draw framebuffer 05C80000–05CFFFFF | Existing 256KiB draw buffer and mirror |
| VDP2 VRAM 05E00000–05EFFFFF | Existing 512KiB VRAM and mirror; each word asks the selected bank's grants |
| VDP2 CRAM 05F00000–05F00FFF | Existing color-mode-dependent aliases |

P0 and P1 aliases share backing memory but retain the existing independent cache
behavior. SCSP software must use halfwords/longwords: its main-interface byte
restriction applies to reads and writes. Existing byte-read lane compatibility
is deterministic emulator behavior, covered to prevent accidental regressions;
it is **not** a claim that such software is hardware-conforming. The original
guest uses only word/longword accesses to sound memory and byte lanes to video
memory. Unaligned word/longword operands still fail without acquiring the bus.

MMIO (including SCSP, VDP registers and CD), A-bus conversion, CPU writes and
on-chip DMAC conversion remain outside this increment. In particular,
[CD ST-162 §3.1/table3.1](https://antime.kapsi.fi/sega/files/ST-162-062094.pdf)
has its own communication-register width contract: item32 alone is insufficient
to infer additional FIFO pops, adjacent register accesses or acknowledgements.
Remaining MMIO/A-bus read conversion and write-strobe contracts require separate
device-specific reconciliation. Existing FIFO, clear-on-read and write tests
continue to apply unchanged.

## Scheduling and reset

[CpuBus::external_read_group](../src/bus.rs) separates the operand from the
aligned longword and marks two physical halfwords. The existing
[transaction journal](../src/bus/transactions.rs) keeps sampled data across waits
and interpreter replay. Each phase independently reserves its target and samples
at completion; no backing longword read supplies the result. A completed first
word survives mutation while the second waits. The retained group prevents CPU,
SCU and refresh interleaving inside that accepted unit; target-local sound owners
can delay either phase. Cache fills retain their existing larger group.

At reset CS2 timing each otherwise uncontended phase uses T1/T2 plus six Tw:
eight main clocks. These are model envelopes, not a new measured SCU latency.
The native vectors independently enumerate clocks8/16. With denied VDP2 grants,
six Tw precede the lookup, blanking starts at 1280, and T1/T2 complete at 1282;
the next blanking-time phase completes at 1290. Revoking grants after the first
word separately delays the second word to 1282.

Power reset cancels queued/active read journals. Manual reset finishes an
accepted external longword (including its remaining word) using the existing
reset-drain domain, discards its architectural result, and never resumes the
old instruction. Incomplete reset-drained units generate no synthetic complete
longword observation. A reset before acceptance cancels the queued request.
Reset does not introduce cache snooping or change memory geometry. The slow-video
reset regression now waits for the second eight-clock phase after its original
blanking-delayed first phase; the peer peripheral must still run during the wait.

See [validation for #37](validation-issue-37.md) for independent vectors, the
original licensed guest, actual negative controls and remaining hardware scope.
