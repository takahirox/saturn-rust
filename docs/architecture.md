# Architecture and roadmap

This is an independently implemented, instruction-counted baseline for Issue #3,
not a complete Saturn emulator. One master SH-2 executes; no slave CPU or
peripheral is simulated. No game compatibility or measured speed is claimed.

## Component boundaries

| Component | Current responsibility |
| --- | --- |
| `src/cpu.rs` | Architectural state, fetch/decode/execute, pending delayed branch, sticky faults |
| `src/bus.rs` | Checked big-endian byte/word/long access to three physical regions |
| `src/lib.rs` | Machine ownership, explicit boot modes, cold reset, bounded runs, original fixture |
| `src/wasm.rs` | Thin wasm-bindgen adapter; registers and errors cross the JS boundary |
| `web/` | Local file selection, controls and state display; static ES modules |

`Cpu::step(&mut impl Memory)` separates instruction execution from bus ownership.
`Machine::reset(Boot)`, `step()`, `run_steps(n)`, and `cpu_state()` provide the
reproducible host interface. CPU state contains R0–R15, PC, SR, PR, GBR, VBR,
MACH/MACL, instruction count, pending target and Reset/Running/Halted/Faulted
status. `halt()` is a debugger stop, not a hardware instruction. Each step retires
one instruction; a fault retires none. Runs accept 1–10000 steps and stop at the
first fault. Instruction counts are not hardware cycles.

## Verified CPU subset

Instruction words are 16-bit and fetched big-endian. Operations use 32-bit
wrapping arithmetic and preserve SR in this subset.

| Instruction | Encoding | Effect |
| --- | --- | --- |
| NOP | `0009` | Advance |
| MOV #imm,Rn | `Enii` | Sign-extend imm8 into Rn |
| MOV Rm,Rn | `6nm3` | Copy all 32 bits |
| ADD #imm,Rn | `7nii` | Add sign-extended imm8 |
| BRA disp12 | `Addd` | Target = instruction address + 4 + 2 × signed disp12 |
| RTS | `000B` | Target = PR |

BRA/RTS latch the target, then the next step executes exactly one delay-slot
instruction before transferring control. `CpuState.pc` means the next fetch
address, including that slot; it is not a pipelined PC operand. Future PC-relative
instructions must apply the manual's architectural PC rules explicitly. These
semantics follow the [SH-1/SH-2 Programming Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/h12p0.pdf),
§6.2 ADD (p.60), §6.8 BRA (pp.68–69), §6.32 MOV (p.112), §6.33 immediate MOV
(p.117), §6.43 NOP (p.132), and §6.51 RTS (pp.141–142), using printed page numbers.

Other opcodes produce a deterministic `UnsupportedOpcode { pc, opcode }` error.
A supported branch in a slot produces `BranchInDelaySlot`; other unsupported
slot instructions produce the ordinary unsupported error. No illegal-instruction,
slot-illegal, address-error exception dispatch or interrupt entry is emulated.
Fetch failures report PC and the bus error. Faults preserve the faulting PC and
pending target, stop execution, and return the same error until reset. Cache,
on-chip registers, timing and all remaining instructions are absent.

## Memory and reset

| Inclusive address range | Size | Access |
| --- | --- | --- |
| `00000000–0007FFFF` | 512 KiB | Read-only external BIOS, unmapped when absent |
| `00200000–002FFFFF` | 1 MiB | Low WRAM, read/write |
| `06000000–060FFFFF` | 1 MiB | High WRAM, read/write |

Byte access accepts any address; words require 2-byte and longs 4-byte alignment.
Alignment is checked before mapping, then the entire access is validated. Invalid
accesses return typed errors and failed writes change no bytes. All other regions,
RAM mirrors, cache-through aliases (including `+0x20000000`), MMIO and internal
SH-2 regions are deliberately unmapped. This is a bounded physical-map subset,
not the full address decoder. The alignment rules follow the
[SH7604 Hardware Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf),
§2.2.2 (pp.18–19); address exceptions are represented as host errors here.

All resets are deterministic **cold power-on** resets, clearing RAM, ROM,
registers, pending slots, counters and errors. They do not model the separate SH-2
manual-reset pins or Saturn SMPC reset sequence. External mode requires exactly
524288 bytes, maps them read-only, reads PC from address 0 and R15 from address 4,
sets VBR=0 and SR interrupt mask=15 (`SR=0xF0`). Hardware-undefined registers and
SR bits are initialized to zero for reproducibility, not claimed as power-on
hardware values. Sources: SH7604 §2.1.4 (p.17), §4.1.3 table 4.3 (p.67),
§4.2.2 (p.70). Invalid BIOS resets leave external mode selected, memory cleared
and CPU in Reset; there is no fallback. Vector alignment/mapping is checked on
fetch, so a correctly sized but invalid image may fault on its first step.

## HLE bootstrap prototype and manual trace

HLE mode maps no ROM. The original `FIXTURE` words are written into high WRAM
at `0x06000000`, with PC set there and R15=`0x06100000` (one past the bank).
No BIOS services are intercepted or replaced yet.

| Address | Word | Original assembly | Result after step |
| --- | --- | --- | --- |
| `06000000` | `E005` | MOV #5,R0 | R0=5, PC=`06000002` |
| `06000002` | `70FF` | ADD #-1,R0 | R0=4, PC=`06000004` |
| `06000004` | `6103` | MOV R0,R1 | R1=4, PC=`06000006` |
| `06000006` | `A001` | BRA +1 | Target=`0600000C`, PC=`06000008` |
| `06000008` | `7103` | ADD #3,R1 (slot) | R1=7, PC=`0600000C` |
| `0600000A` | `E163` | MOV #99,R1 | Skipped |
| `0600000C` | `0009` | NOP | PC=`0600000E`; six instructions retired |
| `0600000E` | `AFFE` | BRA -2 | Target=`0600000E`, PC=`06000010` |
| `06000010` | `0009` | NOP (slot) | PC=`0600000E` |

This manually calculated trace is asserted by native tests and browser smoke
tests. It is BIOS-independent startup, not a compatible HLE Saturn BIOS.

## Later milestones (not implemented)

1. Expand the interpreter with exception/interrupt handling and differential
   traces. Keep it as the correctness reference for future execution engines.
2. Model a separate slave SH-2 state sharing the external bus; add deterministic
   scheduling, bus contention, cache behavior and CPU communication. The
   [Dual CPU User's Guide](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-202-R1-120994.pdf),
   §1.1 (p.1) and §4 (p.3), describes shared devices and separate initialization.
   Boot ROM software initialization is distinct from CPU power-on vectors.
3. Add SCU DMA/DSP/interrupts, SMPC reset/input, VDP1 sprite/polygon processing,
   VDP2 backgrounds/composition, 68EC000/SCSP audio and CD-block behavior. Their
   roles follow the [Saturn Overview Manual](https://www.infochunk.com/saturn/segahtml_en/hard/intr/index.htm),
   §2.2–2.3 and §3.1–3.7. Consult each device manual before implementing registers.
4. Implement individually documented HLE service contracts and tests, then
   local user-disc loading and CD boot. The
   [Boot ROM User's Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-079B-R3-011895.pdf),
   §1–3 (pp.3–8), describes startup and services, but does not establish a complete
   service ABI. Verify those contracts against the System Program/Backup Library
   manuals before claiming HLE compatibility. Region/version differences remain
   unmodeled. [Libretro's Yabause BIOS guidance](https://docs.libretro.com/library/yabause/#bios)
   demonstrates BIOS emulation feasibility and reports reduced compatibility and
   multi-disc limitations; it is not evidence of this prototype's compatibility.
5. Once traces and benchmarks exist, compare the interpreter with predecoded and
   block interpreters, then consider SH-2→Wasm dynamic recompilation and Web
   Workers. Browser CSP, compilation overhead, worker isolation and shared-memory
   requirements need evaluation. No JIT, threading or performance tuning is part
   of this slice.

## Sources and provenance

The Rust implementation, browser client, fixture and synthetic test vectors were
written for this repository from the hardware behavior above. No processor,
Saturn or HLE core, upstream emulator source, SDK sample, manual pseudocode or
firmware was copied or transliterated. The
[saturn-docs index](https://github.com/luckyseoul/saturn-docs) was used to locate
the original Hitachi/Sega manuals; its synthesized emulator-reference notes were
not used as implementation code. Manuals were consulted externally, not vendored.
Their publication in an archive does not grant redistribution rights to their
text, software examples or firmware. Unverified hardware behavior remains
explicitly unsupported rather than guessed.

The only direct Rust dependency is wasm-bindgen (MIT/Apache-2.0) on Wasm;
its infrastructure dependencies retain their own package licenses. Playwright
(Apache-2.0, with dependencies identified in `package-lock.json`) is used only
for development checks. There is no top-level
project license file at this milestone; dependency licenses do not supply a
license for Sega content. Supply your own authorized BIOS locally. Do not commit
or redistribute BIOS, game ROMs/discs or other proprietary assets.
