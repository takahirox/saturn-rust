# VDP1 HDTV / 31 kHz readout

Related to #73 and #17. This implements one video mode, not hardware completion.

## Primary-derived contract

The VDP1 manual [§4.1, table 4.2](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_11.htm)
and [§1.2, table 1.2 and following notes](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p01_20.htm)
separate framebuffer storage, source coordinates and physical output. Bit 3 (VBE)
is independent of the three-bit TVM encoding; no other TVMR bits are defined.

| TVM | TVMR encodings (VBE clear/set) | Storage | Permitted scanning | Erase |
| --- | --- | --- | --- | --- |
| 0 | 0 / 8 | 512×256×16 bits | Noninterlace, single/double density | Display with VBE=0; sampled VBlank with VBE=1 |
| 1 | 1 / 9 | 1024×256×8 bits | Noninterlace, single/double density | Display with VBE=0; sampled VBlank with VBE=1 |
| 2 | 2 / 10 | 512×256×16 bits, rotated readout | Noninterlace or single density | Automatic VBlank, also with VBE=0 |
| 3 | 3 / 11 | 512×512×8 bits, rotated readout | Noninterlace or single density | Automatic VBlank, also with VBE=0 |
| 4 | 4 / 12 | 512×256×16 bits, no rotation | Noninterlace; FBCR.DIE=0 | Automatic VBlank, also with VBE=0; no display erase |
| 5–7 | 5–7 / 13–15 | Prohibited | Prohibited | Diagnosed, no mode change |

VBE=1 requires manual erase/change (FCM=FCT=1). The encoded values alone do
not imply every combination with other registers is legal. Prohibited write
windows/combinations have no claimed hardware behavior. The existing checked
host API retains atomic pair validation for longwords; physical SH-2 writes
retain their existing buffered word transactions. Guests use CPU word stores,
as [ST-210 item 3](https://antime.kapsi.fi/sega/files/ST-210-110194.pdf#page=6)
requires. No transaction width, BSC deadline or DMA policy changes here.

In TVM=4 the source is 320×240 (31 kHz) or 352×240 (HDTV). Each physical
color occupies two horizontal and two vertical pixels. VDP2 [§2.1, table 2.1](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p02_10.htm)
exposes two horizontal sampling densities for each monitor. Therefore:

| VDP2 TVMD.HRESO | Exposed RGBA output | VDP1 source coordinate |
| --- | --- | --- |
| 4, exclusive normal A | 320×480 | `(x, floor(y/2))` |
| 5, exclusive normal B | 352×480 | `(x, floor(y/2))` |
| 6, exclusive high A | 640×480 | `(floor(x/2), floor(y/2))` |
| 7, exclusive high B | 704×480 | `(floor(x/2), floor(y/2))` |

The exclusive normal output has already collapsed each physical horizontal
pair into one exposed dot; doubling it again would crop half the source.
Both vertical samples of source row 239 reach output rows 478/479. Readout does
not grow the 0x40000-byte framebuffer, wrap coordinates, or use the RBG matrix,
coefficients, sprite rotation start or delta registers. Standard-TV/interlace
and rotated readout retain their separate paths.

The exclusive high-resolution backgrounds retain §2.1's paired NBG0/NBG1
setup: same bitmap/pattern data and vertical scroll, horizontal increments 2,
NBG1 horizontal scroll one larger, both horizontal reduction permissions,
equal priority, CRAM mode 0, no line-color insertion, special priority or color
calculation. Sprite priority and transparency still use the existing compositor.
The new guest follows these restrictions and overlaps priority 4 RGB/palette
sprites and priority 2 palette sprites with priority 3 opaque backgrounds.

## Setting windows, corrections and publication conventions

[§4.1](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_11.htm)
permits TVM changes from the second HBlank-in following VBlank-in through the
HBlank-in immediately after VBlank-out. Changing VBE preserves TVM, immediately
after VBlank-in; the first-to-second HBlank-in interval is prohibited.
[The chapter 4 update list](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_10.htm)
says TVM takes effect immediately, VBE after one line's display, and erase
operands at framebuffer change. These agree with the R3 rules and the existing
[sampled-VBE reconciliation](vdp1-vbe-timing.md). FBCR follows its own
[§4.2](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_12.htm)
windows and change requests. No primary correction found in those updates or
ST-210 replaces TVM=4's geometry, repetition or erase capacities, so no task
correction or GitHub Issue edit is needed.

| Event | Hardware requirement / implementation convention |
| --- | --- |
| Completed TVM word in permitted window | Immediate mode setting; MODR reports written setting |
| Actual automatic/manual buffer change | Capture EWDR/EWLR/EWRR, apply FBCR drawing controls; PTMR=2 starts drawing |
| VBlank-in / first HBlank-in | Automatic HDTV erase; reuse the existing first-HBlank decision phase, a functional emulator convention |
| Erase opportunities | Retain color, bounds, geometry and physical display destination; publish individual words |
| Blank end | Publish final budgeted word, cancel suffix, then change buffers/capture operands; equal-edge order is an emulator convention |
| Display | No HDTV erase, irrespective of VBE |
| Reset | Cancel pending samples, jobs, drawing and outstanding transactions |

[§4.4, tables 4.4–4.5](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_14.htm)
give `(raster clocks−200)×(field rasters−display rasters)`:

| Monitor (both exposed horizontal densities) | Arithmetic | 16-bit words |
| --- | --- | --- |
| 31 kHz | `(852−200)×(525−480)` | 29,340 |
| HDTV | `(848−200)×(562−480)` | 53,136 |

These use the existing integer raster. The 200-clock deduction is aggregate,
not a specified idle phase. The existing uniform opportunity distribution after
sampling is retained: `S + ceil(k*(B*L−S)/C)`, k=1..C. This preserves capacity
and partial publication without claiming physical bus cadence. No additional
whole-line deduction is made. Unspecified first-word timing, equal-edge ordering,
whole-line raster phases and deterministic reset initialization remain emulator
conventions. Half-line timing and physical monitor synchronization are outside
this change.

## Independent evidence

[Bus tests](../src/bus/hdtv_tests.rs) cover scheduled words on both SH-2s,
written MODR, all storage corners/out-of-range coordinates, prohibited encodings,
unchanged host widths, literal unsaturated capacity boundaries, pending B versus
captured A, both physical destinations, automatic/manual changes, PTMR starts,
END/status/IRQ, transitions, reset and chunks 1/3/17/10000 with passive inspection.

The original [MIT builder](../tests/fixtures/build_vdp1_hdtv.py) emits
[vdp1-hdtv.s](../tests/fixtures/vdp1-hdtv.s) and
[vdp1-hdtv-erase.s](../tests/fixtures/vdp1-hdtv-erase.s), plus reproducible sparse
ROM bytes under the adjacent [license](../tests/fixtures/LICENSE). The command
guest waits for the permitted TVM window, draws three textured sprites and END,
computes an unsigned framebuffer checksum and individual words, reads MODR,
EDSR, COPR and actual SCU draw-end status, then publishes and holds the buffer.
Texture entries include RGB555, palette index 1, priority 1/index 1, transparent 0,
and different neighboring colors on two rows. Edges cover x319/351, y238/239;
CPU address 0x25CBFFFE independently contains ABCD. Changed color and X operands
and omitted PTMR are independent controls, each with two cold boots per mode.

The erase guest uses VBE=0 and automatic changes, captures full-buffer A=1234,
writes pending B=5678 and a smaller pending rectangle, initializes distinct
physical-buffer sentinels, then reads words 0,29339,29340,53135,53136,131071.
At 31 kHz only the first two are 1234; at HDTV the first four are 1234. Remaining
sentinels stay FFFF, including after another held field. No draw or host result
injection can produce this record. Accidental display erase changes word 29340
in the 31 kHz case and fails the oracle.

[Native Machine tests](../tests/video_transactions.rs) and the required
[`videoTransactions` browser scenario](../scripts/vdp1-hdtv.mjs) execute identical
ROM bytes. Literal RGBA vectors check both repetitions of every sampled source
color, transparency/priority overlap, neighboring rows, row 239, right edges,
changed operands, omitted draw and repeated boots. Browser assertions inspect
both the Wasm framebuffer and production Canvas under `/saturn-rust/`.
See [executed checks, hashes and negative controls](validation-issue-73.md).
