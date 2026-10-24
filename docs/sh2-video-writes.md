# SH-2 video-memory write phases

Related to #43 and #17. This table is authored from the interface rules before
new implementation/test results are used. All timing below is a deterministic
bridge convention, not measured electrical timing.

## Primary contract

[ST-097-R5 §3.2, printed pp. 43–45, figs 3.6–3.8](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-097-R5-072694.pdf)
divides a 32-bit transfer into two 16-bit device transfers. The figures describe
DMA, establishing interface granularity rather than a CPU-write waveform.
[ST-210 §3 item 32, printed p. 9](https://antime.kapsi.fi/sega/files/ST-210-110194.pdf)
preserves requested CPU write size. Item 03 (printed p. 4) prohibits byte/longword
VDP1 **register** writes: memory conversion must not legalize those accesses.
[SH7604 §7.11.2, printed pp. 210–211](https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf)
separates buffer acceptance from external completion; dependent external reads
wait for buffered writes. DMAC TE/IRQ retains its early buffer-acceptance boundary.
[VDP1 §2.1](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p02_10.htm)
gives system-controller memory accesses priority over drawing and exposes only
the draw framebuffer. [VDP2 §3.3](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p03_30.htm)
allocates CPU/display accesses by bank and cycle command.

## Independently derived examples

With reset CS2 width 32 bits, T1/T2 plus six Tw is eight clocks per device phase.
A CPU store enqueued at 0 is accepted internally at 1 while its first external
phase runs 0–8; its second runs 8–16. These envelopes reuse existing BSC timing;
ascending addresses and completion-before-next-grant are model conventions.

| Requested operation | Retained payload and publication | External UBC observation (width in bytes) |
| --- | --- | --- |
| Longword A=25E01000, value 11223344 | at 8: word 1122 at A; at 16: word 3344 at A+2 | One width 4 value 11223344 at 16 |
| Byte A=25E01001, value AA | at 8: byte AA at A; other bytes unchanged | One width 1 at 8 |
| Word A=25E01002, value 5566 | at 8: word 5566 at A; other word unchanged | One width 2 at 8 |
| Little-endian longword 11223344 | word 4433 at A; word 2211 at A+2 | One width 4 value 44332211 |
| BSC width 16 bits longword 11223344 | two existing BSC words 1122/3344 | Existing two width 2 observations |
| BSC width 8 bits longword 11223344 | four existing byte strobes 11/22/33/44 | Existing four width 1 observations |
| DMAC longword from zero-wait CS3 | source 0–2, acceptance 2–3, words 3–11/11–19 | TE/IRQ at 3; one width 4 at 19 |
| DMAC 16-byte unit | Source reads 0–8; words at 17/25/33/41/49/57/65/73 | Preserve four CPU-facing longwords |

For initial AABBCCDD, the first example yields 1122CCDD after 8, then 11223344
after 16. Changing source/register/debugger backing after acceptance cannot change
the pending 3344. Changing the completed word to 7788 must leave 77883344 after 16:
no replay of 1122 is permitted. An external dependent read starts only after 16;
an independent on-chip read can complete during the write. A second write cannot
be accepted into the occupied single-entry buffer.

VDP2 display grants are checked separately for each phase: revoking grants after
8 delays the second completion until blanking 1280 plus T1/T2=1282. With a display
acquisition at 32, stores beginning 14/16/17 complete their second words 30/32/33;
display samples before equal-edge publication. Their first words finish 22/24/25.
VDP1 VRAM and framebuffer ownership is asserted for each full word envelope;
untextured drawing can continue during independent VRAM writes.

## Scope and reset

Only scheduled CPU/on-chip-DMAC memory writes are converted: VDP1 VRAM
05C00000–05C7FFFF, draw framebuffer 05C80000–05CFFFFF (existing mirror), VDP2
VRAM 05E00000–05EFFFFF (existing mirror), and CRAM 05F00000–05F00FFF (existing
color-mode aliases), including P0/P1. Alignment, operand endian conversion,
cache policy and byte/word strobes precede this conversion. Read bridging,
SCU/DSP and sound-memory allocation retain their existing contracts.

Power CPU reset cancels pending words, retaining completed memory and peer state.
Manual CPU reset drains the accepted current longword, including its pending
word, without replaying completed words or resuming the old instruction. Queued
unaccepted work is canceled. System reset clears volatile memory/journals;
canceled writes must never reappear. Debugger inspection acquires no ownership.
These reset crossings preserve existing functional conventions, not calibrated
board signals. Full #17 hardware acceptance and commercial compatibility remain
unverified.

## Original guest oracle

The original MIT guest uses sixteen CPU longword stores followed by a dependent
read at each video endpoint, and a four-longword (16-byte) on-chip-DMA unit. Both
processors execute the same computation. Sixteen device longwords require at
least 256 external clocks, or 32 FRT/8 ticks, before instruction/peripheral costs.
Replacing all sixteen store instructions with NOPs removes their bus demand;
the original interval must exceed the replacement by more than 24 FRT ticks.
This budget separates sixteen-clock writes from the inherited eight-clock
atomic writes while allowing instruction overhead. The DMA test retains data
and early TE rather than claiming final publication at TE; its dependent read
drains the last buffered longword.

The guest separately configures a 4bpp bitmap using palette entry 2 in CRAM mode 2.
An initial 32-bit color 00FF0000 is blue. A longword 000000FF (red), or 0000FF00
(green), first publishes high word 0000, removing blue while the old low word
remains 0000 (black). Its second publication supplies the new red/green. At the
normal four-clock dot cadence, the eight-clock phase gap creates exactly two
black pixels between nonempty blue and red/green prefixes/suffixes on line 16.
The omitted store leaves that row entirely blue. Pixel locations are not copied
from a run: the oracle requires this independently derived color sequence and
two-dot length wherever the guest's bounded horizontal-counter wait lands.
Hosts only load/modify ROM source operands, execute instructions and inspect
records/actual rendered pixels; they supply no running results or framebuffer.

The inherited VDP1 register handler accepts some longword compatibility accesses.
This change leaves that MMIO path intact; those accesses are not established as
hardware-conforming by item 32 and remain subject to item 03's restriction.
