# Shared BIOS initialization and no-media CD startup

Related to #113 and #17. This contract reconstructs shared device behavior from
primary references and original guests. It does not depend on the unpublished
prototype, a proprietary firmware image, a firmware PC/hash, or host-written
acknowledgements. See the [validation record](validation-issue-113.md).

## Source contract and compatibility decisions

| Finding | Primary contract | Implementation and evidence boundary |
| --- | --- | --- |
| SCU IST `+A4` | [System Program reference, SYS_SETSCUIM/SYS_CHGSCUIM](https://www.infochunk.com/saturn/segahtml_en/prgg/sysg/prog/hon/p02_11.htm) explicitly requires status stores. [ST-210](https://antime.kapsi.fi/sega/files/ST-210-110194.pdf) #7 warns against them because interrupts can disappear. | Longword zero-clear/one-preserve pending bits; masks, DMA and A-bus acknowledgement remain separate. The functional scheduler serializes stores and later edges; simultaneous electrical set/clear priority is unknown. A software prohibition does not establish a CPU bus fault. |
| SCU DSTA `+7C` | [ST-097](https://antime.kapsi.fi/sega/files/ST-097-R5-072694.pdf), §3.2 fig.3.13, pp.48–50 assigns activity bits 4/8/12/0 to DMA0/1/2/DSP. ST-210 #13 removes DSTA. | Explicit legacy compatibility subset: accepted pending CPU-level/DSP transfers keep their activity bits until actual completion. No reconstructed wait/break/bus-access flags or measured production readback. Writes, wrong widths, and adjacent removed ports still fail. DSTP's existing policy is preserved. |
| VDP2 initialization | [§3.1 VRSIZE](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p03_10.htm) defines size bit15 and read-only version bits3–0, initially zero. [§16](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p16_10.htm) identifies TVSTAT/HCNT/VCNT as read-only and `+0C/+FE` as reserved. | Base 4 Mbit/version0; bit15=1 is explicitly unsupported before mutation. Ignore stores at `+04/+06/+08/+0A/+0C/+FE`; preserve counters/latches and neighboring live words. Reserved readback zero and inert prohibited stores are compatibility policies, not claims about silicon/open bus. |
| SMPC overlap | [ST-169 §2.2](https://www.infochunk.com/saturn/segahtml_en/hard/smpc/hon/p02_20.htm) requires SF synchronization and prohibits double issuance. Category B has no input parameters. §§2.4/3 define status publication and peripheral continuation. | A **compatibility hypothesis**, independently implemented and tested: one category-B control may wait behind active INTBACK status collection only. This is not evidence of a hardware mailbox. See the state contract below. All other busy transactions retain diagnostics. |
| SCU DSP X field | ST-097 §4.5, X-bus instruction formats (pp.106–110) separately encode RX transfer and P write/selection. | For unused modes001/101, P write-enable is clear: retain P and execute ALU/RX/Y/D1/postincrements normally. This is an explicit inference for unused combinations, not a documented legal encoding. Active RX still conflicts with a DMA-owned RAM bank; dormant selectors do not select RAM. Genuine illegal classes remain errors. |
| SCSP unused words | [ST-077](https://antime.kapsi.fi/sega/files/ST-077-R2-052594.pdf), §4.1 figs.4.2/4.5 maps unused slot words and the MADRS–MPRO gap. | Ignore stores/read zero only at each slot's `+18..+1E` and `+7C0..+7FE`. Undefined readback is a deterministic policy. Live slots, coefficients, MADRS, MPRO, bus timing and other fault regions retain their behavior. The 68000 computes and writes its own acknowledgement. |
| CD no-media queries | [ST-162-062094](https://antime.kapsi.fi/sega/files/ST-162-062094.pdf), §8.2.1 functions1.5/1.6 (pp.76–77) specifies WAIT in OPEN/NODISC for TOC/session queries. §3.3 defines valid WAIT drive reports. | Correct the previous REJECT to WAIT: `87FF FFFF FFFF FFFF` for NODISC; `86FF FFFF FFFF FFFF` for OPEN. No DATATRNS/DRDY or invented TOC/session data. This is independently established; it does not identify the private BIOS's missing transaction. |

## SMPC state contract

The mailbox admits `02/03/06/07/08/09/19/1A` only while an INTBACK status
request has a real pending deadline and no peripheral/continue/VBlank phase.
It rejects a second queued control, parameterized commands, reset/clock/NMI
commands and other overlaps. Issuer checks happen first, so master-only commands
remain prohibited on the slave. IREG stays protected throughout the active and
queued work. SF and the original status deadline are unchanged on admission.

At the original status deadline, publish real status and the SMPC IRQ. Cancel
only the unused peripheral continuation, start the retained control with its
normal full 807 clocks (1075 for CDON/OFF), and keep SF asserted until it finishes.
Its effects still use the ordinary completion path, including CPU/sound/CD state;
this does not acknowledge a sound driver or manufacture a control result. Expose
the occupied slot in the passive native command snapshot. Cold/system reset
cancels it. Bus advancement splits at each deadline, independent of host chunks.
The hypothesis is bounded and reviewable, but electrical overwrite/queue behavior
for software violating double-issue restrictions remains unmeasured.

## Reconstructed no-media sequence and completion bound

The original reset-vector guest performs the following complete **software
initialization** sequence, with the tray closed and no medium. Soft reset is
explicitly not a mechanical drive command (ST-162 §4.1). It does not depend on a
mechanical tray close, media authentication, private BIOS logic or a fabricated
disc. Existing mechanical drive approximations remain separate.

1. Acknowledge CMOK/ESEL, enable CMOK/SCDQ interrupts, arm SCU A-bus acceptance,
   and issue `0401 0000 0000 0000` through word stores to CR1–CR4.
2. Wait for CMOK; observe ESEL and `07FF FFFF FFFF FFFF`. Software reset ends
   through the ordinary command path. ESEL posts reset completion (§3.2/1.7).
3. Get hardware information, TOC, session and status. The TOC/session results
   are WAIT with valid NODISC reports, not invalid-command responses. No
   transfer is started. Record the actual SCU A-bus pending source.
4. Consume all response words, including an architectural CR4 read. Acknowledge
   SCDQ and await two successive periodic reports: `27FF FFFF FFFF FFFF`.
   NODISC is stable, with no pending command, seek, play, DMA or FIFO data.

The existing deterministic command deadline is **1075 reference clocks**.
ST-162 §3.2 discusses a prescribed response timeout without assigning a numeric
value on that page. The 1075-clock latency is an inherited functional convention,
not a derived hardware reset-time bound or a measurement of reset electronics. Each of the five commands completes
at its deadline. A stopped report arrives at the next 60 Hz deadline, at most
**447902 reference clocks** after response release; two report deadlines add at
most 895804 clocks. Thus the device-only five-command/two-report envelope is
**901179 clocks**, plus real guest bus/instruction overhead. This is not a new
arbitrary timeout and does not force readiness. No-disc status is already the
block's valid closed-empty state. The actual defect corrected here is the
OPEN/NODISC query handshake, not an inferred firmware-specific checking timer.

A pending command suppresses periodic replacement. After completion the unread
response remains protected until **guest** CR4 consumption. HIRQ acknowledgement
alone does not release it; periodic SCDQ can still rise. Debugger/Wasm `read_word`
previews CR4 without releasing it. Tests inspect 1074/1075 edges, release versus
preview, repeated resets, power cancellation and actual media replacement.

## Original fixture and independent controls

[Builder](../tests/fixtures/build_bios_initialization.py),
[SH-2 source](../tests/fixtures/bios-initialization.s),
[encoded ROM](../tests/fixtures/bios-initialization.hex),
[literal records](../tests/fixtures/bios-initialization.tsv), and
[MIT license](../tests/fixtures/LICENSE) are public, original source; the builder
also authors the independent 68000 loop and SCU DSP program. Reproduce with:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tests/fixtures/build_bios_initialization.py --check
```

Expanded 524288-byte ROM SHA-256:
`7fca56c6c1d70f095d872ef59195da8498bccf70962b6cb540158ada81d5d513`.

The guest writes 62 longword records at `06020000`, progress at `06020FF8`,
a pending-status marker at `06020FF4` and completion at `06020FFC`. It exercises
IST preserve/clear/reassertion from real raster edges, VDP2 initialization,
SETTIME/status/control sequencing, real 4096-byte SCU DMA data/IRQ, DSP
multiplication/arithmetic and unused parallel fields. External CPU bus work
waits behind the DMA burst, so its guest DSTA snapshot is already idle;
independent endpoint tests cover retained activity throughout the actual beats.
The 68000 clears all 512 slot words and 32 gap words, counts 544 completed
stores and publishes its own sound-RAM acknowledgement. The final back-screen
color comes from the guest's DSP result, not a host framebuffer.

Native Machine and the required `externalStartup` production Wasm group run
identical ROM bytes and literal records, cold boots, advancement chunks
1/127/10000, passive observation, reset during active status and two controls:
changing the input from 7 to 9 changes DSP 84 to 108 and Canvas color; omitting
SNDON prevents the sound acknowledgement and whole-guest completion. Inherited
BIOS-only UI tests cover local-file Start, Pause/Resume/Restart/Reset and input;
inherited media guests retain their changed/omitted transaction controls.
The existing startup ROM now detects its no-media control by the documented
NODISC WAIT response, preserving its no-application/no-HLE outcome.

The endpoint tests retain unsupported widths, issuers, full slots, adjacent
ports, reset domains and genuine illegal DSP operations. SCSP's prefetch-fault
control moves from newly supported unused slot `100018` to unsupported `100430`;
it still verifies halted reset prefetch with no stack writes or recursion.
