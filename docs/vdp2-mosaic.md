# VDP2 mosaic coordinates (Issue #79)

Related to #17. This contract is recorded before the implementation change.

Primary references: [ST-058-R2 §4.11, printed pp.117–119, Figure 4.19 and
MZCTL](https://antime.kapsi.fi/sega/files/ST-058-R2-060194.pdf#page=135),
[1997 Sega §4.11](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p04_b0.htm)
and [§2.2](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p02_20.htm).
The later specification confirms the sizes, density conversion, rotation restriction
and cell-scroll exclusion. It places the interlace-independent note beneath the
horizontal size table; the earlier English PDF places it after the vertical table.
The explicit interlaced vertical table and prose agree, so that note cannot mean
vertical sizes are interlace independent. No new hardware exception is inferred.

The table uses standard-TV output coordinates; the existing exclusive-monitor
normal-screen multiplexing divides X by two before normal mosaic selection.
The standard high-resolution rotation divisor is described below the table.

Let `l` be a zero-based active field line, `p` the field row parity (0 or 1),
`x` an output dot, and `h = MZSZH+1`, `v = MZSZV+1`. The existing progressive
framebuffer stores single-density pictures in `l` and double-density pictures in
`o = 2*l+p`. Mosaic never changes TVMD, raster counters, output dimensions,
windows, sprite coordinates or the source coordinates of an unselected screen.

| TVMD | Screen | Framebuffer row | Image X before scroll/transform | Image Y before scroll/transform |
| --- | --- | --- | --- | --- |
| Noninterlace | Unselected normal/rotation | l | x | l |
| Single density | Unselected normal/rotation | l | x | l |
| Double density | Unselected normal/rotation | 2*l+p | x | 2*l+p |
| Noninterlace | Selected NBG0–3 | l | floor(x/h)*h | floor(l/v)*v |
| Single density | Selected NBG0–3 | l | floor(x/h)*h | floor(l/v)*v |
| Double density | Selected NBG0–3 | 2*l+p | floor(x/h)*h | floor(l/v)*v |
| Noninterlace/single density | Selected RBG0/RBG1 | l | floor(x/h)*h | l |
| Double density | Selected RBG0/RBG1 | 2*l+p | floor(x/h)*h | l |

Thus an interlaced normal mosaic block spans `2*v` rows of the two-field picture,
but `v` field lines/source-picture rows. Multiplying the source Y by two after
quantization would retain double-density resolution and contradict §2.2. Minimum
mosaic in double density reads source rows 0,1,2,3 on each field, **not** 0,2,4,6
or 1,3,5,7. Copying an adjacent framebuffer row alone cannot implement this.

MZCTL bits 0–3 select NBG0–3; bit 4 selects RBG0 with either parameter A or B;
bit 0 also selects RBG1. Bitmap and tile address selection share these coordinates.
NBG0/NBG1 apply scroll and zoom after selecting the upper-left mosaic dot and
exclude vertical cell scroll on that screen. Rotation ignores MZSZV, including
in double density; its single-density Y drives both coefficients and transforms.
Standard high-resolution rotation still divides mosaic-selected X by two as
specified by §2.1 and the existing rotation acquisition contract.

Rotation start/KAst accumulators must retain both output-density and single-density
progressions. Both consume the same captured table operands and one-shot RPRCTL
requests at the same line event; the mosaic picture chooses the latter progression.
Sprites and unselected rotation pictures continue to use the former. Field start
initializes both, including parity only in the output-density progression. This
also prevents snapshot inspection from changing or rearming retained requests.

The manuals specify the upper-left dot and the size/density relationship. They do
not establish physical mosaic acquisition strobes, half-line phase, active-line
control-write behavior or interaction phases for changing line tables inside a
block. We retain the repository's zero-origin blocks, line-start operand capture
and legal display-grant deadlines. Selected normal line-scroll table indices use
the selected image row; general window/back/line-color latches keep output Y.
Tests of field-boundary changes do not claim analog switching-phase conformance.
