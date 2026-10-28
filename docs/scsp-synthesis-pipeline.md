# SCSP synthesis pipeline and consumer deadlines

Related to #67 and #17. Primary contract: [ST-077-R2](https://antime.kapsi.fi/sega/files/ST-077-R2-052594.pdf),
§3.2 printed pp.20–21/Figure 3.2, §4.2 pp.47/55–57/Figures
4.18/4.30–4.31, pp.83–86/Figure 4.56 and IMXL/ISEL definitions,
and §5 p.106/Figure 5.1. The [later manual](https://www.infochunk.com/saturn/segahtml_en/hard/scsp/hon/p04_fm2.htm),
Figures 4.24–4.25, independently states six intervals from OP1 to OP7
and five from OP2 to OP7.

Figure 4.18 positively places the DIRECT and DSP slot output **after** the
waveform level calculation. IMXL/ISEL route that output into MIXS, and Figure
5.1 supplies MIXS to DSP INPUTS. Thus neither consumer may see a result at
waveform-read completion: interpolation and both level intervals must finish.
This follows the output datapath, independently of the stack delay.

There is an off-by-one error in the p.56 prose snapshot: it assigns slot30
both to level calculation and stack writing when OP1 is slot3. The Figure4.30
rows instead put slot29 at OP7, consistent with Figure4.31's explicit six
intervals and the later manual's six-cycle statement. The stage table follows
those mutually consistent definitions, rather than that prose example. The
p.57 equation's A refers to the source's current/past **cycle**, as clarified
by the later manual, not to whether its slot number equals the carrier.

An interval is 1/(44100*32) seconds, or four of the 128 DRAM cycles/Fs.
Figure3.2 labels DSP access as cycles0/2 and PCM as cycles1/3. The existing
emulator numbers PCM opportunities even and DSP opportunities odd: its memory
cycle origin is rotated by one against that figure. The figure defines
alternation and priority, without fixing its cycle0 to MPRO0 or OP1 slot0.
This increment preserves that interface convention and separate read ownership;
it does not reinterpret the diagram's labels as measured absolute phases.

The following deadlines are zero-based memory steps relative to a voice's
OP1 start, not elapsed host instruction counts.

| Stage | Relative step | Retained work / consumer |
| --- | ---: | --- |
| OP1 | 0 | Slot identity, register snapshot, LFO/noise, phase/direction; advance PG/PLFO once |
| OP2 | 4 | Read FM generations, compute displaced addresses/fraction and CA |
| OP3 | 8, 10 | Separate first/second PCM grants; data complete at 9, 11; each grant retains format/address/DRAM geometry |
| OP4 | 12 | Signed interpolation, EG/ALFO; retain interpolation and envelope operands |
| OP5 | 16 | Level calculation in flight |
| OP6 | 20 | Second level interval; result remains private |
| OP7 | 24 | Completed level result enters DIRECT and IMXL/ISEL MIXS once, and sound stack once |

The manuals establish relative stages and the post-level output connection.
They do **not** specify an absolute phase against MPRO0, a CPU-visible register
latch for every stage, nor ordering of equal interface edges. This emulator
chooses OP1 slot 0 at memory step 0 / MPRO0, captures slot registers at OP1,
and publishes completed results before a DSP read or OP2 read on the same
edge. These are explicit deterministic interface conventions, not measured pin
timing. EG fetch suppression uses a preview of the retained envelope calculation;
the actual envelope state advances at OP4. Existing numerical EG slopes remain
an approximation outside #67.

Stack publication uses the existing OP2-to-OP7 five-interval queue. A completed
result replaces its pending zero immediately before that queue commits; there
is no second five-interval delay. Its generation boundary follows OP2 slot31,
not the output sample boundary. STWINH preserves both committed generations.
The current/past selector rules remain those of Table 4.16. Source distance five
is visible on the equal edge; distance four is not.

A physical output sample owns contributions **published** during its 128 steps,
not voices whose OP1 happened in that sample. Slots 26–31 therefore publish
in the following sample; slots 30–31 also acquire waveform data there. The
pipeline is bounded to seven retained slots and survives frame clearing of
DIRECT/MIXS. On a cold start it is empty; no negative-time work is fabricated.
SCSP RESET cancels all stages, grants, stack and DSP state. Sound-CPU disable
retains the SCSP domain. Idle slots still advance PG/LFO/stack on their own
edges; KEYON joins the next eligible OP1, including within an idle sample.

Independent first-generation deadlines (one-based scheduler ticks) are:
slot0 grants 9/11, interpolation 13, publication25;
slot7 grants37/39, interpolation41, publication53;
slot31 grants133/135, interpolation137, publication149. Native tests place
writes on both sides of those grants and DSP reads on both sides of publication.
These values were authored before changing production scheduling.

The inherited immediate-publication PCM oracle is corrected by these references:
its operand mutation, signed arithmetic, DRAM geometry, ownership and routing
assertions remain; only stage deadlines and cold-start sample ownership change.
