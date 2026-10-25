# VDP1 horizontal texture selection

Related to #57 and #17. This contract covers horizontal selection on the existing
scaled/distorted raster lines. It does not establish vertical scaling, polygon
coverage or physical drawing bandwidth.

## Primary references and reconciliation

- [1997 VDP1 §6.3, Figure 6.5](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p06_31.htm),
  all eight rows in each of its three panels, establishes the literal vectors
  below. The [Japanese original](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25-jp/hard/vdp1/hon/p06_31.htm)
  agrees, including the enlargement repeats and HSS end-code statement.
- [ST-013-R3-061694](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-013-R3-061694.pdf),
  §4.2 p.44 and §6.3 pp.81–82 (PDF pp.59, 96–97), has the same EOS meaning,
  HSS prose and Figure 6.5 vectors. The later manual does not replace these
  vectors with endpoint truncation. Neither source specifies an internal
  accumulator circuit or a complete stepping recurrence for arbitrary ratios.
- [§4.2 FBCR/EOS](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_12.htm)
  selects **original source X** parity only on reduced lines with HSS=1.
  Unchanged and enlarged lines use the complete source, independently of EOS.
- [§7.5](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p07_50.htm)
  and [§7.6](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p07_60.htm)
  define scaled/distorted commands, vertex order and command read-direction
  inversion. Geometry and source read flips can cancel each other.
- §6.3's HSS prose and [Chapter 9's command precaution](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p09_10.htm)
  explicitly disable end-code processing at **every** magnification when HSS=1.
  This also agrees with ST-013-R3 p.81. The expansion/ECD=0 row of the
  [ECD summary table](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p06_34.htm)
  conflicts with that explicit rule and is not used to override it. ECD=1
  disables end codes independently; transparency still follows SPD.
- [ST-013-SP1-052794](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-013-SP1-052794.pdf),
  introduction and §§1.1–1.2, explains that version 0 lacks HSS/EOS/Pclp. It
  does not remove these version-1 functions or provide another stepping rule.
  The emulator advertises version 1 in MODR.
- Sega Technical Bulletin #15, June 2 1995, [Sattechs.pdf PDF p.69](https://antime.kapsi.fi/sega/files/Sattechs.pdf#page=69),
  corrects the conflicting Clip/Cmod prose at ST-013 p.79: Clip is bit10,
  Cmod bit9. The implementation retains those corrected fields. This correction
  does not change Figure 6.5 or the HSS end-code prose.

These references confirm the task's distinguishing vectors. No correction to
Issue #57's stated vectors or acceptance criteria was needed. No GitHub state
was changed, as required by this execution's local-only instructions.

## Independent literal vectors

Numbers below are source indices 0..7; the original figure's row prefixes
00/10/20/30/40/50/60/70 identify separate source rows and are omitted here.
EOS is immaterial in the HSS=0 column. At 8/8 and above both HSS panels use
the same full source as HSS=0.

| Figure row / ratio | HSS=0 | HSS=1, EOS=0 | HSS=1, EOS=1 |
| --- | --- | --- | --- |
| 00 / 3/8 | 1,4,6 | 0,4,6 | 1,5,7 |
| 10 / 4/8 | 1,3,5,7 | 0,2,4,6 | 1,3,5,7 |
| 20 / 5/8 | 0,2,4,5,7 | 0,2,2,4,6 | 1,3,3,5,7 |
| 30 / 6/8 | 0,2,3,4,6,7 | 0,2,2,4,4,6 | 1,3,3,5,5,7 |
| 40 / 7/8 | 0,1,2,4,5,6,7 | 0,0,2,2,4,4,6 | 1,1,3,3,5,5,7 |
| 50 / 8/8 | 0,1,2,3,4,5,6,7 | same | same |
| 60 / 9/8 | 0,1,2,3,3,4,5,6,7 | same | same |
| 70 / 10/8 | 0,1,2,2,3,4,5,5,6,7 | same | same |

[Native framebuffer tests](../tests/vdp1_sampling.rs) transcribe all three panels
as literal tables. They do not call the sampler to compute expectations. Tests
exercise both destination directions, each command flip, both command kinds,
all six formats with independently authored packed words/LUT colors, and
literal differing-span distorted rows. Separate cases keep transparency,
end codes, mesh, Gouraud and pre/user clipping meaningful.

## Integer generalization and integration

Let `D = max(abs(Bx-Ax), abs(By-Ay)) + 1` be the existing inclusive raster-line
length, `W` the command source width and `n` its original traversal index.
For HSS=1 with `D<W`, first form a source of length `S=W/2`; otherwise `S=W`.
All legal widths are multiples of eight, so halving is exact.

The selected index in this source is:

- Reduction (`S>D`): `floor((2*n+1)*S/(2*D))`.
- Unchanged size or expansion (`S<=D`, `D>1`): nearest endpoint interpolation
  `n*(S-1)/(D-1)`, with exact half ties selecting the earlier source index.
  Integer form: `floor((2*n*(S-1)+D-2)/(2*(D-1)))`.
- The degenerate `S=D=1` would select zero; legal command widths and the
  reduced single-destination case instead take the reduction branch.

This is an **inferred generalization**, consistent with every illustrated row,
including the half-width HSS intermediate source. It is not evidence of Sega's
internal arithmetic or exhaustive silicon behavior at unillustrated ratios.
For example, centered source-bin selection alone gets enlargement ties wrong;
endpoint truncation gets both enlargement and reduction wrong. Literal extensions
check widths 16, 24 and the maximum 504, stronger reduction, a single dot and
2× enlargement. Physical rounding at other unillustrated ratios remains
unmeasured; this is recorded as uncertainty, not a waived acceptance test.

A command horizontal flip reverses the index within the chosen source, then
HSS maps it back as `2*index+EOS`. Thus EOS still names original source parity.
Geometry reversal places the resulting sequence at reversed destination
coordinates. Pre-clipping traversal reversal evaluates `D-1-n` in the original
line, preserving sample phase and the original denominator. Clipping never
rescales the remaining visible portion. Vertical coordinates and shading
interpolation retain their existing rules.

[`texture_coordinates`](../src/vdp1.rs) supplies both `Drawing::texture_address`
and `draw_dot`. The selected physical word must complete its VRAM acquisition
before framebuffer publication. Existing aligned nibble/byte/RGB extraction,
lookup/shade tables, retained-word reuse, bus ownership, destination coverage,
command progression and cancellation remain in their original paths. There is
no new mutable sampling state or debugger-driven drawing.

The existing two-clock command/texture-word acquisition and framebuffer
publication conventions are inherited emulator timing, not timing established
by Figure 6.5. This change neither reads every discarded source texel nor claims
to model the hardware's full source-walk/end-code discovery bandwidth. The
existing end-code processor acts on acquired samples; exhaustive skipped-texel
end-code timing and vertical/coverage accuracy remain separate #17 work. HSS's
explicit end-code-disable rule is preserved for reduction, 1× and enlargement.

Actual validation, guest artifacts and remaining umbrella scope are recorded in
[Issue #57 validation](validation-issue-57.md).
