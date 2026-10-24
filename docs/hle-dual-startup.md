# HLE dual-CPU interrupt startup contract

Related to #49 and #17. This contract covers HLE capture communication and the
supported slave raster route, not complete BIOS initialization or a Sega ROM
register dump. External BIOS boot and architectural hardware-reset defaults
remain separate.

The [System Program guide's interrupt initialization table][SYS] assigns both
processors' FRT capture vector `64H`, initial priority 15, and priority zero to
other internal groups. [STN-28 attachment, Dual CPU Usage Ver.1.1][DUAL]
§§2.3/4.3–4.5/5 establishes slave raster vectors, entry registration, capture
initialization, retained RAM vectors and restart after clock changes.
[SH7604][H] §§5.3.1–5.3.8/5.4 and §§11.2.4–11.2.5/11.4–11.5 define encodings,
access widths, masking and acknowledgements.

| CPU-local register | Access and HLE value at application entry | Derivation |
| --- | --- | --- |
| TIER `FFFFFE10` | Byte `81` on both CPUs | ICIE bit7 enables capture; compare/overflow enables remain zero; reserved bit0 reads/writes one (§11.2.4) |
| FTCSR `FFFFFE11` | Byte; ICF bit7 clear | Read before writing zero clears observed flags; software cannot assert ICF (§11.2.5) |
| IPRA `FFFFFEE2` | Word `0000` on both CPUs | DIVU, DMAC, WDT/BSC priorities disabled (§5.3.1) |
| IPRB `FFFFFE60` | Word `0F00` on both CPUs | SCI priority zero in bits15–12; FRT priority15 in bits11–8 (§5.3.2, table5.5) |
| VCRC `FFFFFE66` | Word `6465` on both CPUs | Capture vector64 in bits14–8; compare vector65 in bits6–0 (§5.3.6) |
| VCRD `FFFFFE68` | Word `6600` on both CPUs | Overflow vector66 in bits14–8; lower byte reserved (§5.3.7) |
| ICR `FFFFFEE0` | Word write `0001` master, `0000` slave | Master uses board-supplied SCU vectors; slave uses SH-2 autovectors (§5.3.8). NMIL is a read-only input, so normal readbacks are `8001` / `8000` |
| VBR / SP | Master `06000000` / IP master stack; slave `06000400` / IP slave stack | Existing ST-040 metadata contract; zero stack fields select `06002000` / `06001000` |

FRT priority is shared by capture/compare/overflow. TIER enables only capture,
so assigning vectors65/66 does not enable their requests. Other internal vector
registers retain existing values; this increment does not initialize every
module described in the system table.

[The original trampoline](../src/hle/boot.s) runs with SR.I=15. It first disables
FRT requests (TIER=01), sets priorities and FRT vector fields, reads then clears
FTCSR, and enables capture (TIER=81). It then distinguishes CPUs through BCR1,
selects each ICR mode and VBR, selects the slave's metadata stack, and transfers
to the AIP or registered slave entry. No instruction unmasks SR or emits a
communication strobe. Initialization therefore produces no phantom capture IRQ.
The master path retires 67 instructions before AIP, compared with the inherited
23. These are real guest instructions and their normal access/execution costs.

Slave autovector selection is the HLE configuration used to supply the specified
IRL2/vector41 and IRL6/vector43 raster interrupts. It is not a claim about the
exact ICR contents of Sega firmware. The existing board route supports normal
autovectors; slave VECMD=1 board-vector acquisition is still unimplemented. No
D7–D0 acquisition cycle is invented. Raster levels, SR masking, re-entry and SCU
independence retain the [#47 contract](validation-issue-47.md).

## Registration and reconciliation

The older [ST-202-R1][OLD], printed p.3 §4.3, gives entry index164 and §4.4 gives
capture level1. Use the later attachment's §4.3 index94 and §4.4 level15,
corroborated by the System Program initialization table. Even the older guide's
printed p.6 example uses entry index94. These are distinct operations:

- Master `SYS_SETSINT(0x94, entry)` writes `06000000 + 0x94*4 = 06000250`.
  This is the post-initialization slave entry, not an interrupt handler.
- Master `SYS_SETSINT(0x164, handler)` writes `06000000 + 0x164*4 = 06000590`,
  the slave VBR-relative capture vector64. A slave registers its own handler
  with index64. Master capture also uses index64 in the master's table.
- Slave HBlank/VBlank handlers are indexes41/43 locally or141/143 from master.

The attachment's §5 repeats index164 for capture-handler registration, which is
consistent with this mapping. Its translated communication addresses contain
extra/missing zeroes; the older guide §§1.2/5.1/6.1 and its revision history
establish word strobes at `21000000` (slave) and `21800000` (master). Strobe data
is arbitrary; payload travels through shared memory. Guest acknowledgements
must read FTCSR with ICF set, then write ICF zero; interrupt acceptance and RTE
do not clear it. SR.I=15 retains pending capture; SR.I=14 permits level15 while
masking slave raster inputs.

## Restart and coherence

[Installation](../src/hle.rs) populates original ROM, RAM vectors and services
only on HLE disc cold/system reset. SSHOFF resets slave CPU-local peripherals;
SSHON executes the trampoline again, including the FRT configuration and
metadata stack, without reinstalling RAM tables. Both clock selections hold
the slave in reset; the application restarts it through SSHON. Full vector bytes,
application handler registrations and shared SDRAM survive these slave/clock
restarts. SYSRES follows the existing volatile-clear/reload contract, with
power-clear bytes and battery RAM retained; cold boot clears power-clear bytes.

The instruction cache and metadata behavior are unchanged. The diagnostic
communicates and polls through P1 aliases; shared readers cannot assume cache
snooping. It registers vectors before startup and uses actual guest cache reads
to warm the raster handler. Its master uses both all-masked and all-enabled
SCU mask operands with SR.I=15, and slave raster handling retains master IST
H/V IN status. Hosts only load media, advance time and read state.

[Independent fixtures and validation](validation-issue-49.md) cover literal
entry values, both directions of capture, SLEEP/exception frames/acknowledgement/
RTE, subsequent calculations, both clock selections, retained bytes, default and
custom slave stacks, raster controls, SYSRES and negative inherited-bootstrap
experiments. Other #17 hardware and full BIOS/CD-library requirements remain
mandatory; commercial BIOS/game compatibility is unverified.

[SYS]: https://www.infochunk.com/saturn/segahtml_en/prgg/sysg/prog/hon/p01_10.htm
[DUAL]: https://www.infochunk.com/saturn/segahtml_en/info/hon/stn28_1.htm
[OLD]: https://antime.kapsi.fi/sega/files/ST-202-R1-120994.pdf
[H]: https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf
