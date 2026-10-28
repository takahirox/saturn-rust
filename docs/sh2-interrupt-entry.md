# SH-2 interrupt entry (Issue #81)

Related to #17. Phase table recorded before implementation, using Hitachi
[SH7604 Hardware Manual Rev. 4.0](https://antime.kapsi.fi/sega/files/sh7604.pdf)
(H), printed pp. 83–85, 97–103, §§5.2.3, 5.3.8, 5.4–5.7.
The inherited `table5.9` citation is incorrect for this revision: response time
is **Table 5.8**, with the pipeline in Figure 5.7.

| Phase | Main-clock interval relative to acceptance A | Evidence / effect |
| --- | --- | --- |
| Request detection | R | Source edge/level, independent of CPU acceptance |
| Recognition | R..R+2 (NMI), R..R+5 (peripheral/IRL) | Table 5.8; overlaps an executing sequence |
| CPU acceptance | A >= recognition completion | §5.4; next eligible instruction boundary; delay slot/protected successor finishes first |
| Decode | A..A+1 | Figure 5.7 D; replaced instruction does not retire |
| NMI request clear | A+1 | §5.7(2), Figure 5.10: first E after D; edges before clear coalesce, later edges survive |
| Internal E,E | A+1..A+3 | Figure 5.7 |
| SR push | A+3..A+3+m1 | Figure 5.5, old SR at SP-4 |
| PC push | next m2 states | Figure 5.5, return PC at SP-8 |
| Mask / vector-number selection | next internal E | §5.4 steps 6–7; accepted level becomes SR.I |
| External vector acquisition | after stacking, before vector-table read, when VECMD=1 | §5.2.3/Figure 5.4: accepted level on A3–A0, CS inactive, WAIT sampled, programmable waits inapplicable |
| Vector-table read | next m3 states | VBR + vector*4 |
| Internal E | next 1 state | Figure 5.7 |
| First handler IF | A+5+m1+m2+m3, plus external acquisition if applicable | Figure 5.7; IF duration is m4, excluded from response time |

For ideal single-state memory, autovector response is 10 states from NMI
detection or 13 from peripheral/IRL detection (X=0). Saturn's ordinary external
cycles need at least two states and may wait for BSC, DMA, peer CPU and refresh.
The ideal memory example is not a Saturn bus latency claim. External acquisition
uses a two-state T1/T2 cycle with no asserted external WAIT endpoint in this
board model. The reference pipeline does not specify an extra external-vector
cycle's overlap with internal E; serializing it after that E is a deterministic
convention. Equal-edge ordering is completion, device signals, subsequent issue;
this is a scheduler convention, not measured pin timing. Architectural CPU state
is published at completion, while completed bus effects remain visible during
entry, following the existing replay boundary.

## SCU evidence and limits

[ST-097-R5](https://antime.kapsi.fi/sega/files/ST-097-R5-072694.pdf)
§2.2/Table 2.1 establishes the source/level/vector map; §3.5 establishes IMS
masking and IST source bits. Its IST write description is superseded by
[ST-210 item 7](https://antime.kapsi.fi/sega/files/ST-210-110194.pdf), printed
p. 4: software must not write IST. H §5.2.3 affirmatively establishes accepted
level output during acquisition; H §5.6 permits source clearing on detecting
that acquisition. Thus acquisition must never query the latest *global* winner
to identify the accepted level. Only a source at the accepted level can supply
its vector. Unrelated levels must remain pending.

These references do **not** affirmatively specify the SCU's arbitration among
equal-level sources, its exact source-clear edge, or acknowledgement in SH-2
autovector mode (which has no IVECF cycle). Ascending-vector tie order and
autovector source acknowledgement are inherited deterministic board conventions,
not established electrical behavior. They must be identified as such in validation
and conformance records. Slave external-vector wiring remains unestablished and
is not synthesized. No BIOS or commercial compatibility claim follows.
