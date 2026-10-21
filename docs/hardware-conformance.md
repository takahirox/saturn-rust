# Hardware conformance for Issue #17

**Status: incomplete; Issue #17 is the active continuation tracker.** Issues
#11/#13/#15 remain administratively closed; do not reopen them. This checkpoint
includes independent slave SH-2 execution, CPU-local FRT/WDT/SCI/DIVU/DMAC,
internal interrupt arbitration, write-through caches, timed SMPC CPU control,
SCU interrupt/DMA/DSP execution and clock-based NTSC raster events. It preserves the address-error fixtures, Star Collector,
backup RAM and both direct digital pads. It does not complete the Saturn hardware
or either required new demonstration. No umbrella acceptance checkbox is met.

Inspected upstream main: `dbd981b030a46daa6923072ce240680dd2e58ebf` (2026-10-21),
verified with `git ls-remote origin refs/heads/main`. This revision starts from
`6e0a446e225db835f1fec7b1c47db5b13b150c98`, preserving the earlier review and
SH7604 fixes. The full outstanding hardware scope remains mandatory. See [current validation](validation-issue-17.md),
[historical address-error validation](validation-issue-15.md),
[backup/pad validation](validation-issue-13.md) and the
[historical SH-2 checkpoint](validation-issue-11.md). **Pending** means required
coverage or integrated validation remains missing, even when a subset passes.

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
| divide | `div0s_initializes_q_m_t_for_every_sign_pair`, `unsigned_division_guest_sequences_produce_host_quotients`, `div1_signed_mode_vectors_cover_add_subtract_carry_and_borrow`, `div1_same_register_reads_the_shifted_divisor_for_all_q_m_t_combinations` |
| branch | `bra_latches_signed_pc_plus_four_target_and_executes_one_slot`, `calls_jumps_and_pc_relative_literals_use_architectural_pc`, `far_branches_latch_register_targets_and_call_return_addresses`, `conditional_branches_have_no_slot_and_use_signed_pc_plus_four`, `delayed_conditional_branch_latches_t_and_untaken_branch_has_no_slot`, `rts_returns_to_pr_after_one_slot_and_preserves_pr` |
| special | `control_and_system_register_transfers_cover_every_register_and_form`, `interrupts_wait_for_slots_and_successors_of_all_special_transfers`, `delayed_conditional_branches_defer_irq_and_nmi_for_both_conditions` |
| exception | `trapa_rte_round_trip_restores_masked_sr_before_the_delay_slot`, `illegal_and_all_pc_writing_slot_instructions_dispatch_without_retiring`, `illegal_slots_dispatch_vector_six_and_save_the_branch_destination`, `unavailable_exception_vector_is_a_sticky_bounded_bus_diagnostic` |
| sleep | `sleep_is_bounded_and_interrupt_entry_obeys_mask_vector_and_wakes_cpu` |
| nop | `nop_only_advances_pc_and_instruction_count` |

**AE** means [src/cpu/address_tests.rs](../src/cpu/address_tests.rs), whose ten
tests cover vector 9, misaligned operands across addressing modes, completed
auto-updates, saved PC/SR, branch deferral, SLEEP wake, on-chip width/PC-relative/
TAS restrictions and bounded entry failure. Implementation:
[`Cpu::step`, `Cpu::execute`, `Cpu::address_error`](../src/cpu.rs) and
[`Operands::invalid/read/write`](../src/cpu/memory.rs). **AEROM** means
[tests/sh2_address_errors.rs](../tests/sh2_address_errors.rs) and the same original
[synthetic BIOS](../tests/fixtures/sh2-address-errors.hex) executed by the real
browser Wasm in [browser-smoke.mjs](../scripts/browser-smoke.mjs). The guest logs
three independently expected PC/SR frames, repairs an odd PC and returns via RTE.
Neither fixture is Sega firmware or a complete BIOS-service implementation.

**ROM** means [tests/sh2_system.rs](../tests/sh2_system.rs) and the same original
[synthetic ROM](../tests/fixtures/sh2-system.hex) executed through the real browser
Wasm in [browser-smoke.mjs](../scripts/browser-smoke.mjs). **BK** means the original [backup/port-2 guest](../tests/fixtures/backup-pad.hex)
and [tests/backup.rs](../tests/backup.rs), including a temporary local ISO with
the same first-read program. Browser BK evidence uses the real Wasm in
[browser-smoke.mjs](../scripts/browser-smoke.mjs). It proves only odd-byte
loads/stores, reset retention and direct digital input, not BIOS services or full
SMPC timing. It is a diagnostic, not either of the two required new demos.
**DUAL** means [tests/dual_sh2.rs](../tests/dual_sh2.rs) and the identical
[original guest BIOS](../tests/fixtures/dual-sh2.s), reproducibly built as
[dual-sh2.hex](../tests/fixtures/dual-sh2.hex) and executed through browser Wasm
in [browser-smoke.mjs](../scripts/browser-smoke.mjs). It computes shared results
using both CPUs and acknowledges FRT interrupts in guest handlers.
**FRT** means the six manual-derived unit tests in
[src/onchip.rs](../src/onchip.rs), plus the original
[compare IRQ guest](../tests/fixtures/frt-compare.hex) executed by
[tests/dual_sh2.rs](../tests/dual_sh2.rs) and browser Wasm in
[browser-smoke.mjs](../scripts/browser-smoke.mjs). Compare=0/3 tests check the
edge leaving OCRA, SLEEP wake, exception frames, guest acknowledgment and RTE
(H §§11.4.3/11.4.6, figs. 11.7/11.11). These tests do not establish physical timing.
**SC** means actual Star
Collector guest execution in [tests/game.rs](../tests/game.rs) and browser smoke.
**—** means no evidence for that path. Decoder coverage is separate from semantic
coverage: it checks all 65,536 encodings against an independent inventory of 142
instruction forms (53,752 valid encodings, 11,784 illegal encodings).

**PERIPH** means [the eleven independent peripheral tests](../src/onchip/peripheral_tests.rs)
and [the original guest integration](../tests/sh7604.rs), executing
[sh7604.s](../tests/fixtures/sh7604.s) as [sh7604.hex](../tests/fixtures/sh7604.hex)
in native and browser Wasm. It computes signed division results, copies guest
RAM using DMAC, acknowledges DMA/WDT interrupts, handles DMA address error 10
at I=15, polls SCI transmission, performs a watchdog manual reset and recovers
from misaligned exception stacking. **CACHE** means the four vectors in
[tests/cache.rs](../tests/cache.rs) plus PERIPH's DMA/purge/reset guest checks.
The fixture is MIT licensed and reproducible using
[build_sh7604.py](../tests/fixtures/build_sh7604.py), which CI verifies. It is a
CPU diagnostic ROM, not either required new demonstration disc.

**SCUROM** means [tests/scu.rs](../tests/scu.rs) and the identical original
[SCU fixture](../tests/fixtures/build_scu.py), generated as
[scu.hex](../tests/fixtures/scu.hex) and executed by browser Wasm in
[browser-smoke.mjs](../scripts/browser-smoke.mjs). SH-2 instructions load and
execute DSP product/sum programs, configure direct/indirect DMA to VRAM and
acknowledge six interrupts. Independent expected results are 84 and 48, not
host-computed guest output. **SCUV** means [src/scu/tests.rs](../src/scu/tests.rs),
including golden ALU/parallel-bus/control/DMA/timer vectors; **TIME** means
[src/timing.rs](../src/timing.rs) and the SCU timing tests in
[src/bus.rs](../src/bus.rs). All use [SCU] §§2–4 corrected by
[SCU Final Specifications][SCUFINAL] #1–27/30–36. SCUROM is a diagnostic synthetic
ROM, not either required demonstration disc.

## SH-2 instruction families

All rows describe functional interpreter behavior. Common outstanding limitations:
no instruction cycles or pipeline/bus contention; CPU-local cache semantics are tested separately.
Address-error entry is implemented with the invalid-cycle limitations below.
A valid opcode can still encounter a diagnostic
when it accesses a missing device. Architectural valid-instruction decoding does
not turn those missing hardware paths into supported behavior.

| Family | P section | Implemented behavior / tested edges | Native | Browser | Additional limitation | Status |
| --- | --- | --- | --- | --- | --- | --- |
| ADD | 6.2 | Register/immediate, signed imm8, wrapping | arithmetic | SC, ROM | Common limits | Pending |
| ADDC | 6.3 | Carry input/output, full-width carry boundaries | arithmetic | — | Common limits | Pending |
| ADDV | 6.4 | Signed overflow boundaries, preserve other SR bits | arithmetic | — | Common limits | Pending |
| AND | 6.5 | Register/zero-extended imm8/GBR byte RMW | logic | — | No cross-CPU bus arbitration | Pending |
| BF | 6.6 | Signed displacement, both T conditions | branch | SC | Common limits | Pending |
| BF/S | 6.7 | Taken slot; untaken sequential; latch T; IRQ/NMI deferred on both paths (H §4.6) | branch, special | — | See discrepancy below | Pending |
| BRA | 6.8 | Signed disp12 bounds; exactly one slot | branch | SC, ROM | Common limits | Pending |
| BRAF | 6.9 | Relative register target, wrapping, latching | branch | — | Common limits | Pending |
| BSR | 6.10 | Signed disp12, PR=address+4, slot | branch | SC | Common limits | Pending |
| BSRF | 6.11 | Relative register target and latched PR | branch | — | Common limits | Pending |
| BT | 6.12 | Signed displacement, both T conditions | branch | SC | Common limits | Pending |
| BT/S | 6.13 | Taken slot; untaken sequential; latch T; IRQ/NMI deferred on both paths (H §4.6) | branch, special | ROM | See discrepancy below | Pending |
| CLRMAC | 6.14 | Clear MACH and MACL | multiply | — | Common limits | Pending |
| CLRT | 6.15 | Clear T only | logic | — | Common limits | Pending |
| CMP/cond | 6.16 | EQ reg/imm, HS/HI, GE/GT, PL/PZ, STR byte matches | compare | SC (EQ/HS) | Common limits | Pending |
| DIV0S | 6.17 | All operand-sign combinations of Q/M/T | divide | — | Common limits | Pending |
| DIV0U | 6.18 | Clear Q/M/T, preserve other flags | divide | — | Common limits | Pending |
| DIV1 | 6.19 | Non-restoring add/subtract, Q/M/T, same-register read after shift, 32-step guest unsigned division | divide | — | No exhaustive signed-division sequence corpus | Pending |
| DMULS.L | 6.20 | Signed 32×32 to MACH:MACL | multiply | — | Common limits | Pending |
| DMULU.L | 6.21 | Unsigned 32×32 to MACH:MACL | multiply | — | Common limits | Pending |
| DT | 6.22 | Wrapping decrement, zero sets T | arithmetic | SC | Common limits | Pending |
| EXTS | 6.23 | Byte/word sign extension | extend | — | Common limits | Pending |
| EXTU | 6.24 | Byte/word zero extension | extend | — | Common limits | Pending |
| JMP | 6.25 | Latched absolute target, one slot | branch | SC | Common limits | Pending |
| JSR | 6.26 | Latched target and PR | branch | — | Common limits | Pending |
| LDC | 6.27 | SR/GBR/VBR, reg/postincrement; SR reserved-bit mask | special | — | Non-FRT device IRQ wiring absent | Pending |
| LDS | 6.28 | MACH/MACL/PR, reg/postincrement | special | — | Non-FRT device IRQ wiring absent | Pending |
| MAC.L | 6.29 | Signed product, wrapping 64-bit / saturated signed 48-bit; same-register increments | mac | — | Upper 16 MACH bits in S mode deterministic; no hardware comparison | Pending |
| MAC.W | 6.31 | Signed product, wrapping 64-bit / saturated MACL, same-register increments | mac | — | SH-2-specific MACH preservation; see discrepancy | Pending |
| MOV indirect/indexed/auto-update/reg | 6.32 | B/W/L, sign extension, n=m rules, R0 indexing | move | SC, ROM | Common limits | Pending |
| MOV immediate/PC-relative | 6.33 | Sign extension, alignment, slot PC | move, branch | SC, ROM | Common limits | Pending |
| MOV GBR-relative | 6.34 | B/W/L, unsigned scaled disp8 | move | — | Common limits | Pending |
| MOV displacement | 6.35 | B/W disp4 with R0, L disp4 with registers | move | — | Common limits | Pending |
| MOVA | 6.36 | Aligned architectural PC, slot destination | move | — | Common limits | Pending |
| MOVT | 6.37 | Copy T to register | logic, exception | — | Common limits | Pending |
| MUL.L | 6.38 | Low 32-bit result, preserve MACH | multiply | — | Common limits | Pending |
| MULS.W | 6.39 | Signed low words, preserve MACH | multiply | — | Common limits | Pending |
| MULU.W | 6.40 | Unsigned low words, preserve MACH | multiply | — | Common limits | Pending |
| NEG | 6.41 | Two's complement, preserve SR | extend | — | Common limits | Pending |
| NEGC | 6.42 | Negate with borrow input/output | arithmetic | — | Common limits | Pending |
| NOP | 6.43 | Only PC/count change | nop | SC | Common limits | Pending |
| NOT | 6.44 | Full-width complement | extend | — | Common limits | Pending |
| OR | 6.45 | Register/imm/GBR byte | logic | — | Common limits | Pending |
| ROTCL | 6.46 | Incoming T and outgoing MSB | shift, divide | — | Common limits | Pending |
| ROTCR | 6.47 | Incoming T and outgoing LSB | shift | — | Common limits | Pending |
| ROTL | 6.48 | Wrap MSB into LSB and T | shift | — | Common limits | Pending |
| ROTR | 6.49 | Wrap LSB into MSB and T | shift | — | Common limits | Pending |
| RTE | 6.50 | Restore masked SR before slot; restore stack/PC | exception | ROM | Failed stack reads remain host diagnostics | Pending |
| RTS | 6.51 | Latched PR, slot | branch | SC | Common limits | Pending |
| SETT | 6.52 | Set T only | logic | ROM | Common limits | Pending |
| SHAL | 6.53 | Left shift, outgoing MSB to T | shift | — | Common limits | Pending |
| SHAR | 6.54 | Arithmetic right, outgoing LSB to T | shift | — | Common limits | Pending |
| SHLL | 6.55 | Logical left, outgoing MSB to T | shift, arithmetic | SC | Common limits | Pending |
| SHLLn | 6.56 | Shift 2/8/16, preserve T | shift | SC, ROM | Common limits | Pending |
| SHLR | 6.57 | Logical right, outgoing LSB to T | shift | — | Common limits | Pending |
| SHLRn | 6.58 | Shift 2/8/16, preserve T | shift | — | Common limits | Pending |
| SLEEP | 6.59 | Retire once, bounded waiting, accepted CPU IRQ wakes | sleep | ROM (waiting) | Standby control and integrated wake sources missing | Pending |
| STC | 6.60 | SR/GBR/VBR, register/predecrement, IRQ inhibition | special | — | Common limits | Pending |
| STS | 6.61 | MACH/MACL/PR, register/predecrement, IRQ inhibition | special | — | Common limits | Pending |
| SUB | 6.62 | Wrapping subtraction, preserve SR | arithmetic | SC | Common limits | Pending |
| SUBC | 6.63 | Borrow input/output boundaries | arithmetic | — | Common limits | Pending |
| SUBV | 6.64 | Signed overflow boundaries | arithmetic | — | Common limits | Pending |
| SWAP | 6.65 | Swap low bytes / 16-bit halves | extend | — | Common limits | Pending |
| TAS.B | 6.66 | T from old zero, set bit7 in memory | logic | — | Sequential RMW; multi-master lock not integrated | Pending |
| TRAPA | 6.67 | Immediate vector, SR/next PC stack, no delay | exception | ROM | No system BIOS HLE services | Pending |
| TST | 6.68 | Register/imm/GBR byte, preserve memory | logic | SC | Common limits | Pending |
| XOR | 6.69 | Register/imm/GBR byte | logic | — | Common limits | Pending |
| XTRCT | 6.70 | Join lower source and upper destination halves | extend | — | Common limits | Pending |

## SH7604 and system CPU behavior

| Feature / reference | Implemented behavior and edges | Native | Browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- | --- |
| Register/reset model, H §§2/4.2 | Independent CPUs; power-on 0/4 and watchdog manual-reset 8/12 vectors, SR=F0; 512-clock watchdog reset hold retains shared RAM/backup and clears CCR | boot.rs, DUAL, PERIPH | DUAL, PERIPH | External reset pin timing, BSC/UBC preservation and BCR1 CPU identification absent | Pending |
| General/slot illegal, H §4.5 | Vectors 4/6, correct saved PC, no retirement | exception | ROM | No double-fault bus handling | Pending |
| TRAPA/RTE, H §§4.5/4.7 | Guest stack layout and handler return | exception, sh2_system.rs | ROM | Non-FRT system IRQ sources absent | Pending |
| CPU address errors, H §§4.3/4.6/4.7/4.8.3, Tables 4.6/4.10/4.11 | Vector 9, operand/fetch checks, next-PC/SR, auto-update and deferral; misaligned stacking completes original exception then accepts address error without recursion | AE, AEROM, PERIPH | AEROM, PERIPH | Invalid data and unaligned stacking bytes are deterministic policies rather than physical bus signals; misaligned VBR unsupported | Pending |
| Maskable IRQ/NMI, H §4.4 | Explicit caller-supplied level/vector, I mask, NMI vector 11, stack/wake | sleep, special | — | Missing device priority selection, assertion/acknowledgment, NMI signal latch | Pending |
| Deferral, H §4.6 | IRQ/NMI defer through delayed successors and control/system successors; address errors defer only through delayed successors | special, AE | ROM, AEROM | No pipeline-cycle timing or device IRQ selection; unfetchable successor policy below | Pending |
| Cache/address regions, H §8, tables 8.2–8.4, figs.8.10/8.11 | CPU-local 4 KiB/four-way cache, write-through, no snooping; ID/OD, pseudo-LRU, two-way RAM, P1 bypass, P2 purge, P3 address array, P6 data array; TAS external read | CACHE, PERIPH; `Cache` and `CpuBus` | PERIPH | Pipeline/fill-cycle/bus arbitration timing absent; RAM mirrors remain missing | Pending |
| On-chip INTC, H §5.3, table 5.4 | IPRA/IPRB, VCRA–D/VCRWDT, DIVU/DMA vectors, source priorities/tie order and guest acknowledgement | FRT, DUAL, PERIPH | DUAL, PERIPH | External ICR/IRL arbitration, refresh and UBC sources missing | Pending |
| FRT, H §§11.1–11.5, tables 11.2/11.3 | FRC, OCRA/B, capture, divisors, overflow/compare/clear, TIER/FTCSR, shared TEMP, output level state and IRQ; Saturn capture wires (ST-202 §1.2) | FRT, DUAL | FRT (compare), DUAL (capture) | Internal/external pin synchronization, WAIT, clock-switch edges and contention timing absent; no external clock source or output-pin routing | Pending |
| Watchdog, H §§12.2–12.3, table 12.2 | Keyed byte-register/word-write protocol, eight divisors, interval IRQ, read-before-clear, watchdog reset selection/cause and 512-clock CPU hold | PERIPH; `Watchdog` | PERIPH | External WDTOVF pin routing and sub-turn event ordering missing | Pending |
| SCI, H §§13.2–13.3 | SMR/BRR/SCR/TDR/SSR/RDR, timed internal-clock transmission, external completed-character input, overrun/framing/parity flags, IRQs and DMA handshakes | PERIPH; `Serial` | PERIPH (transmit completion) | No external clock-edge adapter or physical serial pin waveform; multiprocessor and format combinations lack exhaustive integration coverage | Pending |
| On-chip DMAC, H §§9.2–9.3 | Both channels; byte/word/long/16-byte transfers; fixed/increment/decrement addresses; auto/DREQ/SCI requests; fixed/round-robin channel selection, TE/AE/NMIF acknowledgement, vectors/IRQ | PERIPH; `Dma` and `Bus::advance_dma` | PERIPH (copy/end IRQ/error 10/no cache snoop) | Single-address DACK mode rejected explicitly; CPU/refresh arbitration, burst/cycle-steal timing and wait states missing | Pending |
| DIVU, H §§10.2–10.4 | Signed 32/32 and 64/32 quotient/remainder, 39/6-clock completion/interlock, DVCR/VCRDIV, sticky overflow and IRQ | PERIPH; `Division` | PERIPH (signed 32/64-bit normal division) | Six-cycle intermediate overflow state is not modeled: deterministic saturation and input-high preservation are used even with OVFIE=1; no full conformance claim | Pending |
| BSC/UBC/standby, H bus/break/power chapters | None | — | — | Software-visible control/register behavior missing | Pending |
| Slave SH-2, [Dual CPU User's Guide][DUALREF] §§1/3/4, [SMPC] table 1.1 | Independent interpreter/registers/stacks/peripherals/cache; shared bus, SSHON/SSHOFF, bidirectional capture IRQs | DUAL, CACHE, PERIPH | DUAL, PERIPH | BCR1 identity, bus contention and full CPU clock accounting absent | Pending |

## MC68EC000 instruction families

**Every row is missing**, with no native or browser conformance test. The core,
all effective-address modes, CCR/SR flags, user/supervisor stacks, exception/IRQ
entry, bus width/address rules, reset and sound-bus integration are absent. A
manual entry in this table is an outstanding requirement, never a checked box.

| Family | Manual entry (68, MC68000 subset) | Implementation | Native / browser | Status |
| --- | --- | --- | --- | --- |
| ABCD | ABCD instruction description | Missing | — / — | Pending |
| ADD | ADD instruction description | Missing | — / — | Pending |
| ADDA | ADDA instruction description | Missing | — / — | Pending |
| ADDI | ADDI instruction description | Missing | — / — | Pending |
| ADDQ | ADDQ instruction description | Missing | — / — | Pending |
| ADDX | ADDX instruction description | Missing | — / — | Pending |
| AND | AND instruction description | Missing | — / — | Pending |
| ANDI | ANDI instruction description | Missing | — / — | Pending |
| ASL | ASL instruction description | Missing | — / — | Pending |
| ASR | ASR instruction description | Missing | — / — | Pending |
| Bcc | Bcc instruction description | Missing | — / — | Pending |
| BCHG | BCHG instruction description | Missing | — / — | Pending |
| BCLR | BCLR instruction description | Missing | — / — | Pending |
| BSET | BSET instruction description | Missing | — / — | Pending |
| BSR | BSR instruction description | Missing | — / — | Pending |
| BTST | BTST instruction description | Missing | — / — | Pending |
| CHK | CHK instruction description | Missing | — / — | Pending |
| CLR | CLR instruction description | Missing | — / — | Pending |
| CMP | CMP instruction description | Missing | — / — | Pending |
| CMPA | CMPA instruction description | Missing | — / — | Pending |
| CMPI | CMPI instruction description | Missing | — / — | Pending |
| CMPM | CMPM instruction description | Missing | — / — | Pending |
| DBcc | DBcc instruction description | Missing | — / — | Pending |
| DIVS | DIVS instruction description | Missing | — / — | Pending |
| DIVU | DIVU instruction description | Missing | — / — | Pending |
| EOR | EOR instruction description | Missing | — / — | Pending |
| EORI | EORI instruction description | Missing | — / — | Pending |
| EXG | EXG instruction description | Missing | — / — | Pending |
| EXT | EXT instruction description | Missing | — / — | Pending |
| ILLEGAL | ILLEGAL instruction description | Missing | — / — | Pending |
| JMP | JMP instruction description | Missing | — / — | Pending |
| JSR | JSR instruction description | Missing | — / — | Pending |
| LEA | LEA instruction description | Missing | — / — | Pending |
| LINK | LINK instruction description | Missing | — / — | Pending |
| LSL | LSL instruction description | Missing | — / — | Pending |
| LSR | LSR instruction description | Missing | — / — | Pending |
| MOVE (including CCR/SR/USP transfers) | MOVE instruction description | Missing | — / — | Pending |
| MOVEA | MOVEA instruction description | Missing | — / — | Pending |
| MOVEM | MOVEM instruction description | Missing | — / — | Pending |
| MOVEP | MOVEP instruction description | Missing | — / — | Pending |
| MOVEQ | MOVEQ instruction description | Missing | — / — | Pending |
| MULS | MULS instruction description | Missing | — / — | Pending |
| MULU | MULU instruction description | Missing | — / — | Pending |
| NBCD | NBCD instruction description | Missing | — / — | Pending |
| NEG | NEG instruction description | Missing | — / — | Pending |
| NEGX | NEGX instruction description | Missing | — / — | Pending |
| NOP | NOP instruction description | Missing | — / — | Pending |
| NOT | NOT instruction description | Missing | — / — | Pending |
| OR | OR instruction description | Missing | — / — | Pending |
| ORI | ORI instruction description | Missing | — / — | Pending |
| PEA | PEA instruction description | Missing | — / — | Pending |
| RESET | RESET instruction description | Missing | — / — | Pending |
| ROL | ROL instruction description | Missing | — / — | Pending |
| ROR | ROR instruction description | Missing | — / — | Pending |
| ROXL | ROXL instruction description | Missing | — / — | Pending |
| ROXR | ROXR instruction description | Missing | — / — | Pending |
| RTE | RTE instruction description | Missing | — / — | Pending |
| RTR | RTR instruction description | Missing | — / — | Pending |
| RTS | RTS instruction description | Missing | — / — | Pending |
| SBCD | SBCD instruction description | Missing | — / — | Pending |
| Scc | Scc instruction description | Missing | — / — | Pending |
| STOP | STOP instruction description | Missing | — / — | Pending |
| SUB | SUB instruction description | Missing | — / — | Pending |
| SUBA | SUBA instruction description | Missing | — / — | Pending |
| SUBI | SUBI instruction description | Missing | — / — | Pending |
| SUBQ | SUBQ instruction description | Missing | — / — | Pending |
| SUBX | SUBX instruction description | Missing | — / — | Pending |
| SWAP | SWAP instruction description | Missing | — / — | Pending |
| TAS | TAS instruction description | Missing | — / — | Pending |
| TRAP | TRAP instruction description | Missing | — / — | Pending |
| TRAPV | TRAPV instruction description | Missing | — / — | Pending |
| TST | TST instruction description | Missing | — / — | Pending |
| UNLK | UNLK instruction description | Missing | — / — | Pending |

## Bus, timing and persistence

| Feature / reference | Current behavior / edge cases | Native | Browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- | --- |
| BIOS ROM, O map | Exact 512 KiB, big endian, read-only, reset vectors; synthetic guest exception handlers | bus.rs, boot.rs, AEROM | ROM, AEROM | Full synthetic boot/MMIO/service coverage missing; genuine BIOS compatibility optional and not independently confirmed | Pending |
| Low/high WRAM, O map | Independent 1 MiB banks; aligned B/W/L; P1 aliases | bus.rs | SC, ROM | Work RAM mirrors and timing missing | Pending |
| Base-system address decoding, O map | Only documented subset in architecture.md | bus.rs | SC | A-bus/cartridge decoding and full base map missing | Pending |
| MMIO widths/side effects, device manuals | Unsupported accesses return errors | devices.rs | SC | Full device routing/width rules missing | Pending |
| Backup byte storage, SCU §1.2 figs.1.3/1.5; SMPC §2.3 SYSRES p.29 | 32 KiB, odd byte addresses in 00180000–0018FFFF; P1 aliases; unformatted zero initialization | BK; `backup_byte_lanes_endpoints_and_cache_through_access_do_not_alias_work_ram` | BK | Even lanes/open bus, word/long semantics, additional mirrors and hardware lane confirmation pending | Pending |
| Backup reset retention, SMPC §2.3 SYSRES p.29 | Bytes survive successful/rejected HLE, disc and ROM resets; new Machine starts empty | BK; `packed_import_is_atomic_and_survives_every_boot_mode_and_rejected_boot` | BK, reset/mode/read-race checks | Guest SYSRES and other power/reset domains missing | Pending |
| Local backup import/export (host interface) | Exact 32768 packed bytes; atomic replacement; explicit local download; stale import cancellation; no network/storage | `packed_import_is_atomic_and_survives_every_boot_mode_and_rejected_boot` | Download byte comparison, offline import, invalid size/read errors, races, fresh-tab isolation | BIOS backup filesystem/calls and cartridge backup absent; no automatic cross-session persistence | Pending |
| Shared event scheduler, O clock domains | Deterministic CPU order, raster/SCU event deadlines and clock-driven DSP/DMA; field deadlines replace instruction budgets | TIME, SCUROM, game.rs, DUAL | SCUROM, SC, DUAL | CPU opcode/on-chip DMA cycles, all remaining clock domains and bus contention missing | Pending |
| Video timing, VDP2 display registers | Clock-based 263-line/224-visible NTSC raster, HBLANK/VBLANK bits and SCU edges; bounded field advancement | TIME, SCUROM, game.rs | SCUROM, SC | Other resolutions, PAL/interlace/clock modes and exact beam phase remain missing | Pending |
| Audio timing, SCSP/CD | None | — | — | Sample clocks, bounded stereo stream, underflow/muting missing | Pending |
| Power/reset domains, SMPC / H | Cold reset clears both CPUs/devices and retains backup; SSHOFF resets slave CPU/FRT while retaining shared state; SLEEP distinct | boot.rs, sh2_system.rs, DUAL | ROM/reset, DUAL | Manual/system reset and remaining power/clock controls missing | Pending |

## SCU and DSP

SCU subsets now execute actual operations. The entire SCU acceptance criterion
remains pending: bus arbitration/waits, all device connections and exhaustive DSP
edge/transfer coverage are still required. Unsupported registers fail explicitly.

| Feature / manual | Implemented behavior | Native / browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- |
| SCU interrupt registers, [SCU] table 2.1/§3.5, [SCUFINAL] #7/10 | IMS/IST, fixed priorities/vectors, master delivery/acknowledgment, A-bus acknowledgment gate | SCUV, SCUROM / SCUROM | Sound, CD and VDP1 producers missing; complete external interrupt pin behavior unverified | Pending |
| A/B-bus coordination, [SCU] §3.6, [SCUFINAL] #8–12 | Prohibited DMA source/destination checks and bounded error handling | SCUV / SCUROM (valid transfers) | ASR/refresh wait-state and CPU/bus contention behavior missing | Pending |
| DMA channels 0–2 direct, [SCU] §§2.1/3.2, [SCUFINAL] #15–24 | Counts, address save/update, unaligned heads/tails, B-bus halfword strides, all seven edge triggers/GO, end/illegal IRQ and priority preemption; one queued retrigger | SCUV, SCUROM / SCUROM (channels 0/1) | Exhaustive trigger/device/boundary tests, actual bus latencies and prohibited three-channel behavior pending | Pending |
| DMA indirect, [SCUFINAL] #24/25 | Final-spec count/write/read layout, source bit-31 end marker, table walking, write-address continuation | SCUV, SCUROM / SCUROM | Entire table alignment/overflow/error contract and all count limits need further vectors | Pending |
| DMA device interactions, [SCU] §2.1 | High-RAM to VDP2 VRAM and executing DSP external RAM transfers, real IRQ handlers | SCUV, SCUROM / SCUROM | VDP1/sound/CD hardware paths and arbitration missing | Pending |
| SCU DSP execution/registers, [SCU] §4.5 pp.91–131/141–158 | ALU logical/arithmetic/shift/rotate, signed multiply, parallel X/Y/D1 moves, immediates, delayed jumps/subroutine control, BTM/LPS, END/ENDI, pause/step and read-cleared E/V | SCUV, SCUROM / SCUROM (product/sum/loop/ENDI) | Full conditional/DMA timing, conflicting bank access and ALU overflow/pipeline edge inventory pending | Pending |
| SCU DSP memory/transfers, [SCU] §§3.3/4.1/4.5 pp.132–140, [SCUFINAL] #16/18/19/26/27 | Four 64-word banks, host ports, address counters, bounded asynchronous input/output DMA and HOLD, END permits DMA continuation | SCUV, SCUROM / SCUROM (immediate-count RAM DMA) | Register-count/program-RAM DMA, cross-device routes and data-bank conflicts lack complete evidence | Pending |
| SCU timers, [SCU] §§2.2/3.4, [SCUFINAL] #30/31 | Raster-driven timer0 clear/compare and timer1 dot countdown, zero=512, no reload while active, compare-line selection | SCUV, TIME, SCUROM / SCUROM | Full television modes and beam-phase conformance pending | Pending |

Implementation: [SCU ports/interrupts/timers](../src/scu.rs),
[DMA beats](../src/scu/dma.rs), [DSP interpreter](../src/scu/dsp.rs),
[clock-domain dispatch](../src/bus.rs) and [raster deadlines](../src/timing.rs).
ST-210 supersedes the earlier indirect-table diagram and removes DSTA/DSTOP;
these ports and prohibited IST writes raise diagnostics rather than behaving as
inert registers. SCU pending edges clear on accepted CPU interrupt acknowledgment.
A-bus requests require AIACK to be armed again after its acknowledge cycle.

The DSP executes every two reference clocks. External SCU DMA issues at most one
beat per eight reference clocks, independent of CPU issue and DIVU wait length.
SCU timer, DSP and raster deadlines split peripheral clock advancement. Native
vectors prove identical results for 1/3/8/17/100-clock chunks. Raster vectors prove
identical event traces for 1/3/8/127/1708/10000-clock chunks. CPU issue/on-chip DMA
still use the interpreter turn, so this is not complete Saturn bus arbitration or
opcode cycle accounting. Three simultaneous DMA channels, bank conflicts and
invalid software parameters do not claim physical electrical behavior.

## VDP1 and VDP2

| Feature / manual | Current behavior / edges | Native | Browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- | --- |
| VDP1 VRAM/framebuffers/registers, [VDP1] | None | — | — | Entire map/access/state missing | Pending |
| Command list/control flow, [VDP1] | None | — | — | End/jump/call/return and bounds handling | Pending |
| Normal/scaled/distorted sprites, [VDP1] | None | — | — | Texture addressing, geometry and rasterization | Pending |
| Polygons/lines/polylines, [VDP1] | None | — | — | Primitive rasterization | Pending |
| Clip/local coordinates, [VDP1] | None | — | — | System/user clipping and coordinate state | Pending |
| Sprite texture/color modes, [VDP1] | None | — | — | Packed/palette/direct color, transparency/end codes | Pending |
| Shading/color calculation, [VDP1] | None | — | — | Gouraud, shadows, half luminance/transparency | Pending |
| Draw/erase/swap/status/IRQ, [VDP1] | None | — | — | Framebuffer lifecycle and SCU drawing-end integration | Pending |
| VDP2 VRAM, [VDP2] §3 | Bank-zero bitmap RAM | devices.rs | SC | VRAM banks/cycle-pattern arbitration; cycle words only stored | Pending |
| VDP2 CRAM, [VDP2] color RAM | None | — | — | Palette modes and access rules | Pending |
| TVMD/TVSTAT, [VDP2] §2.2 | 320×224 noninterlace, display enable, clocked HBLANK/VBLANK and SCU edges | devices.rs, TIME | SC, SCUROM | Other resolutions, PAL/interlace and full beam status/latches | Pending |
| NBG0 bitmap, [VDP2] §§4–5 | RGB555 512-pixel pitch, bank 0, unity zoom | devices.rs | SC | Other formats, transparency, scroll/zoom | Pending |
| NBG0–3 tile/character, [VDP2] scroll screens | None | — | — | Pattern names, maps/planes, character formats | Pending |
| NBG1–3 bitmap/layer control, [VDP2] scroll screens | None | — | — | Applicable screen modes/VRAM/priority | Pending |
| Rotation backgrounds, [VDP2] rotation screens | None | — | — | RBGs, parameters, coefficients, rotation/switching | Pending |
| Line/vertical-cell scroll, [VDP2] scroll controls | None | — | — | Tables and per-line/cell effects | Pending |
| Windows/line windows, [VDP2] windows | None | — | — | Screen/sprite windows and logical operations | Pending |
| Mosaic, [VDP2] mosaic | None | — | — | Horizontal/vertical effects | Pending |
| Priority/composition, [VDP2] priority | PRINA=1 for single NBG0 only | devices.rs | SC | Multiple backgrounds and VDP1 sprite priorities | Pending |
| Color calculation/offset/shadow, [VDP2] color calculation | None | — | — | Ratios, blending, offset, shadow and special functions | Pending |
| Back/line color, [VDP2] color screens | Display-disabled black only | devices.rs | SC | Guest back/line color tables | Pending |
| Combined VDP1/VDP2 pipeline | None | — | — | Overlap, sprite priority, transparency and stable independent guest images | Pending |

## Sound

| Feature / manual | Implemented behavior | Native / browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- |
| Sound RAM / SCSP mapping, [SCSP] | None | — / — | Sound CPU/bus access and RAM/register behavior | Pending |
| 32 slots, PCM8/16, [SCSP] slot registers | None | — / — | Sample decoding, loop/pitch and key on/off | Pending |
| Envelope, [SCSP] slot registers | None | — / — | Attack/decay/sustain/release and timing | Pending |
| FM/modulation/LFO, [SCSP] modulation | None | — / — | Routing, pitch/amplitude modulation and noise | Pending |
| Levels/pan/external inputs, [SCSP] mixer | None | — / — | Stereo mixing including CD-DA | Pending |
| Timers/interrupts, [SCSP] common registers | None | — / — | Counters, enables/pending/clear, 68k/SH-2 delivery | Pending |
| MIDI registers, [SCSP] MIDI | None | — / — | FIFOs/status/IRQ behavior | Pending |
| SCSP DMA, [SCSP] common registers | None | — / — | Transfer register effects/completion/IRQ | Pending |
| SCSP effects DSP, [SCSP] DSP programming model | None | — / — | Instruction execution, registers, ring buffers and routing | Pending |
| Browser audio output | None | — / — | Bounded stereo buffering, gesture enable/mute and underflow | Pending |
| CD-DA, [CD] / [SCSP] external audio | None | — / — | Track samples, pacing, playback status and mix path | Pending |

## CD, boot and SMPC

| Feature / manual | Current behavior / edge cases | Native | Browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- | --- |
| Cooked ISO, [Disc Format][DISC] | Bounded single-area IP + simple ISO9660, first-read file | disc.rs | SC/local ISO | General layouts/size, runtime sectors | Pending |
| BIN/CUE/raw/mixed tracks, [DISC] / [CD] | None | — | — | CUE references, pregaps, raw sectors, TOC/audio and UI selection | Pending |
| CD command/status/HIRQ, [CD] | None | — | — | Command protocol, response timing/IRQ/acknowledgment | Pending |
| CD drive/TOC/session/track state, [CD] | None | — | — | Seek/spin/read/play/status transitions | Pending |
| CD buffers/transfers, [CD] | None | — | — | Sector pool, counts, transfer endpoints/IRQ | Pending |
| Filters/selectors/partitions, [CD] | None | — | — | Connection/filtering/partition data and commands | Pending |
| BIOS-less startup, [Boot ROM][BOOT] / [DISC] | Narrow original first-read HLE boot | disc.rs, game.rs | SC | General startup and region/version contracts | Pending |
| BIOS services, [BOOT] | None | — | — | System/backup/CD calls and per-call ABI/side effects | Pending |
| External BIOS execution, [BOOT] | Genuine reset vectors, original synthetic ROM handlers | boot.rs, sh2_system.rs | ROM | Genuine BIOS initialization/menu/boot **unverified**; no lawful local BIOS supplied | Pending |
| SMPC direct digital port 1, [SMPC] §3 | Active-low nibble selection and button levels | devices.rs | SC | Full I/O mode behavior and timing | Pending |
| SMPC commands/status/timing, [SMPC] §1.2/§2.3 pp.23/24/32 | SF set/busy/clear; OREG31 at command start; 807 reference clocks (30 µs minimum) for 02/03/18; rejects unsupported commands and busy overwrite | smpc_command_tests, DUAL | DUAL | All other commands, SR, INTBACK, command jitter and SCU interrupt missing | Pending |
| SMPC reset/power/clock, [SMPC] table 1.1/§2.3 | SSHON/SSHOFF slave reset domain; NMIREQ latches master NMI until accepted | DUAL | DUAL | Master/sound/CD power commands, SYSRES, clock change and reset-button enable missing | Pending |
| RTC/region/system flags, [SMPC] | None | — | — | RTC registers/commands, deterministic clock input, flags | Pending |
| SMPC direct digital port 2, [SMPC] §1.2 pp.7–9, §3.4 table 3.36 p.97 | Independent PDR2/DDR2, IOSEL2/EXLE2; active-low nibble reports; host keyboard levels | BK; `both_digital_ports_have_independent_direction_selection_and_enable_bits` | BK; I/K/J/L keyboard input and release checks | Settling remains immediate; no timed INTBACK/peripheral packets or external latch IRQ | Pending |
| Analog pad, [SMPC] peripheral data; ST-151 peripheral standards | None | — | — | Analog axes/buttons, packet/command timing, browser mapping | Pending |
| Input focus safety | Both pads use host button levels, blur/view/tab/pause release, no guest-memory shortcuts | game.rs | SC | Additional controller types absent | Pending |

## Concrete unresolved gaps and closure gates

1. Remaining SH-2 semantic/exception edges, complete reset domains, BSC/UBC/standby,
   DIVU overflow intermediates, DMAC DACK/CPU arbitration, external IRQs and bus locking;
   cycle-based dual-CPU scheduling and arbitration. CPU opcode decoding alone
   does not satisfy the CPU or multiprocessor criterion.
2. Every MC68EC000 instruction family above, sound bus, SCSP synthesis/timing/DSP,
   CD-DA mixing and real browser audio. The current build remains silent.
3. Full base memory map, backup bus lanes/widths/mirrors and BIOS filesystem/calls,
   plus hardware clock/event scheduling. Packed backup import/export and byte
   retention pass subset tests; other reset domains remain absent.
4. Complete SCU A/B-bus wait-state/arbitration and device routes, DMA boundary/
   trigger coverage, DSP conditional/pipeline/bank-conflict and program-DMA evidence.
   SCUROM now executes real DMA/DSP/timer/IRQ operations but is subset evidence.
5. VDP1 and remaining VDP2 modes/effects/composition/timing, with independent
   expected pixel outputs. Existing single bitmap mode is the only video evidence.
6. Runtime CD block, BIN/CUE/mixed tracks, complete BIOS HLE ABI, full native BIOS
   startup, SMPC commands/RTC/INTBACK/analog input and system controls.
7. Two additional original licensed demonstrations beyond Star Collector: the
   VDP1+VDP2+audio playable game and the multiprocessor/SCU/runtime-CD/CD-DA/DSP/
   advanced-video/backup showcase. Both remain absent. Current synthetic ROMs
   and temporary backup/port-2 diagnostic ISO do not satisfy these requirements.
8. Full original synthetic BIOS boot/MMIO/interrupt/service fixtures and separate
   HLE ABI tests. Genuine BIOS/retail compatibility is **not independently
   confirmed**, is optional, and never requires someone to supply firmware.
9. Full conformance review and CI/native/Wasm/browser evidence for all the above.
   Existing subset tests pass locally, not a complete hardware conformance suite.
10. Automated execution of both new demonstrations and all mandatory hardware
    checks in native and Wasm/browser contexts. Live Pages inspection, personal
    files, subjective review and external workflow confirmation are not gates.

## Tracker closure and autonomous acceptance

[Issue #17](https://github.com/takahirox/saturn-rust/issues/17) continues #15 and supersedes prior
closure-policy requirements. Partial hardware work can be reviewed using its
actual technical evidence; external auto-close behavior does not block it.
Administrative closure is not proof of completion. Report unmet criteria with
plain references and continue implementation under the current umbrella backlog.
Do not reopen #11/#13/#15 or require workflow-owner confirmation, a real BIOS,
manual hardware testing, subjective visual/audio approval or a live-site check.
Existing CI runs native, Wasm and headless browser tests under `/saturn-rust/`.
PR #18 remains Draft while required implementation/validation is pending. Its
full Issue #17 target is retained; no merge or Issue closure is authorized here.
All hardware gaps above remain technical requirements.

## Dual-CPU/FRT scope and deterministic deviations

`Machine::step` attempts a master boundary followed by a slave boundary (only
when SSHON has completed), performs bounded on-chip DMA quanta, then advances
peripherals/SMPC by eight reference clocks plus DIVU interlock waits. DMA address
errors are latched for vector 10 at the next eligible boundary. Pending NMI
precedes selection among internal IRQs; each CPU applies its own
SR mask, delayed-branch and control-transfer inhibition. An accepted interrupt
consumes that CPU's turn without retiring an instruction. Timers/commands advance
during SLEEP. FRT requests remain asserted until guest software reads/clears
FTCSR; exception acceptance does not clear a device flag. Slave reset discards
its FRT and interrupt state while retaining shared memory and master state.
A failure on either CPU stops subsequent machine turns until reset; successful
earlier accesses within a failing turn are not rolled back.

The eight-clock CPU quantum remains a **functional interleave**, not SH-2 opcode
cycle accounting. `advance_frame` now advances to a clock-based raster field
deadline instead of 60000/20000 instruction budgets. Scanline/HBlank/VBlank and
SCU timer/DSP/DMA events are clock-driven; CPU/on-chip-DMA and full bus
arbitration remain incomplete. The 807-clock SMPC delay rounds the documented minimum 30 µs at
26.8741 MHz upward; firmware jitter is absent. The guest sees completion at a
turn boundary. Unsupported commands still fail explicitly. SF=0 command issue,
SF clear writes and busy command overwrite are rejected; these invalid-software
cases are diagnostics rather than a claim about physical firmware recovery.

FRT byte accesses implement shared TEMP and observed-one/write-zero flags.
The INTC accepts byte/word IPRA/IPRB, VCRA–D and VCRWDT. Internal sources
compete by programmed level and manual table 5.4 tie order. FRT word accesses
and unimplemented device accesses still fail explicitly.
The external-clock selection does not increment without an external clock source,
which is not yet connected. Capture strobes at 01000000/01800000 and their P1
aliases are write-only words, targeting slave/master respectively (ST-202 §1.2).
A full strobe is modeled as a capture for either selected edge; electrical pulse
width and synchronization are not modeled. Captures while the slave is held in
reset are ignored. Other address mirrors and subcycle contention remain pending.
FTOA/B output state is computed but not connected to external devices.
No BSC master/slave identification or retail BIOS boot is claimed.

## Address-error scope and deterministic deviations

H §4.3.2 requires an erroneous operand instruction to complete before accepting
vector 9. `Operands` returns zero for an invalid read and suppresses invalid
writes/device transactions, then the interpreter completes register/flag and
predecrement/postincrement effects. This is an explicit reproducibility policy,
not a claim about physical invalid-cycle data or bus signals. Successful valid
cycles still go through ordinary `Memory`, including MMIO; unsupported hardware
still fails rather than receiving a fabricated successful read.

Acceptance occurs on the next eligible `step`, without retiring an instruction.
The next PC and unchanged SR are stacked; taken branches save their destination.
RTE with erroneous operands still executes its successor before acceptance;
control/system transfers do not defer address errors. Odd/peripheral fetches do
not decode or retire. At an unfetchable delayed successor, the interpreter
finishes the branch boundary without retiring that successor and accepts the
latch at the next step. Pipeline fetch contents and invalid bus data are not
modeled. H §4.8.3 misaligned exception stacking completes its two writes, latches
an address error after the original exception and inhibits recursive stacking
errors in vector 9 entry. Physical stacked data are undefined; the functional
model chooses big-endian bytes at the exact SP without rounding it. Tests cover
all three SP misalignments and an original browser guest. Unavailable memory or
vectors still cause sticky bounded diagnostics. Misaligned VBR handling and full
SH7604 conformance remain pending.

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
  advance sequentially here without a pending jump target. H §4.6 Table 4.10
  separately requires consecutive execution of a delayed branch and its successor.
  Both outcomes therefore inhibit IRQ/NMI until the successor executes; native
  tests check deferral, subsequent acceptance and the stacked PC/SR. Physical/
  differential confirmation of slot restrictions on the untaken path is absent.
- P §6.19 updates Rn with the shift/T input before reading Rm in DIV1. Aliased
  operands therefore use the shifted value as divisor. Original native vectors
  cover all eight Q/M/T combinations, old sign and addition-carry boundaries.
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

[DUALREF]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-202-R1-120994.pdf

[SCUFINAL]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-210-110194.pdf
