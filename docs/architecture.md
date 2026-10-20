# Architecture and compatibility

This original Rust implementation now executes **Star Collector**, a silent,
original Saturn-targeted SH-2 homebrew disc, through a documented BIOS-less HLE
bootstrap. This is one small hardware subset, not general Saturn or commercial
game compatibility. External Sega BIOS boot remains incomplete. No production
playability claim is made before post-merge Pages verification.

## Component boundaries

| Component | Responsibility |
| --- | --- |
| src/cpu.rs | Independent SH-2 interpreter, architectural registers, delayed branches and sticky faults |
| src/bus.rs | Aligned big-endian byte/word/long accesses, RAM, ROM, VRAM and MMIO routing |
| src/disc.rs | Bounded cooked ISO9660 and Saturn IP validation, first-read executable selection |
| src/devices.rs | VDP2 bitmap rasterization and SMPC digital-pad electrical inputs |
| src/lib.rs | Explicit cold-boot modes, CPU/device ownership, bounded deterministic frame scheduling |
| src/wasm.rs | Thin Wasm adapter; no gameplay or browser APIs |
| web/app.js | Local file handling, keyboard levels, animation scheduling and RGBA framebuffer copy to Canvas |
| homebrew/star-collector | Buildable MIT-licensed SH-2 guest and reproducible ISO builder |

`Cpu::step(&mut impl Memory)` owns no bus pointers. `Machine::reset(Boot)` clears
RAM, devices, ROM, counters, delay state and faults before attempting the selected
boot. Invalid input leaves that mode selected and CPU in Reset, with no stale
media or fallback. `step()` makes one bounded CPU step; exception entry or a
sleeping step can retire no instructions. `run_steps(n)` accepts 1–10000 steps.
`advance_frame()` performs 60000 steps with TVSTAT VBLANK clear,
then 20000 with VBLANK set. Every frame has a bounded CPU budget and aborts on
its first fault. This is a deterministic polling reference, not cycle-accurate
Saturn timing, 60 Hz hardware execution, raster interrupts or bus arbitration.
The browser advances at most one frame per requestAnimationFrame callback,
limited to 60 updates per second; it does no catch-up loop. Actual speed depends
on the browser. The guest waits for a VBLANK edge, so holding a key changes state
once per guest update after initialization.

The guest controls every pixel and game rule. Rust converts VDP2 VRAM into RGBA;
JS only uses putImageData. Keyboard handlers set pad button levels, never guest
RAM/registers. The emulator has no game-name/PC/opcode hook, game-loop replacement
or game-state renderer. Read-only `read_long` is a debugger/test facility.

## SH-2 reference interpreter

The interpreter decodes the 142 documented SH-2 instruction forms (53,752 valid
16-bit encodings). [The conformance matrix](hardware-conformance.md) records
instruction families, exact tests, primary references and remaining failures.
An exhaustive decoder test checks every 16-bit value against a separate,
manual-derived encoding inventory; semantic tests separately cover operands,
flags, addressing, aliases, multiplication/division, saturation and control flow.
Decoder coverage alone is not a claim of full processor conformance.

General illegal instructions dispatch vector 4; instructions that rewrite PC or
undefined opcodes in a taken delayed branch's slot dispatch vector 6. A slot
exception saves the pending branch destination (SH7604 §4.5.3). TRAPA stacks the
following PC and uses its unsigned immediate vector. Entry pushes SR, then PC,
then reads VBR + vector × 4. RTE restores PC/SR from the stack, masks reserved SR
bits, and executes its slot with the restored SR before transferring control.
Illegal instructions do not retire; TRAPA and RTE do. The retired-instruction and
exception counters are separate; the last vector/saved PC are inspectable in Debug.

Delayed targets and call return addresses latch before the slot; PC-relative slot
operands use destination + 2. BT/S and BF/S create a slot when taken; the untaken
path proceeds sequentially, following the instruction's specified operation.
Both paths inhibit IRQ/NMI acceptance until the successor executes, independently
of the pending branch target (SH7604 §4.6, Table 4.10).
SLEEP retires once, holds the next PC, and makes subsequent steps return without
fetching or retiring. It is distinct from a debugger halt. `run_steps(n)` bounds
step attempts, including non-retiring exception entry and sleeping steps.

CPU-level maskable interrupt and NMI entry APIs check the current SR mask and
defer through a pending slot or the successor of any system/control-register
transfer or untaken BT/S or BF/S. The caller must select the request/vector and
retry if deferred. A
successful entry saves the old SR/next PC, updates I, wakes SLEEP and jumps to the
handler without retiring an instruction. These APIs are tested directly, but
**Machine has no device interrupt controller or scheduler that calls them yet**.

CPU/address errors (including delayed acceptance), reset variants, cache,
SH7604 on-chip peripherals and dual-CPU integration are outstanding. Bus fetch,
data and exception-entry failures remain explicit, sticky host diagnostics.
Exception entry is bounded on a failing stack/vector access; successful writes
before a later entry failure are not rolled back. This is not hardware double-
fault emulation. The standalone instruction engine does not provide cycle timing,
bus locking/arbitration between processors, or native Sega BIOS compatibility.

## Memory and devices

| Physical range | Behavior |
| --- | --- |
| 00000000–0007FFFF | Optional read-only 512 KiB external BIOS |
| 00100000–0010007F | SMPC, only the port-1 direct registers below |
| 00200000–002FFFFF | 1 MiB low work RAM |
| 05E00000–05E7FFFF | 512 KiB VDP2 VRAM |
| 05F80000–05F8011F | VDP2, only the listed register subset |
| 06000000–060FFFFF | 1 MiB high work RAM |

P1 cache-through addresses 20000000–3FFFFFFF map to the corresponding external
physical address. No cache behavior or RAM mirrors are modeled. Words/longs
require 2/4-byte alignment and full valid ranges. Failed multi-byte writes make
no partial changes. External mode requires exactly 524288 bytes, reads initial
PC at 0 and R15 at 4, sets VBR=0, SR=F0 and deterministic zero values for otherwise
undefined registers. It never loads an HLE game. Real BIOSes still commonly fault on
unimplemented devices or bus behavior; this mode is useful for reset-vector/debugging
experiments, not a claim of Sega boot/game compatibility.

VDP2 supports TVMD=8000 (320×224), BGON=0101 (NBG0 enabled, transparency
disabled), CHCTLA=0032 (512×256 RGB555 bitmap), MPOFN=0, ZMXN0/ZMYN0=00010000
and PRINA=1. Visible pixels come from the top-left 320×224 of a 512-pixel-pitch
bitmap at VRAM bank 0. RGB word bits 0–4/5–9/10–14 become red/green/blue; bit15
is ignored in this transparency-disabled subset. Five-bit channels expand by bit
replication. Display/background disabled produces opaque black. Other enabled
configurations produce actionable unsupported-mode errors. Writes to unimplemented
registers fail rather than simulating success. CYCA0L/U are stored, but VRAM
cycle arbitration is not modeled. Register readback for implemented write-only
VDP2 controls is a debugger convenience, not hardware-accurate read behavior.
No VDP1, CRAM palettes, tilemaps, other backgrounds, scrolling, scaling, blending,
rotation, interlace or alternative resolutions are supported.

SMPC uses byte accesses: PDR1 +75, DDR1 +79, IOSEL +7D, EXLE +7F (relative to
00100000, often accessed at 20100000). DDR1=60, IOSEL1=1 and EXLE1=0 select
standard digital pad direct access. PDR1 TH/TR=11 returns pad identification/L;
01 returns Right/Left/Down/Up; 10 returns Start/A/C/B; 00 returns R/X/Y/Z.
TL is high and button bits are active-low. Unsupported direction/control modes
fail explicitly. Settling is immediate in the emulator; the guest includes a
32-iteration delay after selecting a nibble to satisfy the hardware's 2 µs wait.
Host masks are up/down/left/right/B/C/A/Start/Z/Y/X/R/L at bits 0–12. The browser
maps arrows, Z (A) and Enter (Start), releases on blur and pauses on tab hiding.
No INTBACK, SMPC commands/RTC/reset, multitaps, analog or port 2 is implemented.

## Disc and BIOS-less HLE boot

[Star Collector's build and layout](../homebrew/star-collector/README.md) specify
the supported self-contained guest contract. The parser accepts cooked
2048-byte-sector ISO files, 22 sectors to 16 MiB, with a Saturn hardware identifier,
one region, IP size 1000–8000 hex and aligned stack/first-read destination in high
RAM. It validates primary and terminator descriptors at sectors 16/17, matching
little/big-endian volume/extent values, 2048-byte blocks, root directory records,
names, bounds and contiguous single-volume extents. Unsupported extended
attributes, interleaving, multi-extent files and malformed/truncated data fail
before loading. The first root child must be a regular file (Saturn first-read
file identifier 2); its filename is not special. BIN/CUE, raw 2352-byte sectors,
compressed images, large/general ISO layouts and runtime CD access are unsupported.

HLE copies the IP system area to 06002000, loads the first-read file at the header's
F0 destination, initializes the master stack from E8 (zero means 06002000), then
executes the original AIP at 06002E20 for a single 20-hex-byte area block. The
original bootstrap jumps to the loaded executable. The IP includes no Sega
security/area executable; HLE deliberately skips those proprietary boot stages.
No arbitrary bytes are treated as a disc, no BIOS services are intercepted, and
no CD-block or SCU implementation is needed for this self-contained first target.
The Saturn header's first-read size is ignored for CD boot; the ISO file size is
used, as specified by the manual. This narrow contract does not promise that
arbitrary homebrew or stock-console authenticated discs will boot.

## Preserved debugger fixture

HLE bootstrap prototype is separate from disc boot. It writes original opcodes
at 06000000 with R15=06100000. Reset then Run 6 yields R0=4, R1=7, PC=0600000E:
MOV #5,R0; ADD #-1,R0; MOV R0,R1; BRA +1; ADD #3,R1 (slot); skipped MOV #99,R1;
NOP. Later execution loops in BRA/NOP. This is labeled a fixture, not a game.

## Sources and provenance

All CPU, memory, scheduler, disc, video and input behavior is independently written
in Rust. No third-party emulator/CPU core, reference emulator source, SDK example,
manual pseudocode, firmware or proprietary object was copied or transliterated.
The [saturn-docs archive](https://github.com/luckyseoul/saturn-docs) was used to
locate the primary manuals below; synthesized emulator-reference notes were not
used as implementation code. Manual section/page references use printed pages.

- [SH-1/SH-2 Programming Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/h12p0.pdf), §5 encoding tables and §6 SH-2 instruction descriptions (all architectural forms). Opcode/flag/delay semantics are tested in src/cpu/tests.rs; the independent encoding inventory is tests/fixtures/sh2-encodings.txt. See the conformance matrix for section-specific coverage and discrepancies.
- [SH7604 Hardware Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf), §§2.1–2.3 (registers, alignment, address spaces), §4.1.3 table 4.3 and §4.2.2 (power-on vectors/reset), §§4.4–4.7 (interrupts, instruction exceptions, deferral, stack layout), §14.3 (sleep).
- [Saturn Overview](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-103-R1-040194.pdf), hardware configuration/address map and subsystem roles.
- [VDP2 User's Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-058-R2-060194.pdf), §2.2 pp.18–23 TVMD/TVSTAT; §3 pp.29–40 VRAM/cycle patterns; §4 pp.48–63 BGON, bitmap, RGB555 and CHCTLA; §4 map offset; §5 coordinate increment; §10 priority. Register addresses in the manual are relative to VDP2 VRAM origin 05E00000.
- [SMPC User's Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-169-R1-072694.pdf), §1.2 pp.5–8 byte-only register map; §3.1 pp.74–75 direct access; §3.4 table 3.36 p.97 digital-pad TH/TR protocol.
- [Disc Format Standards](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-040-R4-051795.pdf), §§3–4, especially pp.25–31 IP fields, master stack and first-read file; ISO9660 directory record/volume layout.
- [Boot ROM User's Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-079B-R3-011895.pdf), §§1–3 startup/services. Full BIOS services are not implemented.
- [SCU User's Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-097-R5-072694.pdf) and [VDP1 User's Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-013-R3-061694.pdf) were consulted for subsystem boundaries. Neither device is needed by this target or emulated here.

The [game redistribution audit](homebrew-provenance.md) records candidate decisions,
original-game licensing and build provenance. Existing wasm-bindgen (MIT/Apache-2.0)
and Playwright (Apache-2.0) are infrastructure only. No project-wide license is
added. Manuals/firmware are not redistributed.

## Remaining milestones

Architectural address-error exceptions, hardware interrupt wiring, accurate timers
and cycles, slave SH-2, SCU DMA/DSP/interrupts, SMPC commands/INTBACK, additional VDP2
modes, VDP1, CD-block runtime reads, BIOS services and native BIOS startup remain
future compatibility work. Audio (68EC000/SCSP) is absent; this build is silent.
Reference-emulator/hardware comparison, Gamepad API and performance
measurements have not been performed. Sound is required by Issue #11 and remains
missing. No dynarec, workers,
server backend or general-game compatibility is introduced.

Issue #11 remains open. See [hardware conformance and unresolved gaps](hardware-conformance.md)
and [actual validation for this implementation](validation-issue-11.md).
