# Clock-change standby and recovery

Related to #77 and #17. This describes the functional model and its evidence,
not completion of Saturn hardware or commercial BIOS compatibility.

The primary contracts are the [System Program guide, System clock switching](https://www.infochunk.com/saturn/segahtml_en/prgg/sysg/prog/hon/p01_10.htm),
[SMPC §2.3 commands 9–10](https://www.infochunk.com/saturn/segahtml_en/hard/smpc/hon/p02_30.htm),
and [SH7604 hardware manual](https://antime.kapsi.fi/sega/files/sh7604.pdf)
§§9.2.7, 12.3.3, 14.4.1–5, table 14.3 and figures 14.1–2
(printed pages 243–244, 328, 389–393; PDF pages 258–259, 343, 404–408).

| Stage | Firmware responsibility | Hardware responsibility and ordering |
| --- | --- | --- |
| Preparation | Mask IRQs; retain caller SR, vector, SBYCR and cache control; install an ordinary NMI/RTE handler; disable cache; disable WDT counting and select its wake interval | Ordinary instruction execution, MMIO and bus admission; no recognition of service addresses |
| Command acceptance | Wait for an idle SF, set SF and write CKCHG320/352 | SMPC latches the command and schedules its existing fixed-reference 100 ms deadline; no SCU SMPC interrupt for CKCHG |
| Standby entry | Set SBYCR.SBY and execute SLEEP before the transition deadline | Table 14.3 initializes FRT/SCI and DMA controls, retains CPU/INTC/UBC/BSC and DMA addresses/counts; cancels unfinished DMA bus phases; CPU stops issuing and retiring |
| Transition | No instruction execution while in standby | At command completion select PLL/raster clock, reset VDP1/VDP2/SCU/SCSP, cancel SCU work and shut down slave; preserve master transaction journals and supported memory/CD domains |
| Wake input | No host-assisted return or mailbox writes | Board scheduler delivers one complete NMI pulse through the existing input mechanism; selected edge sets DMAOR.NMIF and starts the WDT-selected settling interval |
| Exception entry | Original SH-2 handler executes RTE with its delay slot | After settling, schedule normal vector 11 entry; architectural PC/SR stacking, handler execution and interrupt return use the CPU and transaction replay paths |
| Recovery | Restore vector/SBYCR, purge/re-enable the previous cache policy, publish SYS_GETSYSCK, re-enable HLE capture service, restore SCU IMS from SYS_GETSCUIM (including A-bus acknowledge when unmasked), issue RESENAB, restore caller SR and return | DMA remains inhibited until guest read-before-clear of NMIF and explicit channel/master enables; video/audio reprogramming remains application work |

The external synthetic firmware independently programs its vector, WDT and SBYCR,
executes SLEEP and handles NMI. It does not call HLE or share its routine bytes.
Its handler records the architectural PC/SR; subsequent instructions compare
those records with source labels and a literal SR. The HLE service temporarily
uses an empty RTE handler and restores the caller's vector afterward. R8–R14,
PR and the caller's stack balance are preserved by the service.

## Specified behavior and deterministic choices

The Sega guide establishes master standby and a post-processing NMI; SMPC
establishes reset domains and the need for firmware. Neither cited Sega page
specifies two NMI exceptions, electrical pulse width, or the CKPREQ/CKPACK wiring.
SH7604 figure 14.1 is an example using opposite edges, not evidence that Saturn
requires that entry sequence. We therefore do not add a speculative entry NMI
or synthesize an interrupt handler that executes SLEEP on the host. One pulse at
command completion is the explicit board convention. NMIE chooses its detected
edge; RESDISA does not suppress this clock-completion event. A raw command without
firmware preparation still generates NMI, but does not manufacture standby or
initialize master peripherals.

The existing PLL model changes frequency atomically at the SMPC completion edge;
its unspecified analog phases are not electrically simulated. Fixed-reference
CD/RTC/audio progress continues while master peripherals are stopped. The WDT
wake delay is measured in main clocks after selecting the new PLL, with existing
NTSC/PAL conversion. The original firmware selects CKS=6 (1,048,576 clocks), a
conservative interval above the manual's crystal-oscillator 10 ms minimum. This
makes its total service time longer than the Sega guide's approximate 110 ms;
that difference is an explicit timing convention, not a measured Saturn BIOS
latency. All eight WDT choices have exact before/at wake vectors. Maskable IRQs
and DMA-error exception entry cannot bypass standby or its settling interval.

Completed bus effects remain completed. Software standby discards unfinished
DMA phases and buffered DMA writes when it initializes CHCR/DMAOR; it does not
complete an abandoned destination transfer upon later re-enable. This is an
explicit functional cancellation convention for standby, distinct from NMI's
specified drain of the accepted DMA transfer unit (§9.2.7), already tested by
`nmi_completes_the_accepted_unit_but_dme_abort_never_issues_another_cycle`.
A clock command alone preserves an outstanding master CPU read/write journal;
NMI entry waits for the current architectural boundary. SCU reset and slave OFF
cancel their unfinished transactions using their existing domain mechanisms.

Table 14.3 gives DIVU undefined contents; the existing deterministic reset policy
retains its operands and cancels pending calculation. Its DMAC summary and
§9.2.5 differ over vector contents: vector registers retain their deterministic
value, without promising a defined post-standby hardware value. Standby preserves
INTC configuration; HLE's write of TIER=0x81 after wake deliberately re-enables
its boot capture service. That is firmware recovery, whereas hardware initializes
TIER to 1. SCI remains initialized until the application configures it. WDT's
counter/status initialize while its clock selection survives; its counter is
zero again at the end of settling.

SMPC commands 9–10 explicitly retain Work RAM. The System Program guide places
SDRAM among unaffected devices and DRAM among non-guaranteed contents. Those
labels do not establish a new low-WRAM clearing rule. Both supported work-RAM
banks retain bytes, as do backup and CD media/register state. Video and sound
memory use the existing deterministic zero-on-domain-reset policy. Physical DRAM
refresh/decay and broader power/reset electrical behavior remain unverified.

## Independent evidence

- `src/bus/clock_tests.rs`: no master retirement or new transaction during
  standby; fixed-clock progress; both TV standards and clock selections;
  actual stacked PC/SR, vector, RTE, WDT wake, pending master reads/writes;
  internal/external DMA phase cancellation and restart; RTC/RAM/CD retention;
  cold boot during standby and pending wake.
- `src/onchip/peripheral_tests.rs`: all eight WDT choices across chunks
  1/3/17/10000, plus existing table 14.3 retention and reset-domain vectors.
- `tests/clock.rs` and original MIT `build_clock.py`: shared native/Wasm bytes,
  literal register records, saved-PC/SR comparisons, NMIF inhibition and restart,
  changed clock/data operands, omitted standby and omitted NMIF-clear controls,
  repeated cold boots and chunks 1/3/17/10000 with passive inspection on/off.
- `scripts/browser-smoke.mjs` clock group: NTSC/PAL, both firmware paths, repeated
  boots, matching controls and guest records, actual production Canvas pixels
  and constant nonzero stereo PCM (4096 per channel). Existing responsiveness,
  focus/input, local-file privacy and three demonstration checks remain required.
- Existing SCU startup vectors cover cancellation before source acquisition,
  after partial source reads, between destination words and after completion,
  across chunks and inspection modes; slave raster vectors retain pending-entry
  cancellation and restart coverage.

The old direct `Bus::change_system_clock` test still expects a running master
FRC: it deliberately exercises only the PLL/device-reset domain without executing
SLEEP. Clearing FRC there would incorrectly make firmware preparation a host-side
side effect. The new executing-guest oracle checks FRT initialization on real
standby entry instead. HLE dual-CPU expectations add exactly two master NMI
exceptions (5 to 7); a synthetic slave-raster board supplies vector 11/RTE so
its original slave assertions remain meaningful.
