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

CPU advancement also yields at CD, SMPC and sound reference-clock deadlines.
These deadlines bound reference ticks after PLL conversion, separately from
main-clock device/recognition deadlines. At non-reference PLL rates, bulk
advancement stops before the reference event and processes that edge through
the existing single-main-tick path. Fixed devices advance once; CPU issue can
occur between two main ticks sharing that reference edge. With the slower PAL
PLL, an edge delivering no main tick still advances fixed devices and returns
to CPU scheduling. This preserves the board's deterministic cross-domain phase
convention while allowing recognition to start at the event's actual edge.

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

### Reconciled CPU/endpoint boundary

The original #81 requirement conflated a CPU bus protocol with the implementation
of its external vector generator. H Figure 5.2, printed p.84 (PDF p.100), explicitly
places the priority encoder and vector generator **outside** the SH7604; Figure
5.3 shows the separate autovector connection. H §5.3.8, p.96 (PDF p.112), defines
VECMD=0 as internal vector generation. H §5.6, p.101 (PDF p.117), permits a source
to clear upon detection of a vector-fetch cycle; it does not specify a mandatory
SCU latch implementation or clear edge. These distinctions justify reconciling
the AI-authored requirement, not treating the CPU manual as an SCU specification.

| Classification | Evidence and implementation consequence |
| --- | --- |
| Specified CPU interface | H §5.2.3/Figures 5.2–5.4, pp.83–85 (PDF pp.99–101): acquisition exposes the accepted level and reads external vector data. It is distinct from a CS memory read; WAIT applies, software waits do not. |
| Specified board mapping | ST-097-R5 §2.2/Table 2.1, p.27 (PDF p.43), maps factors to levels/vectors; §3.5/Table 3.8, pp.57–60 (PDF pp.73–76), defines IMS/IST. ST-210 item 7, p.4 (PDF p.8), supersedes software IST clearing. Neither defines equal-level order or an automatic clear edge. |
| Derived interface invariant | IRL carries a level, not a factor identity. With Table 2.1 as the vector-generator map, selection must be confined to the accepted level; a new global winner at another level cannot supply the vector or be consumed. This is a deduction from the two interfaces, not an additional quoted SCU rule. |
| Autovector boundary | Internal selection creates no IVECF endpoint transaction. The model therefore preserves SCU requests, rather than fabricating acknowledgement from private CPU acceptance metadata. This conservative boundary rule does not claim that all undocumented SCU circuitry has been characterized. |
| Unspecified endpoint details | Equal-level arbitration, the exact automatic clear edge, withdrawn requests/undriven data and response to simultaneous source changes are not established by these references. Ascending-vector arbitration, completion-edge consumption and undriven `7F` are explicit functional model choices. They remain hardware-conformance limits under #17. |

ST-097-R5 §3.6/Table 3.9, p.61 (PDF p.77), instead concerns classification-data
acquisition **from the A-Bus into the SCU** and AIACK rearming. It cannot establish
the SCU-to-SH-2 clear edge. [ST-162-R1](https://antime.kapsi.fi/sega/files/ST-162-R1-092994.pdf),
System Program §1.1, p.1 (PDF p.6), confirms hardware-set master vectors 40–5F
and separate slave autovectors. Its software priority/mask services (pp.2/4/9)
do not specify electrical arbitration; its IST writes cannot supersede ST-210.

`Scu::acquire_vector(level)` owns the functional selection and consumption
policy. The CPU journal calls it only when an external-vector transaction
completes, and records the returned value once. `InterruptSource::Scu` carries no
factor identity and internal autovector selection never calls this endpoint.
The native `autovector_entry_has_no_external_acquisition_and_preserves_scu_requests`
regression fails on the previous implementation (IST `1` instead of `5`). It checks
waited entry, a competing level, saved PC/SR, chunks 1/3/17/10000 and inspection
off/on. `endpoint_model_consumes_only_unmasked_sources_at_requested_level` uses
literal Table 2.1 source sets at every level and five masks; it asserts membership,
isolation and single-source consumption without prescribing an equal-level winner.
The separately named `scu_endpoint_convention_selects_live_lowest_same_level_vector`
tests only the model convention. No convention test proves silicon behavior.

Issue #81's reconciliation retains the CPU phase/guest/negative-oracle/full-suite
requirements and requires these boundary invariants. It does not mark the SCU
electrical limits complete, require a human result, or establish #17 completion.
Slave external-vector wiring remains unestablished and is not synthesized.
No BIOS or commercial compatibility claim follows.
