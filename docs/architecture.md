# Architecture and compatibility

This original Rust implementation executes **Star Collector** and **Prism Courier**,
original Saturn-targeted homebrew discs, through a documented BIOS-less HLE
bootstrap. Prism Courier uses VDP1+VDP2 and guest-computed stereo sound.
This remains a hardware subset, not general Saturn or commercial
game compatibility. External Sega BIOS boot remains incomplete. Acceptance uses
automated local/CI checks; live Pages inspection is not a completion gate.

## Component boundaries

| Component | Responsibility |
| --- | --- |
| src/cpu.rs | Independent SH-2 interpreter, architectural registers, delayed branches and sticky faults |
| src/bus.rs | Aligned big-endian byte/word/long accesses, RAM, ROM, VRAM and MMIO routing |
| src/backup.rs | Packed battery RAM, atomic import and odd-byte storage with word/long bus lanes |
| src/disc.rs | Bounded ISO9660 and Saturn IP validation, first-read executable selection |
| src/hle.rs, src/hle/ | HLE boot tables and original guest-executed vector, callback, mask, semaphore and priority services |
| src/media.rs | Retained cooked/raw/BIN/CUE data/audio media and track indices/gaps |
| src/cd.rs | Timed runtime CD commands, selector/sector pool, FIFO and CD-DA |
| src/devices.rs | Shared device constants and compatibility exports |
| src/vdp1.rs | Clock-driven command lists, primitive/texture drawing and framebuffers |
| src/vdp2.rs | NBG0/1 bitmap sampling, palette and sprite/background composition |
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
| homebrew/prism-courier | Original playable SH-2/68k disc, graphics, audio, input and IRQ source |

`Cpu::step(&mut impl Memory)` owns no bus pointers. `Machine::reset(Boot)` clears
volatile RAM, devices, ROM, counters, delay state and faults before attempting the selected
boot. Invalid input leaves that mode selected and CPU in Reset, with no stale
media or fallback. Battery-backed bytes are retained. `step()` makes one bounded master turn and,
when enabled, one slave turn, then advances peripherals by eight reference clocks
plus inserted BSC software waits and DIVU interlocks (converted from main clocks);
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

The guest controls every pixel and game rule. Rust executes VDP1 commands and
composes the resulting framebuffer with VDP2 VRAM into RGBA;
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
write-through caches implement software coherence and mapped arrays. BSC ordinary
software waits, narrow CS1-CS3 transfers and CS2 endian conversion have guest
evidence. Full BSC memory/idle/arbitration, UBC/standby timing, external reset
signals and hardware cycle scheduling remain outstanding. Missing bus/device implementations and exception-entry failures
remain explicit, sticky host diagnostics.
Exception entry is bounded on a failing stack/vector access; successful writes
before a later entry failure are not rolled back. This is not hardware double-
fault emulation. The standalone instruction engine does not provide cycle timing,
bus locking/arbitration between processors, or native Sega BIOS compatibility.

## Memory and devices

| Physical range | Behavior |
| --- | --- |
| 00000000–0007FFFF | Read-only 512 KiB external BIOS, mirrored to 000FFFFF; HLE disc installs an original reset trampoline |
| 00100000–0010007F | SMPC direct ports, IREG/OREG/SR/SF, implemented commands, odd lanes and mirrors to 0017FFFF |
| 00180000–0018FFFF | 32 KiB backup RAM on odd lanes; even lanes FF/ignored writes; word/long cycles strobe odd bytes |
| 00200000–002FFFFF | 1 MiB low work RAM, mirrored through 003FFFFF |
| 01000000 / 01800000 | Write-only word capture strobes to slave/master FRT |
| 05818000 / 05890000 | CD FIFO and command/HIRQ ports, 16-bit registers and FIFO long transfers |
| 05A00000–05AFFFFF | 512 KiB shared sound RAM with mirror; 68k map 000000–07FFFF |
| 05B00000–05B00FFF | SCSP implemented slot/common/stack ports; 68k map 100000–100FFF |
| 05C00000–05C7FFFF | VDP1 command/texture/LUT/Gouraud VRAM |
| 05C80000–05CFFFFF | 256 KiB VDP1 draw framebuffer with mirror, byte/word/long access |
| 05D00000–05D00017 | VDP1 control/status, word and checked paired-word accesses |
| 05E00000–05EFFFFF | 512 KiB VDP2 VRAM with mirror |
| 05F00000–05F00FFF | VDP2 CRAM, including mode-0 upper-half mirror |
| 05F80000–05F8011F | VDP2, only the listed register subset |
| 05FE0000–05FE00CF | SCU documented port subset; long accesses, prohibited/unsupported ports fail |
| 06000000–060FFFFF | 1 MiB high work RAM, mirrored throughout CS3 through 07FFFFFF |
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

VDP1 executes normal/scaled/distorted sprites, polygons, polylines and lines,
system/user clipping, local coordinates, jump/skip/call/return and END. It uses
guest VRAM textures in six bank/LUT/RGB formats, transparency/end codes, mesh,
Gouraud and half-luminance/transparency/shadow calculations. PTMR starts work;
one fetch or ruled raster row advances per 32 functional reference clocks. This
quantum bounds host work and is not measured VDP1 pixel/bus timing. CEF/COPR and
SCU source 13/DMA trigger 6 reflect actual command progress. Cyclic lists stay
busy; malformed/prohibited commands produce bounded machine diagnostics.
TVMR/FBCR/EWDR/EWLR/EWRR control buffer formats, field changes and erase rectangles.
Erase is applied at the field boundary; physical erase bandwidth, subpixel edge
rounding, high-speed shrink, interlaced/rotated/high-resolution scanout and exact
transfer-over behavior remain incomplete. Non-normal scanout fails explicitly.

VDP2 supports 320×224 noninterlaced NBG0/1 bitmaps, applicable packed/palette/
RGB555/RGB888 formats, dimensions/bank/palette offsets, fixed-point scroll/zoom,
mosaic, rectangular windows, back color tables, priorities, weighted/additive
color calculation and signed color offsets. CRAM modes 0/1/2 select 1024/2048
RGB555 or 1024 RGB888 colors. Sprite types 0/1 and mixed RGB select palette,
priority and color calculation; normal VDP1 scanout composes with both backgrounds.
Five-bit output channels expand by bit replication, preserving Star Collector.
Display disabled produces opaque black. Unsupported enabled modes fail explicitly.
Tilemaps, NBG2/3, rotation, line/cell tables, sprite/line windows, shadows, special
color/priority effects, interlace and alternative resolutions remain required.
CYCA0L/U are stored, but VRAM-cycle arbitration is not modeled. Register readback
for implemented write-only VDP2 controls remains a debugger convenience.

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
file identifier 2); its filename is not special. Retained BIN/CUE media may have
cooked or raw Mode1/Mode2 data and BINARY audio tracks. The boot loader extracts
its first data track through this same validated ISO contract; the runtime CD
block retains the complete medium, including tracks, indices and gaps.
Compressed images and large/general ISO boot layouts remain unsupported.

HLE copies the IP system area to 06002000, loads the first-read file at the header's
F0 destination, initializes the master stack from E8 (zero means 06002000), then
executes the original AIP at 06002E20 for a single 20-hex-byte area block. The
original bootstrap jumps to the loaded executable. The IP includes no Sega
security/area executable; HLE deliberately skips those proprietary boot stages.
No arbitrary bytes are treated as a disc. General BIOS services are not yet
intercepted. An original licensed SH-2 reset trampoline provides a slave handoff
through the work-RAM entry pointer; external BIOS selection never installs it.
The runtime CD block, SCU DMA/IRQ and SCSP external input operate on the actual
retained medium, including the Hardware Lab mixed-disc guest.
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
- [VDP1 User's Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-013-R3-061694.pdf), §§2–7: memory, system registers, command flow, primitive/texture geometry, clipping and color calculations. New rendering and test code is independently authored from these programming contracts.

The [game redistribution audit](homebrew-provenance.md) records candidate decisions,
original-game licensing and build provenance. Existing wasm-bindgen (MIT/Apache-2.0)
and Playwright (Apache-2.0) are infrastructure only. No project-wide license is
added. Manuals/firmware are not redistributed.

## Remaining milestones

Accurate CPU/bus cycles and contention, remaining SH7604 peripherals and
SCU cross-device/arbitration/DSP edge conformance,
remaining SMPC system/clock/analog-direct controls, additional VDP2
modes, remaining VDP1 conformance, full CD drive/command/error behavior, BIOS services and full synthetic boot pathways
remain required work. The executing MC68EC000, direct SCSP PCM and browser audio
and the sample-clock SCSP effects interpreter are implemented; SCSP
envelopes/FM/LFO/noise remain absent. The DSP owns COEF/MADRS/MPRO,
TEMP/MEMS/MIXS/EFREG and sound-RAM ring/table addressing. Slots and external
CD-DA feed real instructions; EFREG outputs use the SCSP effect sends. Hardware
Lab's independent 68k uploads an original effects program, and native/browser
checks compare exact samples and changed guest coefficients. Detailed timing,
full DSP conformance and bus contention remain required.
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

The ordinary BSC interface implements WCR software waits and BCR1 long-wait
selection per external beat, including cache-refill, TAS and write-through
transactions. BCR2 selects byte/word/long external transfers in CS1-CS3; larger
operands split in address order. CS2 ENDIAN reverses operand bytes while leaving
byte addresses/strobes unchanged (SH7604 figs.7.5/7.6), including low-word-first
long transfers on a word bus. Each CPU has its own settings; raw device memory
and the other CPU do not change endian. Cache hits/arrays incur no external
waits. The original [BSC guest](../tests/fixtures/build_bsc_waits.py) initializes
width/endian once before accesses outside CS0 and uses its FRT to measure actual
delays in native/Wasm, with width/endian/data/wait operand changes.

This still adds Tw clocks to the functional CPU issue quantum. It does not
implement T1/T2 pipeline overlap, inter-cycle idle, external WAIT sampling,
refresh/CPU/DMA bus ownership, burst memories or all board lanes. CS0 retains
the existing 32-bit external adapter. Unsupported changed BSC configurations
raise errors, including reserved bus widths and little-endian byte buses.
The game media are unchanged; startup/restart tests allow their executed VRAM
clears to take the additional bus time and still assert gameplay and pixels.

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
No-loop and forward-loop PCM8/16 now use signed Q16 first-order interpolation
before stack writes and mixing (ST-077 §§2.2/3.1, fig.4.30 OP4). A guest-programmed
half-sample pitch produces independently expected intermediate stereo values in
native and browser Wasm. Reverse/alternating traversal still uses sample-point
output; its fractional interpolation and precise boundary behavior remain pending.

Audio frames advance at 44.1 kHz independently of the 11.2896 MHz sound CPU.
Both domains use integer deadlines relative to fixed NTSC 320 reference time;
chunk-invariance tests verify repeatable ordering. The Rust stereo queue,
JavaScript producer backlog and AudioWorklet consumer each have fixed bounds.
An explicit user gesture enables WebAudio; disabled, blocked, paused, muted,
underflow and drop states are observable. The consumer resamples for a different
host sample rate. Browser instrumentation verifies actual nonzero worklet output,
not human listening. Star Collector itself continues to have no soundtrack.
