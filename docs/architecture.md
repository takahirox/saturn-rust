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
media or fallback. `step()` retires one instruction; `run_steps(n)` accepts
1–10000. `advance_frame()` performs 60000 instructions with TVSTAT VBLANK clear,
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

## Supported SH-2 instructions

| Family | Encodings and behavior |
| --- | --- |
| NOP, RTS | 0009 / 000B; RTS transfers to latched PR after one slot |
| MOV immediate, register | Enii sign-extends imm8; 6nm3 copies 32 bits |
| MOV.B/W/L @Rm,Rn | 6nm0/1/2; byte/word sign-extend |
| MOV.B/W/L Rm,@Rn | 2nm0/1/2; truncate to access width |
| MOV.L @(disp,PC),Rn | Dndd; aligned architectural PC + unsigned disp8 × 4; a slot uses branch destination + 2 as architectural PC |
| ADD immediate/register, SUB | 7nii / 3nmC / 3nm8; wrapping arithmetic, no SR change |
| CMP/EQ, CMP/HS | 3nm0 / 3nm2; equality and unsigned greater-or-equal set T |
| TST #imm,R0 | C8ii; unsigned immediate AND, sets T when zero |
| DT | 4n10; wrapping decrement, sets T when result is zero |
| SHLL, SHLL2, SHLL8 | 4n00/08/18; SHLL sets T to old bit31, others preserve T |
| BT/BF | 89dd/8Bdd; signed disp8 × 2 + instruction address + 4, no slot |
| BRA/BSR | Addd/Bddd; signed disp12 × 2 + address + 4, one slot; BSR sets PR=address+4 |
| JMP @Rn | 4n2B; latch target before one slot |

Only specified instructions modify T; all other SR bits are preserved. Branches
in a slot fault before side effects. PC denotes the next fetch, including the
pending slot. Unsupported instructions, unaligned/unmapped accesses and unsupported
MMIO return explicit errors with the faulting PC; no instruction retires and
execution stays faulted until reset. The game masks interrupts and uses no
exception dispatch, interrupt entry, SLEEP, cache, SH-2 on-chip timers or slave
CPU. Those remain unsupported, rather than being approximated by game callbacks.
Tests include manual-derived encodings, signed immediates/displacements, unsigned
comparisons, wraparound/flags, call/jump target latching, PC-relative slot rules,
load sign extension, data/fetch faults and the prior six-instruction trace.

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
undefined registers. It never loads an HLE game. Real BIOSes commonly fault on
unsupported instructions/devices; this mode is useful for reset-vector/debugging
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

- [SH-1/SH-2 Programming Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/h12p0.pdf), §6 instruction descriptions: ADD, BF/BT, BRA/BSR, CMP/EQ/HS, DT, JMP, MOV, MOV immediate/PC-relative, NOP, RTS, SHLL/SHLL2/SHLL8, SUB and TST. Opcode/flag/delay semantics are tested in cpu.rs.
- [SH7604 Hardware Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf), §§2.1–2.3 (registers, alignment, address spaces), §4.1.3 table 4.3 and §4.2.2 (power-on vectors/reset).
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

Broader CPU instructions and exception/interrupt dispatch, accurate timers and
cycles, slave SH-2, SCU DMA/DSP/interrupts, SMPC commands/INTBACK, additional VDP2
modes, VDP1, CD-block runtime reads, BIOS services and native BIOS startup remain
future compatibility work. Audio (68EC000/SCSP) is absent; this build is silent.
Reference-emulator/hardware comparison, Gamepad API, sound and performance
measurements were optional and have not been performed. No dynarec, workers,
server backend or general-game compatibility is introduced.
