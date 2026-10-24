# SCU A-to-B DMA startup

Related to #51 and #17. This increment implements a startup interlock in the
existing hardware-time scheduler; it does not establish hardware completion.

## Primary contract

[Sega ST-210-110194, §3 item12, printed p.5](https://antime.kapsi.fi/sega/files/ST-210-110194.pdf)
gives CPU writes to either A-bus or B-bus priority over activation of a transfer
between those buses, including a continuous CPU write sequence. CPU accesses
wait after activation. Item1 (printed p.4) prohibits DMA writes to A-bus, so the
supported direction is A-to-B. Item8 (printed p.5) prohibits software CPU A/B
access during this transfer and describes a refresh hazard; this implementation
does not invent electrical hang behavior or a B-to-A transfer route.

[ST-097-R5, §2.1](https://antime.kapsi.fi/sega/files/ST-097-R5-072694.pdf)
(printed pp.16–26) defines retained transfer data, direct/indirect operation and
byte fringes. §3.2 (printed pp.41–50), especially its opening priority rule and
fig.3.6 (printed p.44), defines descending level2/1/0 priority and separate physical
B-bus words. §3.6 (printed pp.61–72) defines A-bus selects, bus widths and waits.
ST-210 supersedes obsolete portions: items9/14 remove preread/forced-quit;
items20/35 restrict concurrent use; items21/22 specify enable/start factors and
one retained trigger; item25 defines three-word descriptors in work RAM-H.
Those inherited behaviors remain unchanged.

## Pending startup and active transfer

A channel may have a requested activation, internal initialization, descriptor
acquisition, or actual data work. Descriptor cycles remain ordinary scheduled
CPU-bus accesses. Once a descriptor has been acquired (or direct parameters have
been installed), its effective source and destination determine the route.
Before granting the first A-bus data read of an A-to-B activation, the scheduler
checks both SH-2 request queues, accepted CPU write buffers and reset drains for
pending A/B writes. CPU-bus RAM and SCU register writes, SH-2 DMAC writes, DSP
DMA, and other SCU routes retain existing arbitration.

The channel records activation at the first data-read grant, before its first
physical source word completes. Subsequent source beats, separately completed
B-bus words, preemption/resumption and all descriptors of that activation keep
this state. A later CPU write cannot turn an active transfer back into startup.
Completion or cancellation clears the latch; a queued retrigger is a new
activation and must qualify again. Among eligible SCU requests, selection still
runs level2, level1, level0, then DSP. A waiting startup is ineligible; it cannot
stop an already active lower level from finishing or create blanket CPU priority.
No guarantee is added for prohibited three-channel concurrency or level2 start
during level1 operation.

The specifications do not define the electrical boundary of continuous writes,
equal-edge sampling, or exactly when descriptor fetching counts as activation.
The emulator uses this deterministic convention: writes already queued at an
arbitration opportunity qualify; a request issued before that opportunity wins
over pending startup. An already granted beat is retained. The interpreter does
not inspect future instructions to extend a continuous sequence across a gap
with no queued write. Descriptor reads precede route qualification; the whole
indirect chain retains qualification once A-to-B data starts. These are explicit
sampling conventions, not measurements of physical bus edges.

The SCU latch lives in its channel, so CPU-local reset removes that CPU's work
without resetting SCU state. PLL change cancels SCU work while retaining work RAM and CD consumption; the inherited LSI reset clears video/sound memory.
Shared system/cold reset cancels SCU grants, retained
physical units and channels with its existing volatile-memory/CD reset behavior.
No new reset domain, interrupt path, bus delay or host-time mechanism is added.

## Independent evidence

[Native vectors](../src/bus/dma_startup_tests.rs) place an accepted CPU write
behind an already granted peer RAM read. The expected publication sequence is
CPU high/low words followed by two DMA longwords, each with two separately
published destination words. Seventy-two combinations cover both SH-2s, each DMA
level separately, direct/indirect, GO/hardware trigger and sound/VDP1/VDP2 memory.
They observe each destination change, passive FIFO previews and real DataEnd
source counts, and reject an early DMA-end status. No production arbitration,
width or timing selector constructs expected values.

Additional vectors cover A-bus CPU writes, unrelated CPU/DMA routes, finite
continued writes, late CPU writes, indirect chains, level2 preemption of level0
with a retained first source word, one-deep retrigger, PLL cancellation, actual
SYSRES, CPU-local cancellation, advancement chunks 1/3/17/10000 and passive
inspection. Existing DMA, buffered-write, TAS, refresh, DSP and reset regressions
remain in place with their original expectations.

The original [MIT-licensed](../tests/fixtures/LICENSE)
[mixed-disc builder](../tests/fixtures/build_dma_startup.py) produces
[SH-2 code](../tests/fixtures/dma-startup.s), a separate
[end handler](../tests/fixtures/dma-startup-handler.s),
[data ISO](../tests/fixtures/dma-startup.iso),
[CUE](../tests/fixtures/dma-startup.cue) and a silent original audio track.
The guest reads sector FAD210, requests the real CD FIFO, arms Timer0-started
level0 A-to-B DMA, and executes 512 overlapping cached CPU bitmap stores. It
computes readback/sum records, receives the real end interrupt and reads the
DataEnd transfer count. Source data, CPU operands and the trigger enable are
changed before boot; no host code supplies completion, records or device effects.
Omitting the trigger gives zero FIFO consumption and zero handler entries.
Both [native integration](../tests/dma_startup.rs) and
[production Wasm/browser checks](../scripts/dma-startup.mjs) execute two cold boots
of each variant. Transferred palette indices produce independently expected
red/green/blue/white pixels, including actual Canvas pixels under `/saturn-rust/`.

Reproduce all fixture bytes with:

```sh
python3 tests/fixtures/build_dma_startup.py --check
```

ISO SHA-256:
`d04cf218087823c9a2d35c63b720c204f6e3bb14d575f10a2c71bf9b2749fd05`.
The builder prints hashes of every source/media artifact. It is included in the
existing CI fixture-reproducibility job. See [Issue #51 validation](validation-issue-51.md)
for actual checks and the inherited-arbiter negative controls.
