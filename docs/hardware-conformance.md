# Hardware conformance for Issue #17

**Status: incomplete; Issue #17 is the active continuation tracker.** Issues
#11/#13/#15 remain administratively closed; do not reopen them. This checkpoint
includes independent slave SH-2 execution, CPU-local FRT/WDT/SCI/DIVU/DMAC,
internal interrupt arbitration, executing UBC comparisons, software standby/NMI wake,
write-through caches, timed SMPC CPU control and SYSRES,
SCU interrupt/DMA/DSP execution, an independent MC68EC000 core, SCSP PCM/FM/LFO/noise,
WebAudio delivery, SMPC RTC/SMEM/INTBACK and clock-based NTSC raster events. It preserves the address-error fixtures, Star Collector,
backup RAM and both direct digital pads. It does not complete the Saturn hardware
or the complete required hardware exerciser demonstration. Hardware Lab now
executes runtime CD/DMA/IRQ, dual CPU, SCU DSP, PCM/CD-DA, advanced video and backup
and an executing SCSP effects-DSP scene. Full chip conformance and the complete
acceptance inventory remain required. The original playable Prism
Courier disc now runs with VDP1/VDP2 composition, 68k audio and guest IRQ/input
in native and browser Wasm checks. No umbrella acceptance checkbox is met.

Inspected upstream main: `dbd981b030a46daa6923072ce240680dd2e58ebf` (2026-10-21),
verified with `git ls-remote origin refs/heads/main`. This revision starts from
`7285a713490ab20c2cb7b82af126a72d05c35a44`, preserving the earlier
SCU/SH7604 work and review evidence. The full outstanding hardware scope remains mandatory. See [current validation](validation-issue-17.md),
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
| divide | `div0s_initializes_q_m_t_for_every_sign_pair`, `unsigned_division_guest_sequences_produce_host_quotients`, `div1_signed_mode_vectors_cover_add_subtract_carry_and_borrow`, `div1_same_register_reads_the_shifted_divisor_for_all_q_m_t_combinations`, `signed_division_sequences_cover_signs_extrema_and_truncate_towards_zero` |
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

**INTCG** means the original [interrupt/latch/signed-division builder](../tests/fixtures/build_intc.py),
[ROM](../tests/fixtures/intc.hex), [source-authored quotient/remainder vectors](../tests/fixtures/signed-divisions.tsv),
[native integration](../tests/intc.rs) and identical Wasm guest in
[browser-smoke.mjs](../scripts/browser-smoke.mjs). H §5.3.8/table5.4 defines ICR
and IRL auto/external vector selection; P §6.19 example4 defines signed DIV1
sequences. SMPC table1.5 and VDP2 §2.5/tables2.3–2.4 define the TH latch route.
Guest DMA interrupts use auto-vector 66 then external vector 0x4B; changing the
ICR operand prevents stage completion. Thirteen signed divide/remainder cases
include extrema and both signs; 262 native wide-arithmetic comparisons extend
this corpus. Falling TH inputs on both ports cause guest PAD IRQ handlers to
record held H/V counters and clear-on-read EXLTFG. DDR/output/EXLE/EXLTEN gating,
software latching, repeated levels, reset and independent PAD IRQ routing have
native bus vectors. Normal active-line counter encodings are tested; complete
blanking counter discontinuities, other display modes and pin synchronizers are
still required. Source and generated artifacts use [the adjacent MIT license](../tests/fixtures/LICENSE).
Expanded ROM SHA-256: `4cda5f2cd7fe9504ab7db13ad11e9fcaa7fef9ceb6de25b4136da0b38a46571e`.

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

**UBCG** means the original [UBC guest source](../tests/fixtures/ubc.s),
[reproducible ROM builder](../tests/fixtures/build_ubc.py) and
[native execution](../tests/ubc.rs), repeated through browser Wasm in
[browser-smoke.mjs](../scripts/browser-smoke.mjs). It checks saved PC/SR for
before/after instruction execution, delay-slot deferral, masked CPU data writes
and DMAC writes. [UBC vectors](../src/onchip/ubc.rs) cover registers, sequential
matching, masks, widths, CPU/peripheral and external-bus selection. The
`user_breaks_distinguish_cached_cpu_accesses_from_external_bus_transactions`
and `external_scu_bus_and_local_dmac_have_separate_user_break_sources` tests in
[bus.rs](../src/bus.rs) distinguish cache hits, fills, P1 and SCU bus cycles.
Sources: H §§6.2–6.3, tables 6.1–6.4.

**PWRG** means the original [SYSRES guest](../tests/fixtures/system-reset.s)
and [standby guest](../tests/fixtures/standby.s), built by
[build_system_reset.py](../tests/fixtures/build_system_reset.py) and
[build_standby.py](../tests/fixtures/build_standby.py). The identical ROMs execute
in [system_reset.rs](../tests/system_reset.rs), [standby.rs](../tests/standby.rs)
and browser Wasm. SYSRES tests verify both ExternalBios and HLE-disc restart,
cleared volatile memories, retained backup/SMEM and installed-media retention.
Standby tests verify local clock gating, FRT/SCI initialization, retained CPU
registers and NMI wake/RTE at I=15. Sources: H table 14.3, §§14.2/14.4;
SMPC §2.3.8 p.29. No proprietary firmware is used.

**CDDRV** is the original [CD drive guest builder](../tests/fixtures/build_cd_drive.py),
[ROM](../tests/fixtures/cd-drive.hex) and [native integration](../tests/cd_drive.rs),
also executed in browser Wasm. It issues and polls real CD commands for forward/reverse
scan, pause, empty R–W WAIT, tray open/close and a real TOC FIFO transfer. Native
[CD vectors](../tests/cd.rs) additionally check scan audio attenuation/muting,
60 Hz pause reports, standby deadlines, full-pool pause/resume and CUE CONTROL/Q fields.

**SPRX** is the original [sprite-effects source](../tests/fixtures/sprite-effects.s),
[builder](../tests/fixtures/build_sprite_effects.py), [ROM](../tests/fixtures/sprite-effects.hex)
and [native integration](../tests/sprite_effects.rs), repeated through browser Wasm
and production Canvas with guest-literal mutations. It exercises sprite windows,
normal/MSB shadows and palette-MSB blend conditions (VDP2 §§8/9/14).
[Sprite field vectors](../src/vdp2/sprite.rs) and [pixel vectors](../tests/video.rs)
cover all sixteen sprite types, shared palette bits, priority/ratio selection,
8-bit scanout, shadow enables/order and both CRAM color formats.

**SIOG** is the original [SH-2/68k builder](../tests/fixtures/build_sound_io.py),
[shared original 68k encodings](../tests/fixtures/build_sound.py),
[ROM](../tests/fixtures/sound-io.hex) and [native integration](../tests/sound_io.rs),
also executed in browser Wasm. It computes BCD, MOVEP/MOVEM and arithmetic results,
handles a partial MOVEM bus fault and TRAP/RTE, programs SCSP DMA, polls serial-paced
MIDI and acknowledges timer/MIDI/DMA interrupts at three levels. Changed guest DMA
source operands change actual register/mailbox results. Native MIDI vectors check
31.25 kbit/s 8-N-1 deadlines across advancement chunks, independent main/sound IRQ
acknowledgment and reset cancellation. These are additional guest fixtures, not
complete CPU/chip conformance or new acceptance gates.

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
| DIV0S | 6.17 | All operand-sign combinations of Q/M/T | divide | INTCG | Common limits | Pending |
| DIV0U | 6.18 | Clear Q/M/T, preserve other flags | divide | — | Common limits | Pending |
| DIV1 | 6.19 | Non-restoring add/subtract, Q/M/T, aliased operands, full unsigned and signed division sequences with one’s-complement correction | divide, INTCG | INTCG | Cycle/pipeline boundaries remain | Pending |
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
| Register/reset model, H §§2/4.2 | Independent CPUs; power-on 0/4 and watchdog manual-reset 8/12 vectors, SR=F0; 512-clock watchdog reset hold retains shared RAM/backup and clears CCR | boot.rs, DUAL, PERIPH | DUAL, PERIPH | External reset pin timing remains; UBC contents after manual reset are undefined by H table 6.1 (deterministically zero here); BSC retention/identity covered by BSCG | Pending |
| General/slot illegal, H §4.5 | Vectors 4/6, correct saved PC, no retirement | exception | ROM | No double-fault bus handling | Pending |
| TRAPA/RTE, H §§4.5/4.7 | Guest stack layout and handler return | exception, sh2_system.rs | ROM | Non-FRT system IRQ sources absent | Pending |
| CPU address errors, H §§4.3/4.6/4.7/4.8.3, Tables 4.6/4.10/4.11 | Vector 9, operand/fetch checks, next-PC/SR, auto-update and deferral; misaligned stacking completes original exception then accepts address error without recursion | AE, AEROM, PERIPH | AEROM, PERIPH | Invalid data and unaligned stacking bytes are deterministic policies rather than physical bus signals; misaligned VBR unsupported | Pending |
| Maskable IRQ/NMI, H §4.4 | Device priority selection, I mask, NMI vector11, stack/wake; ICR auto/external vector modes, NMIL/NMIE and selected-edge NMIF/standby wake | sleep, special, PERIPH, INTCG | INTCG, DUAL | Pin synchronization and complete cross-clock assertion boundaries remain | Pending |
| Deferral, H §4.6 | IRQ/NMI defer through delayed successors and control/system successors; address errors defer only through delayed successors | special, AE | ROM, AEROM | No pipeline-cycle timing or device IRQ selection; unfetchable successor policy below | Pending |
| Cache/address regions, H §8, tables 8.2–8.4, figs.8.10/8.11 | CPU-local 4 KiB/four-way cache, write-through, no snooping; ID/OD, pseudo-LRU, two-way RAM, P1 bypass, P2 purge, P3 address array, P6 data array; TAS external read; ordered four-longword refills and two-way RAM tag isolation | CACHE, PERIPH, CACHEG; `Cache` and `CpuBus` | PERIPH, CACHEG | Pipeline/fill-cycle/bus arbitration timing absent; RAM mirrors remain missing | Pending |
| On-chip INTC, H §5.3, table 5.4 | IPRA/IPRB, VCRA–D/VCRWDT, DIVU/DMA vectors, source priorities/tie order and guest acknowledgement | FRT, DUAL, PERIPH | DUAL, PERIPH | UBC source 12 at level 15 covered by UBCG; ICR/IRL auto/external vectors and SCU acknowledgement covered by INTCG; pin synchronization remains; refresh CMI arbitrates at WDT priority | Pending |
| FRT, H §§11.1–11.5, tables 11.2/11.3 | FRC, OCRA/B, capture, divisors, overflow/compare/clear, TIER/FTCSR, shared TEMP, output level state and IRQ; Saturn capture wires (ST-202 §1.2) | FRT, DUAL | FRT (compare), DUAL (capture) | Internal/external pin synchronization, WAIT, clock-switch edges and contention timing absent; no external clock source or output-pin routing | Pending |
| Watchdog, H §§12.2–12.3, table 12.2 | Keyed byte-register/word-write protocol, eight divisors, interval IRQ, read-before-clear, watchdog reset selection/cause and 512-clock CPU hold | PERIPH; `Watchdog` | PERIPH | External WDTOVF pin routing and sub-turn event ordering missing | Pending |
| SCI, H §§13.2–13.3 | SMR/BRR/SCR/TDR/SSR/RDR, timed internal-clock transmission, external completed-character input, overrun/framing/parity flags, IRQs and DMA handshakes | PERIPH; `Serial` | PERIPH (transmit completion) | No external clock-edge adapter or physical serial pin waveform; multiprocessor and format combinations lack exhaustive integration coverage | Pending |
| On-chip DMAC, H §§9.2–9.3 | Both channels; byte/word/long/16-byte transfers; fixed/increment/decrement addresses; auto/DREQ/SCI requests; fixed/round-robin selection, TE/AE/NMIF, vectors/IRQ; single-address DACK direction/polarity through explicit board pin adapters | PERIPH, DACK; `Dma`, `Bus::advance_dma`, pin-adapter vectors | PERIPH (copy/end IRQ/error 10/no cache snoop), DACK (armed wait/reset on standard board) | CPU/refresh arbitration, burst/cycle-steal timing and wait states remain missing; no requesting DACK peripheral is connected on the standard board | Pending |
| DIVU, H §§10.2–10.4 | Signed 32/32 and 64/32 quotient/remainder, 39/6-clock completion/interlock, DVCR/VCRDIV, sticky overflow and IRQ | PERIPH; `Division` | PERIPH (signed 32/64-bit normal division) | Six-cycle intermediate overflow state is not modeled: deterministic saturation and input-high preservation are used even with OVFIE=1; no full conformance claim | Pending |
| BSC identification/interval timer, H §§7.1.4/7.2/7.5.7/7.11.1 | BCR1.MASTER per CPU, read-only word lanes, keyed long writes, RTCNT/RTCOR/RTCSR, all clock divisors, CMF read-before-clear, CMI vector/priority and manual-reset retention; ordinary software waits, narrow transfers and CS2 endian conversion (BWAIT below) | [BSCG/BWAIT](../tests/bsc.rs), [register/reset vectors](../src/onchip/peripheral_tests.rs), [BSC vectors](../src/onchip/bsc.rs), [bus beats](../src/bus.rs) | BSCG, BWAIT | Memory/burst interfaces, inter-cycle idle, WAIT input, board widths/lanes and refresh/CPU/DMA ownership remain; unsupported changes explicitly rejected | Pending |
| Module stop/standby, H §§14.2/14.4, table 14.3 | SBYCR, SCI/FRT reset and gating, preserved INTC/UBC/BSC, retained DMA operands with control reset, WDT-selected NMI settling; `OnChip::enter_sleep/advance_clocks`, `Bus::advance_dma` | [standby/retention vectors](../src/onchip/peripheral_tests.rs), BSCG, PWRG | BSCG, PWRG | Electrical HIZ/pin states, initial five-cycle NMI guard, physical oscillator and clock-switch edges remain unverified; executing stopped MULT instructions is software-prohibited/undefined (H §2.5.2) | Pending |
| UBC, H §6 | A/B registers, masked address/data/size/master matching, sequential conditions, match flags, before/after fetch breaks and deferred level-15 vector 12; CPU/cache/DMAC and external SCU observation; `Ubc::observe`, `Cpu::step/execute` | UBCG and bus vectors | UBCG | Three-cycle configuration pipeline and SH7000-compatible speculative overrun fetch breaks remain unmodeled; instruction/bus cycle scheduling remains pending | Pending |
| Slave SH-2, [Dual CPU User's Guide][DUALREF] §§1/3/4, [SMPC] table 1.1 | Independent interpreter/registers/stacks/peripherals/cache; shared bus, SSHON/SSHOFF, bidirectional capture IRQs | DUAL, CACHE, PERIPH | DUAL, PERIPH | Bus contention and full CPU clock accounting absent; BCR1 identity covered by BSCG | Pending |

## MC68EC000 instruction families

The original [interpreter](../src/m68k.rs) implements the MC68000/MC68EC000
integer instruction families below, all base effective-address modes, 24-bit address
pins, word-aligned long bus cycles, SR/CCR/USP/SSP, reset, normal/seven-word
bus/address exception frames, privilege/illegal/line-A/line-F/trap/trace exceptions,
autovectored IRQ levels and STOP wake-up. No later-model instruction is enabled.
[Native semantic vectors](../src/m68k/tests.rs) include 12,000 deterministic
arithmetic property vectors and a bounded all-encoding safety check; the latter
is not semantic evidence by itself. Sound RAM and SCSP accesses are real shared
bus cycles. **SNDR** means the original [sound guest](../tests/fixtures/build_sound.py)
executed by [native integration](../tests/sound.rs) and the
[browser Wasm suite](../scripts/browser-smoke.mjs). SNDR exercises MOVE/MOVEA,
MOVEQ, ASL/LSL, NEG, ADDQ, Bcc, STOP, RTE, SR and IRQ entry; it does not exercise
every family through Wasm.

These are functional semantics, not complete timing conformance. Effective-address
and instruction cycle costs now follow MC68000UM tables 8-1–8-14; multiplication
counts source bits/transitions, branches distinguish taken/untaken and DBcc
expiry, and sound reset delays first issue by forty clocks. Native vectors include
the complete MOVE source/destination timing tables. DIV and register bit
modifications still use the documented maximum time. Bus contention,
complete instruction-specific bus microsequences and physical RESET pulse duration remain unverified.
Long predecrement/stack write order, failing word-cycle addresses and MOVEM
partial transfer side effects now have independent native bus-spy vectors
([MC68000UM][68UM] §§2.4/5/6.3.9;
[`M68k::write_predecrement/read/write/movem`](../src/m68k.rs),
[`long_predecrement_bus_order_and_partial_write_fault_frames`,
`second_longword_cycle_fault_records_its_own_address_and_preserves_first_write`,
`movem_and_exception_pushes_issue_predecrement_word_cycles_and_retain_progress`,
`movem_postincrement_preserves_completed_loads_when_a_later_bus_cycle_faults`](../src/m68k/tests.rs)).
`M68k::next_word/refill/redirect` now retains prefetched instruction words across
RAM writes and flushes them on branches, exceptions and reset. Faults capture the
internal fetch PC, activity I/N and function code separately; exception-vector
reads use supervisor data. Group1/2 entry faults dispatch group0, while errors
during reset/group0 halt. Traced traps enter the instruction exception before
trace. Independent native vectors check saved PC/SR/IR, word-cycle failures,
handler/vector fetches, both stacks, IRQ reset and trace ordering (68UM
§§5.4.1/6.1/6.3.8–6.3.9.1, fig.6-7).

Ten original [sound-bus fault programs](../tests/fixtures/m68k_faults.py) run in
FAMG native and browser Wasm, comparing [source-authored complete frames](../tests/fixtures/m68k-faults.tsv),
D0/D1/A0 and completed RAM words. They cover second-word read/write faults,
predecrement low-word-first and five-word immediate MOVE failures, two MOVEM partial-transfer boundaries,
address/user faults and sequential/branch prefetch faults. Two additional FAMG
cases distinguish a retained opcode from a fresh extension. SIOG records actual
fault PC/SR and writes an explicit guest recovery label before RTE.
Instruction-specific prefetch placement and full bus arbitration still need
validation; these word-boundary semantics do not establish the complete
CPU/scheduler acceptance criteria.

| Family | Manual entry (68, MC68000 subset) | Implementation | Native / browser | Status |
| --- | --- | --- | --- | --- |
| ABCD | ABCD instruction description | `M68k::bcd / alu / group4` | m68k/tests.rs semantic vectors / SIOG native + browser | Functional vectors; timing pending |
| ADD | ADD instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| ADDA | ADDA instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| ADDI | ADDI instruction description | `M68k::group0` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| ADDQ | ADDQ instruction description | `M68k::execute` | m68k/tests.rs semantic vectors / SNDR | Functional vectors; timing pending |
| ADDX | ADDX instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / SIOG native + browser | Functional vectors; timing pending |
| AND | AND instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| ANDI | ANDI instruction description | `M68k::group0` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| ASL | ASL instruction description | `M68k::shift` | m68k/tests.rs semantic vectors / SNDR | Functional vectors; timing pending |
| ASR | ASR instruction description | `M68k::shift` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| Bcc | Bcc instruction description | `M68k::execute` | m68k/tests.rs semantic vectors / SNDR | Functional vectors; timing pending |
| BCHG | BCHG instruction description | `M68k::group0` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| BCLR | BCLR instruction description | `M68k::group0` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| BSET | BSET instruction description | `M68k::group0` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| BSR | BSR instruction description | `M68k::execute` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| BTST | BTST instruction description | `M68k::group0` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| CHK | CHK instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| CLR | CLR instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| CMP | CMP instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| CMPA | CMPA instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| CMPI | CMPI instruction description | `M68k::group0` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| CMPM | CMPM instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| DBcc | DBcc instruction description | `M68k::execute` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| DIVS | DIVS instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / SIOG native + browser | Functional vectors; timing pending |
| DIVU | DIVU instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| EOR | EOR instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| EORI | EORI instruction description | `M68k::group0` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| EXG | EXG instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| EXT | EXT instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| ILLEGAL | ILLEGAL instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| JMP | JMP instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| JSR | JSR instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| LEA | LEA instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| LINK | LINK instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| LSL | LSL instruction description | `M68k::shift` | m68k/tests.rs semantic vectors / SNDR | Functional vectors; timing pending |
| LSR | LSR instruction description | `M68k::shift` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| MOVE (including CCR/SR/USP transfers) | MOVE instruction description | `M68k::execute / group4 / sr` | m68k/tests.rs semantic vectors / SNDR | Functional vectors; timing pending |
| MOVEA | MOVEA instruction description | `M68k::execute` | m68k/tests.rs semantic vectors / SNDR | Functional vectors; timing pending |
| MOVEM | MOVEM instruction description | `M68k::movem` | m68k/tests.rs semantic vectors / SIOG native + browser | Functional vectors; timing pending |
| MOVEP | MOVEP instruction description | `M68k::group0` | m68k/tests.rs semantic vectors / SIOG native + browser | Functional vectors; timing pending |
| MOVEQ | MOVEQ instruction description | `M68k::execute` | m68k/tests.rs semantic vectors / SNDR | Functional vectors; timing pending |
| MULS | MULS instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / SIOG native + browser | Functional vectors; timing pending |
| MULU | MULU instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| NBCD | NBCD instruction description | `M68k::bcd / alu / group4` | m68k/tests.rs semantic vectors / SIOG native + browser | Functional vectors; timing pending |
| NEG | NEG instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / SNDR | Functional vectors; timing pending |
| NEGX | NEGX instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| NOP | NOP instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| NOT | NOT instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| OR | OR instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| ORI | ORI instruction description | `M68k::group0` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| PEA | PEA instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| RESET | RESET instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| ROL | ROL instruction description | `M68k::shift` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| ROR | ROR instruction description | `M68k::shift` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| ROXL | ROXL instruction description | `M68k::shift` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| ROXR | ROXR instruction description | `M68k::shift` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| RTE | RTE instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / SNDR | Functional vectors; timing pending |
| RTR | RTR instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| RTS | RTS instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| SBCD | SBCD instruction description | `M68k::bcd / alu / group4` | m68k/tests.rs semantic vectors / SIOG native + browser | Functional vectors; timing pending |
| Scc | Scc instruction description | `M68k::execute` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| STOP | STOP instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / SNDR | Functional vectors; timing pending |
| SUB | SUB instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| SUBA | SUBA instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| SUBI | SUBI instruction description | `M68k::group0` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| SUBQ | SUBQ instruction description | `M68k::execute` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| SUBX | SUBX instruction description | `M68k::alu` | m68k/tests.rs semantic vectors / SIOG native + browser | Functional vectors; timing pending |
| SWAP | SWAP instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| TAS | TAS instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| TRAP | TRAP instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / SIOG native + browser | Functional vectors; timing pending |
| TRAPV | TRAPV instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| TST | TST instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |
| UNLK | UNLK instruction description | `M68k::group4 / exception` | m68k/tests.rs semantic vectors / FAMG native + browser | Functional vectors; timing pending |

## Bus, timing and persistence

| Feature / reference | Current behavior / edge cases | Native | Browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- | --- |
| BIOS ROM, O map | Exact 512 KiB, big endian, read-only, reset vectors; synthetic guest exception handlers | bus.rs, boot.rs, AEROM | ROM, AEROM | Full synthetic boot/MMIO/service coverage missing; genuine BIOS compatibility optional and not independently confirmed | Pending |
| Low/high WRAM, O map | Independent 1 MiB banks; aligned B/W/L; low-RAM and CS3 high-RAM mirrors; P1 aliases; BWAIT software waits and narrow CS3 beats | bus.rs, BWAIT | SC, ROM, BWAIT | Base CPU cycles, refresh and bus contention timing missing | Pending |
| Base-system address decoding, O map | Only documented subset in architecture.md | bus.rs | SC | A-bus/cartridge decoding and full base map missing | Pending |
| MMIO widths/side effects, device manuals | Unsupported accesses return errors | devices.rs | SC | Full device routing/width rules missing | Pending |
| Backup byte storage, SCU §1.2 figs.1.3/1.5; SMPC §2.3 SYSRES p.29 | 32 KiB, odd-byte storage; even lanes read FF and ignore writes; word/long strobes; P1 aliases; unformatted zero initialization | BK; `backup_byte_lanes_endpoints_and_cache_through_access_do_not_alias_work_ram` | BK | Additional backup aliases, filesystem/service integration and remaining reset domains pending | Pending |
| Backup reset retention, SMPC §2.3 SYSRES p.29 | Bytes survive successful/rejected HLE, disc and ROM resets and guest SYSRES; new Machine starts empty | BK; `packed_import_is_atomic_and_survives_every_boot_mode_and_rejected_boot`, PWRG | BK, PWRG, reset/mode/read-race checks | SYSRES guest retention now covered by PWRG; remaining power/reset and service contracts pending | Pending |
| Local backup import/export (host interface) | Exact 32768 packed bytes; atomic replacement; explicit local download; stale import cancellation; no network/storage | `packed_import_is_atomic_and_survives_every_boot_mode_and_rejected_boot` | Download byte comparison, offline import, invalid size/read errors, races, fresh-tab isolation | BIOS backup filesystem/calls and cartridge backup absent; no automatic cross-session persistence | Pending |
| Shared event scheduler, O clock domains | Deterministic CPU order, raster/SCU event deadlines and clock-driven DSP/DMA; field deadlines replace instruction budgets | TIME, SCUROM, game.rs, DUAL | SCUROM, SC, DUAL | CPU opcode/on-chip DMA cycles, contention, refresh and full cross-clock ordering missing; CD command/sector/sample deadlines now exercised by CLAB | Pending |
| Video timing, VDP2 display registers | Clock-based 263-line/224-visible NTSC raster, HBLANK/VBLANK bits and SCU edges; bounded field advancement | TIME, SCUROM, game.rs | SCUROM, SC | 320/352 normal NTSC clocks now have CLKG evidence below; other resolutions, PAL/interlace and exact beam phase remain missing | Pending |
| Audio timing, SCSP/CD | Independent 68k/sample deadlines, bounded stereo output and worklet; underflow/mute/blocked instrumentation | SNDV, SNDR | SNDR | Exact CPU/bus timing and effects DSP | Pending |
| Power/reset domains, SMPC / H | Cold reset clears both CPUs/devices and retains backup; SSHOFF resets slave CPU/FRT while retaining shared state; SLEEP distinct | boot.rs, sh2_system.rs, DUAL | ROM/reset, DUAL | Manual/system reset and remaining power/clock controls missing | Pending |

## SCU and DSP

SCU subsets now execute actual operations. The entire SCU acceptance criterion
remains pending: bus arbitration/waits, all device connections and exhaustive DSP
edge/transfer coverage are still required. Unsupported registers fail explicitly.

| Feature / manual | Implemented behavior | Native / browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- |
| SCU interrupt registers, [SCU] table 2.1/§3.5, [SCUFINAL] #7/10 | IMS/IST, fixed priorities/vectors, master delivery/acknowledgment, A-bus acknowledgment gate | SCUV, SCUROM / SCUROM | VDP1 source 13 and CD A-bus source 16 now execute guest handlers (CLAB); complete external pin/arbitration behavior unverified | Pending |
| A/B-bus coordination, [SCU] §3.6, [SCUFINAL] #8–12 | Prohibited DMA source/destination checks and bounded error handling | SCUV / SCUROM (valid transfers) | ASR/refresh wait-state and CPU/bus contention behavior missing | Pending |
| DMA channels 0–2 direct, [SCU] §§2.1/3.2, [SCUFINAL] #15–24 | Counts, address save/update, unaligned heads/tails, B-bus halfword strides, all seven edge triggers/GO, end/illegal IRQ and priority preemption; one queued retrigger | SCUV, SCUROM / SCUROM (channels 0/1) | Exhaustive trigger/device/boundary tests, actual bus latencies and prohibited three-channel behavior pending | Pending |
| DMA indirect, [SCUFINAL] #24/25 | Final-spec count/write/read layout, source bit-31 end marker, table walking, write-address continuation | SCUV, SCUROM / SCUROM | Entire table alignment/overflow/error contract and all count limits need further vectors | Pending |
| DMA device interactions, [SCU] §2.1 | High-RAM to VDP2 VRAM and executing DSP external RAM transfers, real IRQ handlers | SCUV, SCUROM / SCUROM | CLAB adds CD FIFO to high-RAM fixed-source DMA/IRQ; VDP1 routes and full cross-device arbitration remain | Pending |
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

**VIDEO** means [the independent video vectors](../tests/video.rs), covering
literal texture/primitive/color/register/pixel results from [VDP1] §§2–7 and
[VDP2] §§3–6,8–13. Implementations are
[`Vdp1`](../src/vdp1.rs) and [`Vdp2`](../src/vdp2.rs), routed by
[`Bus`](../src/bus.rs). **PRISM** means the original licensed
[Prism Courier disc/source](../homebrew/prism-courier/README.md), executed by
[the native integration](../tests/prism_courier.rs) and browser Wasm in
[browser-smoke.mjs](../scripts/browser-smoke.mjs). PRISM proves normal RGB555
sprites, solid polygons, sequential END/IRQ, NBG0 bitmap, composition and guest
input/audio/gameplay. It does not establish every mode in VIDEO in the browser.
**EFFECTS** means [the independent pixel and guest vectors](../tests/video_effects.rs),
[original SH-2 fixture](../tests/fixtures/effects.s), its
[reproducible builder](../tests/fixtures/build_effects.py) and the same guest in the
extended browser suite, including mutated matrix data and actual Canvas pixels.
Rotation equations and coefficient sampling are in
[rotation.rs](../src/vdp2/rotation.rs); normal line/cell coordinates and line windows
are in `Vdp2::normal_coordinates/window`.
**VIRQ** means `vdp1_end_interrupt_and_draw_end_dma_trigger_obey_clock_deadlines`
in bus.rs (1/3/8/17/32-clock chunks, reset, source 13 and DMA trigger 6).

| Feature / manual | Current behavior / edges | Native | Browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- | --- |
| VDP1 VRAM/framebuffers/registers, [VDP1] §§2,4 | 512 KiB VRAM, two draw/display buffers, byte/word/long memory; checked paired register writes; MODR | VIDEO | PRISM | All bus lanes/mirrors/waits; alternative scanout modes | Pending |
| Command list/control flow, [VDP1] §6.1 | END, next/assign/call/return and skip variants, one return level, invalid links/prohibited commands, bounded cyclic lists | VIDEO | PRISM (sequential END) | Full guest control-flow corpus, precise fetch timing | Pending |
| Normal/scaled/distorted sprites, [VDP1] §§6.2,7.4–7.6 | Width/height, flips, two-point and nine anchor scales, ruled quadrilateral scan conversion | VIDEO | PRISM (normal) | Physical edge/zoom rounding, high-speed shrink and preclip direction | Pending |
| Polygons/lines/polylines, [VDP1] §§7.7–7.9 | Solid ruled quads and integer line segments | VIDEO | PRISM (polygons) | Full independent sloped-edge and repeated-pixel corpus | Pending |
| Clip/local coordinates, [VDP1] §§7.1–7.3 | System/user inside/outside clips, local offsets | VIDEO | PRISM (system) | Full extreme-coordinate/clip boundary corpus | Pending |
| Sprite texture/color modes, [VDP1] §§6.3–6.6 | Packed 4-bit bank/LUT, 64/128/256 bank and RGB555, transparent pixels, two end codes per raster row | VIDEO | PRISM (RGB555) | Scaling/end-code interaction, wrapping and all browser formats | Pending |
| Shading/color calculation, [VDP1] §6.3 | Gouraud, mesh, MSB-only, shadow, half-luminance/transparency, combined Gouraud effects | VIDEO | PRISM (replace only) | Subpixel interpolation and duplicate-coverage edges; non-normal scanout | Pending |
| Draw/erase/swap/status/IRQ, [VDP1] §4 | PTMR, forced stop, manual/auto change and display erase, persistent VBE, CEF/BEF/COPR/LOPR; SCU IRQ/DMA trigger | VIDEO, VIRQ, PRISM | PRISM (auto/IRQ) | Physical erase bandwidth/timing, transfer-over, interlace/rotation | Pending |
| VDP2 VRAM, [VDP2] §3 | Bitmap RAM and bank offsets | VIDEO, SC | PRISM, SC | VRAM bank/cycle arbitration; CYCA0 words still only stored | Pending |
| VDP2 CRAM, [VDP2] §3.4 | Modes 0/1 RGB555 and 2 RGB888; mode-0 upper-half mirror; palette offsets | VIDEO | — | Full lane/alias and guest/browser palette corpus | Pending |
| TVMD/TVSTAT, [VDP2] §2.2 | 320×224 noninterlace, display enable, clocked blanking and SCU edges | VIDEO, TIME | PRISM, SC, SCUROM | Other resolutions, PAL/interlace, beam status/latches | Pending |
| NBG0 bitmap, [VDP2] §§4–6 | 4/8/11-bit palette, RGB555/RGB888, bitmap dimensions/base, transparency, fixed-point scroll/zoom | VIDEO | PRISM, SC (RGB555, unit zoom) | Full format/dimension/zoom vectors and browser coverage | Pending |
| NBG0–3 tile/character, [VDP2] §§4.4–4.8, figs.4.9/4.10, table4.8 | `Vdp2::character_control/tile/background`: one/two-word names, both supplement modes, 8×8/16×16 characters, 4/8/11-bit palette and RGB555/888 where applicable, flips, plane/page alignment/maps/wrap | [seven independent pixel/guest tests](../tests/video_tiles.rs) | TILE (four layers, names, palette/VRAM, window, mutation) | VRAM cycle arbitration, bandwidth/layer restrictions and exhaustive guest corpus remain | Pending |
| NBG1–3 bitmap/layer control, [VDP2] §§4–6 | NBG1 bitmap composition, formats applicable to NBG1, scroll/zoom/priority | VIDEO | — | Complete layer restrictions and guest format corpus; NBG2/3 tile layers now exercised by TILE | Pending |
| Rotation backgrounds, [VDP2] §6.1 p.147, §6.3 figs.6.2–6.4, §6.4 fig.6.7 | RBG0 fixed-point A/B matrices, viewpoint/center/Z/translation/scales, 16-plane tile or bitmap, four screen-over modes, 16/32-bit coefficient modes in VRAM or upper CRAM, coefficient line-color data, per-dot/line indexing, transparency and parameter/window switching; RBG1 parameter B/cells/B0 characters/B1 names, NBG0 effects controls | EFFECTS, R1, LINE | EFFECTS + R1 + LINE (matrix/coefficient/color mutations and Canvas) | Complete RBG1 boundaries and bank contention; scanline parameter latching/reloads and cycle/bank arbitration remain required | Pending |
| Line/vertical-cell scroll, [VDP2] §5.3 figs.5.4–5.8 | Packed enabled X/Y/zoom fields, relative offsets and held line intervals, fractional coordinates, independent NBG0/1, interleaved vertical cell tables and mosaic precedence | EFFECTS | EFFECTS (line X) | Raster-time table reads, cycle/bandwidth arbitration and full guest field/interval corpus remain | Pending |
| Windows/line windows, [VDP2] §8.1 figs.8.2–8.4 | W0/W1 rectangle or VRAM line tables and SD-bit sprite windows, inside/outside, inclusive edges, empty/inverted ranges, AND/OR on normal/rotation/sprite/color calculation; rotation-parameter windows | VIDEO, EFFECTS, SPRX | EFFECTS, SPRX (Wasm/Canvas and changed guest controls) | Full raster-window update timing and complete independent guest corpus remain | Pending |
| Mosaic, [VDP2] §5 | NBG0–3 horizontal/vertical grouping | VIDEO | — | All independent vertical/layer/guest vectors | Pending |
| Priority/composition, [VDP2] §§9–11 | Four normal backgrounds, RBG0/RBG1 and VDP1; all sixteen palette sprite types (including shared bits) and mixed RGB; priority-0 disable/ties; SFPRMD character/dot priority from names, supplement or bitmap flags and SFCODE/SFSEL | VIDEO, PRISM, SPRX, SPECIAL | PRISM, SPRX, SPECIAL (guest flag/code mutations and Canvas) | Full display/layer/raster restrictions and complete special-priority boundary corpus | Pending |
| Color calculation/offset/shadow, [VDP2] §§11–14 | Top/second-image ratio selection, gradation horizontal filtering and priority, additive and three/four-image extended blending; SFCCMD character/dot/palette-MSB conditions; signed offsets, normal/MSB/transparent shadows after blend/offset | VIDEO, SPRX, LINE, SPECIAL, GRAD | SPRX, PRISM, LINE, SPECIAL, GRAD | Gradation border/timing conformance, full guest format/extended-composition corpus and raster timing remain | Pending |
| Back/line color, [VDP2] §§7.1/11.3 | Back-screen single/per-line VRAM table, signed offset; LNCLEN top-only insertion, single/per-line LCTA palette data and coefficient-derived low seven address bits; CCRLB line/back ratios | VIDEO, LINE | PRISM (back), LINE (two line colors, coefficient mutations, extended calculation, Canvas) | Interlace/display-mode line rules, raster-time fetches and complete guest format corpus remain | Pending |
| Combined VDP1/VDP2 pipeline | Sprite/background overlap, priority, transparency, game result/restart, texture mutation changes actual pixels | VIDEO, PRISM | PRISM + Canvas assertions | Remaining modes/effects and timing above | Pending |

VDP1 work advances one command fetch or ruled raster row per 32 reference clocks;
this bounds each scheduler event but is a functional quantum, not measured draw
cycles. Erase takes effect at a field boundary. Color calculations use integer
interpolation; exact physical edge rounding/transfer-over remain required. Native
vectors caught and corrected shadow MSB halving and automatic-start BEF handling.
Cyclic lists remain busy without reporting END. Invalid lists stop the machine
explicitly. Rotated VDP1 scanout and enabled unimplemented VDP2 display modes fail
explicitly. Readback of VDP2 write-only controls remains a debugger convenience.
No game-specific rendering, guest identity check, canned pixels or host result
mailbox writes are used.

## Sound

**SNDV**: [native SCSP vectors](../src/sound/tests.rs).
**DSPV**: [original DSP register/arithmetic/ring/float/output vectors](../src/sound/dsp/tests.rs).
**SNDR**: [original SH-2 + 68k sound ROM](../tests/fixtures/build_sound.py),
[native guest assertions](../tests/sound.rs) and
[browser guest/WebAudio assertions](../scripts/browser-smoke.mjs).
Implementation: [shared sound bus, PCM and clock domain](../src/sound.rs),
[68k interpreter](../src/m68k.rs), [producer](../web/audio.js) and
[bounded worklet consumer](../web/audio-worklet.js).

| Feature / manual | Implemented behavior | Native / browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- |
| Sound RAM / SCSP mapping, [SCSP] §§1.2/4.1 | 512 KiB RAM shared by SH-2 and independent 68k; big-endian byte/word/long cycles; SCSP byte lanes; SNDON/OFF reset vectors and RAM retention; RESET output resets SCSP controls | SNDV, SNDR / SNDR | Additional memory-size configuration, exact reset pulse/bus timing (sound RAM mirrors now tested in bus.rs) | Pending |
| 32 slots, PCM8/16, [SCSP] §4.2 table 4.14 | Guest key-on/off, signed PCM, source bit controls, all 32 slots, no/forward/reverse/alternative loops, Q16 OCT/FNS pitch, signed first-order PCM8/16 interpolation for all loop directions, sound stack | SNDV, SNDR (including guest half-pitch vectors) / SNDR (half-pitch vectors) | Exact hardware fractional boundary rounding and complete envelope-controlled playback | Pending |
| Envelope, [SCSP] §4.2 pp.40–44 | Attack/decay1/decay2/release, zero-rate holds, EGHOLD, decay level, key scaling, loop-start transitions and release retrigger; EG multiplies waveform before TL/ALFO, modulation stack and DSP send | SNDV, ENV / ENV + operand mutations + WebAudio | Numeric slope/key-scaling/attenuation policy is functional, not corroborated chip timing; complete synthesis/pipeline conformance remains | Pending |
| FM/modulation/LFO, [SCSP] §4.2 figs.4.18/4.31, tables4.16–4.24 | Executing recent/past sound-stack selection with five-slot pipeline, signed averaging/MDL, guard-waveform reads; per-slot AM/PM LFO waveforms, frequency/depth/reset; internal noise/zero source | SNDV, SYNTH / SYNTH operand mutations and WebAudio | Exact chip noise sequence is unspecified; physical sub-sample rounding, full FM overflow/loop boundaries and envelope integration remain | Pending |
| Levels/pan/external inputs, [SCSP] §4.2 tables 4.27/28 | Binary total-level attenuation, direct send and fixed-point stereo pan, clipping, master output control; independently expected full-volume stereo buffers | SNDV, SNDR / SNDR | Intermediate master gain conformance and complete routing boundary corpus | Pending |
| Timers/interrupts, [SCSP] §4.2 tables 4.33–38 | Three sample-clock prescaled counters; sound/main pending/enables/clear; programmed autovector levels, 68k IRQ/RTE, SCU source 6 | SNDV, SNDR / SNDR | Complete cross-device contention/timer reset edge evidence | Pending |
| MIDI registers, [SCSP] §4.2 figs.4.59–61 | Four-byte input/output FIFOs, status/overflow, byte lanes, serial-paced 8-N-1 output, independent main/sound empty IRQs and reset cancellation | SNDV, SIOG / SIOG | Serial bit-level input/external wiring; the standard console has no MIDI connector (fig.4.59) | Pending |
| SCSP DMA, [SCSP] §4.2 fig.4.66/table 4.39 | Word transfers in both directions, zero gate, count/address progress, DEXE completion and IRQ; DMA-register target rejected | SNDV, SIOG / SIOG (68k register DMA, completion IRQ and source mutation) | Exact transfer timing/arbitration and full boundary/route evidence | Pending |
| SCSP effects DSP, [SCSP] §§4.1/4.3/5; [ST-228][SCDSPASM] Reference 1/2 | Executing multiply/add pipeline; COEF/MADRS/MPRO/TEMP/MEMS/MIXS/EFREG, slot and EXTS inputs, ring/table RAM reads/writes, linear/float conversion, stereo effect routing | DSPV, CLAB / CLAB coefficient mutation, exact stereo, WebAudio | Full field/pipeline boundary corpus, RAM-access contention and independently corroborated converter/timing edges | Pending |
| Browser audio output | Bounded 8192-frame producer/ring, stereo sample-rate adaptation, explicit gesture enable, pause/mute/blocked/underflow/drop states; worklet renders nonzero guest PCM | SNDV / SNDR WebAudio instrumentation | Full synthesis/DSP/CD-DA validation depends on missing hardware | Pending |
| CD-DA, [CD] §§3/4/8 / [SCSP] table 4.31 | Guest play/repeat, signed stereo BINARY tracks; PRE 50/15 µs de-emphasis with continuous sector history; unchanged-start pause/resume retains the sample/filter state; EXTS0/1 EFSDL/EFPAN, mixed with 68k PCM at sample deadlines | [CD-DA vectors](../tests/cd.rs), [original CD-DA guest](../tests/cdda.rs), CLAB / CD-DA guest reference buffers + AudioWorklet, CLAB changed audio track | Full effects pipeline/subcode and complete synthesis remain; scan attenuation/muting has native vectors | Pending |

The sound CPU has an independent 11.2896 MHz domain and SCSP has a 44.1 kHz
sample domain relative to the fixed 26.8741 MHz reference. Native traces match
across 1/3/8/17/611/10000-clock advancement chunks. These deadlines do not make
the approximate 68k opcode budgets or one-word-per-sample SCSP DMA electrically
accurate. Unsupported synthesis and invalid DSP instructions/ports fail explicitly. Original guest arithmetic
changes alter both RAM waveform and exact native/Wasm PCM; canned buffers and
no-op CPU/device implementations cannot satisfy those assertions.

## CD, boot and SMPC

| Feature / manual | Current behavior / edge cases | Native | Browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- | --- |
| Cooked ISO, [Disc Format][DISC] | Bounded IP/ISO9660 first-read startup; retained media and runtime sector/file reads; checked LBA/FAD/volume/record bounds and atomic directory changes | disc.rs, media.rs, cd.rs, CLAB | SC, CLAB/local ISO | General boot layouts/size, interleaved/XA filesystem records | Pending |
| BIN/CUE/raw/mixed tracks, [DISC] / [CD] §§4/7.2.3 | Caller-resolved files, cooked/2352-byte Mode1/Mode2/audio, shared/separate files, INDEX00–99, gaps and TOC; subindices and PRE/DCP/4CH CONTROL metadata drive TOC/status/Q reporting; malformed/duplicate flags rejected; EDC validation, bounded P/Q correction for Mode1/XA Form1, Form2 EDC and PRE audio processing | [media vectors](../tests/media.rs), [parity-matrix/XA vectors](../src/media/integrity.rs), [directory guest](../tests/cd_drive.rs), CLAB | Directory guest on clean/correctable/uncorrectable raw media; CLAB multi-file CUE, INDEX02/99, malformed ordering, missing-reference/reset/privacy | Raw non-XA Mode2 and complete decode/timing boundaries remain; WAVE/compression absent | Pending |
| CD command/status/HIRQ, [CD] §§3.1–3.4 | Separate command/response latches; 1075-clock command completion, read-protected responses, CMOK/DRDY/CSCT/end flags, masks, clear-zero HIRQ and SCU A-bus IRQ | `tests/cd.rs`, CLAB | CLAB command/FIFO/DMA/IRQ | Full command-specific timing and register mirrors; BIN/CUE has no R–W stream, with empty FIFO WAIT now tested | Pending |
| CD drive/TOC/session/track state, [CD] §§4/7/8.2 | TOC/session/Q, bounded seek, data play, CD-DA/repeat/end, media-dependent FAD/track/index reports; timed CDON/OFF, 60 Hz pause reports, programmed standby, scan directions/range/muting, tray/media retention and deterministic virtual close | `tests/cd.rs`, CLAB, CDDRV | CLAB, CDDRV | Remaining initialization/subcode options and full drive/response lifecycle boundary evidence | Pending |
| CD buffers/transfers, [CD] §§3.4/5.3/5.4/8.2 | 200-sector pool, 24 partitions, get/delete/get-delete/put, short/over transfers, size/info/search, FIFO word/long and fixed-source SCU DMA | `tests/cd.rs`, CLAB | CLAB actual DMA/IRQ/checksum, sector mutation | Pool-full pause/resume at the blocked FAD has native data vectors; complete transfer overlap and timing/error inventory remain | Pending |
| Filters/selectors/partitions, [CD] §§5.2/5.3/8.2.5 | FAD and masked subheader comparisons, reverse conditions, true/false routes, reset, copy/move through destination filter | `tests/cd.rs::selectors_fad_range_connections_copy_move_and_size`, Mode2 reverse vectors | CLAB default filter/partition route | Full connection constraints and all boundary/error vectors remain | Pending |
| BIOS-less startup, [Boot ROM][BOOT] / [DISC] | Original first-read HLE plus licensed SH-2 master/slave reset trampoline and system-service initialization, separate from ExternalBios | disc.rs, game.rs, CLAB, src/hle/boot.s, HLESYS | SC, CLAB, HLESYS | Remaining clock/multiplayer, backup/CD services and full startup/region/version contracts | Pending |
| BIOS services, [BOOT], ST-162-R1 System Program §2.2 | Original guest-executed vector/callback registration, SCU mask/shadow, shared semaphores, copied priorities and power-clear bytes; see HLESYS below | tests/hle_system.rs | HLESYS, including nested IRQs, mutations and reset | Clock/multiplayer, backup/CD calls and complete per-call ABI/side effects remain | Pending |
| External BIOS execution, [BOOT] | Genuine reset vectors, original synthetic ROM handlers | boot.rs, sh2_system.rs | ROM | Genuine BIOS initialization/menu/boot **unverified**; no lawful local BIOS supplied | Pending |
| SMPC direct digital port 1, [SMPC] §3 | Active-low nibble selection; per-bit PDR/DDR input/output merging; TH input visible with EXLE | devices.rs, INTCG | SC, INTCG | DIGG settling and CLKG clock/reset vectors below; full system timing remains required | Pending |
| SMPC commands/status/timing, [SMPC] §§1.2/2.3/2.4/3.2 | SF/IREG/OREG/SR, CPU commands, INTBACK status/peripheral phases, continue/break, VBlank collection and optimization; SCU source 7 | smpc/tests.rs, tests/smpc.rs, DUAL | SMPR, DUAL | Remaining system commands, firmware jitter and PAL/interlace clock modes | Pending |
| SMPC reset/power/clock, [SMPC] table 1.1/§2.3 | MSHON, SSHON/OFF, SNDON/OFF, CDON/OFF, NMIREQ, SYSRES (volatile reset, installed-ROM or HLE-disc restart and battery retention); RESENAB/DISA control reset-button NMI edges and VBlank-latched RESB | DUAL, SNDV, SNDR, PWRG, smpc/tests.rs | DUAL, SNDR, PWRG | Clock switching, full status/power domains | Pending |
| RTC/region/system flags, [SMPC] §§2.4/2.5 | Deterministic Gregorian/BCD RTC, SETTIME, retained four-byte SETSMEM, status/region Japan, output control flags | smpc/tests.rs, tests/smpc.rs | SMPR | Full system-control/region configuration; remaining reset domains | Pending |
| SMPC direct digital port 2, [SMPC] §1.2 pp.7–9, §3.4 table 3.36 p.97 | Independent PDR2/DDR2, IOSEL2/EXLE2; active-low nibble reports; host keyboard levels | BK; `both_digital_ports_have_independent_direction_selection_and_enable_bits` | BK; I/K/J/L keyboard input and release checks | DIGG implements both-port settling; complete clock/reset packet timing remains | Pending |
| External latch/PAD IRQ, [SMPC] §1.2/table1.5, [VDP2] §2.5/tables2.3–2.4 | Both TH input falling edges gated by EXLE/DDR; independent SCU PAD IRQ and VDP2 EXLTEN; software/read latches, held HCNT/VCNT, EXLTFG read-clear, non-interlaced ODD; `Smpc::set_th`, `Bus::set_peripheral_th`, `Vdp2::read16/external_latch` | INTCG, bus.rs::smpc_input_edges_latch_raster_counters_and_raise_pad_irqs_with_independent_enables | INTCG (both ports, repeated edges and control mutation) | Pin synchronization, blanking-counter discontinuities and other display modes | Pending |
| Analog input, [SMPC] tables 3.19/3.37; [STN43] Analog mode | ID15 X/Y/Z and ID16 X/Y/AR/AL, active-low buttons, trigger hysteresis (146 on / 85 off); timed INTBACK and direct TH/TR/TL handshake, transaction latching/abort, independent ports; standard Gamepad radial axes/triggers and focus neutralization | smpc/tests.rs, tests/smpc.rs; `Analog`, `Handshake` in [controller.rs](../src/smpc/controller.rs) | SMPR + PADR (both direct ports, changed axis, ID15/16 Gamepad/focus) | Full electrical timing, mode-switch calibration, external-latch synchronizers and complete command/reset/clock integration remain | Pending |
| Input focus safety | Both pads use host button levels, blur/view/tab/pause release, no guest-memory shortcuts | game.rs | SC | Additional controller types absent | Pending |

## Concrete unresolved gaps and closure gates

1. Remaining SH-2 semantic/exception edges, complete reset domains, BSC configuration and remaining UBC/standby timing,
   DIVU overflow intermediates, DMAC DACK/CPU arbitration, external IRQs and bus locking;
   cycle-based dual-CPU scheduling and arbitration. CPU opcode decoding alone
   does not satisfy the CPU or multiprocessor criterion.
2. Complete MC68EC000 timing/prefetch edge conformance and sound bus arbitration;
   SCSP timed envelopes and complete modulation conformance remain required; FM/LFO/noise now have SYNTH subset evidence; effects DSP and CD-DA have CLAB subset evidence. Direct PCM,
   executing sound CPU, timers/DMA/FIFO subsets and WebAudio now have evidence.
3. Full base memory map, remaining backup mirrors and BIOS filesystem/calls,
   plus hardware clock/event scheduling. Packed backup import/export and byte
   retention pass subset tests; other reset domains remain absent.
4. Complete SCU A/B-bus wait-state/arbitration and device routes, DMA boundary/
   trigger coverage, DSP conditional/pipeline/bank-conflict and program-DMA evidence.
   SCUROM now executes real DMA/DSP/timer/IRQ operations but is subset evidence.
5. Remaining VDP1 edge/scanout/timing conformance and VDP2 tile timing, complete RBG1 edge coverage,
   parameter reloads, gradation border/timing conformance and remaining display modes, with
   complete independent guest and browser coverage. VIDEO, EFFECTS and PRISM
   provide actual subset pixel/composition evidence.
6. Complete remaining CD drive/command/timing/error behavior (see CD inventory),
   BIOS HLE ABI and full synthetic
   boot pathways; complete SMPC system/reset/clock timing and full external-latch
   behavior. CLKG/DIGG below add clock transitions and direct digital settling. RTC/SMEM/INTBACK/IRQ, analog
   joystick/3D-pad packets and direct analog handshakes have subset evidence.
7. The multiprocessor/SCU/runtime-CD/CD-DA/SCSP-DSP/advanced-video/backup
   hardware exerciser now has the original Hardware Lab disc (CLAB), covering
   all those named paths, including its new 68k-uploaded effects-DSP scene.
   Full remaining chip behavior and comprehensive independent evidence are still mandatory. Prism Courier supplies the
   original playable VDP1+VDP2+audio guest demonstration, including guest input,
   win/loss/restart and actual native/Wasm execution. The existing synthetic
   ROMs remain supporting fixtures; CLAB does not yet satisfy the entire exerciser criterion.
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
peripherals/SMPC by eight reference clocks plus inserted BSC software waits and
DIVU interlock waits, converting main clocks through the PLL. DMA address
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
BCR1 master/slave identification is independently tested by BSCG. Retail BIOS
boot remains unconfirmed.

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
[CD]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-162-062094.pdf
[BOOT]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-079B-R3-011895.pdf
[DISC]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-040-R4-051795.pdf

[DUALREF]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-202-R1-120994.pdf

[SCUFINAL]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-210-110194.pdf

## SMPC status/peripheral guest evidence

**SMPR** is the original [SMPC guest builder](../tests/fixtures/build_smpc.py),
[native execution](../tests/smpc.rs) and [browser Wasm execution](../scripts/browser-smoke.mjs).
The guest sets BCD time and four-byte battery memory, enables SCU SMPC IRQ,
reads status, toggles continue, waits for VBlank collection, and reads both ports.
Native calendar vectors cover leap years, 2100, year/day boundaries, malformed
BCD atomicity, retention and advancement chunks. Browser synthetic Gamepad levels
produce the same active-low digital and X/Y/Z packets; focus loss produces released
buttons and neutral axes. No JavaScript writes guest result mailboxes.

The clock initially uses NTSC 320; CLKG below adds the 352 PLL and its reset
domains. Status collection uses 8600 reference clocks;
SETTIME/SETSMEM use 1882/1075 clocks. Peripheral collection uses the same bounded
8600-clock functional duration; after initial measurement, enabled optimization
schedules readiness one millisecond before VBlank-in. Firmware polling jitter,
physical serial handshakes and other television/interlace modes are not claimed.
Battery RTC/SMEM survive volatile machine resets. This is additional implementation
and executable evidence; all missing umbrella requirements above remain mandatory.


## Additional original review fixtures

**BSCG** is [bsc.s](../tests/fixtures/bsc.s), built and checked by
[build_bsc.py](../tests/fixtures/build_bsc.py), with
[native integration](../tests/bsc.rs) and the extended
[browser suite](../scripts/browser-smoke.mjs). Both CPUs execute the same reset
entry, select their own mailbox and stack from BCR1.MASTER, take separate refresh
CMI exceptions and return with RTE. Reset repeats produce the same traces.
Its expanded 512 KiB ROM SHA-256 is
`edb24ebd073a9dcc7d9993df2d0229f75b63a5e1facc3b4da8c4af0321bdaa6b`.

**TILE** is [tiles.s](../tests/fixtures/tiles.s), generated by
[build_tiles.py](../tests/fixtures/build_tiles.py), with
[native pixel/guest vectors](../tests/video_tiles.rs) and the extended browser
suite. SH-2 instructions write all four normal layers' character RAM, names,
palettes, priorities and a window. Changing only a ROM palette literal changes
native/Wasm pixels; the browser presents through the app's production frame path
and asserts Canvas pixels. Its expanded ROM SHA-256 is
`7a68d198cee6cb7295cb013d80b2c030791d6140101d813807ff1ea89916df9d`.
Both fixtures are original, covered by the adjacent MIT license, reproducible in
CI, and contain no firmware/SDK objects. They are diagnostics; the required
hardware exerciser disc and the other umbrella criteria remain outstanding.

[68UM]: https://www.nxp.com/docs/en/reference-manual/MC68000UM.pdf


## Original advanced-video and controller fixtures

**EFFECTS** is [effects.s](../tests/fixtures/effects.s), reproduced with
[build_effects.py](../tests/fixtures/build_effects.py). Guest instructions generate
all bitmap pixels, matrices and coefficient/line-scroll/window tables. Native and
browser tests check thirteen independent composition coordinates plus the blank
boundary. Mutating My from 3 to 2 changes the rotated pixels in both paths; the
browser asserts production Canvas presentation. Expanded ROM SHA-256:
`c537273e864fd3410441d618bfe78e8642137838d869d154f002c3c33c3a77d5`.
The renderer takes a frame snapshot, including these tables. This does not yet
implement raster-time writes or parameter reload strobes. Coefficient line color
and CRAM coefficient sources are now implemented and tested by LINE below;
enabled unsupported parameter reloads still fail explicitly.

**PADR** is [controller.s](../tests/fixtures/controller.s), reproduced with
[build_controller.py](../tests/fixtures/build_controller.py). The guest configures
both ports, drives TH/TR, polls actual TL acknowledgments, stores the received
ID16 and ID15 nibbles and ends each transaction. Native and browser compare exact
pin bytes, reset repetition and changed axis input. Unit vectors separately check
53/54-clock response boundaries, latched input, abort and trigger hysteresis.
Expanded ROM SHA-256:
`add157309fb3badcbc05344bcba2d775bdbe44c11afb9e3e2cbf8c11077c0e57`.
The 54-clock response is a deterministic 2-microsecond functional deadline in the
fixed NTSC clock domain, not measured controller electrical timing.

Both are wholly original, licensed by the adjacent MIT license and reproducible in
CI. They are diagnostic ROMs and do not replace the required hardware exerciser
disc. No mandatory umbrella criterion is declared complete by these fixtures.

[STN43]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/info/hon/stn43.htm


## Runtime CD and mixed-disc guest evidence

**CLAB** is the original [Hardware Lab source and guest contract](../homebrew/hardware-lab/README.md),
[reproducible builder](../homebrew/hardware-lab/build.py),
[native integration](../tests/hardware_lab.rs) and [browser suite](../scripts/browser-smoke.mjs).
Implementation is [owned media](../src/media.rs), [CD registers/drive/filter/FIFO](../src/cd.rs),
[bus/event/SCU wiring](../src/bus.rs), [SCSP external-input/sample route](../src/sound.rs),
and [independent HLE reset trampoline](../src/hle/boot.s).

The CD reference link above is corrected to **ST-162-062094**, which contains
CD Communication Interface §§3–8; ST-162-R1 is the system/SMPC/backup library
volume. Register word encodings are cross-checked against the independent
[CD protocol register tables](https://wiki.yabause.org/index.php5?title=CDCommands).
No emulator source, firmware or SDK routines were copied or transliterated.

`tests/cd.rs` supplies authored response/FIFO/sector/filter/directory/audio vectors;
`tests/media.rs` supplies raw/cooked equivalence, TOC, gaps/indices, missing/ambiguous
files, truncation and deterministic malformed-text mutations. The guest's LBA60
payload is not part of first-read code. Its checksum comes from runtime CD play,
selector storage, FIFO consumption by SCU DMA, IRQ delivery and guest summation.
Changing only that sector changes the checksum. Changing only the audio track
changes Wasm's mixed stereo buffer, rejecting a disconnected CD-DA route. The
browser also asserts actual production Canvas and AudioWorklet delivery.

Every new guest/asset/trampoline has original source and adjacent MIT licensing.
CI checks all generated media and both new reproducibility builders. Hardware Lab
now includes an executing SCSP effects-DSP program (see below). All other
mandatory unresolved features and comprehensive conformance evidence remain
required for #17. No umbrella checkbox is satisfied by this evidence alone.


Reverse and alternating fractional PCM now interpolate decoded signed PCM8/16
in traversal order, with reverse wrapping and alternating endpoint holds.
`src/sound/tests.rs::fractional_reverse_and_alternating_loops_interpolate_in_traversal_direction`
uses independently authored positive/negative half-pitch vectors. The browser
patches only the original 68k sound guest's SCSP pitch/key-on immediate words and
checks an exact reverse-loop stereo vector in Wasm. Exact hardware sub-sample
rounding, timed envelopes, complete modulation conformance and full effects-DSP conformance remain mandatory work.


## Executing SCSP effects processor and guest evidence

The independent [effects interpreter](../src/sound/dsp.rs) executes 128 parallel
steps per sample. Its signed 24-bit inputs, 13-bit multiplier operand, 26-bit
accumulator, previous-accumulator shifter, TEMP/sample decrement and sound RAM
paths follow [SCSP][SCSP] §5 fig.5.1; register locations/widths follow §4.1
figs.4.5–4.7, and RBL/RBP follow §4.3 table4.41. [ST-228][SCDSPASM] Reference1
and Reference2 describe the original assembler operations and delayed signal
flow. The documentation omits some low-level instruction/pipeline details;
this functional implementation is not evidence of electrically accurate SCSP
DRAM cycles or complete converter/undocumented-bit conformance. No third-party
processor core, assembler, SDK object or effect program was used.

| Register/instruction group | Implementation | Native evidence | Wasm/guest evidence and remaining work |
| --- | --- | --- | --- |
| COEF/MADRS/MPRO; TEMP/MEMS/MIXS/EFREG; byte lanes | `Dsp::read/write`; `Scsp::write8` | DSPV `coefficient_internal_ram_and_byte_lanes_obey_physical_widths`; RESET vector in SNDV | CLAB uploads coefficients/program/MEMS and SH-2 reads EFREG1; complete per-port guest corpus remains |
| TRA/TWA/TWT, sample decrement, read-before-write | `Dsp::sample` circular TEMP indices | DSPV `temp_pointer_moves_once_per_sample_and_reads_precede_parallel_writes` | CLAB arithmetic; delayed TEMP guest scene remains |
| XSEL/YSEL/IRA, signed multiplication, ZERO/BSEL/NEGB | `Dsp::sample` signed multiplier/26-bit accumulator | DSPV `multiply_accumulate_uses_signed_13_bit_y_and_previous_accumulator_output` | CLAB MIXS0/MEMS0/EXTS0/1 and two coefficients; all ALU control combinations remain |
| SHIFT, EWT/EWA, saturation/wrap | `Dsp::sample` prior accumulator and 16-bit effect accumulation | DSPV `shifter_clips_or_wraps_and_effect_writes_accumulate_with_sixteen_bit_width` | CLAB exact bipolar stereo; complete overflow guest corpus remains |
| YRL/FRCL/ADRL/ADREB | `Dsp::sample` latches and address gate | DSPV `y_fraction_and_address_latches_use_previous_step_values` | Complete interpolation/address-modulation guest vectors remain |
| MRD/MWT/IWT/IWA, TABLE/MASA/NXADR, RBL/RBP | `Dsp::sample` sound RAM path, read latch after two steps | DSPV `ring_table_linear_reads_use_two_step_latency_and_preserve_read_before_write` | Complete sound RAM/pipeline guest corpus and CPU/DMA contention remain |
| NOFL linear/normalized-float conversion | `pack/unpack`, `Dsp::sample` | DSPV `floating_memory_format_has_independent_bit_patterns_and_bounded_error` | Full independent float/subnormal/rounding and delayed memory-cycle corroboration remains |
| IMXL/ISEL, EFSDL/EFPAN, external CD route | `Dsp::add_slot`, `Scsp::sample` | DSPV `slot_send_wraps_mix_without_saturation_and_dsp_output_obeys_routing`; CLAB exact samples | CLAB in browser: changed guest coefficient changes exact stereo and computed result; real AudioWorklet, changed CD track; full synthesis remains |
| Invalid instructions/ports | `Dsp::sample/read/write` bounded diagnostics | DSPV `illegal_memory_steps_and_reserved_ports_fail_without_faking_effects` | Guest fault corpus remains |

Hardware Lab's eight original DSP instructions are uploaded by actual 68k
`MOVE.W` instructions. Half-gain MIXS0 drives EFREG0, a half-gain MEMS0 multiply
computes EFREG1=0900, and half/quarter-gain EXTS0/1 drive EFREG2/3. The SH-2
reads EFREG1 into result offset28. Tests change only the 68k coefficient immediate
in the original disc and obtain 0900/0480/0000 and exact wet/dry PCM vectors.
With silent CD, the four stereo frames at half gain are
`[2304,4096],[-2304,-4096],[-4608,-8192],[4608,8192]`.
CD direct sends are muted, so changed CD-DA data must pass through the effects
processor to change the mixed buffer. Program/data/reset effects arise from
emulated instructions/MMIO; the host supplies media and inputs only.

The reproducible Hardware Lab ISO SHA-256 is now
`8bc56657bf0376fece7dece1a094f9e159f0d365af94adb6f87cb2fa5dc7995c`.
Its audio/CUE, Star Collector and Prism Courier remain byte-identical. See the
[validation record](validation-issue-17.md) and [guest contract](../homebrew/hardware-lab/README.md).
This implementation/evidence does not satisfy the full audio or umbrella criteria.

[SCDSPASM]: https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-228-R1-030596.pdf


## Executing SCSP slot modulation and independent guest evidence

**SYNTH** is [the original reproducible fixture](../tests/fixtures/build_synthesis.py),
[68k source builder](../tests/fixtures/build_sound.py), [native guest integration](../tests/synthesis.rs)
and the same [browser Wasm suite](../scripts/browser-smoke.mjs). The fixture and
all generated bytes use the adjacent MIT license. Expanded 512 KiB ROM SHA-256:
`9a40992d946a7872b0418c2ac07eea1c3e8b1aad9a7e6fb94282f94f558d4ccd`.

Implementation: [slot synthesis](../src/sound/synthesis.rs) and
[SCSP sample path](../src/sound.rs). Independent native vectors are in
[synthesis/tests.rs](../src/sound/synthesis/tests.rs) and [sound/tests.rs](../src/sound/tests.rs).
References: ST-077 §4.2 figs.4.18/4.31 and tables4.16–4.24. The sound-stack table,
including its recent/past selection columns, is applied after the documented
five-slot producer-to-consumer delay. STWINH retains committed data; FM changes
waveform reads without modifying the phase generator or CA. Source-bit controls
also apply to internal noise/zero sources.

The 68k writes two original guard ramps, starts a silent modulator and FM carrier,
and runs a separate square-ALFO voice. The SH-2 observes the 68k timer-IRQ result.
Native and browser assertions independently expect left samples
`1024,1152,1280,1408,1536,1664,1792,1920` and right levels `1024/64`.
Changing only the guest TL, MDL, PLFO or ALFO operands changes those samples;
reset repeats the output. Separate guest source-control mutations exercise internal
noise/zero output, signed noise, reset repetition and noise independence from LFORE. PLFO vectors use a separate mathematical cents oracle
with a one-unit PCM rounding tolerance. Browser checks send these synthesized
samples through the production AudioWorklet.

Oscillator periods carry rational remainders using the published hundredth-Hz
frequencies. Gain/pitch arithmetic uses fixed integer powers of two, identically
in native and Wasm. The manual does not specify a noise seed/polynomial; the
explicit functional policy uses independent maximal 16-bit LFSRs clocked per
sample, unaffected by LFORE or LFOF. This policy is repeatable, not a claim about
the chip's exact random bit sequence. Fine LFO frequency/phase/rounding and FM
address-overflow boundaries still require corroboration. EGHOLD with AR=0 and
silent AR=0 are implemented. The additional ENV fixture below now exercises timed envelope stages, release, key scaling and loop linking; numeric rate/attenuation calibration remains an explicit functional approximation. This adds executing hardware and
objective evidence without declaring the complete audio or umbrella acceptance
criterion satisfied.


## Original dual-rotation guest and RBG1 evidence

**R1** is the original [builder](../tests/fixtures/build_rbg1.py),
[SH-2 source](../tests/fixtures/rbg1.s), [native guest test](../tests/rbg1.rs),
[independent pixel vectors](../tests/video_effects.rs) and the identical
[browser Wasm/Canvas execution](../scripts/browser-smoke.mjs). All new source and
ROM bytes use the adjacent MIT license; no SDK/firmware/external assets are used.
Expanded ROM SHA-256:
`e213231f49cb9f045ba47f7f141ef6b183eb0540fa80012fd45ee964d6778141`.

Primary reference: ST-058-R2 §§4.1/6.2/6.4, table6.1, appendix RBG1.
[Rotation sampling](../src/vdp2/rotation.rs) uses parameter B for RBG1 and
[the cell reader/compositor](../src/vdp2.rs) reads names from VRAM-B1 and
characters from VRAM-B0. RBG1 uses NBG0's cell formats, transparency, mosaic,
window, priority, ratio and color-offset controls. Dual rotation disables the
normal layers. RBG0 must be enabled, mode0 selected and VRAM-B split; invalid
configurations fail explicitly. Coefficient B's MSB is ignored for RBG1.
Outside-bank image reads produce transparent pixels under the functional model;
the exact hardware invalid-bank output and contention are not claimed.

The guest constructs B0/B1 cells, two matrices, a red RBG0 bitmap and an RBG1
window. Native/Wasm pixels and the production Canvas independently match a
quarter-turn blue/green grid over red. Changing only My in the original guest
moves the grid and exercises B's transparent screen-over mode. Native vectors
also cover independent A/B movement, all five cell color formats, both pattern
name widths, priority, blending, window, coefficient MSB and color offsets.
RBG1 still samples the existing frame snapshot; scanline reloads, cycle/bank
arbitration and complete special-function/edge evidence remain required.
This closes an implementation gap without satisfying the full video criterion.

## Line color, CRAM coefficients and special/extended composition

**LINE** is the original [SH-2 source](../tests/fixtures/line-color.s),
[reproducible builder](../tests/fixtures/build_line_color.py),
[native register/pixel/guest tests](../tests/video_line_color.rs) and identical
[browser Wasm/Canvas execution](../scripts/browser-smoke.mjs).
**SPECIAL** is [special-video.s](../tests/fixtures/special-video.s), its
[builder](../tests/fixtures/build_special_video.py),
[native vectors/guest](../tests/video_special.rs) and the same browser suite.
Both sources and generated ROMs use the [original MIT fixture license](../tests/fixtures/LICENSE).
No third-party firmware, core, SDK routine or sampled reference output is used.

| Programming group / primary reference | Implementation | Independent native evidence | Wasm/guest evidence |
| --- | --- | --- | --- |
| LCTA/LCCLMD/LNCLEN, VDP2 §§7.1/11.3 | `Vdp2::render`: insert palette line only below enabled top; single/per-line table, word-address wrap, separate sprite enable | LINE: normal/sprite top, all CRAM modes, window/offset ordering, ignored bits, wrap, no visible line without top calculation | LINE: guest creates both line words, palettes and image, exact eight pixels and Canvas |
| RAMCTL.CRKTE, KTCTL/KTAOF, VDP2 §6.4 pp.163–169 | `Vdp2::coefficient`: 16/32-bit upper-CRAM or VRAM reads, signed coefficient widths, ignored offset, bank-local wrapping | LINE: both widths, signed scaling, wrap, source isolation; invalid CRAM mode rejection | LINE: actual CRAM writes, changed coefficient scale changes image sampling |
| KTCTL.RxKLCE, VDP2 §6.4 fig.6.6 | `rotation_screen`: line address low seven bits from 32-bit coefficient; correct A/B selection for modes0–3 and RBG1; short form ignores line enable | LINE and EFFECTS: A/B, mode2 A data with B image, window-selected B, dual-rotation A data, short form | LINE: changed coefficient line index changes palette color, not image |
| SFSEL/SFCODE/SFPRMD, VDP2 §§10.3/11.2 | `scroll_layer`, `tile_map`: per-character/dot priority LSB; original dot code before palette offsets; bitmap, one/two-word names, RBG0/RBG1 | SPECIAL and EFFECTS: all normal screens/name widths, zero priority raised to1, code A/B and RGB mode restrictions | SPECIAL: changed guest SFSEL/bitmap priority flag changes real overlap pixels |
| SFCCMD, VDP2 §12.3 | `color`, `scroll_layer`: per-screen/character/dot/palette-MSB; RGB mode3 always permits calculation | SPECIAL: bitmap/name flags, raw codes, RGB555/888, color-address offsets, transparency, reserved RGB mode2 | SPECIAL: changed guest color flag removes blending while preserving priority |
| CCRTMD/CCRLB/EXCCEN, VDP2 §12.1 fig.12.3/table12.2 | `render`, `extended_color`: second-image ratios, four ordered images, raw lower-screen enables, palette-format restrictions | LINE: top/second/line/back ratios, additive mode, three/four-image colors and CRAM-mode restrictions; top-only special conditions do not gate lower images | LINE: changed guest CCCTL enables extended calculation and changes exact eight-pixel output |

The [1997 Sega HTML manual §12.1/table12.2](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/hard/vdp2/hon/p12_12.htm)
explicitly gives **2:1:1** for the mode0 four-image case, correcting **2:1:0**
in the older ST-058-R2 PDF. The implementation follows that later primary table
and fig.12.3. Integer channel arithmetic is deterministic in native and Wasm;
physical fractional precision remains subject to the existing video limitations.

Expanded 512 KiB ROM SHA-256: LINE
`e611e93e15b673ba915291bab6e9886af2f8700ccca7b19187fcbdaa0d4b9c1f`;
SPECIAL `118254a4eec38b8a2f4c0d10fcf47ebe9a251a61deeae361fc1395ff0d185923`.
CI checks both builders. Native/browser tests repeat resets and mutate only guest
literal operands, rejecting canned output and inert MMIO. All existing demo media
remain byte-identical. Gradation, display modes/interlace, raster parameter reloads,
VRAM cycle arbitration and comprehensive remaining hardware acceptance are still
required. No umbrella checkbox is declared complete by these additions.

## Runtime ISO and CD-DA review regressions

Related to #17. [CD directory parsing](../src/cd.rs) validates PVD type/version,
2048-byte block size, the root record, matching endian copies, volume sequence,
record/name/sector boundaries and volume/media extent bounds before publishing a
new directory. LBA-to-FAD and file-range arithmetic are checked. A rejected
navigation preserves the prior directory; mounting a new medium invalidates the
old file table and transfer. The exact 17-sector overflowing-root reproducer,
root/child mutation corpus and atomicity cases are in [CD vectors](../tests/cd.rs).
The original directory-command SH-2 fixture executes Change/Read Directory and
scope commands through real MMIO in [native](../tests/cd_drive.rs) and
[browser Wasm](../scripts/browser-smoke.mjs) tests. No debugger writes command
responses or result mailboxes.

[Raw-sector integrity](../src/media/integrity.rs) derives CRC-32 EDC and P/Q
product-code parity from [ECMA-130 §14.3 and annex A](https://ecma-international.org/wp-content/uploads/ECMA-130_2nd_edition_june_1996.pdf).
A bounded four-pass decoder repairs single-symbol codeword errors and recoverable
interleaved bursts, and requires matching EDC/parity before returning data.
Mode1 headers, reserved bytes, Mode2 duplicated XA subheaders and Form2 EDC are
checked. Form2 has no P/Q correction. Native tests verify the CRC check value,
both parity-check-matrix rows, every protected Mode1 byte position, every nonzero
error byte at selected positions, XA Form1/subheader correction, Form2 error
rejection and unrecoverable sectors. Browser tests generate original raw media
using an [independent parity-matrix encoder](../scripts/raw-media-fixture.mjs),
then execute the directory guest against clean, corrected and rejected sectors.
The [Wasm CUE mount API](../src/wasm.rs) attaches caller-owned bytes to a running
synthetic BIOS without resetting it or invoking HLE. Guest-configured ECC
repetitions, retry scheduling and error-output policies are implemented below.

[CD audio processing](../src/cd_audio.rs) applies IEC 60908's 50/15 µs response
when the audio track's CONTROL/PRE bit is set, before SCSP routing and scan
attenuation. The digital implementation uses a documented 44.1 kHz bilinear
transform and Q24 stereo history; it does not claim analog circuit equivalence.
History crosses sector boundaries, resets on explicit seeks/media changes and
survives pause/resume with unchanged position. A new original
[SH-2 routing/play guest](../tests/fixtures/build_cdda.py) drives CD and SCSP MMIO.
[Native guest tests](../tests/cdda.rs) and browser Wasm compare 1178 stereo frames
to independently evaluated SI-time-constant reference values (within 0.51 sample
unit), including impulses at frames 587/588. The browser delivers those actual
filtered samples to the production AudioWorklet and asserts rendered frames and
nonzero peak. Independent unit vectors also assert DC unity and Nyquist gain.

Both new fixtures are covered by [the original fixture MIT license](../tests/fixtures/LICENSE):

| Builder | Expanded 512 KiB ROM SHA-256 |
| --- | --- |
| `python3 tests/fixtures/build_cd_directory.py --check` | `e96346d309c35807053e92c87203fa7a781dfdfe0e29139f43590d7b15790413` |
| `python3 tests/fixtures/build_cdda.py --check` | `9253c539e776793666b6e18309ea8d5055687be5f179c5200416c4c16f8e1d9f` |

These regressions do not establish completion of the other required hardware
contracts or the umbrella acceptance criteria above.


## Guest-configured CD correction, retries and initialization

Related to #17; the full umbrella remains incomplete. Primary behavior follows
[CD][CD] §8.2.1, `CDC_CdInit` parts 1–4, §4.2/table 4.3 and §5.5.
Implementation: [`CdBlock::execute/drive_frame`](../src/cd.rs),
[`Media::decode_sector`](../src/media.rs) and
[`integrity::decode`](../src/media/integrity.rs).

| Required behavior | Implementation and independent evidence | Remaining deviation |
| --- | --- | --- |
| ECC settings 00–05, 80 and FF | Runtime executes one through six P/Q pairs, disables correction or retains the prior setting. A source-authored parity graph joins P0 to Q0/Q2 and P2 to Q2/Q50: one pass fails, two through six restore the exact sector. Unit vectors and `tests/cd.rs` check received bytes and configuration atomicity. | Individual decoder-clock latency and physical reread timing are not modeled. Strict boot/parser reads retain their separate bounded four-pass policy. |
| Finite retries 0–15, error output and infinite retry | Every failed attempt consumes a drive deadline. RETRY holds FAD and publishes no sector/CSCT; exhaustion yields ERROR or routes the erroneous sector according to bit6. Infinite retries remain interruptible by drive commands and bounded by machine execution. Register vectors assert each attempt, pool counts, FAD, bytes, cancellation and chunk invariance. | Immutable media reproduce the same defect on reread; stochastic optical recovery is not claimed. Retry deadlines use the selected 75/150 Hz virtual sector cadence. |
| XA Form2 retry and confirmation flags | Form2 error data are delivered immediately unless Form2 retry is enabled; the ordinary retry/output policy then applies. Subheader confirmation can be disabled for decode while selector matching still uses the first subheader. Independent EDC golden vectors isolate the controls. Failed XA correction restores the received header. | Raw non-XA Mode2 and a complete subcode/error-boundary corpus remain required. |
| Initialization retention/reset | Independent no-change fields; rejected settings are atomic. Software reset ignores other arguments, cancels unfinished transfer, resets host/selector/play information and invalidates file caches. Tray open invalidates file information; tests check observable DRDY/pool/FIFO/scope changes. | R–W stream decoding and remaining command-specific lifecycle/timing coverage remain required. |

[Original MIT SH-2 source builder](../tests/fixtures/build_cd_decode.py),
[generated guest ROM](../tests/fixtures/cd-decode.hex) and
[native guest test](../tests/cd_decode.rs) execute the same MMIO commands in
[native and browser Wasm](../scripts/browser-smoke.mjs). The guest observes
RETRY/ERROR, reads received and corrected bytes, escapes infinite retry by reading
a good sector, and writes all result mailboxes itself. Tests mutate only original
media bytes, include a two-pass parity graph, and compare repeated-reset traces.
The browser's raw encoder uses an independent parity-check matrix. There is no
host result write, disc detection hook or canned sector replacement.

Reproduce with `python3 tests/fixtures/build_cd_decode.py --check` (also in CI).
Expanded 512 KiB ROM SHA-256:
`416b0180eb6cde016b42be791a71357f01002530ee4dd80702623f3ccb4fc77c`.
The [fixture MIT license](../tests/fixtures/LICENSE) covers source, ROM and original
synthetic sector data. Existing demo discs remain byte-identical.

## Review evidence: instruction timing, DACK, envelopes and ISO startup

Related to #17. This implementation does not satisfy the full umbrella checklist.

**FAMG**: [73 source-authored MC68000 vectors](../tests/fixtures/m68k_families.py),
[reproducible builder](../tests/fixtures/build_m68k_families.py),
[native sound-bus execution](../tests/m68k_families.rs) and
[browser Wasm](../scripts/browser-smoke.mjs). The master uploads actual 68k code;
the sound CPU computes values/CCR, takes illegal/line-A/line-F/CHK/TRAPV exceptions
and RTE, and returns to ordinary PCM/SCSP setup. Cases cover all base EA forms,
including PC-relative forms, and supplement SNDR/SIOG's decimal, extend, MOVEM
and MOVEP evidence. Undefined CHK flags are deliberately excluded. Browser mutation
of ADD.B to SUB.B changes only that computed result/CCR.
Expanded 512 KiB ROM SHA-256:
`a417f5d283e544d5f4a2683a811f7c190a48c6260fea6ac61b4a309c666ac03d`.
The adjacent MIT license covers source and generated ROM/TSV; no SDK or firmware.

[`M68k::ea_clocks/execute/group0/group4/alu/shift/movem`](../src/m68k.rs)
uses [MC68000UM][68UM] tables8-1–8-14 for 16-bit-bus instruction costs.
[Native timing tests](../src/m68k/tests.rs) execute every MOVE source/destination
combination and authored branch/DBcc/Scc, register/memory, jump, MOVEM, shift,
exception and multiply vectors. Sound reset first-issue latency has an independent
[clock-boundary test](../src/sound/tests.rs). This improves sound-clock ordering;
the new word-boundary prefetch/fault evidence above does not replace the still-required SH-2/bus scheduler.

**DACK**: [DMA transfer engine](../src/onchip/dma.rs), explicit
[`Memory::dack_read/dack_write`](../src/bus.rs), native pin-adapter vectors, and
[original SH-2 source](../tests/fixtures/dack.s),
[builder](../tests/fixtures/build_dack.py),
[native guest/reset assertions](../tests/sh7604.rs) and browser Wasm.
H §9.2.4/§9.3.4 table9.7 defines single-address operation. Both channels
transfer byte/word/long data through DACK; AM selects direction and AL selects
polarity. The DACK side's address and address mode are ignored, the memory side
updates its address, and each consumed DREQ edge transfers one unit. Native
vectors assert data, addresses, count/TE/IRQ and illegal SCI/auto/burst/on-chip
modes. The standard board has no requesting DACK peripheral; its guest proves
that armed channels wait without touching ignored addresses, and reset clears
them. An unsupported board endpoint fails explicitly; it never becomes an inert
success or a substituted memory-to-memory copy. Expanded ROM SHA-256:
`81b1a7b1fc63a4958e38318d49f3a088f7315c061d1ce0518e929202cfdb4856`.
Source and generated ROM use the adjacent MIT license. Physical arbitration
and cycle-steal/burst bus ownership remain required.

**ENV**: [envelope state machine](../src/sound/envelope.rs),
[32-slot integration](../src/sound.rs),
[original 68k builder](../tests/fixtures/build_sound.py),
[reproducible ROM builder](../tests/fixtures/build_envelope.py),
[native stereo/guest mutations](../tests/synthesis.rs) and browser Wasm/WebAudio.
The 68k sets ordinary AR/D1R/D2R/DL/RR controls and keys off in its timer handler.
Independent mathematical stereo vectors assert decay, sustained level and release;
RR0 holds, AR0 stays silent, D2R changes sustain, and LPSLNK forces early decay.
Native tests cover all decay levels/rates, key scaling, held-attack key-off,
32 independent slots, release retrigger and SDIR bypass. The published SCSP
register manual supplies stage transitions but no numeric rate table: dyadic
Q16 slopes and the key-scaling/gain calibration are an explicit functional policy,
not independent evidence of chip envelope times. No full audio acceptance claim.
Expanded ROM SHA-256:
`f086078191c69e137bf70c2c8acc5ceca09724505ca4dbdbc32abfcc291806fc`.
Source/generated ROM use the adjacent MIT license.

[ISO bootstrap](../src/disc.rs) and [runtime CD directory commands](../src/cd.rs)
now scan descriptor sets (ST-040 §3.2.1), preserving the primary namespace across
boot/supplementary descriptors. They accept hidden regular files and reject
duplicate primaries, missing terminators and extents outside the declared volume.
Physical padding is not part of that volume. A zero first-read address disables
that transfer as specified by ST-040 §4.3; original AIP instructions still execute.
[Native startup vectors](../tests/disc.rs),
[original runtime guest](../tests/cd_drive.rs) and browser Wasm execute these cases.
General HLE system/backup/CD ABI, multi-area startup and other required media
contracts remain required; this change does not substitute for those services.

## Gradation and cache refill/RAM implementation

Related to #17. All mandatory umbrella scope remains required; these additions
alone do not complete any umbrella checkbox.

**GRAD**: [independent pixel/guest tests](../tests/video_gradation.rs),
[original SH-2 source](../tests/fixtures/gradation.s),
[builder](../tests/fixtures/build_gradation.py),
[generated ROM](../tests/fixtures/gradation.hex) and
[browser Wasm/production Canvas checks](../scripts/browser-smoke.mjs).
Behavior derives from [VDP2] §12.2/figure12.4 and
[the 1997 manual's blur description](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/hard/vdp2/hon/p12_13.htm)
and [CCCTL table](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/hard/vdp2/hon/p12_14.htm).

| Required group | Implementation | Native evidence | Wasm/guest evidence | Remaining deviations |
| --- | --- | --- | --- | --- |
| CCCTL BOKEN/BOKN | `Vdp2::gradation_screen/validate/render` selects sprite, RBG0, NBG0/RBG1, NBG1, NBG2 or NBG3; prohibited selectors and non-mode0 CRAM fail explicitly | All seven screen selections use actual VRAM/framebuffer/palette samples; disabled selector bits and prohibited settings | GRAD executes guest writes to enable NBG0 gradation; disabling BOKEN changes pixels | Remaining display modes and bank/cycle timing are still required |
| Horizontal 1:1:2 calculation and priority | Selected raw colors enter a two-dot pipeline even while occluded; filtered second image replaces second only when the selected screen is top or second | Golden interior RGB vectors, four normal layers, both rotation layers and sprite; third-image exclusion, concealed history, scanline reset and repeated inspection | GRAD has top/second priority regions, two scanlines, repeated boots and independently expected RGBA/Canvas values | Manuals do not define pre-display history: first two pixels use black pipeline seeds. Transparent boundaries are prohibited by the manual; absent samples use black. Frame snapshots and expanded eight-bit component arithmetic remain functional policies |
| Calculation enable/ratio/offset/window, LNCLEN/EXCCEN exclusion | Top controls remain authoritative; selected top supplies the forced second ratio; line-color insertion and extended calculation are disabled with BOKEN | Top enable, post-blend offset, color window and hidden/visible cases | Guest enables LNCLEN while BOKEN is active; changed EXCCEN cannot change filtered pixels, while disabled BOKEN restores normal line-color composition | Complete raster-time and hardware border/precision evidence remain required |

**CACHEG**: [original SH-2 source](../tests/fixtures/cache.s),
[builder](../tests/fixtures/build_cache.py),
[ROM](../tests/fixtures/cache.hex), [native execution](../tests/cache.rs)
and the same guest in [browser Wasm](../scripts/browser-smoke.mjs).
Primary behavior is [H] §8.4.1/figure8.4 and §8.5.4.

| Required group | Implementation | Native evidence | Wasm/guest evidence | Remaining deviations |
| --- | --- | --- | --- | --- |
| Two-way cache/RAM isolation | `Cache::first_cache_way/read/written` excludes RAM ways0/1 from cache tag reads and write-through updates | CACHEG creates matching valid tags in both RAM ways, reads external data and verifies RAM survives a hit write; existing array/LRU/width/coherence tests remain | CACHEG executes from P1, computes six result words, repeats boots and changes source operands; browser checks exact changed results | Full physical bus ownership, timing and reset-domain corpus remain required |
| Refill bus ordering | `Cache::read` and `CpuBus::read` perform four longword transactions starting after the requested longword, wrapping within the line and delivering the requested longword last | `refill_bus_beats_wrap_and_requested_longword_is_last_for_every_offset`; failure vectors verify bounded reads and no published partial line/tag/LRU state; `cache_refill_wrap_order_is_observable_by_the_other_cpus_sequential_ubc` verifies actual bus ordering via the other CPU's sequential break/IRQ | Existing native/Wasm cache guests execute through the new refill path | Transactions are ordered functional accesses, not cycle-delimited physical arbitration; full bus/MMIO failure and contention contracts remain required |

Both fixtures use [the original MIT fixture license](../tests/fixtures/LICENSE),
covering source and generated ROM. CI verifies both builders. No SDK object,
firmware, copied emulator implementation or host-generated guest result is used.

| Builder | Expanded 512 KiB ROM SHA-256 |
| --- | --- |
| `python3 tests/fixtures/build_gradation.py --check` | `358df4380f519123fe4096d53b4703ff8cca7d1add4b5eb2212cad975b967009` |
| `python3 tests/fixtures/build_cache.py --check` | `22534b689efcce1ace6cb36b4798c9e20abaf24e8c0585d6b2462739bd2d4791` |

## Original HLE system-service execution

Related to #17. **HLESYS** means the original MIT
[service routines](../src/hle/system.s), [ABI and memory layout](../src/hle/README.md),
[boot-table installation](../src/hle.rs), [guest-disc source](../tests/fixtures/build_hle_system.py),
[authored register records](../tests/fixtures/hle-system.tsv),
[native integration](../tests/hle_system.rs) and identical guest execution in
[the browser Wasm suite](../scripts/browser-smoke.mjs). The primary behavior is
[ST-162-R1 System Program §§1/2.2](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-162-R1-092994.pdf),
services 1.1–3.2, 5.1 and 7.1, with the
[later function specification](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/prgg/sysg/prog/hon/p02_11.htm)
and production [SCU Final Specifications][SCUFINAL] #7/10.

| Required system interface | Implementation / effect | Native and actual browser Wasm evidence | Remaining limits |
| --- | --- | --- | --- |
| SYS_SETSINT / SYS_GETSINT, §2.2 1.1/1.3 | Guest VBR-relative vector access; zero restores the dummy or SCU wrapper, excluding reserved 4E/4F | Master TRAPA executes a registered handler; alternate VBR and independently executing slave access their own tables; default restoration | Complete exception/interrupt and bus-cycle conformance remains required |
| SYS_SETUINT / SYS_GETUINT, §2.2 1.2/1.4 | Guest callback table, independent of architectural vector overrides | Registration/readback, restored SCU wrapper and actual DMA0/1 callbacks; R0–R7/MACH/MACL/PR/SP/SR restored after nested handlers | Complete SCU pin, priority-edge and arbitration coverage remains required |
| SYS_SETSCUIM / SYS_CHGSCUIM / SYS_GETSCUIM, §2.2 2.1–2.3 | Guest atomic mask/shadow publication and modification; AIACK arm when A-bus enabled; public shadow is data | Exact set/change values, AIACK, nested mask FBFF/FFFF and restoration to B3FF; data cache enabled/primed and disabled caller cases | ST-210 #7 prohibits the earlier manual's IST writes; follow production behavior without an ignored-write escape |
| SYS_TASSEM / SYS_CLRSEM, §2.2 3.1/3.2 | Actual SH-2 TAS and shared P1 byte storage | Endpoints 0/127/128/255; both CPUs contend for semaphore 9 and release/reacquire it; cold reset clears all 256 bytes | Full hardware bus-lock arbitration remains required |
| SYS_CHGUIPR, §2.2 5.1 | Guest copy of all 32 priority words, used by executing interrupt wrappers | Caller overwrites its table after copying; DMA0 callback observes SR=2, nested DMA1 SR=15; changing only the priority operand changes outer SR to 3 | Full cross-clock/priority-edge conformance remains required |
| SYS_PCLRMEM, §2.2 7.1 | Eight actual work-RAM bytes, cold-cleared and retained over NMI/SYSRES | Original guest derives the second word after NMI, requests timed SYSRES, then reads both words and reset callback/mask/semaphore state; repeats cold boots | Other reset/clock domains and remaining system services remain required |

Private master-only service state uses P0 reads/writes so the caller's resident
write-through lines remain consistent with public values. Semaphores and guest
inter-CPU mailbox polls use P1. The guest uses cache-through SMPC/SCU MMIO.
Independent data and priority operand mutations must change the computed traces;
no host writes running guest results or detects a game/PC/opcode. HLE startup now
executes 23 original reset instructions before AIP. Existing three-instruction AIP
and four-instruction zero-first-read tests retain their original guest assertions
and account for the additional reset instructions explicitly.

The disc includes its MIT LICENSE.TXT and uses only original code, ISO metadata
and authored numeric values. [Fixture licensing](../tests/fixtures/LICENSE) covers
the caller source/ISO/records; [HLE licensing](../src/hle/LICENSE) covers service
source/build output. Neither source contains Sega SDK routines or firmware.
Existing Star Collector, Prism Courier and Hardware Lab media are unchanged.

| Reproducible artifact | SHA-256 |
| --- | --- |
| `python3 src/hle/build.py --check`: `system.bin` | `b93d3a0e8ff1242c7ad5fa411f2009fb3a177755138f079263190fe51b124e1a` |
| `python3 tests/fixtures/build_hle_system.py --check`: `hle-system.iso` | `20087f76e82f3aaafb049a7c1fbad46c9ea032e6b9638b3118cb0b30e472b0fa` |

SYS_CHGSYSCK/SYS_GETSYSCK now have CLKG evidence below. SYS_EXECDMP,
backup-library/filesystem and CD-library ABIs remain mandatory and unimplemented. Full synthetic BIOS/service acceptance
and the other required CPU/peripheral, bus/reset/scheduler, SCU/DSP, video/audio,
CD/SMPC and demonstration evidence remain incomplete. HLESYS is a diagnostic
disc, not a replacement for either required demonstration. No umbrella checkbox
is marked complete by these service implementations.


## SMPC PLL changes, guest clock ABI and digital input settling

**CLKG** is the original [clock builder](../tests/fixtures/build_clock.py),
[synthetic BIOS](../tests/fixtures/clock.hex),
[HLE disc](../tests/fixtures/clock.iso),
[authored records](../tests/fixtures/clock.tsv) and
[native assertions](../tests/clock.rs), repeated through actual browser Wasm
and the production Canvas in [browser-smoke.mjs](../scripts/browser-smoke.mjs).
References: [SMPC][SMPC] §§1.1, 2.3.9–10, 2.4.2 and [VDP2][VDP2] §2.4;
[System Program §2.2 services 4.1/4.2](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-162-R1-092994.pdf).

| Contract | Implementation | Independent evidence | Remaining scope |
| --- | --- | --- | --- |
| CKCHG320/352 commands | `Smpc::write/advance_clocks` accepts 0F/0E, retains SF until 100 ms of reference time, clears sound enable and reports DOTSEL in OREG10 | SMPC unit vectors at deadline−1/deadline across chunks; CLKG issues both commands and INTBACK, compares 35/75 status | Oscillator settling uses the same deterministic 100 ms policy as SYSRES; no analog oscillator accuracy claimed |
| Reset domains at clock completion | `Bus::change_system_clock` resets VDP1, VDP2, SCU and sound, clears slave state/control, retains master state/cache/on-chip state, work RAM, CD and backup | Native bus vectors cancel/mask SCU IRQs, retain master FRC/cache/CD media and mask, clear slave/video/sound; both guests start an independent slave computation, verify it stops with retained results, return through retained code/stack and compare written sentinel values | Full CPU/access timing and arbitration remain required |
| Main/reference clock relationship | `SystemClock` carries fractional phase, converts deadlines, and advances FRT/SCU/VDP/raster at 26.8741/28.6364 MHz while sound/CD/SMPC keep reference time | Rational arithmetic/deadline inequalities over varied chunks; FRT counts 33592/35795 in 10 ms, both produce 441 stereo sample frames; first fast HBlank at reference tick1322 | CPU issue still uses fixed turns; this does not resolve the hardware-time CPU/DMA ownership finding |
| Normal graphic B display | `Vdp2::dimensions/render` accepts TVMD=8001 and emits 352×224 pixels; Wasm exposes dimensions and Canvas resizes/reset clears correctly | CLKG guest writes a green back screen; native/Wasm compare all pixel bytes, production Canvas checks the last visible pixel | Other required resolutions/interlace, raster latching and cycle-pattern arbitration remain pending |
| SYS_CHGSYSCK/GETSYSCK | Original SH-2 service at ROM4D00, installed through RAM320, issues SMPC MMIO and publishes data at RAM324; masks/restores SR and synchronizes reset IMS shadow | Same independent HLE disc and separate synthetic BIOS have matching reset-domain records; changing only MOV #0,R4 to MOV #1,R4 changes public value and DOTSEL | SYS_EXECDMP, backup/CD libraries and full startup/service ABI acceptance remain required |

**DIGG** is the original [digital timing builder](../tests/fixtures/build_digital_timing.py),
[synthetic BIOS](../tests/fixtures/digital-timing.hex),
[authored records](../tests/fixtures/digital-timing.tsv) and
[native guest](../tests/digital_timing.rs), also executed in browser Wasm.
[SMPC §3.4/table3.36][SMPC] requires reading after 2 µs following TH/TR
selection. `DigitalMux` in [controller.rs](../src/smpc/controller.rs) implements
54 reference clocks of settling independently on both ports. Early reads retain
the preceding bank as an explicit deterministic policy for otherwise premature
software accesses; they do not establish physical pre-settle pin values.

SMPC unit vectors check clocks53/54, staggered ports, repeated same-selection
writes, reset cancellation and all digital banks. Bus vectors wait for the
specified interval. The SH-2 guest samples before/after its own delay using actual
MMIO, with independent port buttons; native and Wasm tests change those buttons
and compare changed reports. Existing Gamepad/keyboard/focus and both games'
settling loops remain exercised by the extended suite. Analog TH/TR/TL deadlines
now participate in event splitting alongside direct digital deadlines.

Both new diagnostic builders/outputs are covered by the
[fixture MIT license](../tests/fixtures/LICENSE); the clock disc includes its
LICENSE.TXT. Neither is proprietary firmware or a substitute for the two required
demonstrations. CI checks both builders in addition to existing media.

| Artifact | SHA-256 |
| --- | --- |
| `clock.iso` | `462e605c6d5adb46876f69d2b575ee167c20f01ea09f558fb265262f3417b907` |
| `clock.hex` (text) | `8d3f59725b520f55e313c4082f377ee6524654ae48f8780b09f206c3b9204ff1` |
| `digital-timing.hex` (text) | `8d6476f8aa89357aff6abcf99c0e56fcd486512644bc902cc138d531f2e6d476` |

The remaining review findings and every unmet umbrella acceptance criterion
remain mandatory in the existing Issue #17. These changes do not establish full
SH7604/68k, bus/scheduler/backup, SCU/DSP, video/audio, CD/media or BIOS/HLE
conformance, and no completion checkbox is changed.

## Ordinary BSC software waits, narrow transfers and CS2 endian conversion

Related to #17. **BWAIT** is the original MIT
[guest builder](../tests/fixtures/build_bsc_waits.py),
[synthetic BIOS](../tests/fixtures/bsc-waits.hex),
[authored timer vectors](../tests/fixtures/bsc-waits.tsv),
[authored endian vectors](../tests/fixtures/bsc-endian.tsv),
[native integration](../tests/bsc.rs) and identical actual browser Wasm execution
in [browser-smoke.mjs](../scripts/browser-smoke.mjs). Primary references are
[SH7604][H] §§7.1.4, 7.2.1–3, 7.3.1–2 (figs.7.5/7.6) and 7.4.2 (fig.7.11).
The [fixture MIT license](../tests/fixtures/LICENSE) covers source and generated
ROM/vectors. No Sega firmware, SDK routine, external core or host-written running
guest result is involved.

| Software-visible contract | Implementation and objective evidence | Remaining limits |
| --- | --- | --- |
| WCR software waits and BCR1 AnLW | `Bsc::software_waits` decodes zero/one/two Tw or long waits of three through six, independently for CS0–3; CS2/3 share AHLW. Register vectors cover every area/selection, keys and atomic rejection. BWAIT measures eight uncached CS3 reads with each CPU's own FRT, comparing seven literal expected deltas | Tw clocks are added to the existing issue quantum; base T1/T2 overlap, inter-cycle idle, external WAIT and full bus arbitration remain required |
| BCR2 byte/word/long interfaces | `CpuBus::external_read/external_write` split larger operands and cache-refill requests into ordered physical beats. Bus vectors check exact per-beat wait sums, data, write-through/cache hits and actual sequential external UBC observations | CS0 retains the existing 32-bit board adapter; complete standard-board lane/bridge behavior, burst/DRAM/SDRAM protocols and arbitration remain required |
| BCR1 CS2 endian | `Bsc::little_endian` selects per-CPU CS2 conversion. Actual transfers reverse operand bytes, leave byte addresses/strobes unchanged and transfer low words first on word-wide buses. Literal native vectors assert physical bytes, both word offsets, all byte lanes, writes and the other CPU's independent view | Cache-through use follows §7.3.2; this does not establish all cache/MMIO/board-width combinations |
| Register programming rules | Wrong keys do not change registers; reserved bus widths and unsupported little-endian byte buses are rejected atomically. Changed memory/burst/partial-share/idle configurations still raise explicit errors | These remaining valid configurations must be implemented; error reporting does not satisfy their acceptance |
| Cache/TAS and reset | Waits apply to actual refill beats, cache-through accesses, TAS external reads and write-through hits. Cache hits/arrays incur none; cold reset restores six software waits | CPU/DMA contention, TAS bus locks, refresh ownership and complete reset-cycle timing remain required |

The guest initializes width/endian once after reset **before** accessing outside
CS0, preserving those initial choices while changing the permitted long-wait
fields. Each independently executing CPU identifies itself through BCR1.MASTER,
uses its own stack/FRT and writes its own 24-word mailbox. Separate cold boots
change original width/endian operands: long/word/byte interfaces yield different
FRT deltas; endian changes alter computed reads and guest-reconstructed physical
bytes. Additional single-operand wait/data mutations alter only the corresponding
records. Reset repeats the same traces. All versions execute SH-2 instructions
and actual memory/MMIO accesses in native and browser Wasm.

FRT compare/SLEEP tests now use elapsed clocks including ROM fetch waits, retaining
the before-edge, edge and next-boundary IRQ/stack assertions. Star Collector and
Prism Courier tests wait within a bound for guest initialization; their original
media bytes, input, pixel, completion/loss and restart assertions are preserved.
The whole extended browser suite still runs under `/saturn-rust/`, including
Canvas, WebAudio, privacy, input/focus and all three demonstrations.

| Reproducible artifact | SHA-256 |
| --- | --- |
| `bsc-waits.hex` text | `a5b1d66b880c6581f548541d83107b76f96f4ba95e7eedf54ea842cdcba37efc` |
| Expanded 512 KiB original BIOS | `f4f7e6379a45b45a86604b613257a97afbe831a65088dffc18b6972c711a78ed` |
| `bsc-waits.tsv` | `5ecd8afbc26eb0962062539e8e575d36014368addf7e9baf1ec628214bbbfab3` |
| `bsc-endian.tsv` | `95de3de1f2d74e5da6a0cea05fea79e455a169c1d1f5c5b4ee1f2ff803b9bf24` |

All remaining umbrella hardware implementation and validation criteria stay
mandatory. In particular, DIVU still has its documented overflow-intermediate
substitution; 68k bus sequencing, full coordinated CPU/DMA ownership, backup
filesystem/HLE/CD services, SCU/DSP, video/audio/media/SMPC and comprehensive
integration evidence remain incomplete. No umbrella checkbox is marked complete.
