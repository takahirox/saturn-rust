# VDP1 VBE sampling and blank erase

Related to #65. Related to #17; this is partial hardware-completion progress.

## Primary contract and reconciliation

[ST-013-R3](https://antime.kapsi.fi/sega/files/ST-013-R3-061694.pdf), printed
p.35, and the [later chapter 4 update list](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_10.htm)
separate immediate TVM changes from VBE becoming effective after the display
portion of the first line following VBlank-in. [§4.1](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_11.htm)
(R3 p.36) requires the same TVM when changing VBE, permits the immediate
post-interrupt window, and prohibits accesses between the first and second
HBlank-in interrupts. These are consistent rules, not permission to sample every
CPU write. [§4.2](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_12.htm)
(R3 pp.39–42, table 4.3(b)) requires manual erase/change (FCM=FCT=1) for VBE,
retains VBE across fields, interrupts unfinished erase at blank end and changes
the physical buffers then. It also describes returning VBE to zero immediately
after VBlank-out; the next VBlank-in window can likewise explicitly clear it.
FBCR has its own programming windows; the erase/change sequence explicitly
permits requesting that change immediately after VBlank-in.

[§4.4 tables 4.4–4.5](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_14.htm)
(R3 pp.49–50) specify capacity `(L - 200) * B`, where L is raster clocks and B
is the number of blank rasters. Examples: NTSC 320×224 is 58,812; 352×224
is 63,180; PAL 320×256 is 85,956 and 352×256 is 92,340. The 200 term is
an aggregate capacity deduction; those tables do not locate 200 idle cycles at
the beginning of every raster. Subtracting an additional whole raster for VBE
sampling would contradict these capacities. The old `VBlank-in + 201` first-word
assertion incorrectly promoted an implementation choice to a hardware contract.
The coordinate expression with a factor of eight uses encoded horizontal groups;
pixel geometry is specified separately (eight 16-bit or sixteen 8-bit pixels per
X unit), and a word publishes one 16-bit or two 8-bit pixels. It is not eight
additional clocks per output pixel. [ST-210 §3 item 3](https://antime.kapsi.fi/sega/files/ST-210-110194.pdf)
requires VDP1 register word accesses; the later VDP1 chapter9 prohibits
system-register DMA. ST-210 item30 confirms the 263/224-line standard-TV
example, and item31 gives the 427/455-dot timer ranges. Fixture configuration
uses CPU word stores. No cited correction replaces
the VBE window or these standard-TV capacities.

## Explicit emulator phase convention

The existing integer raster places VBlank-in at the start of the first blank
line and HBlank-in at H=1280/1408 main clocks for the slow/fast PLL.
VBlank-out falls at horizontal sync of the last blank line, at H=1388/1500
(dot 347/375), so the first HBlank-in after it is line zero's; exclusive
monitor modes keep VBlank-out at line zero. Mednafen places VBLANK's end at the
same dot. Sample VBE once at that first
HBlank-in, before external bus completions at an equal clock. This chooses a
phase within the specified ordering; it is not measured electrical timing.
A write completed strictly before that edge participates. An equal-edge write
has written readback afterward but misses this field's sample. Writes inside
the prohibited interval are regression probes only, with no claimed hardware
result.

Let S be that sample offset, P the VBlank-in to VBlank-out interval (`B*L`
less the 320 clocks after horizontal sync) and T=`P-S`. Preserve C=`(L-200)*B` word
opportunities by placing opportunity k at `S + ceil(k*T/C)`, for k=1..C.
This uniform integer cadence publishes individual retained words, never before
the decision, and consumes exactly C opportunities by blank end. This is an
aggregate functional phase convention, **not a claim of physical erase bus
cadence**. There is no per-line restart or catch-up burst at sampling. A small
rectangle completes early; a large rectangle retains its uncompleted suffix.
The final opportunity at VBlank-out is processed before cancellation and buffer
change. For NTSC 320×224, first publication is S+2 and last is B*L-320.
Rotation needs automatic blank erase even with VBE=0 (§4.4); it uses the
same sample/start phase and aggregate capacity convention. The automatic-mode
predicate also includes HDTV bit2, as §4.4 requires. The inherited checked TVMR
write API rejected TVM=4 (HDTV) at the #65 checkpoint. The subsequent
[HDTV increment](vdp1-hdtv.md) accepts it, suppresses display erase and composes
exclusive-monitor sprites using this same blank-erase convention. Complete
monitor/half-line timing remains outside this increment.

The field change is latched at the first HBlank-in after VBlank-OUT, not at
the VBlank-OUT edge. [§4.2](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_12.htm)
says FCM/FCT written by the CPU are updated inside VDP1 at the field change,
asks for them to be written immediately after the VBlank-OUT interrupt, and
forbids access from that HBlank-in to the next. A write in that interrupt
therefore decides this field's change. Mednafen also decides and performs the
swap at that HBlank-in. A retail FMV relies on this: it writes FBCR from its
VBlank-OUT handler, and changing at the VBlank-OUT edge instead displayed its last
RGB frame for one field under the following palette-only sprite settings.

At the latch, buffers swap, the previous field's drawing ends, and EOS/DIE/DIL,
EWDR/EWLR/EWRR, LOPR and EDSR are captured as before. The bus applies the latch
before composing line zero, so the new front is displayed for the whole field;
the video worker receives that front at the same HBlank-in. Automatic drawing
(PTM=10B) begins at this same edge. [§4.3](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_13.htm)
starts PTM=10B drawing with the frame and gives no earlier edge. Command tables
that a VBlank-OUT DMA rewrites before this edge are drawn in the same field. A
request written after the prohibited window waits for the next field. Original
guests that request a later change or use the new back buffer after VBlank-OUT
wait for line two. This phase is a convention, not measured electrical timing.

| Event / legal written sequence | Effective decision / publication |
| --- | --- |
| VBlank-in; write 0→1, unchanged TVM, manual erase/change | No writes yet; MODR reports written 1 |
| Several writes 1→0→1 before first HBlank-in | Last completed value wins; one accepted job |
| Several writes 0→1→0 in the same window | Disabled; no accepted job |
| First HBlank-in | Sample once; retain captured EWDR/EWLR/EWRR, geometry and physical display buffer |
| First opportunity | Publish first retained word; drawing/status/IRQ unaffected |
| Later register writes | Cannot launch/restart/recolor the accepted job |
| Next field without TVMR writes | Persistent 1 accepts one new job; consumed manual-change request does not repeat |
| Legal clear after VBlank-out or next VBlank-in | Next sample disables erase; written readback is separate |
| Last opportunity / VBlank-out | Last allowed word, cancel suffix |
| First HBlank-in after VBlank-out | Requested swap and operand capture, using FBCR as written by then |
| SYSRES / cold reset | Discard pending sample, retained job and outstanding register transactions |

Reset values are deterministic emulator initialization, not hardware power-on
claims. Display erase microsequencing, PTMR changes, rasterizer changes and VDP2
half-line timing are not established here. Genuine BIOS/commercial compatibility
and all remaining #17 CPU/peripheral, multiprocessor/bus/reset, SCU/DSP,
video/audio, CD/media/HLE, SMPC and comprehensive integration work remain required.
