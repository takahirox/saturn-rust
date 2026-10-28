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
line, HBlank-in at H=1280/1408 main clocks for the slow/fast PLL, and
VBlank-out at the next field's line zero. Sample VBE once at that first
HBlank-in, before external bus completions at an equal clock. This chooses a
phase within the specified ordering; it is not measured electrical timing.
A write completed strictly before that edge participates. An equal-edge write
has written readback afterward but misses this field's sample. Writes inside
the prohibited interval are regression probes only, with no claimed hardware
result.

Let S be that sample offset and T=`B*L-S`. Preserve C=`(L-200)*B` word
opportunities by placing opportunity k at `S + ceil(k*T/C)`, for k=1..C.
This uniform integer cadence publishes individual retained words, never before
the decision, and consumes exactly C opportunities by blank end. This is an
aggregate functional phase convention, **not a claim of physical erase bus
cadence**. There is no per-line restart or catch-up burst at sampling. A small
rectangle completes early; a large rectangle retains its uncompleted suffix.
The final opportunity at VBlank-out is processed before cancellation and buffer
change. For NTSC 320×224, first publication is S+2 and last is B*L.
Rotation needs automatic blank erase even with VBE=0 (§4.4); it uses the
same sample/start phase and aggregate capacity convention. The automatic-mode
predicate also includes HDTV bit2, as §4.4 requires. The inherited checked TVMR
write API rejected TVM=4 (HDTV) at the #65 checkpoint. The subsequent
[HDTV increment](vdp1-hdtv.md) accepts it, suppresses display erase and composes
exclusive-monitor sprites using this same blank-erase convention. Complete
monitor/half-line timing remains outside this increment.

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
| Last opportunity / VBlank-out | Last allowed word, cancel suffix, then requested swap and operand capture |
| SYSRES / cold reset | Discard pending sample, retained job and outstanding register transactions |

Reset values are deterministic emulator initialization, not hardware power-on
claims. Display erase microsequencing, PTMR changes, rasterizer changes and VDP2
half-line timing are not established here. Genuine BIOS/commercial compatibility
and all remaining #17 CPU/peripheral, multiprocessor/bus/reset, SCU/DSP,
video/audio, CD/media/HLE, SMPC and comprehensive integration work remain required.
