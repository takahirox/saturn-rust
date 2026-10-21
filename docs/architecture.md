# Architecture and compatibility

This original Rust implementation now executes **Star Collector**, a silent,
original Saturn-targeted SH-2 homebrew disc, through a documented BIOS-less HLE
bootstrap. This is one small hardware subset, not general Saturn or commercial
game compatibility. External Sega BIOS boot remains incomplete. Acceptance uses
automated local/CI checks; live Pages inspection is not a completion gate.

## Component boundaries

| Component | Responsibility |
| --- | --- |
| src/cpu.rs | Independent SH-2 interpreter, architectural registers, delayed branches and sticky faults |
| src/bus.rs | Aligned big-endian byte/word/long accesses, RAM, ROM, VRAM and MMIO routing |
| src/backup.rs | Packed internal battery RAM, checked import and odd-byte access |
| src/disc.rs | Bounded cooked ISO9660 and Saturn IP validation, first-read executable selection |
| src/devices.rs | VDP2 bitmap rasterization |
| src/m68k.rs | Original MC68EC000 integer interpreter, addressing and exceptions |
| src/sound.rs | Shared sound RAM, direct PCM SCSP, independent 68k/sample clocks |
| src/smpc.rs | SMPC commands, RTC/SMEM, digital/analog joystick levels and INTBACK |
| src/lib.rs | Explicit cold-boot modes, independent master/slave ownership, bounded functional interleaving |
| src/onchip.rs | CPU-local FRT/WDT/SCI/DIVU/DMAC and internal INTC |
| src/scu.rs, src/scu/ | SCU interrupt controller, three DMA channels and original DSP interpreter |
| src/timing.rs | Clock-based NTSC raster deadlines |
| src/wasm.rs | Thin Wasm adapter; no gameplay or browser APIs |
| web/app.js | Local file handling, keyboard/Gamepad levels, animation scheduling and RGBA copy |
| web/audio.js, web/audio-worklet.js | Bounded emulated PCM delivery and WebAudio consumer/status |
| homebrew/star-collector | Buildable MIT-licensed SH-2 guest and reproducible ISO builder |

`Cpu::step(&mut impl Memory)` owns no bus pointers. `Machine::reset(Boot)` clears
volatile RAM, devices, ROM, counters, delay state and faults before attempting the selected
boot. Invalid input leaves that mode selected and CPU in Reset, with no stale
media or fallback. Battery-backed bytes are retained. `step()` makes one bounded master turn and,
when enabled, one slave turn, then advances peripherals by eight reference clocks;
exception entry or a sleeping turn can retire no instructions. `run_steps(n)` accepts 1–10000 steps.
`advance_frame()` waits for the next NTSC raster field deadline with a bounded
turn limit and aborts on its first fault. Raster, SCU timer and DSP deadlines split
peripheral advancement; SCU external DMA advances on eight-clock boundaries.
CPU issue remains a deterministic functional interleave without SH-2 opcode cycle
accounting or full bus arbitration. See the
[dual-CPU/FRT timing limits](hardware-conformance.md#dual-cpufrt-scope-and-deterministic-deviations).
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
handler without retiring an instruction. Machine retries CPU-local FRT/WDT/SCI/
DIVU/DMAC requests and SMPC master NMI at each processor boundary. Internal IRQs
use IPRA/IPRB levels and the manual table 5.4 tie order. Acceptance consumes a
turn without retiring an instruction; device flags require guest acknowledgement.
DMA address errors use vector 10 independently of SR.I. External IRL/ICR,
refresh/UBC sources and full bus arbitration remain missing.

CPU address errors dispatch vector 9 with next-PC/SR stacking, operand completion
and delayed-branch deferral (SH7604 §§4.3/4.6/4.7). `src/cpu/memory.rs` checks
alignment, on-chip widths and PC-relative/TAS restrictions before device access.
Invalid reads return deterministic zero and invalid writes are suppressed;
register/flag and auto-update effects still complete. This reproducibility policy
does not emulate physical invalid-cycle bus data/signals. The pending latch is
observable in `CpuState` and Wasm; reset clears it. Original synthetic BIOS tests
exercise handler return and independently expected PC/SR traces in native/Wasm.
See [address-error deviations](hardware-conformance.md#address-error-scope-and-deterministic-deviations).

The slave starts held in reset; timed SMPC SSHON releases its power-on vectors
from the shared BIOS and SSHOFF resets only the slave CPU/FRT, preserving shared
RAM and master state. Both CPUs execute the same original interpreter with
independent registers, stacks, delay/fault state and on-chip registers. The
original diagnostic guest uses a guest-written shared flag to select setup;
BCR1 hardware CPU identification remains missing. SMPC NMIREQ latches a master
NMI and can wake SLEEP independently of SR.I. A fault on either CPU stops the
entire machine until reset and identifies the slave when applicable.

The watchdog implements power/manual reset selection, a 512-clock CPU hold,
physical vector pairs 0/4 or 8/12, and retained RAM/backup/cause. CPU-local
write-through caches implement software coherence and mapped arrays. BSC/UBC,
standby, external reset signals and hardware cycle scheduling remain outstanding. Missing bus/device implementations and exception-entry failures
remain explicit, sticky host diagnostics.
Exception entry is bounded on a failing stack/vector access; successful writes
before a later entry failure are not rolled back. This is not hardware double-
fault emulation. The standalone instruction engine does not provide cycle timing,
bus locking/arbitration between processors, or native Sega BIOS compatibility.

## Memory and devices

| Physical range | Behavior |
| --- | --- |
| 00000000–0007FFFF | Optional read-only 512 KiB external BIOS |
| 00100000–0010007F | SMPC direct ports, IREG/OREG/SR/SF, implemented command subset |
| 00180000–0018FFFF | 32 KiB backup RAM on odd byte addresses; other lanes/widths explicitly unsupported |
| 00200000–002FFFFF | 1 MiB low work RAM |
| 01000000 / 01800000 | Write-only word capture strobes to slave/master FRT |
| 05A00000–05A7FFFF | 512 KiB shared sound RAM; 68k map 000000–07FFFF |
| 05B00000–05B00FFF | SCSP implemented slot/common/stack ports; 68k map 100000–100FFF |
| 05E00000–05E7FFFF | 512 KiB VDP2 VRAM |
| 05F80000–05F8011F | VDP2, only the listed register subset |
| 05FE0000–05FE00CF | SCU documented port subset; long accesses, prohibited/unsupported ports fail |
| 06000000–060FFFFF | 1 MiB high work RAM |
| FFFFFE10–FFFFFE19 | CPU-local FRT, byte accesses only |
| FFFFFE00–FFFFFE05 | CPU-local SCI, byte access |
| FFFFFE60–FFFFFE68 / FFFFFEE2–FFFFFEE5 | CPU-local INTC priorities/vectors, byte/word |
| FFFFFE71–FFFFFE72 | DMAC SCI/DREQ selection, byte access |
| FFFFFE80–FFFFFE83 | WDT keyed word writes and byte reads |
| FFFFFE92 | CCR cache control, byte access |
| FFFFFF00–FFFFFF14 | DIVU, long access; DVCR/VCRDIV also word access |
| FFFFFF80–FFFFFFB3 | DMAC channels/control/vectors, long access |

P1 cache-through addresses 20000000–3FFFFFFF map to the corresponding external
physical address. `CpuBus` supplies independent 4 KiB caches, write-through
stores, P1 bypass, P2 associative purge, P3 address arrays and P6 data arrays.
`Memory::fetch16` selects instruction versus operand replacement controls;
TAS reads bypass the cache. DMA does not snoop either cache. Debugger `Bus`
reads inspect external memory; CPU cache arrays are accessed via `CpuBus`.
RAM mirrors are not yet modeled. Words/longs
require 2/4-byte alignment and full valid ranges. RAM access validation prevents partial writes. MMIO long transfers on the
16-bit sound bus issue two ordered word cycles; a later unsupported cycle can
fail after an earlier cycle has taken effect. External mode requires exactly 524288 bytes, reads initial
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

SMPC uses byte accesses: PDR1/PDR2 +75/+77, DDR1/DDR2 +79/+7B,
IOSEL +7D, EXLE +7F (relative to
00100000, often accessed at 20100000). DDR1=60, IOSEL1=1 and EXLE1=0 select
standard digital pad direct access. PDR1 TH/TR=11 returns pad identification/L;
01 returns Right/Left/Down/Up; 10 returns Start/A/C/B; 00 returns R/X/Y/Z.
TL is high and button bits are active-low. Unsupported direction/control modes
fail explicitly. Settling is immediate in the emulator; the guest includes a
32-iteration delay after selecting a nibble to satisfy the hardware's 2 µs wait.
Host masks are up/down/left/right/B/C/A/Start/Z/Y/X/R/L at bits 0–12. The browser
maps arrows, Z (A) and Enter (Start), releases on blur and pauses on tab hiding.
Port 2 uses the same direct protocol with independent button, PDR and DDR state,
IOSEL bit 1 and EXLE bit 1. The browser maps I/K/J/L (up/down/left/right),
U/O/P (A/B/C), Right Shift (Start); both pads release on focus loss, pause, settings
and view navigation. COMREG supports SSHON=02, SSHOFF=03 and NMIREQ=18. Guest software sets SF
to one before issuing a command; OREG31 reports it immediately and SF clears at
completion, after at least 807 reference clocks (minimum 30 µs at 26.8741 MHz,
rounded to a turn). Busy command overwrite and unsupported commands fail.
INTBACK now implements status, continue/break, VBlank-timed peripheral collection
and SCU SMPC IRQ. SETTIME/SETSMEM provide deterministic BCD Gregorian time and
four-byte battery storage, retained across volatile resets. SNDON/OFF control the
independent sound CPU; RESENAB/DISA gate reset-button NMI edges, with RESB sampled
at VBlank-in. Standard analog joystick ID15 reports buttons and unsigned X/Y/Z;
browser Gamepad input/focus release has native and Wasm guest evidence. SYSRES,
MSHON/CD controls, clock changes, multitaps, analog direct handshakes and the
six-byte 3D pad remain absent. See the conformance matrix for per-command timing
and deviations. These subsets do not establish full SMPC conformance.

Internal backup RAM is independent of volatile resets. The SCU manual §1.2,
figs. 1.3/1.5 (pp.4/6) places its 64 KiB window at 00180000; the SMPC manual's
SYSRES command (§2.3 p.29) specifies retention of 256 Kbit battery RAM. Supported
MOV.B accesses use odd addresses 00180001–0018FFFF and their P1 cache-through
aliases. Even byte lanes, word/long accesses and further mirrors remain explicit
unsupported/unmapped accesses; bus-lane/open-bus conformance is pending. New
machines start with zeroed, unformatted bytes; no BIOS filesystem is synthesized.
Cold resets, successful boot-mode changes and rejected boot images retain these
bytes while discarding volatile RAM/CPU/device state.

The native/Wasm API accepts exactly 32768 packed bytes (ascending odd-address
order, no header) and validates before mutation. Browser import pauses playback
and replaces only backup RAM; it preserves CPU, media and runtime diagnostics.
Export downloads a raw copy. Reset/mode changes cancel pending imports; a newer
selection supersedes an older read. File errors and invalid lengths preserve all
bytes. There is no automatic browser storage or network transfer. Reloading or
closing loses the in-memory copy; users must explicitly export/import to retain
it across sessions. These operations do not implement BIOS backup-library calls,
cartridge RAM, save states or battery/RTC behavior. The original shared
[diagnostic guest](../tests/fixtures/backup-pad.hex) and [tests](../tests/backup.rs)
exercise real SH-2 loads/stores and port-2 reads natively and in Chromium/Wasm.

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
- [Dual CPU User's Guide](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-202-R1-120994.pdf), §§1.1/1.2 and §§3/4 shared devices, common power-on vectors and FRT capture signaling. The original diagnostic contains no SDK or manual sample code.
- [SMPC User's Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-169-R1-072694.pdf), §1.2 pp.5–8 byte-only register map; §3.1 pp.74–75 direct access; §3.4 table 3.36 p.97 digital-pad TH/TR protocol.
- [Disc Format Standards](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-040-R4-051795.pdf), §§3–4, especially pp.25–31 IP fields, master stack and first-read file; ISO9660 directory record/volume layout.
- [Boot ROM User's Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-079B-R3-011895.pdf), §§1–3 startup/services. Full BIOS services are not implemented.
- [SCU User's Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-097-R5-072694.pdf), §§2–4, corrected by [Final Specifications](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-210-110194.pdf) #1–27/30–36: interrupt, DMA, DSP and raster timer behavior. The original guest programs contain no SDK/manual sample code.
- [VDP1 User's Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-013-R3-061694.pdf) defines remaining VDP1 work; this device is still absent.

The [game redistribution audit](homebrew-provenance.md) records candidate decisions,
original-game licensing and build provenance. Existing wasm-bindgen (MIT/Apache-2.0)
and Playwright (Apache-2.0) are infrastructure only. No project-wide license is
added. Manuals/firmware are not redistributed.

## Remaining milestones

Accurate CPU/bus cycles and contention, remaining SH7604 peripherals and
SCU cross-device/arbitration/DSP edge conformance,
remaining SMPC system/clock/analog-direct controls, additional VDP2
modes, VDP1, runtime CD/BIN/CUE, BIOS services and full synthetic boot pathways
remain required work. The executing MC68EC000, direct SCSP PCM and browser audio
are implemented; SCSP envelopes/FM/LFO/noise/effects DSP and CD-DA remain absent.
Reference-emulator/physical-hardware comparison and performance measurements
have not been performed. Synthetic Gamepad input is automatically tested. No dynarec, workers,
server backend or general-game compatibility is introduced.

Issue #17 continues all unmet hardware criteria; #11/#13/#15 remain closed.
See [hardware conformance and unresolved gaps](hardware-conformance.md)
and [current validation](validation-issue-17.md).

## SCU and raster clock domains

The SCU now interprets guest-loaded DSP instructions, moves actual bus data with
three DMA channels and external DSP DMA, and delivers fixed-priority IRQs to the
master SH-2. Native and browser tests use the same original synthetic guest ROM.
See [the conformance matrix](hardware-conformance.md#scu-and-dsp) for operation,
register and test links and the remaining limitations.

`timing::Raster` emits clock-based NTSC 320-dot scanline/HBlank/VBlank deadlines.
`Bus::advance_clocks` splits device advancement at raster, SCU timer and DSP
boundaries, with external DMA beats on eight-clock boundaries. `advance_frame`
waits for a raster field boundary with a hard turn bound. It no longer assigns
60000/20000 retired-instruction budgets to active/blank phases. The CPU issue
quantum and on-chip DMA arbitration remain incomplete timing models; this change
does not establish full hardware-time acceptance. Star Collector completion tests
wait for the guest result within a bound rather than requiring the old budget.

## Sound CPU and browser audio

`m68k::M68k` independently decodes the MC68EC000 integer instruction families.
It uses a separate 24-bit, big-endian sound bus; long accesses split into two
word cycles and need only word alignment. User/supervisor stacks, SR/CCR, normal
and seven-word address/bus frames, traps, privilege/illegal/trace, autovectored
IRQs, STOP wake and RESET output are exercised by original vectors. It excludes
later 68k model instructions. Opcode budgets are functional approximations;
prefetch, bus contention and exact exception fault PCs remain pending.

The SH-2 uploads sound programs and samples into shared 512 KiB sound RAM and
starts the 68k with SMPC SNDON. SCSP implements 32 direct PCM slots, signed 8/16-bit
samples, key/loop/pitch/source-bit controls, direct pan/mix, three timers,
interrupt routing, bounded MIDI FIFOs and a word-at-a-time DMA subset. Only
SDIR direct mode is synthesized; unsupported envelope/FM/LFO/DSP settings fail.
The original dual-architecture fixture computes its waveform on the 68k,
programs SCSP, handles a timer IRQ and returns through RTE. Changing its arithmetic
changes both shared RAM and exact output samples natively and through Wasm.

Audio frames advance at 44.1 kHz independently of the 11.2896 MHz sound CPU.
Both domains use integer deadlines relative to fixed NTSC 320 reference time;
chunk-invariance tests verify repeatable ordering. The Rust stereo queue,
JavaScript producer backlog and AudioWorklet consumer each have fixed bounds.
An explicit user gesture enables WebAudio; disabled, blocked, paused, muted,
underflow and drop states are observable. The consumer resamples for a different
host sample rate. Browser instrumentation verifies actual nonzero worklet output,
not human listening. Star Collector itself continues to have no soundtrack.
