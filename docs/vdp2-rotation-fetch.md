# Rotation image transaction contract

Related to #39 and #17. Written before the implementation and its pixel oracle.

## Primary requirements

Sega [ST-058-R2 §3.3](https://antime.kapsi.fi/sega/files/ST-058-R2-060194.pdf)
and the later [§3.3 display access description](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p03_30.htm)
distinguish rotation image ports from normal cycle commands. RBG0 RAMCTL roles
are 2 for names and 3 for characters/bitmaps. Role 1 is coefficients, outside
this image increment. A rotation-assigned bank is occupied for the entire cycle;
its normal cycle commands are ignored. RBG1 names are fixed to B1, characters
to B0. CPU ownership remains at A/B port granularity, including both split halves.

[§6.2 RAMCTL](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p06_20.htm)
requires eligible addresses in the assigned banks. Unsplit sides use A0/B0 roles
for their full 256 KiB; split sides use four 128 KiB banks. RBG1 requires split
B, RBG0 enabled, mode 0, and excludes normal backgrounds. RBG0 can accompany
normal screens whose image banks are independent. Image data are not shared
between rotation screens or with normal screens.

[§4.4](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p04_40.htm),
[§4.6](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p04_60.htm),
§§4.5/4.7–4.9 specify 8/16-dot characters, one/two-word names, page/plane/map
geometry, and 4/8/11-bit palette and RGB555/RGB888 images. RBG0 also supports
512×256/512 bitmaps; RBG1 is cell format. Retain the name flags, palette,
character selection, cell offset and image format with the image operand.
[§6.1](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p06_10.htm)
specifies transformed coordinates: consecutive output dots need not address
consecutive source words. An address revisit is a new acquisition, not a cache
hit. Screen-over and A/B selection apply before image addressing.

[§6.3](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p06_31.htm)
requires RPRCTL requests to clear with the next parameter read. Preserve the
existing line-start latch and its one-shot requests; image address generation,
replay, debugger snapshots and repeated rendering must not consume them.
Coefficient interpretation and coefficient-table timing remain separate.

## Deterministic phase assumptions

The cited specifications do not establish the physical rotation fetch-to-display
latency, internal subaccess order, or packing of the rotation image port. This
implementation chooses one transaction per output dot at the beginning of that
dot: four main clocks in normal resolution, two in high resolution. It acquires
a complete one/two-word name, then an aligned 32-bit character/bitmap operand,
including both RGB888 words. These logical subaccesses are atomic at that edge;
no normal-bank bandwidth is borrowed. Every output dot issues fresh transfers,
even if its addresses repeat. This is independent of #19's normal two-cycle
lead; rotation never prefetches an upcoming line's parameter requests.

Line-start parameter/table reads precede image acquisition. Image acquisition
precedes physical CPU/DMA completion at the same edge, matching the existing
board event order. Each dot retains its physical addresses, raw words and address
interpretation until scanout. Palette and composition controls remain live.
Missing/ineligible image operands produce no layer; production scanout cannot
repair them from backing VRAM. Debugger snapshots decode current RAM without
changing acquisition state. A new line/field or reset discards stale operands.
Exclusive-monitor rotation remains unsupported, as in the existing decoder.

## Reconciled corrections

The later primary HTML manual is used alongside ST-058-R2. In particular, its
§3.3 separates rotation accesses from the normal-only bandwidth tables3.2–3.4;
those tables do not establish rotation latency. Its §6.2 states unsplit role
selection and RBG1 B0/B1 routing explicitly. The later §6.3 RPRCTL final paragraph
requires repeated writes for repeated reloads (the #35 contract remains intact).
The [Sega technical bulletins](https://antime.kapsi.fi/sega/files/Sattechs.pdf)
were also inspected (PDF page numbers, one-based):

- #13, June 2 1995, p.55: name storage restrictions cover rotation too. The
  dual-screen vectors use the legal A0/B1 pairing; unsplit vectors use one side.
- #14, June 2 1995, pp.56–68: bitmap layout/transfer examples do not specify a
  rotation pipeline delay or override the dedicated-bank rules.
- #33, November 8 1995, pp.107–108, and #37, May 6 1996, pp.116–117:
  normal-screen flip restrictions, prohibited SCU VRAM source reads, counter
  correction and high-resolution normal-cell timing corrections remain intact.
  DMA vectors here write VRAM from work RAM. No guest switches TV resolution.
- #SOA-6, August 16 1995, pp.142–146: normal-cycle guidance is not evidence of
  rotation latency. #SOA-11, same date, pp.165–169, explains transformations
  conceptually and explicitly distinguishes them from physical implementation.

The §4.5 N0CHCN/R0CHCN tables prohibit RGB888 in high resolution and all rotation
in exclusive monitors. Acquisition preserves these restrictions (and now rejects
high-resolution RGB888 for rotation, matching the existing NBG0 validation).
No Issue acceptance criterion needed weakening or external Issue changes.
Community emulator output is never an address, timing or pixel oracle.

## High-resolution picture coordinates (#53)

Related to #53 and #17. The 1997 [Japanese §2.1, high-resolution mode](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/hard/vdp2/hon/p02_10_j.htm)
and [English §2.1](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p02_10.htm)
agree that both rotation pictures retain normal-mode horizontal resolution.
[STN-24, high-resolution precautions](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/info/hon/stn24.htm)
independently gives that same rule. Consequently the shared address decoder uses
`h = floor(output_x / 2)` in standard-TV HRESO=010/011; in normal modes it uses
`h = output_x`. No completed framebuffer is resized. The normal/high mode pairs
cover respectively 320/640 and 352/704 output dots, with the final high-resolution
pair addressing picture coordinate 319 or 351. This changes both RBG0 parameter
sets, tile/bitmap addressing and RBG1's parameter-B fixed-bank tile path.

Apply `h` to the horizontal terms of [§6.3's equations](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p06_30.htm):

- `Sx = latched_Xst + delta_X * h`, and similarly for `Sy`;
- `KA = latched_KAst + delta_KAx * h`, with integer truncation after addition;
- feed those screen coordinates and the selected coefficient into the
  [§6.1 transform](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p06_10.htm).

Signed/fractional increments are applied in fixed point before truncation;
dividing a transformed integer address or halving the increment loses fractional
phase. [§6.4](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p06_40.htm)
coefficient modes, transparency-driven parameter selection and RBG1's ignored
coefficient MSB keep their meanings. Horizontal coefficient indices repeat with
picture coordinates, including positive/negative fractional delta_KAx.

Windows use an independent output coordinate. [§8.1 and table8.1](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p08_10.htm)
define window registers in TV coordinates; all ten horizontal bits are valid in
standard-TV high resolution. Layer windows and RPMD=3's parameter window can
therefore change between output dots 2 and 3 even though both have `h=1`.
Normal-screen sampling, VDP1 readout, priorities, palette interpretation and
composition remain in their existing output-coordinate domain. Existing mosaic
snapping precedes the image-coordinate conversion; mosaic behavior is otherwise
unchanged. Exclusive-monitor rotation remains unsupported.

Picture spacing does **not** specify a physical read/retention phase. No inspected
primary reference establishes a different high-resolution image acquisition
cadence. Retain the deterministic convention above: one fresh logical acquisition
per output dot, at main-clock phases `2*x` in high resolution (`4*x` in normal).
Adjacent paired dots can thus acquire different words if memory changes between
those edges. A simultaneous physical publication follows acquisition. Scheduled
name/image/address/format/coefficient-selection operands remain retained until
consumption; unavailable or absent acquisitions stay absent. Debugger rendering
uses the same picture equations against current memory, without advancing clocks,
consuming reload requests, adding acquisitions or repairing scheduled misses.
Field tags, new-line discard and reset cancellation remain unchanged. Coefficient
bus scheduling and internal physical subaccess phases remain unspecified here.

### Correction reconciliation

The [1997 correction history](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/hard/vdp2/history.htm)
(and its [Japanese original](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/hard/vdp2/history_j.htm))
incorporates STN-13 name-location restrictions and STN-37 VBLANK, V-counter and
normal-cell timing corrections; it supplies no replacement rotation-picture rule.
The archive's [STN-13](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/info/hon/stn13.htm)
and [STN-37](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/info/hon/stn37.htm)
redirect to the amended manual sections. The tests keep legal A0/B1 names and
A1/B0 characters, the corrected window layouts and existing RGB888 restrictions.
STN-37's resolution link points to [§2.4 HRESO](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p02_40.htm#HRESO_),
including safe A/B clock selection and the allowed VDP1/VDP2 combinations.
[STN-22](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/info/hon/stn22.htm)
permits normal and rotated VDP1 readout with high-resolution VDP2. These rules do
not make VDP1's coordinate path a rotation-background picture coordinate.

The new guests issue and wait for the appropriate SMPC CKCHG command before video
setup (graphic A slow PLL, B fast PLL), then construct their own operands and
enable the display. They never switch an active A/B display. No BOOT-ROM routine
or proprietary firmware is needed for this original diagnostic. No source-Issue
criterion was changed, and no physical acquisition timing is claimed from spatial
resolution. See [executed validation](validation-issue-53.md).
