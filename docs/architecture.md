# Architecture and compatibility

This original Rust implementation executes **Star Collector** and **Prism Courier**,
original Saturn-targeted homebrew discs, and the Hardware Lab diagnostic through a documented BIOS-less HLE
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
| src/hle.rs, src/hle/ | HLE boot tables and original guest-executed vector, callback, mask, semaphore, priority, clock and backup-library services |
| src/media.rs | Retained cooked/raw/BIN/CUE data/audio media, track indices/gaps and bounded caller-supplied SRW1 companions |
| src/cd.rs, src/cd_rw.rs | Timed runtime CD commands, selector/sector pool, shared FIFO, CD-DA and scheduled R–W acquisition/parity/ring |
| src/devices.rs | Shared device constants and compatibility exports |
| src/vdp1.rs | Clock-driven command lists, primitive/texture drawing and framebuffers |
| src/vdp2.rs | NBG0–3 tile/bitmap, RBG0/RBG1, line/window/effect tables and sprite/background composition |
| src/m68k.rs | Original MC68EC000 integer interpreter, addressing and exceptions |
| src/sound.rs | Shared sound RAM, direct PCM SCSP, independent 68k/sample clocks |
| src/smpc.rs | SMPC commands, RTC/SMEM, digital/analog joystick levels and INTBACK |
| src/lib.rs | Explicit cold-boot modes, independent master/slave ownership, bounded functional interleaving |
| src/onchip.rs | CPU-local FRT/WDT/SCI/DIVU/DMAC and internal INTC |
| src/scu.rs, src/scu/ | SCU interrupt controller, three DMA channels and original DSP interpreter |
| src/timing.rs | NTSC/PAL PLLs, selectable display intervals and field/counter deadlines |
| src/wasm.rs | Thin Wasm adapter; no gameplay or browser APIs |
| web/app.js | Local file handling, keyboard/Gamepad levels, animation scheduling and RGBA copy |
| web/audio.js, web/audio-worklet.js | Bounded emulated PCM delivery and WebAudio consumer/status |
| homebrew/star-collector | Buildable MIT-licensed SH-2 guest and reproducible ISO builder |
| homebrew/prism-courier | Original playable SH-2/68k disc, graphics, audio, input and IRQ source |

`Cpu::step(&mut impl Memory)` owns no bus pointers. `Machine::reset(Boot)` clears
volatile RAM, devices, ROM, counters, delay state and faults before attempting the selected
boot. Invalid input leaves that mode selected and CPU in Reset, with no stale
media or fallback. Battery-backed bytes are retained. `step()` issues only CPUs
whose independent opcode/wait deadlines have arrived (master first on ties),
then advances to the next CPU deadline in the PLL main-clock domain. CPU issue
can occur between two main ticks sharing one reference tick, so single-state
instructions do not accumulate per-instruction PLL rounding. Public elapsed
time and sound/CD/RTC advancement remain in reference ticks. SH-2 base execution
states follow the
Programming Manual table7.2; actual external beats occupy T1/T2 and add BSC,
VRAM and DIVU waits only to the issuing CPU. Sleeping polling is bounded at eight
main clocks and retires no instruction. `run_steps(n)` accepts 1–10000
scheduler boundaries. `advance_frame()` waits for a raster field with a bounded
turn limit and aborts on the first fault. Raster, SCU timer/DSP and on-chip DMA
deadlines split peripheral advancement. Both on-chip DMACs progress without
CPU issue through distinct source/destination bus grants, shared with CPU and
SCU transactions. Cycle-steal yields after a unit; bursts retain ownership and
TAS excludes DMA. Bounded IF/ID scheduling is described below; complete refresh,
display, sound and peripheral-bus ownership/timing remain required. See the
[CPU timing evidence](hardware-conformance.md#independent-sh-2-issue-deadlines-and-clocked-on-chip-dma).
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

Both SH-2s retain bounded CPU-local IF/ID state in [fetch.rs](../src/cpu/fetch.rs).
PR memory transfers use ordinary load/store MA scheduling and IF contention
(Hitachi Figures 7.86–7.87), retaining implicit-PR writeback dependencies.
Real IF advances two instructions ahead of EX through the existing cache, replay
journal and transaction arbiter. Sampled bytes and decode readiness survive older
unretired instructions; actual waits stretch slots, and MA wins over real IF.
Retained-halfword `if` shares MA. Redirects keep only the documented delay slot;
delay-slot MOV contention follows the target stream's alignment. LDC.L holds
successor ID through initial EX and actual operand MA, then resumes younger IF
beside its final EX, stretching each waited slot once.
Exceptions/SLEEP/reset discard younger work. Lookahead is limited to side-effect-free
ROM/RAM/cache routes. MMIO and invalid-address exceptions are deferred until an
instruction is selected. PLL retains the master stream and resets the slave.
The [stage/cancellation table and oracle](sh2-fetch-pipeline.md),
[native grant traces](../src/bus/fetch_pipeline_tests.rs) and original
[native](../tests/sh2_fetch.rs)/[Wasm guest](../scripts/sh2-fetch.mjs) identify exact
coverage and deterministic phase conventions. This does not complete unrelated
EX sequences or establish general compatibility.

Ordinary memory-to-GPR MOV instructions schedule MA independently of their
addressing form. Synchronous cache and cache-array reads sample at MA completion;
scheduled peripheral/external reads enter the existing arbiter after EX. The
CPU-local load pipeline retains MA/EX overlap for aligned cached instructions.
A real IF splits that slot; a dependent successor consumes the same split,
including destination-only hazards and the documented load/MAC exceptions.
Clock observations and completed effects survive journal replay; reset replaces
both dependency and overlap state. [The stage table](sh2-load-pipeline.md) and
[Issue #55 validation](validation-issue-55.md) identify precise scope, original
native/browser guests. The newer [IF/ID contract](sh2-fetch-pipeline.md) extends
fetch overlap to cold and uncached streams.

Ordinary MOV stores use the same MA qualification and aligned successor overlap,
without creating a WB dependency. A preceding memory load forwards data to store
MA (Renesas Rev.5.00 §7.2.2, p.419/fig7.14); address/base and R0-index
uses still interlock in EX. Multiplier/system-register writeback retains its
existing timing. The interpreter resolves address/data from the retained
instruction snapshot; scheduled writes request at MA start, synchronous cache
arrays mutate at MA end, and the BSC owns accepted external payloads until their
physical publication. The one-entry buffer, external-read fence and separately
published video words retain their existing bus contracts. Invalid operands
complete MA before publishing register/branch state and latching address error.
[The store stage/deadline table](sh2-store-pipeline.md) and
[Issue #63 validation](validation-issue-63.md) record the precise scope and
native/browser evidence. Other instruction microsequences remain #17 work.

`src/cpu/multiplier.rs` retains CPU-local multiplier availability. MULS/MULU,
MUL.L/DMUL, MAC.W/MAC.L and both directions of MACH/MACL LDS/STS (including
CLRMAC) interlock at their MA access stage. The two/four-clock mm tails advance
through actual fetch, operand and on-chip waits. The first MAC operand can
complete before waiting for the second operand's multiplier access. IF/MA slot
splits depend on longword alignment, including cache hits; STS GPR writeback
keeps its existing independent interlock. Clock observations and local waits
are journaled with memory effects, so instruction replay neither moves a sampled
boundary nor repeats an operand. The second MAC operand and MACH/MACL memory
transfers also journal whether the access is synchronous: cache arrays and
resident reads wait through MA completion. Scheduled reads begin at their
ordinary MA start; the bus scheduler extends the final beat through the
journaled multiplier boundary. Operand latency and mm contention overlap in
full, with data sampled at the later completion. STS.L keeps its existing
one-clock admission after the accumulator wait before supplying write data.
For MAC and memory transfers, the CPU also retains the successor's permitted EX
time. The scheduler recovers its overlap with MA without moving memory effects
earlier, including scheduled peripheral reads and writes. Multi-clock operand
reads stretch the shared MA/EX slot while preserving its overlap (P §7.2.3,
fig7.89); IF/MA splits, waited successor fetches and exception entry consume
that interval.
Hardware reset replaces this state; exception
entry advances time without inventing a multiplier flush. The
[primary-derived timing table and scope](sh2-multiplier.md) and
[validation record](validation-issue-31.md) describe independent native and
production Wasm guest evidence. Full speculative pipeline completion remains
separate #17 work.

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
refresh and UBC sources are implemented; full bus arbitration remains missing.

[Interrupt entry phases](sh2-interrupt-entry.md) now run as resumable work in
both CPU journals. Source detection precedes the Table 5.8 recognition interval
(two states for NMI, five for peripheral/IRL); recognition overlaps an executing
instruction. Acceptance still waits for delayed branches and protected successors.
Only the IRQ acceptance attempt receives the recognized source context. If the
CPU declines it, instruction execution resumes without that context, so a native
UBC entry cannot acquire the declined request's vector or acknowledge its source.
Journaled decode/internal waits separate SR push, PC push, mask update, vector
selection, vector-table read and the first handler IF. Stack writes complete
synchronously through the arbiter; cache RAM accesses occupy at least one state.
NMI request clear occurs in the first E after decode, independently of entry
completion, so a later edge cannot be erased by replay or completion.

Master external-vector acquisition owns a distinct two-state bus transaction.
It outputs the accepted level, samples the SCU endpoint at completion and ignores
BSC programmable waits and memory/cache decoding. Other levels stay pending.
The transaction retains its accepted level and requested architectural vector;
the acquisition result and acknowledgement are journaled together. Autovector
acknowledgement retains the accepted source identity. Same-level source order,
autovector acknowledgement and the exact SCU clear edge are documented board
conventions whose SCU-specific primary evidence remains incomplete. This is a
partial #17 increment; see the [Issue #81 validation](validation-issue-81.md).

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
BCR1 identifies each physical processor. The DCC supplies slave HBlank IRL2
(vector `0x41`) and VBlank IRL6 (vector `0x43`) as live levels, independently of master
SCU masks/status/acknowledgement. `Bus::interrupt_request` uses physical blanking flags,
so DISP=0's TVSTAT readback override cannot create a permanent request. The
existing INTC arbitration gives external inputs priority over equal-level
internal sources, except UBC; CPU SR.I and branch/control-transfer deferral
still apply. Acceptance does not clear either raster level. An eligible RTE
can therefore re-enter within the same blank. Normal slave autovectors are
supported; slave VECMD=1 board-vector acquisition remains unsupported.

Raster events update levels before CPU issue at the next hardware boundary.
An issued CPU-boundary journal retains its sampled request through pending
stack/vector accesses; replay publishes one exception and no retired instruction.
Reset cancels that journal. SSHON starts with reset SR.I=15 and subsequently
unmasked software observes current live levels. DCC synchronizer latency and
measured electrical phase are unspecified here; the inherited raster phase and
whole-line interlace conventions remain. See [slave raster evidence](validation-issue-47.md).
SMPC NMIREQ latches a master
NMI and can wake SLEEP independently of SR.I. A fault on either CPU stops the
entire machine until reset and identifies the slave when applicable.

The watchdog implements power/manual reset selection, a 512-clock CPU hold,
physical vector pairs 0/4 or 8/12, and retained RAM/backup/cause. On both
processors each vector longword uses two 16-bit CS0 phases; cold PC/SP words
sample at clocks 8, 16, 24 and 32 under reset waits. The journal publishes PC
and SP together after all four words, before any instruction. These controlled
bus-vector deadlines are not measured console startup latency. CPU-local
write-through caches implement software coherence and mapped arrays. BSC ordinary
software waits, fixed 16-bit CS0, narrow CS1-CS3 transfers and CS2 endian conversion have guest
evidence. Full BSC memory/arbitration, UBC/standby timing, external reset
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
ROM, work RAM, sound RAM and video RAM mirrors are decoded as listed above. Words/longs
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
command fetches retain a 32-main-clock functional deadline, while raster traversal
advances one dot per main clock and retains texture/end-code/shading state between
dots. ENDR stops within thirty clocks without synthesizing END or its IRQ. CEF/COPR and
SCU source 13/DMA trigger 6 reflect actual command progress. Cyclic lists stay
busy; malformed/prohibited commands produce bounded machine diagnostics.
TVMR/FBCR/EWDR/EWLR/EWRR control buffer formats, field changes and erase rectangles.
EWDR/EWLR/EWRR retain separate written and captured copies. An actual automatic
or manual framebuffer change captures all three together; manual erase requests
and held manual fields keep the previous capture. Both display and VBlank erase
consume that capture, and each accepted operation retains its color, bounds,
geometry and physical destination until completion or cancellation. MODR still
reports written mode settings. At equal raster/data-completion edges the existing
scheduler captures before the external register write completes; this is an
emulator convention, since ST-013 does not specify an electrical phase. See the
[erase-capture validation](validation-issue-25.md) for independent evidence.
Normal erase follows each displayed scanline. Written VBE is sampled once at
the first HBlank-in after VBlank-in; CPU register writes never launch blank
erase. TVM remains immediate and MODR reports written settings. The retained
job publishes individual words after sampling, preserves the manual's aggregate
`(raster clocks - 200) * blank rasters` capacity, and stops before a field-end
buffer change. A uniform integer cadence reconciles delayed sampling with that
capacity; it is an explicit functional phase convention, not physical bus timing.
Rotation and HDTV retain automatic blank erase with VBE clear. HDTV never
erases during display. TVM=4 retains 512×256×16-bit storage; exclusive normal
320/352×480 output reads `(x,y/2)`, exclusive high 640/704×480 reads
`(x/2,y/2)`, with integer division and no rotation parameters. Encodings 5–7
remain diagnosed. See the [mode contract and primary mapping](vdp1-hdtv.md),
[Issue #73 validation](validation-issue-73.md), the
[event table and capacity reconciliation](vdp1-vbe-timing.md) and
[Issue #65 validation](validation-issue-65.md). Manual drawing retains its cursor across a buffer change;
BEF/LOPR capture unfinished work. Full CPU/VRAM arbitration, subpixel edge rounding
and command/effect-specific fetch costs remain incomplete. Standard-TV normal,
high-resolution and rotated 16/8-bit framebuffer readout are composed by VDP2.
Rotation uses parameter A start/deltas quantized to the sprite link's signed Q9
widths, without its matrix/coefficients or framebuffer wrapping. Double-density
drawing selects even/odd logical rows and packs them into physical framebuffer
rows; scanout retains both historical fields. EOS/DIE/DIL latch on buffer changes. HSS/EOS selects the source X parity on
reduced scaled/distorted sprite rows, retains normal sampling on expansion and
disables end codes at all magnifications. Native and browser guest vectors cover
both parities, flips, draw polling, MODR and actual Canvas pixels.

VDP2 supports NBG0–3 tile/bitmap formats, RBG0/RBG1 rotation/coefficient
parameters, packed/palette/RGB555/RGB888 colors, fixed-point scroll/zoom,
line/cell scroll, mosaic, rectangular/line/sprite windows, back/line color,
priorities, weighted/additive color calculation, gradation, shadows and color
offsets. All sixteen sprite selector layouts are decoded. Standard TV graphics
select 320/352/640/704 dots, 224/240 lines (256 on PAL), and noninterlaced,
single-density or double-density images. Double density exposes both image
fields in a full-height framebuffer. TVSTAT reports the separate physical PAL
strap and current odd/even field; EXLTFG still clears on read. `set_pal` configures
the physical standard independently of SMPC area codes and survives resets.
[Mosaic coordinates](vdp2-mosaic.md) follow VDP2 §4.11: selected NBG0–3
quantize in field-line units before scroll/zoom; selected RBG0 A/B and RBG1
quantize only X. Double-density mosaic selects a single-density image row for
that screen, preserving TVMD, output rows, windows, sprites and unselected
screens. Both snapshot decoding and scheduled address generation use this rule.
Line-scroll operands follow the selected normal image row; cell-scroll exclusion
remains per selected NBG0/NBG1. Rotation retains separate single-density and
output-density Xst/Yst/KAst progressions from the same captured table operands
and one-shot requests, so a mosaic picture cannot change shared sprite starts.
See [native/guest/browser evidence](validation-issue-79.md).
The PLL uses ST-169 table1.1's PAL 26.6875/28.4375 MHz frequencies; sound/CD/RTC
keep their fixed clocks, including reference ticks without a main-clock edge.

Machine presentation captures each visible scanline at HBlank and publishes it
at VBlank. Later register/VRAM writes cannot repaint preceding lines. Double
density updates alternate rows and retains the other field; published dimensions
stay paired with their pixels. Before the first completed field, the debugger may
inspect current state. Xst/Yst/KAst have independent A/B RPRCTL reloads at line
start, otherwise accumulating the current vertical delta. Active-line writes
cannot replace that line's latched origins. Normal-background names, characters,
bitmaps and vertical-cell operands now acquire at hardware-time cycle-slot events,
using independent retained paths and packet address metadata. Rendering cannot
fill a missing scheduled image fetch. Retained transfers are keyed by source group as
well as physical address, so repeated characters in later groups refetch while
enlargement preserves packed data within a group. The explicit two-cycle
horizontal lead and its unmeasured physical phase are documented in the
[Issue #19 record](validation-issue-19.md#explicit-phase-convention). Rotation image
ports now acquire complete names and aligned character/bitmap words at separate
per-dot deadlines, after the existing line-start parameter latch. Each output dot
retains address/format/name/coefficient-selection inputs; repeated addresses issue
fresh acquisitions. RBG0 follows split/unsplit RAMCTL roles; RBG1 fixes names to
B1 and characters to B0, independently of normal cycle commands. The
[rotation transaction contract](vdp2-rotation-fetch.md) distinguishes documented
bank/layout requirements from the assumed atomic per-dot phase. In standard-TV
640/704-dot modes, both rotation pictures use `floor(output_x/2)` for matrix and
horizontal coefficient increments, covering the normal 320/352 picture dots.
Parameter/layer windows, normal screens and VDP1 composition keep independent
output-dot coordinates. The same mapping serves debugger decoding and scheduled
address generation; paired coordinates still issue separate two-clock
acquisitions. See [high-resolution evidence](validation-issue-53.md). Physical
rotation latency, separate coefficient/table strobes, complete physical bank arbitration
and half-line interlace phase remain required. Exclusive monitors select 480
active lines, 525/562-line fields and 852/848-clock rasters. Exclusive high
resolution multiplexes NBG0/NBG1 even/odd pixels, following the guest's paired scroll/zoom setup. Monitor window and counter
bit layouts differ from standard TV. Color-depth resource sharing disables the
specified companion normal layers. All eight cycle-pattern words drive the CPU
VRAM grant mask, including split-bank matching, idle lead/linked grants, disabled
screen release, rotation-bank ownership and high-resolution slot limits. Denied CPU
requests accumulate waits until a grant or HBlank; cache hits issue no request.
CPU/on-chip DMAC/SCU transactions publish through their resumable physical bus
beats. Display deadlines split advancement independently of presentation, and
acquisition precedes a simultaneous data-phase completion. This functional
schedule does not establish measured physical arbitration or rotation-fetch latency.
BDCLMD selects the back screen during display blanking (the lowermost line for a
line back table), or opaque black. Reserved display encodings and 256-line mode on NTSC fail.
Unsupported enabled effects still fail explicitly. Register readback for
implemented write-only VDP2 controls remains a debugger convenience.

SMPC uses byte accesses: PDR1/PDR2 +75/+77, DDR1/DDR2 +79/+7B,
IOSEL +7D, EXLE +7F (relative to
00100000, often accessed at 20100000). DDR1=60, IOSEL1=1 and EXLE1=0 select
standard digital pad direct access. PDR1 TH/TR=11 returns pad identification/L;
01 returns Right/Left/Down/Up; 10 returns Start/A/C/B; 00 returns R/X/Y/Z.
TL is high and button bits are active-low. Unsupported direction/control modes
fail explicitly. The selected nibble retains its previous electrical value during the modeled
2 µs settling interval; independent native/Wasm guests sample both sides of
this deadline.
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
browser Gamepad input/focus release has native and Wasm guest evidence. SYSRES, MSHON/CD controls, clock changes, timed direct analog handshakes,
multitaps and six-byte 3D-pad reports are implemented in the device model.
Optimized INTBACK derives its collection deadline from the selected display
interval and PLL rather than a hardcoded 224-line NTSC field. See the conformance matrix for per-command timing
and deviations. These subsets do not establish full SMPC conformance.

Internal backup RAM is independent of volatile resets. The SCU manual §1.2,
figs. 1.3/1.5 (pp.4/6) places its 64 KiB window at 00180000; the SMPC manual's
SYSRES command (§2.3 p.29) specifies retention of 256 Kbit battery RAM. Supported
MOV.B accesses use odd addresses 00180001–0018FFFF, their seven further
64 KiB select aliases through 001FFFFF, and their P1 cache-through aliases. Even byte reads return FF and writes do not strobe storage; word/long accesses
strobe the odd lanes in address order. Complete board/open-bus conformance is
still pending. New machines start with zeroed, unformatted bytes. HLE disc boot
installs the original guest-executed BUP library with its separately identified
SRBUP001 filesystem; unknown imported bytes remain untouched until guest Format.
Cold resets, successful boot-mode changes and rejected boot images retain these
bytes while discarding volatile RAM/CPU/device state.

The native/Wasm API accepts exactly 32768 packed bytes (ascending odd-address
order, no header) and validates before mutation. Browser import pauses playback
and replaces only backup RAM; it preserves CPU, media and runtime diagnostics.
Export downloads a raw copy. Reset/mode changes cancel pending imports; a newer
selection supersedes an older read. File errors and invalid lengths preserve all
bytes. There is no automatic browser storage or network transfer. Reloading or
closing loses the in-memory copy; users must explicitly export/import to retain
it across sessions. Raw import/export is separate from the guest BUP ABI and does not implement
cartridge RAM or save states. The original shared
[diagnostic guest](../tests/fixtures/backup-pad.hex) and [tests](../tests/backup.rs)
exercise real SH-2 loads/stores and port-2 reads natively and in Chromium/Wasm.

## Disc and BIOS-less HLE boot

[Star Collector's build and layout](../homebrew/star-collector/README.md) specify
the supported self-contained guest contract. The parser accepts cooked
2048-byte-sector ISO files, 18 to 450,000 sectors, with a Saturn hardware identifier,
distinct J/T/U/E area symbols, IP size 1000–8000 hex and aligned stack/first-read destination in high
RAM. It scans the descriptor set starting at sector 16 for primary and terminator descriptors, checking matching
little/big-endian volume/extent values, 2048-byte blocks, root directory records,
names, bounds and single-volume extents. Extended-attribute blocks are skipped;
file-unit/gap addressing gathers interleaved first-read data. Unsupported
multi-extent files and malformed/truncated data fail
before loading. The first root child must be a regular file (Saturn first-read
file identifier 2); its filename is not special. Retained BIN/CUE media may have
cooked or raw Mode1/Mode2 data and BINARY audio tracks. Plain ECMA-130 Mode2
retains all 2336 payload bytes without interpreting XA subheaders/parity. CUE does
not encode the XA/plain distinction; bounded repeated-subheader detection is the
default, with `Media::cue_with_mode2_format` for explicit source metadata. A failed
XA decode is never retried as unchecked plain data. Bootstrap requires 2048-byte
logical sectors; other lengths remain available through runtime CD transfers.
`disc::SectorReader` supplies checked INDEX-01-relative logical sectors from the
retained medium. Cold boot and HLE SYSRES use `parse_sectors` with that same
medium; neither extracts a cooked track nor mounts a duplicate ISO. Cooked
sectors are copied directly; required raw Mode1/XA Form1 sectors use the strict
four-pass ECC/EDC policy and fail explicitly when unrecoverable. Unread raw
sectors are never decoded. Scratch storage is bounded by a 32 KiB IP, a
128 KiB root directory, one descriptor at a time, and the legal first-read file.
The reader checks descriptor, declared-volume, extent/interleave and stored-track
bounds before reading. Zero first-read, descriptor scanning and area groups
retain their existing behavior. Bulletin #35 excludes `060FF000..060FFFFF` from
first-read destinations, independently of medium capacity.

Resource policy preserves the broader runtime model: at most 450,000 stored
ISO sectors (921,600,000 bytes), or 1,058,400,000 aggregate CUE BINARY bytes
(450,000 × 2352), plus 1 MiB CUE text and at most 99 files/tracks. CUE leadout
must fit FAD 450150. These are implementation resource bounds, not a new
63-minute hardware rejection. Major native allocations use fallible reservation;
JS file/Blob reads and Wasm marshaling failures stop the prior machine and report
an error. Browser admission uses one aggregate Blob buffer, transfers ownership
of the Wasm input vector to shared immutable track storage, then releases JS
input buffers. Restart and SYSRES share stored bytes through `Arc<Vec<u8>>`.
Native borrowed constructors make one retained owned copy; owned constructors
transfer allocation ownership. No streaming/compression framework is introduced.
The runtime CD block retains the complete medium, including tracks, indices and
gaps. Compressed media and unsupported ISO record formats remain unsupported.
See [Issue #33 validation](validation-issue-33.md) for generated media and limits.

HLE copies the IP system area to 06002000, loads the first-read file at the header's
F0 destination, initializes the master stack from E8 (zero means 06002000) and
the slave stack from EC (zero means 06001000), then executes the original AIP
after the area group at `06002E00 + 20 hex × area count`. The reset trampoline's
two original literal-pool values carry those metadata-selected addresses. The
original bootstrap jumps to the loaded executable. The IP includes no Sega
security/area executable; HLE deliberately skips those proprietary boot stages.
No arbitrary bytes are treated as a disc. System/vector/clock and backup services execute original SH-2 instructions,
without instruction interception. SYS_EXECDMP and CD-library services remain
required. An original licensed SH-2 reset trampoline provides a slave handoff
through the work-RAM entry pointer; external BIOS selection never installs it.
The runtime CD block, SCU DMA/IRQ and SCSP external input operate on the actual
retained medium, including the Hardware Lab mixed-disc guest.
Runtime directory services retain self/parent plus up to 254 selected child
records. ReadDir changes the held FID window within the current directory. XA
system-use fields supply unit/gap/file number/attributes; file reads select the
actual interleaved sectors and program the documented filter/partition settings.
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
remaining SMPC command/clock/reset boundaries, raster-time VDP2
behavior, remaining VDP1 conformance, full CD drive/command/error behavior, BIOS services and full synthetic boot pathways
remain required work. The executing MC68EC000, direct SCSP PCM and browser audio
and the sample-clock SCSP effects interpreter are implemented; SCSP
envelopes/FM/LFO/noise have functional coverage with remaining rate/pipeline
conformance gaps. The DSP owns COEF/MADRS/MPRO,
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

`timing::Raster` emits scanline/HBlank/VBlank deadlines for the selected
NTSC/PAL display interval and 320/352 PLL. Interlaced fields alternate ODD and
double-density VCNT parity. Half-line phase remains required.
`Bus::advance_clocks` splits device advancement at raster, SCU timer and DSP
boundaries, with external DMA data phases retiring after their BSC/A-bus/VRAM
waits. On-chip DMAC source and destination phases use separate journals from
the CPU and preserve the four-read/four-write order of 16-byte units. `advance_frame`
waits for a raster field boundary with a hard turn bound. It no longer assigns
60000/20000 retired-instruction budgets to active/blank phases. Independent opcode deadlines replace the CPU issue quantum. Pipeline and
full refresh/display/sound DMA arbitration remain incomplete timing models; this change
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

Scheduled SH-2 reads of sound RAM, VDP1 VRAM/draw framebuffer and VDP2 VRAM/CRAM
now pass through an explicit SCU memory bridge: byte/word operands acquire an
aligned external longword, with two independently completed downstream halfwords.
The journal retains the first sample through target waits and replay. CPU-side
external observations describe the assembled longword, while CPU-local operand
observations retain their requested lanes and sizes. Cache hits remain local;
refills preserve their wrap order. The [bridge transaction contract](sh2-b-bus-reads.md)
records primary corrections, alignment, lane extraction, SCSP byte restrictions,
reset domains and the distinction between specified granularity and deterministic
timing conventions. MMIO, A-bus conversion and CPU writes remain separate contracts.

The intercycle state machine also implements IW0–3 using the preceding read's
area/direction, a minimum one-cycle read/write gap and subtraction of already
empty clocks (SH7604 §7.9, fig.7.47). Occupancy includes T1/T2 and Tw so busy
clocks cannot satisfy a gap. Actual cache hits/arrays leave this history alone;
CPU/SCU ownership changes, reset and standby terminate the old history. The
original [cached TAS guest](../tests/fixtures/build_bsc_idle.py) measures both
CPUs' IW3=0/1/2 through their FRTs in native and Wasm execution.

Tw/turnaround clocks now delay only the CPU that issued the access. External
T1/T2 occupancy bounds that CPU’s instruction interval; pipeline overlap, external WAIT sampling,
complete refresh/CPU/DMA arbitration and all board lanes remain required.
The [VA0.5 board drawing](https://consolemods.org/wiki/images/1/1b/Sega_saturn_va0.5_schematics.pdf),
sheet 1, identifies MD4=0/MD3=1 on both CPUs and a word-output boot ROM.
[SH7604](https://antime.kapsi.fi/sega/files/sh7604.pdf) §3.3/table3.9 maps
those fixed pins to 16-bit CS0. BCR2 and MCR configure other areas independently;
construction, system/watchdog resets and slave stop/start cannot change CS0.

`Bsc::access_timing` implements §7.8/figs.7.44–46: an eligible longword read
has T1+Tw+T2 on its first word and Tw+T2 on its second. With zero software
waits both words retain T1+T2. Byte/word reads and writes are ordinary accesses.
Cache fills and 16-byte DMAC reads restart at each of their four longwords.
Phase identity comes from the current transfer's width, first marker and word
address; there is no retained nibble-address state to survive cancellation or
ownership changes. Existing journals retain sampled words, the BSC write buffer
publishes each word separately, and CPU-group ownership excludes peers/refresh
until the accepted operand completes. Self-refresh inhibits new groups after
an already accepted narrow CPU operand drains. Narrow device contracts, SMPC
byte accesses, backup odd-byte lanes, mirrors and ROM protection remain intact;
SCU routes to CS0 remain prohibited. See the
[CS0 conformance evidence](hardware-conformance.md#fixed-cs0-word-bus-and-burst-rom-issue-23).
Unsupported changed BSC configurations raise errors, including reserved bus
widths and little-endian byte buses.
The game media are unchanged; startup/restart tests allow their executed VRAM
clears to take the additional bus time and still assert gameplay and pixels.

## Sound CPU and browser audio

`m68k::M68k` independently decodes the MC68EC000 integer instruction families.
It uses a separate 24-bit, big-endian sound bus; long accesses split into two
word cycles and need only word alignment. User/supervisor stacks, SR/CCR, normal
and seven-word address/bus frames, traps, privilege/illegal/trace, autovectored
IRQs, STOP wake and RESET output are exercised by original vectors. It excludes
later 68k model instructions. Opcode budgets are functional approximations;
generic two-word prefetch, access-frame records and partial transfers have
independent tests. Instruction-specific prefetch placement, operand-dependent
DIV timing and complete sound-bus arbitration remain pending.

The SH-2 uploads sound programs and samples into shared 512 KiB sound RAM and
starts the 68k with SMPC SNDON. SCSP implements 32 direct PCM slots, signed 8/16-bit
samples, key/loop/pitch/source-bit controls, direct pan/mix, three timers,
interrupt routing, bounded MIDI FIFOs and a word-at-a-time DMA subset. PCM envelope stages, FM stack modulation, tonal/noise LFO and an executing
effects DSP also produce real samples. Numeric envelope-rate calibration, physical
converter/interface phases and full DMA arbitration remain required. The
[seven-stage synthesis pipeline](scsp-synthesis-pipeline.md) retains PG, FM/address,
two waveform operands and post-interpolation work across sample boundaries.
DIRECT and DSP MIXS receive a result only after both level intervals; stack
publication uses the same completed value with the documented five-interval
OP2-to-OP7 relationship. Idle PG/LFO/stack edges remain distributed, and KEYON
joins the next eligible slot within a sample. SCSP RESET cancels retained work;
SNDOFF preserves the running synthesizer. Absolute MPRO/slot phase, OP1 register
capture and equal-edge consumer ordering are explicit emulator conventions.
[Native and production Wasm evidence](validation-issue-67.md) includes an original
68k-uploaded DSP probing early/late input availability and late-slot wrap.
The original dual-architecture fixture computes its waveform on the 68k,
programs SCSP, handles a timer IRQ and returns through RTE. Changing its arithmetic
changes both shared RAM and exact output samples natively and through Wasm.
No-loop and forward-loop PCM8/16 now use signed Q16 first-order interpolation
before stack writes and mixing (ST-077 §§2.2/3.1, fig.4.30 OP4). A guest-programmed
half-sample pitch produces independently expected intermediate stereo values in
native and browser Wasm. Reverse/alternating traversal also interpolates in its traversal direction;
independent vectors exercise its attack/loop boundaries.

Audio frames advance at 44.1 kHz independently of the 11.2896 MHz sound CPU.
Both domains use integer deadlines relative to fixed NTSC 320 reference time;
chunk-invariance tests verify repeatable ordering. The Rust stereo queue,
JavaScript producer backlog and AudioWorklet consumer each have fixed bounds.
An explicit user gesture enables WebAudio; disabled, blocked, paused, muted,
underflow and drop states are observable. The consumer resamples for a different
host sample rate. Browser instrumentation verifies actual nonzero worklet output,
not human listening. Star Collector itself continues to have no soundtrack.


DSP A-bus output DMA accepts both fixed and incrementing longword destinations,
including HOLD. The original [DSP DMA guest](../tests/fixtures/build_dsp_dma.py)
uses register-count transfers, fixed writes, postincremented counter operands
and program-RAM DMA. Nonzero-address bootstraps load PRAM from location zero
through an independent DMA cursor; both SCU MMIO and executing DSP MVI PC enter
the transferred code. A full 256-word transfer executes across PC FF→00.
Native/Wasm records and source/data/count mutations cover these paths.
Instructions may use other banks during DMA; an instruction touching the active
DMA data bank fails with a bounded diagnostic under SOA #8 §4.1's programming
restriction, before changing architectural state. This does not model a physical
result for a prohibited access. Full DSP pipeline, physical RAM-port timing and bus arbitration remain
required in the [conformance inventory](hardware-conformance.md).


SCSP CPU sound-memory operations now participate in the same 128-cycle/sample
allocation as PCM/DSP, refresh and internal DMA. The
[transaction contract](scsp-cpu-memory.md) separates bus eligibility, memory
admission, memory completion and later interface acknowledgement. The
[controller](../src/sound/memory.rs) retains addresses, byte lanes, write payloads,
per-word geometry and completed reads independently of SH-2/SCU and 68k journals.
A CPU-side main longword uses two memory cycles; SH-2 read bridging still samples
its two external halfwords independently. Main-interface and 68k pending requests
wake idle memory scheduling, and an admitted operation cannot be preempted.
SCU completion follows the final destination publication. Debugger inspection is
passive. SCSP RESET drops unfinished grants but retains a still-selected external
request and its completed halves, re-admitting only unfinished words after release; SNDOFF cancels only the
sound CPU. Manual SH-2 reset preserves the accepted operand drain. These are
functional interface/phase conventions, not measured electrical console timing.
See the [Issue #41 validation record](validation-issue-41.md) for native traces,
original guest computation/audio, production Wasm and negative controls. This is
a partial #17 increment; full hardware completion and commercial compatibility
remain unestablished.


SCSP output now reaches [retained MC68EC000 input](m68k-interrupt-input.md)
throughout pending computation and bus work. A boundary snapshot consumes its
accepted request once; completion preserves a new live level7 transition.
Instruction-boundary entry, separate IACK/stack scheduling and reset cancellation
remain in the existing transaction scheduler. The original SH-2/68k rearm guest
and its controls run natively and in production browser Wasm. Electrical IPL
synchronization and complete CPU/bus timing remain unverified.


R–W media enters after disc deinterleaving and before GF(64) P/Q correction.
The reference-clock scheduler independently advances two pre-roll acquisition
frames, a bounded eight-pack input window and four decoder publications/frame.
Decoded data occupies a 24-slot ring; DATATRNS pins its physical slot until
DataEnd and shares arbitration with all existing CD transfer users. Audio
output draining cannot advance acquisition. The [R–W contract](cd-rw.md)
specifies FAD/symbol layout, parity matrices, revision reconciliation, timing
conventions, overwrite recovery and reset domains. Native and production Wasm
use one CUE parser and explicit selected companion bytes; no path or network
loader is added. [Validation](validation-issue-75.md) records the original
MIT guest and inherited-main negative control. Related to #75 and #17; genuine
BIOS operation and broad commercial-game compatibility remain unverified.

The [clock standby protocol](clock-standby.md) separates instruction-executed
firmware preparation/recovery from SMPC reset and NMI delivery. Software standby
stops CPU issue and cancels unfinished on-chip DMA phases; clock completion
preserves master CPU journals, resets the documented device domains and delivers
normal NMI input. WDT-selected main clocks precede exception entry. HLE restores
the retained SCU mask and capture service and enables hot reset through MMIO;
it leaves DMA restart to the guest. Explicit timing and RAM-retention conventions
and the stage-by-stage evidence are recorded in that contract.
