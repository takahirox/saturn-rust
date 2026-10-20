# Hardware conformance for Issue #11

**Status: incomplete; Issue #11 must remain open.** This checkpoint expands the
standalone SH-2 interpreter. It does not deliver a comprehensive Saturn emulator.
No pre-merge acceptance criterion for the overarching Issue is marked complete.
A functional instruction row below is evidence for that operation, not for the
SH7604 chip, an integrated subsystem, timing accuracy or retail compatibility.

Inspected upstream main: `83c548fe48ee5878fde87ab97cc98a0dadbc7e85` (2026-10-20).
The assigned upstream checkpoint contained identical repository files. No technical
diagnosis was supplied by the readiness node. See [validation](validation-issue-11.md)
for actual checks. Update this matrix and the concrete gaps with every hardware change.

## References and evidence keys

- **P**: [Hitachi SH-1/SH-2 Programming Manual][P], §5 encodings, §6 instruction descriptions. Section numbers below refer to this manual; §6.30 is SH-1-only and excluded.
- **H**: [SH7604 Hardware Manual][H], especially §§2–4 and §14.3.
- **68**: [Motorola M68000 Family Programmer's Reference Manual][68], MC68000-applicable entries in the integer instruction descriptions. Later CPU extensions are excluded.
- **O**: [Saturn Overview][O], hardware configuration and memory map.
- Device references link original manuals, not reference-emulator implementations. A reference on a missing row identifies required work, not an implemented or verified behavior.

Native test keys refer to [src/cpu/tests.rs](../src/cpu/tests.rs):

| Key | Tests (exact names or shared name prefix) |
| --- | --- |
| decode | `all_65536_encodings_match_the_independent_manual_inventory` |
| arithmetic | `carry_borrow_and_signed_overflow_match_wide_arithmetic`, `add_immediate_sign_extends_and_wraps_without_changing_flags`, `arithmetic_flags_compare_unsigned_and_shift_carry_without_clobbering_sr` |
| compare | `comparisons_and_byte_string_comparison_cover_signed_and_unsigned_order` |
| move | `memory_instructions_obey_sign_extension_width_and_big_endian_access`, `indirect_auto_update_handles_source_destination_aliases`, `indexed_displaced_and_gbr_addressing_obey_width_and_sign_extension`, `pc_relative_word_and_mova_use_signed_data_and_slot_destination`, `mov_immediate_sign_extends_at_boundaries_into_every_register`, `mov_register_copies_all_32_bits_and_leaves_source_and_flags` |
| extend | `swaps_extensions_extract_and_negate_preserve_non_t_flags` |
| logic | `logical_register_immediate_and_byte_memory_operations` |
| shift | `shifts_and_rotates_cover_sign_carry_and_preservation` |
| multiply | `multiply_width_signedness_and_full_64_bit_accumulation` |
| mac | `mac_signed_products_saturation_and_same_register_postincrements` |
| divide | `div0s_initializes_q_m_t_for_every_sign_pair`, `unsigned_division_guest_sequences_produce_host_quotients`, `div1_signed_mode_vectors_cover_add_subtract_carry_and_borrow` |
| branch | `bra_latches_signed_pc_plus_four_target_and_executes_one_slot`, `calls_jumps_and_pc_relative_literals_use_architectural_pc`, `far_branches_latch_register_targets_and_call_return_addresses`, `conditional_branches_have_no_slot_and_use_signed_pc_plus_four`, `delayed_conditional_branch_latches_t_and_untaken_branch_has_no_slot`, `rts_returns_to_pr_after_one_slot_and_preserves_pr` |
| special | `control_and_system_register_transfers_cover_every_register_and_form`, `interrupts_wait_for_slots_and_successors_of_all_special_transfers` |
| exception | `trapa_rte_round_trip_restores_masked_sr_before_the_delay_slot`, `illegal_and_all_pc_writing_slot_instructions_dispatch_without_retiring`, `illegal_slots_dispatch_vector_six_and_save_the_branch_destination`, `unavailable_exception_vector_is_a_sticky_bounded_bus_diagnostic` |
| sleep | `sleep_is_bounded_and_interrupt_entry_obeys_mask_vector_and_wakes_cpu` |
| nop | `nop_only_advances_pc_and_instruction_count` |

**ROM** means [tests/sh2_system.rs](../tests/sh2_system.rs) and the same original
[synthetic ROM](../tests/fixtures/sh2-system.hex) executed through the real browser
Wasm in [browser-smoke.mjs](../scripts/browser-smoke.mjs). **SC** means actual Star
Collector guest execution in [tests/game.rs](../tests/game.rs) and browser smoke.
**—** means no evidence for that path. Decoder coverage is separate from semantic
coverage: it checks all 65,536 encodings against an independent inventory of 142
instruction forms (53,752 valid encodings, 11,784 illegal encodings).

## SH-2 instruction families

All rows describe functional interpreter behavior. Common outstanding limitations:
no instruction cycles, pipeline/bus contention, architectural address-error
exceptions or cache interactions. A valid opcode can still encounter a diagnostic
when it accesses a missing device. Architectural valid-instruction decoding does
not turn those missing hardware paths into supported behavior.

| Family | P section | Implemented behavior / tested edges | Native | Browser | Additional limitation |
| --- | --- | --- | --- | --- | --- |
| ADD | 6.2 | Register/immediate, signed imm8, wrapping | arithmetic | SC, ROM | Common limits |
| ADDC | 6.3 | Carry input/output, full-width carry boundaries | arithmetic | — | Common limits |
| ADDV | 6.4 | Signed overflow boundaries, preserve other SR bits | arithmetic | — | Common limits |
| AND | 6.5 | Register/zero-extended imm8/GBR byte RMW | logic | — | No cross-CPU bus arbitration |
| BF | 6.6 | Signed displacement, both T conditions | branch | SC | Common limits |
| BF/S | 6.7 | Taken slot; untaken sequential; latch T | branch | — | See discrepancy below |
| BRA | 6.8 | Signed disp12 bounds; exactly one slot | branch | SC, ROM | Common limits |
| BRAF | 6.9 | Relative register target, wrapping, latching | branch | — | Common limits |
| BSR | 6.10 | Signed disp12, PR=address+4, slot | branch | SC | Common limits |
| BSRF | 6.11 | Relative register target and latched PR | branch | — | Common limits |
| BT | 6.12 | Signed displacement, both T conditions | branch | SC | Common limits |
| BT/S | 6.13 | Taken slot; untaken sequential; latch T | branch | ROM | See discrepancy below |
| CLRMAC | 6.14 | Clear MACH and MACL | multiply | — | Common limits |
| CLRT | 6.15 | Clear T only | logic | — | Common limits |
| CMP/cond | 6.16 | EQ reg/imm, HS/HI, GE/GT, PL/PZ, STR byte matches | compare | SC (EQ/HS) | Common limits |
| DIV0S | 6.17 | All operand-sign combinations of Q/M/T | divide | — | Common limits |
| DIV0U | 6.18 | Clear Q/M/T, preserve other flags | divide | — | Common limits |
| DIV1 | 6.19 | Non-restoring add/subtract, Q/M/T, 32-step guest unsigned division | divide | — | No exhaustive signed-division sequence corpus |
| DMULS.L | 6.20 | Signed 32×32 to MACH:MACL | multiply | — | Common limits |
| DMULU.L | 6.21 | Unsigned 32×32 to MACH:MACL | multiply | — | Common limits |
| DT | 6.22 | Wrapping decrement, zero sets T | arithmetic | SC | Common limits |
| EXTS | 6.23 | Byte/word sign extension | extend | — | Common limits |
| EXTU | 6.24 | Byte/word zero extension | extend | — | Common limits |
| JMP | 6.25 | Latched absolute target, one slot | branch | SC | Common limits |
| JSR | 6.26 | Latched target and PR | branch | — | Common limits |
| LDC | 6.27 | SR/GBR/VBR, reg/postincrement; SR reserved-bit mask | special | — | Device IRQ wiring absent |
| LDS | 6.28 | MACH/MACL/PR, reg/postincrement | special | — | Device IRQ wiring absent |
| MAC.L | 6.29 | Signed product, wrapping 64-bit / saturated signed 48-bit; same-register increments | mac | — | Upper 16 MACH bits in S mode deterministic; no hardware comparison |
| MAC.W | 6.31 | Signed product, wrapping 64-bit / saturated MACL, same-register increments | mac | — | SH-2-specific MACH preservation; see discrepancy |
| MOV indirect/indexed/auto-update/reg | 6.32 | B/W/L, sign extension, n=m rules, R0 indexing | move | SC, ROM | Common limits |
| MOV immediate/PC-relative | 6.33 | Sign extension, alignment, slot PC | move, branch | SC, ROM | Common limits |
| MOV GBR-relative | 6.34 | B/W/L, unsigned scaled disp8 | move | — | Common limits |
| MOV displacement | 6.35 | B/W disp4 with R0, L disp4 with registers | move | — | Common limits |
| MOVA | 6.36 | Aligned architectural PC, slot destination | move | — | Common limits |
| MOVT | 6.37 | Copy T to register | logic, exception | — | Common limits |
| MUL.L | 6.38 | Low 32-bit result, preserve MACH | multiply | — | Common limits |
| MULS.W | 6.39 | Signed low words, preserve MACH | multiply | — | Common limits |
| MULU.W | 6.40 | Unsigned low words, preserve MACH | multiply | — | Common limits |
| NEG | 6.41 | Two's complement, preserve SR | extend | — | Common limits |
| NEGC | 6.42 | Negate with borrow input/output | arithmetic | — | Common limits |
| NOP | 6.43 | Only PC/count change | nop | SC | Common limits |
| NOT | 6.44 | Full-width complement | extend | — | Common limits |
| OR | 6.45 | Register/imm/GBR byte | logic | — | Common limits |
| ROTCL | 6.46 | Incoming T and outgoing MSB | shift, divide | — | Common limits |
| ROTCR | 6.47 | Incoming T and outgoing LSB | shift | — | Common limits |
| ROTL | 6.48 | Wrap MSB into LSB and T | shift | — | Common limits |
| ROTR | 6.49 | Wrap LSB into MSB and T | shift | — | Common limits |
| RTE | 6.50 | Restore masked SR before slot; restore stack/PC | exception | ROM | Failed stack reads remain host diagnostics |
| RTS | 6.51 | Latched PR, slot | branch | SC | Common limits |
| SETT | 6.52 | Set T only | logic | ROM | Common limits |
| SHAL | 6.53 | Left shift, outgoing MSB to T | shift | — | Common limits |
| SHAR | 6.54 | Arithmetic right, outgoing LSB to T | shift | — | Common limits |
| SHLL | 6.55 | Logical left, outgoing MSB to T | shift, arithmetic | SC | Common limits |
| SHLLn | 6.56 | Shift 2/8/16, preserve T | shift | SC, ROM | Common limits |
| SHLR | 6.57 | Logical right, outgoing LSB to T | shift | — | Common limits |
| SHLRn | 6.58 | Shift 2/8/16, preserve T | shift | — | Common limits |
| SLEEP | 6.59 | Retire once, bounded waiting, accepted CPU IRQ wakes | sleep | ROM (waiting) | Standby control and integrated wake sources missing |
| STC | 6.60 | SR/GBR/VBR, register/predecrement, IRQ inhibition | special | — | Common limits |
| STS | 6.61 | MACH/MACL/PR, register/predecrement, IRQ inhibition | special | — | Common limits |
| SUB | 6.62 | Wrapping subtraction, preserve SR | arithmetic | SC | Common limits |
| SUBC | 6.63 | Borrow input/output boundaries | arithmetic | — | Common limits |
| SUBV | 6.64 | Signed overflow boundaries | arithmetic | — | Common limits |
| SWAP | 6.65 | Swap low bytes / 16-bit halves | extend | — | Common limits |
| TAS.B | 6.66 | T from old zero, set bit7 in memory | logic | — | Sequential RMW; multi-master lock not integrated |
| TRAPA | 6.67 | Immediate vector, SR/next PC stack, no delay | exception | ROM | No system BIOS HLE services |
| TST | 6.68 | Register/imm/GBR byte, preserve memory | logic | SC | Common limits |
| XOR | 6.69 | Register/imm/GBR byte | logic | — | Common limits |
| XTRCT | 6.70 | Join lower source and upper destination halves | extend | — | Common limits |

## SH7604 and system CPU behavior

| Feature / reference | Implemented behavior and edges | Native | Browser | Outstanding failure |
| --- | --- | --- | --- | --- |
| Register/reset model, H §§2/4.2 | Separate Cpu instances possible, power-on PC/SP vectors, SR=F0 | boot.rs | synthetic BIOS | Only master is owned/scheduled by Machine; manual reset absent |
| General/slot illegal, H §4.5 | Vectors 4/6, correct saved PC, no retirement | exception | ROM | No double-fault bus handling |
| TRAPA/RTE, H §§4.5/4.7 | Guest stack layout and handler return | exception, sh2_system.rs | ROM | System-level IRQ sources absent |
| CPU address errors, H §4.3 | Bus validates alignment | bus.rs, CPU fault tests | bus diagnostic | **Missing** architectural vector 9 and delayed acceptance |
| Maskable IRQ/NMI, H §4.4 | Explicit caller-supplied level/vector, I mask, NMI vector 11, stack/wake | sleep, special | — | Missing device priority selection, assertion/acknowledgment, NMI signal latch |
| Deferral, H §4.6 | Delay slots and one successor of control/system transfers | special | ROM (slots) | Address-error deferral missing |
| Cache/address regions, H §2 and cache chapter | P1 external-bus alias only | bus.rs | SC | Cache lines, purge/associative access and coherency absent |
| On-chip INTC, H interrupt controller | None | — | — | All registers/priority/vector interactions missing |
| FRT, H timer chapter | None | — | — | Counter, compares, capture, overflow, IRQ missing |
| Watchdog, H WDT chapter | None | — | — | Keyed registers, interval/reset modes missing |
| SCI, H serial chapter | None | — | — | Serial registers, timing/status/IRQ missing |
| On-chip DMAC, H DMA chapter | None | — | — | Channels, transfers, triggers/status/IRQ missing |
| DIVU, H division-unit chapter | None (separate from DIV1 instructions) | — | — | Mapped 32/64-bit division unit missing |
| BSC/UBC/standby, H bus/break/power chapters | None | — | — | Software-visible control/register behavior missing |
| Slave SH-2, O / Dual CPU User's Guide | None | — | — | Independent machine-owned CPU, reset/start/stop, signaling, scheduling missing |

## MC68EC000 instruction families

**Every row is missing**, with no native or browser conformance test. The core,
all effective-address modes, CCR/SR flags, user/supervisor stacks, exception/IRQ
entry, bus width/address rules, reset and sound-bus integration are absent. A
manual entry in this table is an outstanding requirement, never a checked box.

| Family | Manual entry (68, MC68000 subset) | Implementation | Native / browser |
| --- | --- | --- | --- |
| ABCD | ABCD instruction description | Missing | — / — |
| ADD | ADD instruction description | Missing | — / — |
| ADDA | ADDA instruction description | Missing | — / — |
| ADDI | ADDI instruction description | Missing | — / — |
| ADDQ | ADDQ instruction description | Missing | — / — |
| ADDX | ADDX instruction description | Missing | — / — |
| AND | AND instruction description | Missing | — / — |
| ANDI | ANDI instruction description | Missing | — / — |
| ASL | ASL instruction description | Missing | — / — |
| ASR | ASR instruction description | Missing | — / — |
| Bcc | Bcc instruction description | Missing | — / — |
| BCHG | BCHG instruction description | Missing | — / — |
| BCLR | BCLR instruction description | Missing | — / — |
| BSET | BSET instruction description | Missing | — / — |
| BSR | BSR instruction description | Missing | — / — |
| BTST | BTST instruction description | Missing | — / — |
| CHK | CHK instruction description | Missing | — / — |
| CLR | CLR instruction description | Missing | — / — |
| CMP | CMP instruction description | Missing | — / — |
| CMPA | CMPA instruction description | Missing | — / — |
| CMPI | CMPI instruction description | Missing | — / — |
| CMPM | CMPM instruction description | Missing | — / — |
| DBcc | DBcc instruction description | Missing | — / — |
| DIVS | DIVS instruction description | Missing | — / — |
| DIVU | DIVU instruction description | Missing | — / — |
| EOR | EOR instruction description | Missing | — / — |
| EORI | EORI instruction description | Missing | — / — |
| EXG | EXG instruction description | Missing | — / — |
| EXT | EXT instruction description | Missing | — / — |
| ILLEGAL | ILLEGAL instruction description | Missing | — / — |
| JMP | JMP instruction description | Missing | — / — |
| JSR | JSR instruction description | Missing | — / — |
| LEA | LEA instruction description | Missing | — / — |
| LINK | LINK instruction description | Missing | — / — |
| LSL | LSL instruction description | Missing | — / — |
| LSR | LSR instruction description | Missing | — / — |
| MOVE (including CCR/SR/USP transfers) | MOVE instruction description | Missing | — / — |
| MOVEA | MOVEA instruction description | Missing | — / — |
| MOVEM | MOVEM instruction description | Missing | — / — |
| MOVEP | MOVEP instruction description | Missing | — / — |
| MOVEQ | MOVEQ instruction description | Missing | — / — |
| MULS | MULS instruction description | Missing | — / — |
| MULU | MULU instruction description | Missing | — / — |
| NBCD | NBCD instruction description | Missing | — / — |
| NEG | NEG instruction description | Missing | — / — |
| NEGX | NEGX instruction description | Missing | — / — |
| NOP | NOP instruction description | Missing | — / — |
| NOT | NOT instruction description | Missing | — / — |
| OR | OR instruction description | Missing | — / — |
| ORI | ORI instruction description | Missing | — / — |
| PEA | PEA instruction description | Missing | — / — |
| RESET | RESET instruction description | Missing | — / — |
| ROL | ROL instruction description | Missing | — / — |
| ROR | ROR instruction description | Missing | — / — |
| ROXL | ROXL instruction description | Missing | — / — |
| ROXR | ROXR instruction description | Missing | — / — |
| RTE | RTE instruction description | Missing | — / — |
| RTR | RTR instruction description | Missing | — / — |
| RTS | RTS instruction description | Missing | — / — |
| SBCD | SBCD instruction description | Missing | — / — |
| Scc | Scc instruction description | Missing | — / — |
| STOP | STOP instruction description | Missing | — / — |
| SUB | SUB instruction description | Missing | — / — |
| SUBA | SUBA instruction description | Missing | — / — |
| SUBI | SUBI instruction description | Missing | — / — |
| SUBQ | SUBQ instruction description | Missing | — / — |
| SUBX | SUBX instruction description | Missing | — / — |
| SWAP | SWAP instruction description | Missing | — / — |
| TAS | TAS instruction description | Missing | — / — |
| TRAP | TRAP instruction description | Missing | — / — |
| TRAPV | TRAPV instruction description | Missing | — / — |
| TST | TST instruction description | Missing | — / — |
| UNLK | UNLK instruction description | Missing | — / — |

## Bus, timing and persistence

| Feature / reference | Current behavior / edge cases | Native | Browser | Outstanding failure |
| --- | --- | --- | --- | --- |
| BIOS ROM, O map | Exact 512 KiB, big endian, read-only, reset vectors | bus.rs, boot.rs | synthetic ROM | Real BIOS execution not validated |
| Low/high WRAM, O map | Independent 1 MiB banks; aligned B/W/L; P1 aliases | bus.rs | SC, ROM | Work RAM mirrors and timing missing |
| Base-system address decoding, O map | Only documented subset in architecture.md | bus.rs | SC | A-bus/cartridge decoding and full base map missing |
| MMIO widths/side effects, device manuals | Unsupported accesses return errors | devices.rs | SC | Full device routing/width rules missing |
| Backup RAM, O / Backup System Production Standard | None | — | — | Odd-byte lanes, mirrors, reset preservation and browser export/import missing |
| Shared event scheduler, O clock domains | Bounded active/blank instruction budgets only | game.rs | SC | Shared hardware time, scanlines, clocks/contention/order missing |
| Video timing, VDP2 display registers | Pollable frame VBLANK bit | game.rs | SC | HBLANK, scanlines, PAL/NTSC, interlace/field behavior and IRQs missing |
| Audio timing, SCSP/CD | None | — | — | Sample clocks, bounded stereo stream, underflow/muting missing |
| Power/reset domains, SMPC / H | Cold reset clears machine; CPU SLEEP separate | boot.rs, sh2_system.rs | ROM/reset | Hard/soft reset distinction, persistent memory, secondary CPU controls missing |

## SCU and DSP

All missing rows require executable native and Wasm guest fixtures; accepting or
storing a register without effects will not count as implementation.

| Feature / manual | Implemented behavior | Native / browser | Outstanding failure |
| --- | --- | --- | --- |
| SCU interrupt registers, [SCU] | None | — / — | Requests, masks, priorities, vectors, CPU delivery/acknowledgment |
| A/B-bus coordination, [SCU] | None | — / — | Access ordering/waits/errors |
| DMA channels 0–2 direct, [SCU] | None | — / — | Registers, lengths, increments, triggers, completion/errors |
| DMA indirect, [SCU] | None | — / — | Descriptor walking/end, address updates |
| DMA device interactions, [SCU] | None | — / — | VDP/sound/CD transfers and IRQs |
| SCU DSP execution/registers, [SCUDSP] | None | — / — | ALU/multiply, parallel operations, moves/immediates, jumps/loops, END/control |
| SCU DSP memory/transfers, [SCUDSP] | None | — / — | Internal banks, ports, address counters, DSP DMA |

## VDP1 and VDP2

| Feature / manual | Current behavior / edges | Native | Browser | Outstanding failure |
| --- | --- | --- | --- | --- |
| VDP1 VRAM/framebuffers/registers, [VDP1] | None | — | — | Entire map/access/state missing |
| Command list/control flow, [VDP1] | None | — | — | End/jump/call/return and bounds handling |
| Normal/scaled/distorted sprites, [VDP1] | None | — | — | Texture addressing, geometry and rasterization |
| Polygons/lines/polylines, [VDP1] | None | — | — | Primitive rasterization |
| Clip/local coordinates, [VDP1] | None | — | — | System/user clipping and coordinate state |
| Sprite texture/color modes, [VDP1] | None | — | — | Packed/palette/direct color, transparency/end codes |
| Shading/color calculation, [VDP1] | None | — | — | Gouraud, shadows, half luminance/transparency |
| Draw/erase/swap/status/IRQ, [VDP1] | None | — | — | Framebuffer lifecycle and SCU drawing-end integration |
| VDP2 VRAM, [VDP2] §3 | Bank-zero bitmap RAM | devices.rs | SC | VRAM banks/cycle-pattern arbitration; cycle words only stored |
| VDP2 CRAM, [VDP2] color RAM | None | — | — | Palette modes and access rules |
| TVMD/TVSTAT, [VDP2] §2.2 | 320×224 noninterlace, display enable and polled VBLANK | devices.rs | SC | PAL/NTSC/resolutions/interlace/beam status/IRQ |
| NBG0 bitmap, [VDP2] §§4–5 | RGB555 512-pixel pitch, bank 0, unity zoom | devices.rs | SC | Other formats, transparency, scroll/zoom |
| NBG0–3 tile/character, [VDP2] scroll screens | None | — | — | Pattern names, maps/planes, character formats |
| NBG1–3 bitmap/layer control, [VDP2] scroll screens | None | — | — | Applicable screen modes/VRAM/priority |
| Rotation backgrounds, [VDP2] rotation screens | None | — | — | RBGs, parameters, coefficients, rotation/switching |
| Line/vertical-cell scroll, [VDP2] scroll controls | None | — | — | Tables and per-line/cell effects |
| Windows/line windows, [VDP2] windows | None | — | — | Screen/sprite windows and logical operations |
| Mosaic, [VDP2] mosaic | None | — | — | Horizontal/vertical effects |
| Priority/composition, [VDP2] priority | PRINA=1 for single NBG0 only | devices.rs | SC | Multiple backgrounds and VDP1 sprite priorities |
| Color calculation/offset/shadow, [VDP2] color calculation | None | — | — | Ratios, blending, offset, shadow and special functions |
| Back/line color, [VDP2] color screens | Display-disabled black only | devices.rs | SC | Guest back/line color tables |
| Combined VDP1/VDP2 pipeline | None | — | — | Overlap, sprite priority, transparency and stable independent guest images |

## Sound

| Feature / manual | Implemented behavior | Native / browser | Outstanding failure |
| --- | --- | --- | --- |
| Sound RAM / SCSP mapping, [SCSP] | None | — / — | Sound CPU/bus access and RAM/register behavior |
| 32 slots, PCM8/16, [SCSP] slot registers | None | — / — | Sample decoding, loop/pitch and key on/off |
| Envelope, [SCSP] slot registers | None | — / — | Attack/decay/sustain/release and timing |
| FM/modulation/LFO, [SCSP] modulation | None | — / — | Routing, pitch/amplitude modulation and noise |
| Levels/pan/external inputs, [SCSP] mixer | None | — / — | Stereo mixing including CD-DA |
| Timers/interrupts, [SCSP] common registers | None | — / — | Counters, enables/pending/clear, 68k/SH-2 delivery |
| MIDI registers, [SCSP] MIDI | None | — / — | FIFOs/status/IRQ behavior |
| SCSP DMA, [SCSP] common registers | None | — / — | Transfer register effects/completion/IRQ |
| SCSP effects DSP, [SCSP] DSP programming model | None | — / — | Instruction execution, registers, ring buffers and routing |
| Browser audio output | None | — / — | Bounded stereo buffering, gesture enable/mute and underflow |
| CD-DA, [CD] / [SCSP] external audio | None | — / — | Track samples, pacing, playback status and mix path |

## CD, boot and SMPC

| Feature / manual | Current behavior / edge cases | Native | Browser | Outstanding failure |
| --- | --- | --- | --- | --- |
| Cooked ISO, [Disc Format][DISC] | Bounded single-area IP + simple ISO9660, first-read file | disc.rs | SC/local ISO | General layouts/size, runtime sectors |
| BIN/CUE/raw/mixed tracks, [DISC] / [CD] | None | — | — | CUE references, pregaps, raw sectors, TOC/audio and UI selection |
| CD command/status/HIRQ, [CD] | None | — | — | Command protocol, response timing/IRQ/acknowledgment |
| CD drive/TOC/session/track state, [CD] | None | — | — | Seek/spin/read/play/status transitions |
| CD buffers/transfers, [CD] | None | — | — | Sector pool, counts, transfer endpoints/IRQ |
| Filters/selectors/partitions, [CD] | None | — | — | Connection/filtering/partition data and commands |
| BIOS-less startup, [Boot ROM][BOOT] / [DISC] | Narrow original first-read HLE boot | disc.rs, game.rs | SC | General startup and region/version contracts |
| BIOS services, [BOOT] | None | — | — | System/backup/CD calls and per-call ABI/side effects |
| External BIOS execution, [BOOT] | Genuine reset vectors, original synthetic ROM handlers | boot.rs, sh2_system.rs | ROM | Genuine BIOS initialization/menu/boot **unverified**; no lawful local BIOS supplied |
| SMPC direct digital port 1, [SMPC] §3 | Active-low nibble selection and button levels | devices.rs | SC | Port 2 and full I/O mode behavior |
| SMPC commands/status/timing, [SMPC] command reference | None | — | — | SF, command sequencing/latency, INTBACK/status/interrupt |
| SMPC reset/power/clock, [SMPC] system management | None | — | — | Master/slave/sound CPU controls and system reset/clock changes |
| RTC/region/system flags, [SMPC] | None | — | — | RTC registers/commands, deterministic clock input, flags |
| Digital port 2 and analog pad, [SMPC] peripheral data | None | — | — | Both-port reports, analog axes/buttons, browser mapping |
| Input focus safety | Host button levels, blur/view/tab release, no guest-memory shortcuts | game.rs | SC | Additional controller types absent |

## Concrete unresolved gaps and closure gates

1. SH-2 architectural address errors and their delayed acceptance, reset domains,
   cache/coherency, mapped SH7604 peripherals, bus locking and integrated IRQs;
   independent slave execution and shared scheduling. CPU opcode decoding alone
   does not satisfy the CPU or multiprocessor criterion.
2. Every MC68EC000 instruction family above, sound bus, SCSP synthesis/timing/DSP,
   CD-DA mixing and real browser audio. The current build remains silent.
3. Full base memory map, backup RAM/persistence and hardware clock/event scheduler.
4. SCU register/interrupt controller, all DMA modes/channels and executing SCU DSP.
5. VDP1 and remaining VDP2 modes/effects/composition/timing, with independent
   expected pixel outputs. Existing single bitmap mode is the only video evidence.
6. Runtime CD block, BIN/CUE/mixed tracks, complete BIOS HLE ABI, full native BIOS
   startup, SMPC commands/RTC/port 2/analog input and system controls.
7. Second original licensed VDP1+VDP2+audio disc, dual-CPU/SCU/runtime-CD guest
   fixtures, both DSP test programs and required video/reset/backup fixtures.
   The synthetic CPU ROM is not a substitute for those integration examples.
8. Lawful local real-BIOS verification beyond vectors. No genuine BIOS was supplied
   or searched for, and no retail/native BIOS compatibility is claimed. Future
   verification must record only version/hash identifiers and observable results;
   never commit or transmit firmware. Hardware/software gaps above must be fixed
   before this verification is meaningful.
9. Full conformance review and CI/native/Wasm/browser evidence for all the above.
   Existing subset tests pass locally, not a complete hardware conformance suite.
10. Required public-site verification after eventual full implementation, merge and
    Pages deployment. No push, PR, merge, comment or deployment is part of this node.

## Provenance and manual discrepancies

This checkpoint independently implements behavior from the linked primary manuals.
No emulator implementation, lookup table, CPU library, SDK object or firmware was
copied or imported. Wide Rust arithmetic expresses carry, overflow, signed products
and saturation directly; DIV1 is derived as a non-restoring arithmetic recurrence.
The encoding inventory records architectural bit-pattern facts independently of
the decoder. Unit vectors and the synthetic ROM are original; [fixture licensing](../tests/fixtures/LICENSE)
applies to files in that directory. No project-wide relicensing is implied.
Manual PDFs and temporary extraction tools were used outside the worktree and are
not redistributed. No differential reference-emulator or physical-console
observations were used in this checkpoint.

- P §6.31 describes a MACH overflow flag but its SH-2-specific annotation on
  printed p.110 excludes the SH-1 flag update. This interpreter preserves MACH
  during saturated MAC.W; the native test checks preservation even at both limits.
- P §§6.7/6.13 describe slot restrictions broadly, while the specified operation
  executes a slot only for a taken conditional branch. Untaken BT/S and BF/S
  advance sequentially here; the native test asserts that no delay state remains.
  Physical/differential confirmation of this distinction is still absent.
- P §6.29 defines signed saturation on the lower 48 accumulator bits. This
  interpreter deterministically sign-extends the saturated value into the full
  64-bit pair; validation of upper MACH bits against hardware is outstanding.
- H §4.5.3 explicitly saves the branch's jump destination on a slot-illegal
  exception. The shared guest fixture returns to that destination with RTE.

[P]: https://github.com/luckyseoul/saturn-docs/blob/main/files/h12p0.pdf
[H]: https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf
[68]: https://github.com/luckyseoul/saturn-docs/blob/main/files/M68000PRM.pdf
[O]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-103-R1-040194.pdf
[SCU]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-097-R5-072694.pdf
[SCUDSP]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-240-A-042795.pdf
[VDP1]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-013-R3-061694.pdf
[VDP2]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-058-R2-060194.pdf
[SCSP]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-077-R2-052594.pdf
[SMPC]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-169-R1-072694.pdf
[CD]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-162-R1-092994.pdf
[BOOT]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-079B-R3-011895.pdf
[DISC]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-040-R4-051795.pdf
