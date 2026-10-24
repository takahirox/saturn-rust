# Hardware conformance for Issue #17

**Status: incomplete; Issue #17 is the active continuation tracker.** Issues
#11/#13/#15 remain administratively closed; do not reopen them. This checkpoint
includes independent slave SH-2 execution, CPU-local FRT/WDT/SCI/DIVU/DMAC,
internal interrupt arbitration, executing UBC comparisons, software standby/NMI wake,
write-through caches, timed SMPC CPU control and SYSRES,
SCU interrupt/DMA/DSP execution, an independent MC68EC000 core, SCSP PCM/FM/LFO/noise,
WebAudio delivery, SMPC RTC/SMEM/INTBACK and NTSC/PAL raster events and selectable standard-TV graphics. It preserves the address-error fixtures, Star Collector,
backup RAM and both direct digital pads. It does not complete the Saturn hardware
or the complete required hardware exerciser demonstration. Hardware Lab now
executes runtime CD/DMA/IRQ, dual CPU, SCU DSP, PCM/CD-DA, advanced video and backup
and an executing SCSP effects-DSP scene. Full chip conformance and the complete
acceptance inventory remain required. The original playable Prism
Courier disc now runs with VDP1/VDP2 composition, 68k audio and guest IRQ/input
in native and browser Wasm checks. No umbrella acceptance checkbox is met.

Inspected upstream main: `dbd981b030a46daa6923072ce240680dd2e58ebf` (2026-10-21),
verified again on 2026-10-22 with `git ls-remote origin refs/heads/main`. This revision starts from
`7285a713490ab20c2cb7b82af126a72d05c35a44`, preserving the earlier
SCU/SH7604 work and review evidence. The full outstanding hardware scope remains mandatory. See [current validation](validation-issue-17.md),
[historical address-error validation](validation-issue-15.md),
[backup/pad validation](validation-issue-13.md) and the
[historical SH-2 checkpoint](validation-issue-11.md). **Pending** means required
coverage or integrated validation remains missing, even when a subset passes.

## Ordinary SH-2 MOV load pipeline (Issue #55)

Related to #17. [The independent stage/clock table](sh2-load-pipeline.md)
connects P §§7.2–7.5 and fig7.11 to
[operand scheduling and shared-slot recovery](../src/cpu/timing.rs),
[CPU execution](../src/cpu.rs) and the existing
[transaction journal/arbiter](../src/bus/transactions.rs).
All ordinary memory-load forms preserve byte/word sign extension, aliases,
postincrement and delayed-branch PC-relative addressing.

[Literal CPU vectors](../src/cpu/load_tests.rs),
[both-CPU scheduling vectors](../src/load_scheduler_tests.rs) and
[executing bus vectors](../src/bus/load_tests.rs) cover alignment, combined
IF/dependency contention, resident cache/cache RAM/peripheral/external sampling,
wait states, refill retention, FIFO replay, CPU reset cancellation, existing
peer/DMAC/SCU grants, advancement chunks 1/3/17/10000 and passive inspection.
The original MIT [guest source](../tests/fixtures/sh2-load.s),
[builder](../tests/fixtures/build_sh2_load.py),
[hashes](../tests/fixtures/sh2-load.sha256) and
[authored records](../tests/fixtures/sh2-load.tsv) execute in
[native Machine](../tests/sh2_load.rs) and
[production browser Wasm](../scripts/sh2-load.mjs), including alignment,
operand and dependency controls and repeated cold boots. CI checks reproducibility
and runs the extended browser suite. See [actual validation](validation-issue-55.md).

Precise pipeline claims apply to warmed instruction-cache execution. Demand-driven
uncached/refill fetches continue to execute; speculative IF/ID completion and
ordinary stores remain incomplete. Remaining CPU/peripheral, bus/reset/arbitration,
SCU/DSP, video/audio, CD/media/HLE, SMPC and comprehensive integration acceptance
remain mandatory. Genuine BIOS and commercial-game compatibility are unverified.
This is partial #17 progress and does not close #17.

## SH-2 B-bus memory read bridge (Issue #37)

Related to #17. Scheduled reads on both SH-2s now distinguish the architectural
operand, aligned CPU-facing longword and two separately sampled B-bus halfwords.
The [primary-derived transaction table](sh2-b-bus-reads.md) defines endpoint
geometry, lane extraction, completion, SCSP byte restrictions and model timing
conventions. Production changes are in [the CPU adapter](../src/bus.rs) and
[transaction journal](../src/bus/transactions.rs).

Independent [bridge vectors](../src/bus/bridge_tests.rs),
[sound arbitration vectors](../src/bus/sound_tests.rs) and
[VDP1 ownership vectors](../src/bus/vdp1_tests.rs) cover both processors,
all operand lanes, mirrors/P1, cache refills/hits, retained samples, separately
arbitrated phases, competing CPU/SCU progress, power/manual reset and debugger
inspection with advancement chunks 1/3/17/10000. The external UBC observes an
assembled longword, rather than the downstream word addresses.

The original [guest source](../tests/fixtures/b-bus-reads.s),
[builder](../tests/fixtures/build_b_bus_reads.py),
[ROM bytes](../tests/fixtures/b-bus-reads.hex) and
[authored records](../tests/fixtures/b-bus-reads.tsv) are covered by the
[fixture MIT license](../tests/fixtures/LICENSE). Native
[Machine execution](../tests/b_bus_reads.rs) and production
[browser Wasm execution](../scripts/b-bus-reads.mjs) run identical guest bytes,
with guest-written sound/video data, both SH-2s, guest FRT observations, operand
mutations, replaced-read controls and repeated boots. Timing columns in the
golden TSV are zero placeholders for independently checked bounds, not injected
guest results. Expanded ROM SHA-256:
`d7838e209c8b493aad57b4e572f1f3aba8263dd786bc5c9704923d4363ae2473`.

See [validation #37](validation-issue-37.md) for actual failing inherited-path
negative controls and executed checks. MMIO/A-bus conversion and CPU writes are
outside this increment; their device-specific contracts remain required. Exact
SCU pin timing, comprehensive board arbitration/reset, the remaining CPU and
peripheral semantics, SCU/DSP, video/audio, CD/media/HLE, SMPC and complete
integration acceptance remain mandatory under #17. Commercial-game compatibility
remains unverified. This checkpoint does not complete or close #17.

## References and evidence keys

- **P**: [Hitachi SH-1/SH-2 Programming Manual][P], §5 encodings, §6 instruction descriptions. Section numbers below refer to this manual; §6.30 is SH-1-only and excluded.
- **H**: [SH7604 Hardware Manual][H], especially §§2–4 and §14.3.
- **68**: [Motorola M68000 Family Programmer's Reference Manual][68], MC68000-applicable entries in the integer instruction descriptions. Later CPU extensions are excluded.
- **O**: [Saturn Overview][O], hardware configuration and memory map.
- Device references link original manuals, not reference-emulator implementations. A reference on a missing row identifies required work, not an implemented or verified behavior.

**MEMG**, **M2G** and **RASTG** below identify the additional original
[memory](../tests/memory_contracts.rs), [plain Mode2](../tests/mode2.rs) and
[raster-history](../tests/raster_history.rs) guests, all repeated in browser Wasm
and covered by the [fixture MIT license](../tests/fixtures/LICENSE).

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
table7.2 base execution states are scheduled independently for both CPUs; pipeline
hazards and transaction-level bus contention remain required. CPU-local cache
semantics are tested separately.
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
| SLEEP | 6.59 | Retire once, bounded waiting, accepted CPU IRQ wakes; software standby/NMI | sleep, PWRG | ROM, PWRG | Complete interrupt/clock boundary evidence remains | Pending |
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
| CPU address errors, H §§4.3/4.6/4.7/4.8.3, Tables 4.6/4.10/4.11 | Vector 9, operand/fetch checks, next-PC/SR, auto-update and deferral; misaligned stacking completes original exception then accepts address error without recursion | AE, AEROM, PERIPH | AEROM, PERIPH | Invalid data, unaligned stacking and vector-read bytes follow deterministic exact-address policies rather than physical bus signals; VBR alignment dispatch/repair is covered by VBRG below | Pending |
| Maskable IRQ/NMI, H §4.4 | Device priority selection, I mask, NMI vector11, stack/wake; ICR auto/external vector modes, NMIL/NMIE and selected-edge NMIF/standby wake | sleep, special, PERIPH, INTCG | INTCG, DUAL | Pin synchronization and complete cross-clock assertion boundaries remain | Pending |
| Deferral, H §4.6 | IRQ/NMI defer through delayed successors and control/system successors; address errors defer only through delayed successors | special, AE | ROM, AEROM | No pipeline-cycle timing or device IRQ selection; unfetchable successor policy below | Pending |
| Cache/address regions, H §8, tables 8.2–8.4, figs.8.10/8.11 | CPU-local 4 KiB/four-way cache, write-through, no snooping; ID/OD, pseudo-LRU, two-way RAM, P1 bypass, P2 purge, P3 address array, P6 data array; TAS external read; ordered four-longword refills and two-way RAM tag isolation | CACHE, PERIPH, CACHEG; `Cache` and `CpuBus` | PERIPH, CACHEG | Pipeline/fill-cycle/bus arbitration timing remains required; work-RAM mirrors are decoded | Pending |
| On-chip INTC, H §5.3, table 5.4 | IPRA/IPRB, VCRA–D/VCRWDT, DIVU/DMA vectors, source priorities/tie order and guest acknowledgement | FRT, DUAL, PERIPH | DUAL, PERIPH | UBC source 12 at level 15 covered by UBCG; ICR/IRL auto/external vectors and SCU acknowledgement covered by INTCG; pin synchronization remains; refresh CMI arbitrates at WDT priority | Pending |
| FRT, H §§11.1–11.5, tables 11.2/11.3 | FRC, OCRA/B, capture, divisors, overflow/compare/clear, TIER/FTCSR, shared TEMP, output level state and IRQ; Saturn capture wires (ST-202 §1.2) | FRT, DUAL | FRT (compare), DUAL (capture) | Internal/external pin synchronization, WAIT, clock-switch edges and contention timing absent; no external clock source or output-pin routing | Pending |
| Watchdog, H §§12.2–12.3, table 12.2 | Keyed byte-register/word-write protocol, eight divisors, interval IRQ, read-before-clear, watchdog reset selection/cause and 512-clock CPU hold | PERIPH; `Watchdog` | PERIPH | External WDTOVF pin routing and complete CPU reset-cycle ordering remain; reset assertion/512-clock hold now split advancement without discarding preceding FRT/DIVU clocks | Pending |
| SCI, H §§13.2–13.3 | SMR/BRR/SCR/TDR/SSR/RDR, timed internal-clock transmission, external completed-character input, overrun/framing/parity flags, IRQs and DMA handshakes | PERIPH; `Serial` | PERIPH (transmit completion) | No external clock-edge adapter or physical serial pin waveform; multiprocessor and format combinations lack exhaustive integration coverage | Pending |
| On-chip DMAC, H §§9.2–9.3 | Both channels; byte/word/long/16-byte transfers; fixed/increment/decrement addresses; auto/DREQ/SCI requests; fixed/round-robin selection, TE/AE/NMIF, vectors/IRQ; single-address DACK direction/polarity through explicit board pin adapters | PERIPH, DACK; `Dma`, `Bus::prepare_dmac`, [DMABG bus vectors](../src/bus/dmac_tests.rs), pin-adapter vectors | PERIPH, DACK and [DMABG native](../tests/dmac_bus.rs)/[Wasm](../scripts/dmac-bus.mjs): both CPUs, all widths, short overlapping bursts, IRQs and source mutations | DMABG adds deferred phases, CPU/SCU exclusion, cycle-steal/burst ownership and actual BSC waits; RFSHG below adds refresh ownership; complete display/sound arbitration, DREQ synchronizer and peripheral-bus timing remain required; no requesting DACK peripheral is connected on the standard board | Pending |
| DIVU, H §§10.2–10.4/table 10.2 | Signed 32/32 and 64/32 quotient/remainder; six-clock overflow exposes three division steps, saturating only the quotient when OVFIE=0; 39-clock normal completion/interlock; word DVCR/VCRDIV, sticky overflow and IRQ | PERIPH, [DIVG](../tests/sh7604.rs), [independent boundary vectors](../src/onchip/peripheral_tests.rs); `Division::start/overflow_result` | DIVG on both CPUs, normal/overflow signs/zero, aliases, IRQ acknowledgement, repeat boots and changed dividend | The one-clock read-after-write interlock is covered by MEMG below; full CPU bus scheduling remains | Pending |
| BSC identification/memory/interval timer, H §§7.1.4/7.2/7.5–7.7/7.11.1 | BCR1.MASTER per CPU, read-only word lanes, keyed long writes, RTCNT/RTCOR/RTCSR, all clock divisors, CMF read-before-clear, CMI vector/priority and manual-reset retention; ordinary software waits, intercycle turnaround, narrow transfers, CS2 endian conversion, SDRAM/DRAM/pseudo-SRAM widths and timing, open rows and refresh recovery (BWAIT/IDLEG/BCMEM below) | [BSCG/BWAIT](../tests/bsc.rs), [register/reset vectors](../src/onchip/peripheral_tests.rs), [BSC vectors](../src/onchip/bsc.rs), [bus beats](../src/bus.rs) | BSCG, BWAIT, IDLEG, BCMEM | BUSG below adds all six SDRAM MRS encodings, precharge and command timing; RFSHG below adds retained CPU groups, refresh/DMAC ownership and self-refresh bus inhibition; external WAIT input and full board widths/lanes remain | Pending |
| Module stop/standby, H §§14.2/14.4, table 14.3 | SBYCR, SCI/FRT reset and gating, preserved INTC/UBC/BSC, retained DMA operands with control reset, WDT-selected NMI settling; `OnChip::enter_sleep/advance_clocks`, `Bus::advance_dma` | [standby/retention vectors](../src/onchip/peripheral_tests.rs), BSCG, PWRG | BSCG, PWRG | Electrical HIZ/pin states, initial five-cycle NMI guard, physical oscillator and clock-switch edges remain unverified; executing stopped MULT instructions is software-prohibited/undefined (H §2.5.2) | Pending |
| UBC, H §6 | A/B registers, masked address/data/size/master matching, sequential conditions, match flags, before/after fetch breaks and deferred level-15 vector 12; CPU/cache/DMAC and external SCU observation; `Ubc::observe`, `Cpu::step/execute` | UBCG and bus vectors | UBCG | Three-clock propagation, independent successive-write deadlines and last-register read barrier are implemented in `Ubc::advance/read`; DIVG handles data breaks on both CPUs, including slave acknowledgement after exception acceptance. SH7000-compatible unused words of real IF longwords are covered by IFUBC below; complete speculative IF/MA pipeline scheduling remains pending | Pending |
| Slave SH-2, [Dual CPU User's Guide][DUALREF] §§1/3/4, [SMPC] table 1.1 | Independent interpreter/registers/stacks/peripherals/cache; shared bus, SSHON/SSHOFF, bidirectional capture IRQs and live DCC HBlank/VBlank levels | DUAL, CACHE, PERIPH, [slave raster vectors](../src/bus/slave_raster_tests.rs) | DUAL, PERIPH, [RASTER](#slave-dcc-raster-interrupts-raster) | BUSG adds deferred round-robin CPU transactions and TAS exclusion; complete IF/MA overlap, on-chip DMAC and display-fetch contention remain; BCR1 identity covered by BSCG | Pending |

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
Indexed EA and source-predecrement internal waits now precede operand sampling;
[independent bus and sound-clock vectors](#retained-vdp1-tables-and-characters-68k-ea-waits-and-cd-transfer-completion)
cover the two-clock intervals and reset cancellation. Instruction-specific prefetch placement and full bus arbitration still need
validation; these word-boundary semantics do not establish the complete
CPU/scheduler acceptance criteria.

The [instruction completion vectors and original DIV trace guests](#mc68ec000-instruction-completion-and-tracereset-boundaries)
add timed retirement, post-computation trace entry and reset retention of a
completed result while trace stacking is in flight. Physical DIV timing,
instruction-specific microsequences and SCSP register-bus/physical interface phases remain pending; CPU DRAM allocation is covered below.

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
| RESET | RESET instruction description; [68UM] §5.5 | `M68k::group4`, sound transaction journal | m68k/tests.rs; [reset/trace guest, native + Wasm](#scheduled-68k-resetiack-and-vdp1-local-memory-ownership) | Scheduled 124-clock output and trace deferral; CPU DRAM allocation below; physical interface phases and general instruction timing pending |
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
| Base-system address decoding, O map | Only documented subset in architecture.md | bus.rs | SC | Absent CPU/A-bus selects return pulled-up data and ignore writes (MEMG); full device map/lanes and transaction timing remain | Pending |
| MMIO widths/side effects, device manuals | Unsupported accesses return errors | devices.rs | SC | Full device routing/width rules missing | Pending |
| Backup byte storage, SCU §1.2 figs.1.3/1.5; SMPC §2.3 SYSRES p.29 | 32 KiB, odd-byte storage; even lanes read FF and ignore writes; word/long strobes; P1 aliases; unformatted zero initialization | BK; `backup_byte_lanes_endpoints_and_cache_through_access_do_not_alias_work_ram` | BK | BUPG adds guest-executed filesystem/service integration; MEMG adds all eight 64 KiB aliases and invalid-import/reset checks; remaining reset domains pending | Pending |
| Backup reset retention, SMPC §2.3 SYSRES p.29 | Bytes survive successful/rejected HLE, disc and ROM resets and guest SYSRES; new Machine starts empty | BK; `packed_import_is_atomic_and_survives_every_boot_mode_and_rejected_boot`, PWRG | BK, PWRG, reset/mode/read-race checks | SYSRES guest retention now covered by PWRG; remaining power/reset and service contracts pending | Pending |
| Local backup import/export (host interface) | Exact 32768 packed bytes; atomic replacement; explicit local download; stale import cancellation; no network/storage | `packed_import_is_atomic_and_survives_every_boot_mode_and_rejected_boot` | Download byte comparison, offline import, invalid size/read errors, races, fresh-tab isolation | BUPG provides independent HLE backup calls; optional cartridge backup is absent; no automatic cross-session persistence | Pending |
| Shared event scheduler, O clock domains | Deterministic CPU order, raster/SCU event deadlines and clock-driven DSP/DMA; field deadlines replace instruction budgets | TIME, SCUROM, game.rs, DUAL | SCUROM, SC, DUAL | BUSG adds CPU/SCU arbitration, deferred data phases and reset cancellation; DMABG/CPUIO/RFSHG add local DMAC/internal buses and refresh ownership; full CPU pipeline, display/sound ownership and cross-clock ordering remain; CD command/sector/sample deadlines are exercised by CLAB | Pending |
| Video timing, VDP2 display registers | Selectable NTSC/PAL 224/240/256-visible raster, interlace field/counter flags, HBLANK/VBLANK and SCU edges; bounded field advancement | TIME, SCUROM, game.rs | SCUROM, SC | CLKG and TVG below cover PLL/display intervals; half-line beam phase, counter discontinuities and bus arbitration remain required | Pending |
| Audio timing, SCSP/CD | Independent 68k/sample deadlines, bounded stereo output and worklet; underflow/mute/blocked instrumentation | SNDV, SNDR | SNDR | Exact CPU/bus timing and effects DSP | Pending |
| Power/reset domains, SMPC / H | Cold reset clears both CPUs/devices and retains backup; SSHOFF resets slave CPU/FRT while retaining shared state; SLEEP distinct | boot.rs, sh2_system.rs, DUAL | ROM/reset, DUAL | PWRG and CLKG implement manual/system/clock reset domains; complete reset-cycle and bus-boundary evidence remains | Pending |

## SCU and DSP

SCU subsets now execute actual operations. The entire SCU acceptance criterion
remains pending: bus arbitration/waits, all device connections and exhaustive DSP
edge/transfer coverage are still required. Unsupported registers fail explicitly.

| Feature / manual | Implemented behavior | Native / browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- |
| SCU interrupt registers, [SCU] table 2.1/§3.5, [SCUFINAL] #7/10 | IMS/IST, fixed priorities/vectors, master delivery/acknowledgment, A-bus acknowledgment gate | SCUV, SCUROM / SCUROM | VDP1 source 13 and CD A-bus source 16 now execute guest handlers (CLAB); complete external pin/arbitration behavior unverified | Pending |
| A/B-bus coordination, [SCU] §3.6, [SCUFINAL] #8–12 | Prohibited DMA source/destination checks and bounded error handling | SCUV / SCUROM (valid transfers) | ABUS and BUSG add ASR/refresh waits and deferred CPU/SCU ownership; complete device/display/on-chip-DMAC contention remains | Pending |
| DMA channels 0–2 direct, [SCU] §§2.1/3.2, [SCUFINAL] #15–24 | Counts, address save/update, individual byte heads/tails, separately sampled B-bus halfwords and strides, all seven edge triggers/GO, end/illegal IRQ and priority preemption; one queued retrigger | SCUV, SCUROM, BUSG, [FRINGE](#scu-byte-fringes-and-separate-b-bus-halfwords-fringe) / SCUROM, BUSG, FRINGE (all three channels) | Exhaustive trigger/device/boundary tests, remaining shared-memory ownership/wait arbitration and prohibited three-channel behavior pending | Pending |
| DMA indirect, [SCUFINAL] #24/25 | Final-spec count/write/read layout, source bit-31 end marker, separately granted descriptor words, table walking, write-address continuation | SCUV, IDESC, SCUROM / SCUROM | IDESC and BUSG cover descriptor bounds/alignment, all channel count widths, partial table faults/preemption/reset and deferred BSC/VRAM waits; complete device contention and transfer-error/count boundaries remain | Pending |
| DMA device interactions, [SCU] §2.1 | High-RAM to VDP2 VRAM and executing DSP external RAM transfers, real IRQ handlers | SCUV, SCUROM / SCUROM | CLAB adds CD FIFO to high-RAM fixed-source DMA/IRQ; VDP1 routes and full cross-device arbitration remain | Pending |
| SCU DSP execution/registers, [SCU] §4.5 pp.91–131/141–158 | ALU logical/arithmetic/shift/rotate, signed multiply, parallel X/Y/D1 moves, immediates, delayed jumps/subroutine control, BTM/LPS, END/ENDI, pause/step, read-cleared E/V and arithmetic-replaced V ([DSP arithmetic evidence](#scu-dsp-arithmetic-status-lifetime)) | SCUV, SCUROM / SCUROM (product/sum/loop/ENDI); DSPD now executes an MVI-PC entry into DMA-loaded code and its delay-slot write | One-successor PRAM-DMA prefetch has DSPBUS native vectors below; prohibited DMA-bank instruction accesses have explicit native/Wasm diagnostics. The ADD/SUB/AD2 status contract now has independent native/guest/Wasm evidence below; full conditional/DMA timing and the remaining pipeline edge inventory remain | Pending |
| SCU DSP memory/transfers, [SCU] §§3.3/4.1/4.5 pp.132–140, [SCUFINAL] #16/18/19/26/27, [SOA8] §5.6 | Four 64-word banks, host ports, address counters, bounded asynchronous input/output DMA and HOLD, fixed/incrementing A-bus writes, separately granted B-bus halfwords, retained transfer data, END permits DMA continuation; PRG DMA begins at zero independently of host ports | SCUV, SCUROM, DSPD, DSPBUS / SCUROM + DSPD + DSPBUS; DSPD adds nonzero bootstraps, 256-word PRAM DMA, executed FF→00 wrap and operand/count mutations | Complete cross-device arbitration/waits, physical RAM-port timing and the rest of the pipeline inventory remain | Pending |
| SCU timers, [SCU] §§2.2/3.4, [SCUFINAL] #30/31 | Raster-driven timer0 clear/compare and timer1 dot countdown, zero=512, no reload while active, compare-line selection | SCUV, TIME, SCUROM / SCUROM | Full television modes and beam-phase conformance pending | Pending |

Implementation: [SCU ports/interrupts/timers](../src/scu.rs),
[DMA beats](../src/scu/dma.rs), [DSP interpreter](../src/scu/dsp.rs),
[clock-domain dispatch](../src/bus.rs) and [raster deadlines](../src/timing.rs).
ST-210 supersedes the earlier indirect-table diagram and removes DSTA/DSTOP;
these ports and prohibited IST writes raise diagnostics rather than behaving as
inert registers. SCU pending edges clear on accepted CPU interrupt acknowledgment.
A-bus requests require AIACK to be armed again after its acknowledge cycle.

The DSP executes every two main clocks. BUSG/DMABG now grant external SCU and
on-chip DMA data phases through the same transaction arbiter and charge
BSC/A-bus/VRAM waits before publication, independently of CPU retirement.
SCU timer, DSP, bus-completion and raster deadlines split peripheral advancement. Native
vectors prove identical results for 1/3/8/17/100-clock chunks. Raster vectors prove
identical event traces for 1/3/8/127/1708/10000-clock chunks. Complete
refresh/display/sound arbitration and pipeline overlap remain required. Three simultaneous DMA channels, bank conflicts and
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
| VDP1 VRAM/framebuffers/registers, [VDP1] §§2,4 | 512 KiB VRAM, two draw/display buffers, byte/word/long memory; checked paired register writes; MODR | VIDEO | PRISM | All bus lanes/mirrors/waits; exclusive-monitor readout | Pending |
| Command list/control flow, [VDP1] §6.1 | END, next/assign/call/return and skip variants, one return level, invalid links/prohibited commands, bounded cyclic lists | VIDEO, [local-memory vectors](#scheduled-68k-resetiack-and-vdp1-local-memory-ownership) | PRISM; command-fetch guest | Full guest control-flow corpus and command-dependent fetch costs | Pending |
| Normal/scaled/distorted sprites, [VDP1] §§6.2,7.4–7.6 | Width/height, flips, two-point and nine anchor scales, ruled quadrilateral scan conversion, HSS/EOS source selection | VIDEO, HSSG | PRISM (normal), HSSG | Physical edge/zoom rounding, preclip direction and draw bandwidth | Pending |
| Polygons/lines/polylines, [VDP1] §§7.7–7.9 | Solid ruled quads and integer line segments | VIDEO | PRISM (polygons) | Full independent sloped-edge and repeated-pixel corpus | Pending |
| Clip/local coordinates, [VDP1] §§7.1–7.3 | System/user inside/outside clips, local offsets | VIDEO | PRISM (system) | Full extreme-coordinate/clip boundary corpus | Pending |
| Sprite texture/color modes, [VDP1] §§3.1/5.1/5.2/6.3–6.6 | Packed 4-bit bank/LUT, 64/128/256 bank and RGB555, retained 16-word LUT before drawing, separately fetched character words and framebuffer publication, transparent pixels, two end codes per raster row | VIDEO; [table/character vectors](#retained-vdp1-tables-and-characters-68k-ea-waits-and-cd-transfer-completion) | PRISM (RGB555); original lookup/character rewrite guests | Scaling/end-code interaction, wrapping, physical prefetch overlap and all browser formats | Pending |
| Shading/color calculation, [VDP1] §6.3 | Gouraud vertex-word fetches, mesh, MSB-only, six-clock shadow/transparency, half-luminance and combined Gouraud effects | VIDEO, [VDP1 memory/effect vectors](#vertical-counter-and-vdp1smpc-memory-boundaries) | PRISM; original FRT/pixel effects guest | Subpixel interpolation and duplicate-coverage edges; complete readout/effects corpus | Pending |
| Draw/erase/swap/status/IRQ, [VDP1] §4 | PTMR; bounded 30-clock ENDR; per-dot draw cursor, manual transfer-over; scanline display erase and word-clock VBlank erase with 200-clock/raster overhead; manual/auto change, persistent VBE, CEF/BEF/COPR/LOPR; SCU IRQ/DMA trigger | VIDEO, VIRQ, PRISM, VTX; [local-memory vectors](#scheduled-68k-resetiack-and-vdp1-local-memory-ownership) | PRISM, VTX; command-fetch/ownership guests | Complete command/effect memory costs, mid-blank enable and half-line/swap phase | Pending |
| VDP2 VRAM, [VDP2] §3 | Bitmap RAM/bank offsets; all eight cycle words select CPU grants, split-bank matching and idle/linked timing, unused-screen release, rotation ownership and high-resolution slot limits; beam output requires addressed-bank display grants and format/reduction bandwidth | VIDEO, SC, VTX, [FETCHG](#vdp2-display-fetch-grants-and-line-table-latches-fetchg); `access.rs`, `vram_grant_tests.rs` | PRISM, SC, VTX, FETCHG | Per-word display-fetch phase/retention and complete SCU/DMA arbitration remain | Pending |
| VDP2 CRAM, [VDP2] §3.4 | Modes 0/1 RGB555 and 2 RGB888; mode-0 upper-half mirror; palette offsets | VIDEO | — | Full lane/alias and guest/browser palette corpus | Pending |
| TVMD/TVSTAT, [VDP2] §2.2 | 320/352/640/704 dots, 224/240/PAL256 lines, noninterlace/single/double density; 480-line exclusive monitors and NBG0/NBG1 pixel multiplexing; border/display enable, PAL/ODD and blanking flags | VIDEO, TIME, TVG, VTX | PRISM, SC, SCUROM, TVG, VTX | Complete raster-time latches, half-line phase and switching transients remain | Pending |
| NBG0 bitmap, [VDP2] §§4–6 | 4/8/11-bit palette, RGB555/RGB888, bitmap dimensions/base, transparency, fixed-point scroll/zoom | VIDEO | PRISM, SC (RGB555, unit zoom) | Full format/dimension/zoom vectors and browser coverage | Pending |
| NBG0–3 tile/character, [VDP2] §§4.4–4.8, figs.4.9/4.10, table4.8 | `Vdp2::character_control/tile/background`: one/two-word names, both supplement modes, 8×8/16×16 characters, 4/8/11-bit palette and RGB555/888 where applicable, flips, plane/page alignment/maps/wrap | [seven independent pixel/guest tests](../tests/video_tiles.rs) | TILE (four layers, names, palette/VRAM, window, mutation) | VRAM cycle arbitration, bandwidth/layer restrictions and exhaustive guest corpus remain | Pending |
| NBG1–3 bitmap/layer control, [VDP2] §§4–6 | NBG1 bitmap composition, formats applicable to NBG1, scroll/zoom/priority | VIDEO | — | Complete layer restrictions and guest format corpus; NBG2/3 tile layers now exercised by TILE | Pending |
| Rotation backgrounds, [VDP2] §6.1 p.147, §6.3 figs.6.2–6.4, §6.4 fig.6.7 | RBG0 fixed-point A/B matrices, viewpoint/center/Z/translation/scales, 16-plane tile or bitmap, four screen-over modes, 16/32-bit coefficient modes in VRAM or upper CRAM, coefficient line-color data, per-dot/line indexing, transparency and parameter/window switching; RBG1 parameter B/cells/B0 characters/B1 names, NBG0 effects controls | EFFECTS, R1, LINE | EFFECTS + R1 + LINE (matrix/coefficient/color mutations and Canvas) | RSCG below implements A/B start reloads; complete RBG1 boundaries and cycle/bank arbitration remain required | Pending |
| Line/vertical-cell scroll, [VDP2] §5.3 figs.5.4–5.8 | Packed enabled X/Y/zoom fields, relative offsets and held line intervals, fractional coordinates, independent NBG0/1, interleaved vertical cell tables and mosaic precedence | EFFECTS | EFFECTS (line X) | Raster-time table reads, cycle/bandwidth arbitration and full guest field/interval corpus remain | Pending |
| Windows/line windows, [VDP2] §8.1 figs.8.2–8.4 | W0/W1 rectangle or VRAM line tables and SD-bit sprite windows, inside/outside, inclusive edges, empty/inverted ranges, AND/OR on normal/rotation/sprite/color calculation; rotation-parameter windows | VIDEO, EFFECTS, SPRX | EFFECTS, SPRX (Wasm/Canvas and changed guest controls) | Full raster-window update timing and complete independent guest corpus remain | Pending |
| Mosaic, [VDP2] §5 | NBG0–3 horizontal/vertical grouping | VIDEO | — | All independent vertical/layer/guest vectors | Pending |
| Priority/composition, [VDP2] §§9–11 | Four normal backgrounds, RBG0/RBG1 and VDP1; all sixteen palette sprite types (including shared bits) and mixed RGB; priority-0 disable/ties; SFPRMD character/dot priority from names, supplement or bitmap flags and SFCODE/SFSEL | VIDEO, PRISM, SPRX, SPECIAL | PRISM, SPRX, SPECIAL (guest flag/code mutations and Canvas) | Full display/layer/raster restrictions and complete special-priority boundary corpus | Pending |
| Color calculation/offset/shadow, [VDP2] §§11–14 | Top/second-image ratio selection, gradation horizontal filtering and priority, additive and three/four-image extended blending; SFCCMD character/dot/palette-MSB conditions; signed offsets, normal/MSB/transparent shadows after blend/offset | VIDEO, SPRX, LINE, SPECIAL, GRAD | SPRX, PRISM, LINE, SPECIAL, GRAD | Gradation border/timing conformance, full guest format/extended-composition corpus and raster timing remain | Pending |
| Back/line color, [VDP2] §§7.1/11.3 | Back-screen single/per-line VRAM table, signed offset; LNCLEN top-only insertion, single/per-line LCTA palette data and coefficient-derived low seven address bits; CCRLB line/back ratios | VIDEO, LINE | PRISM (back), LINE (two line colors, coefficient mutations, extended calculation, Canvas) | Interlace/display-mode line rules, raster-time fetches and complete guest format corpus remain | Pending |
| Combined VDP1/VDP2 pipeline | Sprite/background overlap, priority, transparency, game result/restart, texture mutation changes actual pixels | VIDEO, PRISM | PRISM + Canvas assertions | Remaining modes/effects and timing above | Pending |

VDP1 command fetch retains its 32-main-clock functional deadline. Gouraud reads
use four separate VRAM word deadlines; shadow/translucency retain a background
read through a six-clock dot operation. Replacement advances one dot per clock.
ENDR has a bounded thirty-clock stop.
Normal erase follows scanline readout; VBlank erase has clocked word writes and
per-raster overhead, and leaves unfinished pixels at VBlank-OUT. VTX independently
checks the manual's 58,812-word NTSC budget and manual transfer-over status.
Color calculations use integer interpolation; full physical edge rounding,
CPU/VRAM arbitration and command/effect-specific access costs remain required. Native
vectors caught and corrected shadow MSB halving and automatic-start BEF handling.
Cyclic lists remain busy without reporting END. Invalid lists stop the machine
explicitly. RSCG below adds rotated/interlaced sprite readout and independent
start-coordinate/coefficient reloads. Unsupported enabled VDP2 effects still fail
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
| Sound RAM / SCSP mapping, [SCSP] §§1.2/4.1 | 512 KiB RAM shared by SH-2 and independent 68k; big-endian byte/word/long cycles; SCSP byte lanes; SNDON/OFF reset vectors and RAM retention; RESET output resets SCSP controls | SNDV, SNDR / SNDR | MEMG adds MEM4MB geometry for CPU/PCM/DMA/DSP and absent chip selection; [CPU cycle allocation](#cpu-sound-memory-allocation) adds retained grants/completions and cancellation; physical bus phase remains a convention | Pending |
| 32 slots, PCM8/16, [SCSP] §4.2 table 4.14 | Guest key-on/off, signed PCM, source bit controls, all 32 slots, no/forward/reverse/alternative loops, Q16 OCT/FNS pitch, signed first-order PCM8/16 interpolation for all loop directions, sound stack | SNDV, SNDR (including guest half-pitch vectors) / SNDR (half-pitch vectors) | Exact hardware fractional boundary rounding and complete envelope-controlled playback | Pending |
| Slot monitor, [SCSP] fig.4.3 / §4.2 p.89 | MSLC selects all 32 voices; CA[10:7] reports the current output address in 4K samples; byte lanes and loop normalization | SCMON (below) native / SCMON Wasm + AudioWorklet and mutations | Inactive/noise-source values and within-sample read phases remain uncorroborated; complete sound timing is required | Pending |
| Envelope, [SCSP] §4.2 pp.40–44 | Attack/decay1/decay2/release, zero-rate holds, EGHOLD, decay level, key scaling, loop-start transitions and release retrigger; EG multiplies waveform before TL/ALFO, modulation stack and DSP send | SNDV, ENV / ENV + operand mutations + WebAudio | Numeric slope/key-scaling/attenuation policy is functional, not corroborated chip timing; complete synthesis/pipeline conformance remains | Pending |
| FM/modulation/LFO, [SCSP] §4.2 figs.4.18/4.31, tables4.16–4.24 | Executing recent/past sound-stack selection with five-slot pipeline, signed averaging/MDL, guard-waveform reads; per-slot AM/PM LFO waveforms, frequency/depth/reset; internal noise/zero source | SNDV, SYNTH / SYNTH operand mutations and WebAudio | Exact chip noise sequence is unspecified; physical sub-sample rounding, full FM overflow/loop boundaries and envelope integration remain | Pending |
| Levels/pan/external inputs, [SCSP] §4.2 tables 4.27/28 | Binary total-level attenuation, direct send and fixed-point stereo pan, clipping, master output control; independently expected full-volume stereo buffers | SNDV, SNDR / SNDR | Intermediate master gain conformance and complete routing boundary corpus | Pending |
| Timers/interrupts, [SCSP] §4.2 tables 4.33–38 | Three sample-clock prescaled counters; sound/main pending/enables/clear; programmed autovector levels, 68k IRQ/RTE, SCU source 6 | SNDV, SNDR / SNDR | Complete cross-device contention/timer reset edge evidence | Pending |
| MIDI registers, [SCSP] §4.2 figs.4.59–61 | Four-byte input/output FIFOs, status/overflow, byte lanes, serial-paced 8-N-1 output, independent main/sound empty IRQs and reset cancellation | SNDV, SIOG / SIOG | Serial bit-level input/external wiring; the standard console has no MIDI connector (fig.4.59) | Pending |
| SCSP DMA, [SCSP] §3.2/§4.2 fig.4.66/tables4.39–40 | Separately granted/completed DRAM-cycle words in both directions, zero gate, retained address/direction/geometry, count/address progress, DEXE=0 preserves busy and completion IRQ; DSP/refresh priority and main/sound CPU waits; prohibited DMA-register target diagnosed | SNDV, SIOG, [DMA/refresh vectors](../src/sound/dma/tests.rs), [board vectors](../src/bus/sound_tests.rs), [original timed guest](../tests/scsp_dma.rs) / SIOG, [DMA Wasm/WebAudio](../scripts/scsp-dma.mjs) | PCM-slot ownership is implemented below; [CPU allocation](#cpu-sound-memory-allocation) adds all scheduled clients; complete physical phases and synthesis corpus remain; [functional timing policy](#scsp-shared-dram-dma-refresh-and-cpu-request-priority) | Pending |
| SCSP effects DSP, [SCSP] §§4.1/4.3/5; [ST-228][SCDSPASM] Reference 1/2 | Individually scheduled 128-instruction/sample multiply/add pipeline; COEF/MADRS/MPRO/TEMP/MEMS/MIXS/EFREG, slot and EXTS inputs, ring/table RAM reads/writes, linear/float conversion, stereo effect routing | DSPV, CLAB, [DSP clock vectors](../src/sound/dsp/clock_tests.rs), [board contention](../src/bus/sound_tests.rs), [68k/DSP guest](../tests/scsp_clock.rs) / CLAB and [SCSP clock Wasm/WebAudio](../scripts/scsp-clock.mjs) | Full field/pipeline/converter corpus and physical interface phases remain; PCM and [CPU memory allocation](#cpu-sound-memory-allocation) are implemented; DMA/refresh and buffered main-before-sound requests are covered below | Pending |
| Browser audio output | Bounded 8192-frame producer/ring, stereo sample-rate adaptation, explicit gesture enable, pause/mute/blocked/underflow/drop states; worklet renders nonzero guest PCM | SNDV / SNDR WebAudio instrumentation | Full synthesis/DSP/CD-DA validation depends on missing hardware | Pending |
| CD-DA, [CD] §§3/4/8 / [SCSP] table 4.31 | Guest play/repeat, signed stereo BINARY tracks; PRE 50/15 µs de-emphasis with continuous sector history; unchanged-start pause/resume retains the sample/filter state; EXTS0/1 EFSDL/EFPAN, mixed with 68k PCM at sample deadlines | [CD-DA vectors](../tests/cd.rs), [original CD-DA guest](../tests/cdda.rs), CLAB / CD-DA guest reference buffers + AudioWorklet, CLAB changed audio track | Full effects pipeline/subcode and complete synthesis remain; scan attenuation/muting has native vectors | Pending |

The sound CPU has an independent 11.2896 MHz domain and SCSP has a 44.1 kHz
sample domain relative to the fixed 26.8741 MHz reference. Native traces match
across 1/3/8/17/611/10000-clock advancement chunks. These deadlines do not make
the instruction-boundary 68k model or interface phase conventions electrically
accurate. DMA and CPU requests now use separately admitted DRAM words. Unsupported synthesis and invalid DSP instructions/ports fail explicitly. Original guest arithmetic
changes alter both RAM waveform and exact native/Wasm PCM; canned buffers and
no-op CPU/device implementations cannot satisfy those assertions.

## CD, boot and SMPC

| Feature / manual | Current behavior / edge cases | Native | Browser | Outstanding failure | Status |
| --- | --- | --- | --- | --- | --- |
| Cooked ISO, [Disc Format][DISC] | Bounded multi-area IP/ISO9660 startup; extended-attribute offsets and interleaved first-read/runtime files; XA file metadata, held FID windows and checked LBA/FAD/volume bounds | disc.rs, media.rs, cd.rs, CLAB, filesystem/layout evidence below | SC, CLAB/local ISO, original filesystem/layout Wasm guests below | Remaining record formats and complete runtime command/timing/error corpus | Pending |
| BIN/CUE/raw/mixed tracks, [DISC] / [CD] §§4/7.2.3 | Caller-resolved files, cooked/2352-byte Mode1/Mode2/audio, shared/separate files, INDEX00–99, gaps and TOC; subindices and PRE/DCP/4CH CONTROL metadata drive TOC/status/Q reporting; malformed/duplicate flags rejected; EDC validation, bounded P/Q correction for Mode1/XA Form1, Form2 EDC and PRE audio processing | [media vectors](../tests/media.rs), [parity-matrix/XA vectors](../src/media/integrity.rs), [directory guest](../tests/cd_drive.rs), CLAB | Directory guest on clean/correctable/uncorrectable raw media; CLAB multi-file CUE, INDEX02/99, malformed ordering, missing-reference/reset/privacy | M2G adds plain 2336-byte Mode2 payloads; complete decode/timing boundaries remain; WAVE/compression absent | Pending |
| CD command/status/HIRQ, [CD] §§3.1–3.4 | Separate command/response latches; 1075-clock command completion, read-protected responses, CMOK/DRDY/CSCT/end flags, masks, clear-zero HIRQ and SCU A-bus IRQ | `tests/cd.rs`, CLAB | CLAB command/FIFO/DMA/IRQ | Full command-specific timing and register mirrors; BIN/CUE has no R–W stream, with empty FIFO WAIT now tested | Pending |
| CD drive/TOC/session/track state, [CD] §§4/7/8.2 | TOC/session/Q, bounded seek, data play, CD-DA/repeat/end, media-dependent FAD/track/index reports; timed CDON/OFF, 60 Hz pause reports, programmed standby, scan directions/range/muting, tray/media retention and deterministic virtual close; state-valid reports/Q, distinct Seek Home recovery and retained repeat/error position, lead-out reports | `tests/cd.rs`, CLAB, CDDRV, [CDRPT](#cd-report-validity-and-retained-position) | CLAB, CDDRV, CDRPT | Full mechanical/drive timing, remaining initialization/subcode options and R–W decoding | Pending |
| CD buffers/transfers, [CD] §§3.2/3.4/5.3/5.4/8.2 | 200-sector pool, 24 partitions, get/delete/get-delete/put, early-ended writes retain all reserved sectors and their accepted length, use filter routing and sector-I/O EHST; independent selector/host command classes, identity-safe GET/DELETE, size/info/search, FIFO word/long and fixed-source SCU DMA | `tests/cd.rs`, CLAB; [transfer vectors](#retained-vdp1-tables-and-characters-68k-ea-waits-and-cd-transfer-completion) | CLAB actual DMA/IRQ/checksum, sector mutation; extended CD drive guest checks early end, routing, capacity, IRQ and changed payload | Pool-full pause/resume at the blocked FAD has native data vectors; complete transfer overlap and timing/error inventory remain | Pending |
| Filters/selectors/partitions, [CD] §§5.2/5.3/8.2.5 | FAD and masked subheader comparisons, reverse conditions, true/false routes, reset, copy/move through destination filter | `tests/cd.rs::selectors_fad_range_connections_copy_move_and_size`, Mode2 reverse vectors | CLAB default filter/partition route | [CDSEL](#scheduled-cd-selector-transfers-and-independent-command-classes-cdsel) adds asynchronous raw copy/move, full-buffer WAIT/move, retained partial failures and concurrent host/selector commands; remaining command-specific connection/timing boundaries stay mandatory | Pending |
| Scheduled directory acquisition, [CD] §§3.2/6.2/8.2.8 | ChgDir/ReadDir acquire descriptors and directory sectors through drive deadlines, runtime ECC/retries and one reserved shared-pool workspace; atomic publication, cancellation invalidation, independent command engines | [Scheduled vectors/guest](../tests/cd_directory_schedule.rs), existing malformed/XA/window corpus | Same original guest/media, FRT progress, SCU masked/unmasked EFLS handler, navigation/omission/abort controls and cold boots | Deterministic seek policy; remaining filesystem/drive/media/HLE contracts stay required; see [Issue #21 validation](validation-issue-21.md) | Increment covered; umbrella pending |
| BIOS-less startup, [Boot ROM][BOOT] / [DISC] | Original first-read HLE plus licensed SH-2 master/slave reset trampoline and system-service initialization, separate from ExternalBios | disc.rs, game.rs, CLAB, src/hle/boot.s, HLESYS, DPLAYER below | SC, CLAB, HLESYS, DPLAYER | Full startup/region/version, CD-library and per-call/reset contracts remain mandatory | Pending |
| BIOS services, [BOOT], ST-162-R1 System Program §2.2 | Original guest-executed vector/callback registration, SCU mask/shadow, shared semaphores, copied priorities, power-clear bytes and non-returning SYS_EXECDMP CD player | tests/hle_system.rs, tests/hle_player.rs | HLESYS and DPLAYER, including digital/analog input, real CD-DA/Canvas/WebAudio, mutations and reset | Clock and BUPG backup calls have dedicated evidence below; complete CD-library ABI and remaining per-call side effects/timing remain | Pending |
| External BIOS execution, [BOOT] | Genuine reset vectors, original synthetic ROM handlers | boot.rs, sh2_system.rs | ROM | Genuine BIOS initialization/menu/boot **unverified**; no lawful local BIOS supplied | Pending |
| SMPC direct digital port 1, [SMPC] §3 | Active-low nibble selection; per-bit PDR/DDR input/output merging; TH input visible with EXLE | devices.rs, INTCG | SC, INTCG | DIGG settling and CLKG clock/reset vectors below; full system timing remains required | Pending |
| SMPC commands/status/timing, [SMPC] §§1.2/2.3/2.4/3.2 | SF/IREG/OREG/SR, CPU commands, INTBACK status/peripheral phases, continue/break, VBlank collection and optimization; SCU source 7 | smpc/tests.rs, tests/smpc.rs, DUAL | SMPR, DUAL | TVG covers selected standard-TV clocks and collection intervals; SMPB adds zero-byte packing, BREAK during each collection phase, timeout and direct analog-route cancellation; remaining command/latch/reset boundaries remain | Pending |
| SMPC reset/power/clock, [SMPC] table 1.1/§2.3 | MSHON, SSHON/OFF, SNDON/OFF, CDON/OFF, NMIREQ, SYSRES (volatile reset, installed-ROM or HLE-disc restart and battery retention); RESENAB/DISA control reset-button NMI edges and VBlank-latched RESB | DUAL, SNDV, SNDR, PWRG, smpc/tests.rs | DUAL, SNDR, PWRG | Clock switching, full status/power domains | Pending |
| RTC/region/system flags, [SMPC] §§2.4/2.5 | Deterministic Gregorian/BCD RTC, SETTIME, retained four-byte SETSMEM, all eight documented board regions, independent CD/sound reset status, output control flags | smpc/tests.rs, tests/smpc.rs | SMPR, original status guest | Full system-control and remaining reset domains; PAL timing is separate from area reporting | Pending |
| SMPC direct digital port 2, [SMPC] §1.2 pp.7–9, §3.4 table 3.36 p.97 | Independent PDR2/DDR2, IOSEL2/EXLE2; active-low nibble reports; host keyboard levels | BK; `both_digital_ports_have_independent_direction_selection_and_enable_bits` | BK; I/K/J/L keyboard input and release checks | DIGG implements both-port settling; complete clock/reset packet timing remains | Pending |
| External latch/PAD IRQ, [SMPC] §1.2/table1.5, [VDP2] §2.5/tables2.3–2.4 | Both TH input falling edges gated by EXLE/DDR; independent SCU PAD IRQ and VDP2 EXLTEN; software/read latches, held HCNT/VCNT, EXLTFG read-clear, non-interlaced ODD; `Smpc::set_th`, `Bus::set_peripheral_th`, `Vdp2::read16/external_latch` | INTCG, bus.rs::smpc_input_edges_latch_raster_counters_and_raise_pad_irqs_with_independent_enables | INTCG (both ports, repeated edges and control mutation) | Pin synchronization, blanking-counter discontinuities and other display modes | Pending |
| Analog input, [SMPC] tables 3.19/3.37; [STN43] Analog mode | ID15 X/Y/Z and ID16 X/Y/AR/AL, active-low buttons, trigger hysteresis (146 on / 85 off); timed INTBACK and direct TH/TR/TL handshake, transaction latching/abort, independent ports; standard Gamepad radial axes/triggers and focus neutralization | smpc/tests.rs, tests/smpc.rs; `Analog`, `Handshake` in [controller.rs](../src/smpc/controller.rs) | SMPR + PADR (both direct ports, changed axis, ID15/16 Gamepad/focus) | Full electrical timing, mode-switch calibration, external-latch synchronizers and complete command/reset/clock integration remain | Pending |
| Input focus safety | Both pads use host button levels, blur/view/tab/pause release, no guest-memory shortcuts | game.rs | SC | Additional controller types absent | Pending |

## Concrete unresolved gaps and closure gates

1. Remaining SH-2 semantic/exception edges, complete reset domains, BSC configuration and remaining UBC/standby timing,
   remaining DIVU/CPU timing boundaries, DMAC DACK/CPU arbitration, external IRQs and bus locking;
   cycle-based dual-CPU scheduling and arbitration. CPU opcode decoding alone
   does not satisfy the CPU or multiprocessor criterion.
2. Complete MC68EC000 timing/prefetch edge conformance and sound bus arbitration;
   SCSP timed envelopes and complete modulation conformance remain required; FM/LFO/noise now have SYNTH subset evidence; effects DSP and CD-DA have CLAB subset evidence. Direct PCM,
   executing sound CPU, timers/DMA/FIFO subsets and WebAudio now have evidence.
3. Full base memory map, remaining backup reset/service boundaries and BIOS calls,
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

`Machine::step` issues the master and slave only when each CPU's own opcode/wait
deadline has arrived. Equal deadlines use master first. Base SH-2 states follow
P table7.2; CPU-local ordinary T1/T2 occupancy, BSC/VRAM waits and DIVU interlocks
bound the issuing CPU's next deadline, converted through the PLL. A slow master
no longer charges its wait time to a ready slave. Only SLEEP/reset polling uses
an eight-reference-clock bound. See [the timing inventory below](#independent-sh-2-issue-deadlines-and-clocked-on-chip-dma).

On-chip DMA advances on separate source/destination completion deadlines even
without CPU issue. [DMABG](#on-chip-dmac-bus-transactions-dmabg) replaces its
eight-clock copy cadence with BSC-sized phases, cycle-steal/burst ownership and
TAS exclusion. Exact external request sampling and complete
refresh/display/sound ownership remain required; independent internal phases and
DIVU interlocks are covered below. DMA address errors latch for vector10
at the next eligible CPU boundary. NMI precedes internal IRQ selection; each CPU
applies its own SR mask, delayed-branch and control-transfer inhibition. Accepted
interrupts consume a boundary without retiring an instruction. FRT requests
remain asserted until guest software reads/clears FTCSR. Slave reset discards
its FRT/IRQ/DMA deadline state while retaining shared memory and master state.
Successful earlier accesses within a failing boundary are not rolled back.

`advance_frame` advances to a clock-based raster field deadline. Raster,
on-chip timers/DMA, SCU timer/DSP/DMA, sound and SMPC continue during SLEEP.
Instruction-specific minimum states do not implement a complete SH-2 pipeline:
load-use/multiplier hazards, IF/MA overlap, speculative prefetch, CPU/DMA
transaction arbitration and complete reset/bus behavior remain mandatory.
The SMPC command minimum rounds 30 µs at 26.8741 MHz upward to 807 clocks;
completion is delivered at a scheduler boundary. Unsupported commands still fail
explicitly. Invalid SF/busy writes remain diagnostics, not physical recovery claims.

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
`8ef46a1e059ff3618f1ca2faf0b329bbcb4b24fa85177639906c3f7d50137511`.

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
The direct VDP2 debugger renderer samples current state; Machine presentation now
uses scanline history (RASTG below). RSCG below adds independent A/B Xst/Yst/KAst
line-start reloads. Mid-line fetches and other table strobes remain required.
Coefficient line color and CRAM coefficient sources are implemented and tested
by LINE below; reserved parameter reload bits still fail explicitly.

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

The independent [effects interpreter](../src/sound/dsp.rs) executes 128
individually scheduled instructions per sample, with parallel datapath fields in
each instruction. Its signed 24-bit inputs, 13-bit multiplier operand, 26-bit
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
| TRA/TWA/TWT, sample decrement, read-before-write | `Dsp::step` circular TEMP indices | DSPV `temp_pointer_moves_once_per_sample_and_reads_precede_parallel_writes` | CLAB arithmetic; delayed TEMP guest scene remains |
| XSEL/YSEL/IRA, signed multiplication, ZERO/BSEL/NEGB | `Dsp::step` signed multiplier/26-bit accumulator | DSPV `multiply_accumulate_uses_signed_13_bit_y_and_previous_accumulator_output` | CLAB MIXS0/MEMS0/EXTS0/1 and two coefficients; all ALU control combinations remain |
| SHIFT, EWT/EWA, saturation/wrap | `Dsp::step` prior accumulator; 16-bit effect stores replace the preceding value ([ST-228][SCDSPASM] p.31) | DSPV `shifter_clips_or_wraps_and_effect_stores_replace_the_previous_sixteen_bits`; [all sixteen effect-port deadline vectors](../src/sound/dsp/clock_tests.rs) | CLAB exact bipolar stereo; [repeated effect-store guest](#scsp-effect-register-store-semantics) with negative/zero results and omitted-store controls; complete overflow guest corpus remains |
| YRL/FRCL/ADRL/ADREB | `Dsp::step` latches and address gate | DSPV `y_fraction_and_address_latches_use_previous_step_values` | Complete interpolation/address-modulation guest vectors remain |
| MRD/MWT/IWT/IWA, TABLE/MASA/NXADR, RBL/RBP | `Dsp::step` sound RAM path, read latch after two steps | DSPV `ring_table_linear_reads_use_two_step_latency_and_preserve_read_before_write` | Complete sound RAM/pipeline guest corpus and CPU/DMA contention remain |
| NOFL linear/normalized-float conversion | `pack/unpack`, `Dsp::step` | DSPV `floating_memory_format_has_independent_bit_patterns_and_bounded_error` | Full independent float/subnormal/rounding and delayed memory-cycle corroboration remains |
| IMXL/ISEL, EFSDL/EFPAN, external CD route | `Dsp::add_slot`, `Scsp::sample` | DSPV `slot_send_wraps_mix_without_saturation_and_dsp_output_obeys_routing`; CLAB exact samples | CLAB in browser: changed guest coefficient changes exact stereo and computed result; real AudioWorklet, changed CD track; full synthesis remains |
| Invalid instructions/ports | `Dsp::step/read/write` bounded diagnostics | DSPV `illegal_memory_steps_and_reserved_ports_fail_without_faking_effects` | Guest fault corpus remains |

The sound scheduler uses the rational 5.6448 MHz instruction clock from
[SCSP] §2.2/table2.2 (128 steps at 44.1 kHz). Each instruction reads live MPRO,
COEF, MADRS and RAM at its own deadline. An MRD result remains latched through
its two-step delay even if another processor subsequently changes RAM. Only
step127 commits output and advances timers/sample interrupts. Each slot's
source operands use the [separately granted PCM reads](#separately-retained-pcm-operands-and-sound-memory-ownership).
An untouched DSP skips empty
instruction deadlines; the first MPRO write joins the current phase without
replaying elapsed slots.

[SCSP] §3.2/fig3.2 gives DSP requests priority over main/sound CPU requests.
A DSP RAM instruction owns one memory-cycle interval through the next DSP
boundary. The [board adapter](../src/bus/transactions.rs) retains a conflicting
SH-2 or SCU beat until release; the [sound adapter](../src/sound/transactions.rs)
extends reset/data/prefetch cycles and instruction retirement with sound-clock
wait states. A delayed RESET prefetch moves both external pulse edges and
retirement, so an event cannot be scheduled into the past. This implements
DSP ownership. The [DMA/refresh/request-priority entry](#scsp-shared-dram-dma-refresh-and-cpu-request-priority)
adds internal transfers, refresh reservations and buffered main-before-sound
requests. PCM ownership now has the separate operand evidence below;
complete physical grant-phase evidence remains mandatory, along
with the other unresolved audio inventory entries.

The original MIT [SCSP clock fixture builder](../tests/fixtures/build_scsp_clock.py)
emits a synthetic BIOS with independent SH-2, 68k and DSP programs. The 68k
uploads a program, polls its early EFREG0 result, changes sound RAM and then
records EFREG1 from a later MRD of the same address. Authored results are
`091A/11A2` for `1234 -> 2345`, `091A/1A2B` for `1234 -> 3456`, and
`091A/091A` for the unchanged-operand control. Repeated native/Wasm cold boots
also require the guest completion marker, no CPU exceptions, the actual RAM
word and deterministic nonzero stereo. The browser sends fresh emulated samples
through production AudioWorklet and verifies the expected peak, rendered frames,
underflow and no dropped samples. Expanded 512 KiB ROM SHA-256:
`6a9889236deb40ff2d55e4c804c3aea7116940b28314fc88dd8ea3bedf372759`.

Independent [deadline vectors](../src/sound/dsp/clock_tests.rs) cover every side
of MRD/IWT/multiply/EWT, live program/coefficient/address changes, frame output,
reset at twelve pipeline boundaries, sound-off independence, idle activation,
three reset-read wait states, multiply retirement, canceled/stalled RESET and
simultaneous reset edges at all 128 positions without losing a sample boundary.
[Board vectors](../src/bus/sound_tests.rs) cover both SH-2s and all three SCU
channels, retained writes and separate halfword completion. Four advancement
sizes must produce the same values and boundaries. Existing PCM/FM/envelope,
CD-DA, Hardware Lab and sound CPU diagnostics remain enabled. This evidence
addresses these sound contracts and does not complete Issue #17.

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
RBG1 presentation now participates in scanline history (RASTG below); precise
parameter reloads, cycle/bank arbitration and complete special-function/edge
evidence remain required.
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
new directory. LBA-to-FAD and file-range arithmetic are checked. A rejected command preserves the prior directory; an accepted acquisition
invalidates it until scheduled completion (see Issue #21 below). Mounting a new medium invalidates the
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
| XA Form2 retry and confirmation flags | Form2 error data are delivered immediately unless Form2 retry is enabled; the ordinary retry/output policy then applies. Subheader confirmation can be disabled for decode while selector matching still uses the first subheader. Independent EDC golden vectors isolate the controls. Failed XA correction restores the received header. | M2G below adds plain Mode2; a complete subcode/error-boundary corpus remains required. |
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

**FAMG**: [74 source-authored MC68000 vectors](../tests/fixtures/m68k_families.py),
[reproducible builder](../tests/fixtures/build_m68k_families.py),
[native sound-bus execution](../tests/m68k_families.rs) and
[browser Wasm](../scripts/browser-smoke.mjs). The master uploads actual 68k code;
the sound CPU computes values/CCR, takes illegal/line-A/line-F/CHK/TRAPV exceptions
and RTE, and returns to ordinary PCM/SCSP setup. Cases cover all base EA forms,
including PC-relative forms, and supplement SNDR/SIOG's decimal, extend, MOVEM
and MOVEP evidence. Undefined CHK flags are deliberately excluded. Browser mutation
of ADD.B to SUB.B changes only that computed result/CCR.
Expanded 512 KiB ROM SHA-256:
`fdfbc9457de3504e4a979ab29f26a22ae7b5b2f0c1f543d67161dd75f8df6733`.
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
`6ec3ef18e07484d4993051afd32b13c8ca928b672f20fd7508c4e844dbaca1e9`.
Source/generated ROM use the adjacent MIT license.

[ISO bootstrap](../src/disc.rs) and [runtime CD directory commands](../src/cd.rs)
now scan descriptor sets (ST-040 §3.2.1), preserving the primary namespace across
boot/supplementary descriptors. They accept hidden regular files and reject
duplicate primaries, missing terminators and extents outside the declared volume.
Physical padding is not part of that volume. A zero first-read address disables
that transfer as specified by ST-040 §4.3; original AIP instructions still execute.
[Native startup vectors](../tests/disc.rs),
[original runtime guest](../tests/cd_drive.rs) and browser Wasm execute these cases.
At that revision, general HLE system/backup/CD ABI, multi-area startup and other
media contracts remained required. The later ISO/XA bootstrap inventory below
records additional implementations without substituting for the pending services.

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

SYS_CHGSYSCK/SYS_GETSYSCK now have CLKG evidence below. BUPG below adds the independent backup-library/filesystem ABI. SYS_EXECDMP
and CD-library ABIs remain mandatory and unimplemented. Full synthetic BIOS/service acceptance
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
| Normal graphic B display | `Vdp2::dimensions/render` accepts TVMD=8001 and emits 352×224 pixels; Wasm exposes dimensions and Canvas resizes/reset clears correctly | CLKG guest writes a green back screen; native/Wasm compare all pixel bytes, production Canvas checks the last visible pixel | TVG below adds standard TV resolutions/interlace; raster latching and cycle-pattern arbitration remain pending |
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
| WCR software waits and BCR1 AnLW | `Bsc::software_waits` decodes zero/one/two Tw or long waits of three through six, independently for CS0–3; CS2/3 share AHLW. Register vectors cover every area/selection, keys and atomic rejection. BWAIT measures eight uncached CS3 reads with each CPU's own FRT, comparing seven literal expected deltas | Tw clocks delay the issuing CPU’s independent opcode deadline; base T1/T2 overlap, external WAIT and full bus arbitration remain required; IDLEG below adds intercycle turnaround |
| BCR2 byte/word/long interfaces | `CpuBus::external_read/external_write` split larger operands and cache-refill requests into ordered physical beats. Bus vectors check exact per-beat wait sums, data, write-through/cache hits and actual sequential external UBC observations | CS0 now has the fixed 16-bit board strap (Issue #23 below); BCMEM below adds MCR-selected SDRAM/DRAM/pseudo-SRAM widths and burst timing. Complete board lane/bridge behavior, WAIT/mode-register cycles and arbitration remain required |
| BCR1 CS2 endian | `Bsc::little_endian` selects per-CPU CS2 conversion. Actual transfers reverse operand bytes, leave byte addresses/strobes unchanged and transfer low words first on word-wide buses. Literal native vectors assert physical bytes, both word offsets, all byte lanes, writes and the other CPU's independent view | Cache-through use follows §7.3.2; this does not establish all cache/MMIO/board-width combinations |
| Register programming rules | Wrong keys do not change registers; reserved bus widths and unsupported little-endian byte buses are rejected atomically. Reserved IWn=11 is rejected atomically; valid IWn=0/1/2 have transaction effects (IDLEG below). BCR1 memory/partial-share/BSTROM selections and MCR timing now have behavior (BCMEM below). Reserved memory selections, TRAS/AMX and prohibited self-refresh settings reject atomically | Complete initialization/mode-register and WAIT behavior remain required; accepted configuration alone is not conformance |
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
| `bsc-waits.hex` text | `8c8788632f87a5269a72b9056c33d572dcd61ea9f83c459d10534af5aa390fc6` |
| Expanded 512 KiB original BIOS | `823017e8691072b0b81d519c3234f660cbebcff04fe82a392ecb5c82fce65251` |
| `bsc-waits.tsv` | `6c024728abbe44bff6f4e3baa21c8b090b93e5ecb508bc874bacd696dcef9891` |
| `bsc-endian.tsv` | `95de3de1f2d74e5da6a0cea05fea79e455a169c1d1f5c5b4ee1f2ff803b9bf24` |

All remaining umbrella hardware implementation and validation criteria stay
mandatory. DIVU overflow intermediates are implemented below; 68k bus
sequencing, full coordinated CPU/DMA ownership, backup
filesystem/HLE/CD services, SCU/DSP, video/audio/media/SMPC and comprehensive
integration evidence remain incomplete. No umbrella checkbox is marked complete.


## DIVU, UBC, peripheral deadlines and SMPC status corrections

The original [DIVG builder](../tests/fixtures/build_divu.py) and adjacent
[MIT license](../tests/fixtures/LICENSE) produce a synthetic external BIOS;
[its literal table](../tests/fixtures/divu.tsv) is authored independently of
execution. [SH7604][H] §§10.2–10.4/table10.2 specify three setup clocks and
three division clocks on overflow, intermediate DVDNTH in both OVFIE modes,
and saturation of DVDNT(L) only with interrupts disabled. The implementation
uses signed non-restoring division steps (programming manual [P] §6.19),
retaining the carry/sign state between steps. Normal division retains its
39-clock quotient/remainder behavior. Both CPU-local units execute all normal
sign combinations and overflow cases, including zero divisor, MIN/-1, the
negative/negative maximum-quotient-with-remainder boundary and 64-bit input.
The guest observes both quotient aliases, remainder, DVCR, DVSR and VCRDIV,
acknowledges a real overflow IRQ, and verifies an independent UBC data break.
Native and browser checks compare all 164 words from each CPU and repeat reset.
A changed 64-bit dividend changes only its two partial-quotient records.

H §6.3.7(4)'s three-clock UBC configuration delay preserves the previous
comparison until the deadline. Successive writes have separate deadlines;
reading the register written last is the specified synchronization barrier.
[Component vectors](../src/onchip/ubc.rs) check clocks 0/1/2/3, old/new addresses,
non-barrier reads and word/long barriers. [Bus vectors](../src/bus.rs) retain
cache-hit, refill-order and CPU/DMAC/external-master distinctions. The slave
now acknowledges an accepted UBC request just as the master does, preventing
re-entry on RTE. DIVG exercises that fix on both actual guest processors.
Speculative fetch/pipeline behavior and full instruction timing remain required.

`OnChip::clocks_to_event` exposes DIVU, UBC, FRT compare/overflow, BSC refresh,
SCI shifter and WDT overflow/reset-hold deadlines to `Bus::advance_clocks`.
[Native vectors](../src/onchip/peripheral_tests.rs) assert authored transitions
at clocks 1/8/16/39/320 and verify that 14 clocks before watchdog reset are
counted before its 512-clock hold. Six advance chunk sizes and both CPU
identities yield the same retained FRT/DIVU state. This improves peripheral
ordering; CPU/DMA readiness, contention and opcode cycle accounting still
require the coordinated transaction scheduler identified in review.

[SMPC][SMPC] §2.4.2's OREG9 table is implemented as typed board region straps
for all eight documented codes. Reserved/prohibited codes reject through the
Wasm adapter before mutation; media does not select the region. Region straps
survive volatile and system resets. TVG below adds independently configured PAL
timing; area reporting never selects the TV standard. OREG11 bit6 now reports CD reset independently of OREG10's
sound-reset bit, at the command completion deadline. The original MIT
[status guest](../tests/fixtures/build_smpc_status.py), [native integration](../tests/smpc.rs)
and browser Wasm checks execute CDOFF/SNDON/CDON/SNDOFF and INTBACK for every
region and repeat reset. The 68k STOP program is uploaded and executed through
the real sound bus; no host test writes guest result memory.

[SCSP][SCSP] §4.2/table4.40 says writing DEXE=0 does not stop execution.
`Scsp::write16` now keeps busy asserted until the last DMA word completes.
[Component vectors](../src/sound/tests.rs) check intermediate busy/count/data
and end IRQ; the original [68k DMA/MIDI guest](../tests/fixtures/build_sound.py)
now writes zero before polling, so an early-cleared busy bit exposes uncopied
register data in both [native](../tests/sound_io.rs) and browser Wasm checks.
The existing source-operand mutation, MIDI, interrupt, stereo and AudioWorklet
assertions are retained. Full SCSP transfer timing/arbitration remains required.

| Reproducible artifact (original MIT content) | SHA-256 |
| --- | --- |
| `divu.hex` text | `0c68ce19d942105c9c5b604cd2aeb0d0b8afb610e14a21a395ed4f493a4c8494` |
| `divu.tsv` | `4fffc7d4e5cae2a024aee440e964d329093bb93203e8e9af54ee43f24b47433c` |
| `smpc-status.hex` text | `ebc72a7f0deb1a2bdbef2fb5e9e368e707015f5a702b880233fd9a9e921946dd` |
| Expanded `sound-io.hex` BIOS | `3f1e40e2f276d04b9d5c3ce81a9046a714ae870c85fb5d73dfafabb9402dc07c` |

Seven deliberate implementation regressions are rejected by the corresponding
native tests: suppressed partial quotient, immediate UBC comparison changes,
wrong-CPU UBC acknowledgement, discarded pre-reset clocks, fixed-Japan status,
missing CD-reset status and early SCSP DMA busy clearing. These are executable
regression checks; they do not establish the remaining umbrella acceptance.


## Guest-executed HLE backup library (BUPG)

Primary specifications: [ST-162-R1](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-162-R1-092994.pdf), Backup Library chapter, data
specifications 1–4 and function specifications 1–11; Sega's original
[public ABI declarations](https://github.com/johannes-fetz/joengine/blob/master/Compiler/COMMON/SGL_302j/INC/SEGA_BUP.H)
provide numeric statuses and vector offsets. No SDK routines/objects or firmware
were copied. The [original source and storage contract](../src/hle/README.md#independent-backup-library)
explain the deliberately separate SRBUP001 HLE format. Raw backup bus behavior
is unchanged; external BIOS mode never installs these routines or converts RAM.
A genuine BIOS/private filesystem compatibility result is not claimed or required.

BUPG means [the original MIT diagnostic builder](../tests/fixtures/build_hle_backup.py),
[ISO](../tests/fixtures/hle-backup.iso), [158 independent ABI records](../tests/fixtures/hle-backup.tsv),
[six native tests](../tests/hle_backup.rs) and identical actual Wasm execution in
[browser-smoke.mjs](../scripts/browser-smoke.mjs). Guest code computes, reads and
verifies payloads through the emulated odd-byte bus; the host does not intercept
calls or fill running guest results. Media contain the adjacent fixture MIT license.

| Documented interface | Implementation | Native and browser evidence | Deviations / remaining coverage |
| --- | --- | --- | --- |
| Init, function 1; BupConfig | [ROM5000 copy/relocation/publication](../src/hle/system.s), [ROM10000 library](../src/hle/backup.s), [HLE-only installation](../src/hle.rs) | Actual 16 KiB expansion to two different caller addresses, RAM354 vector table/slots4–40, device0 unit1/partition1, absent devices1/2, unchanged backup on Init | Helpers execute in the original guest ROM; optional backup peripherals absent |
| SelPart/Format, functions 2/3 | `bup_selpart`, `bup_format` | Partition0, invalid partition, absent device; explicit format, unknown raw import remains untouched | Internal device is writable; no optional cartridge write protection |
| Stat, function 4; BupStat | `bup_stat`, `scan_next` | All six fields at empty/partial/full/freed capacity; 0/257-byte data counts and maximum size; 510 real allocated blocks | Capacity policy and metadata overhead belong to documented SRBUP001 format |
| Write, function 5 | `bup_write`, `write_allocate` | Empty/257/65/32600-byte files, duplicate-name mode, shrinking overwrite, full device, size overflow and failed-write retention | Original HLE format; interrupted writes need not be transactional (manual reset-disable warning) |
| Read/Verify, functions 6/9 | `bup_read`, `bup_verify`, `checksum_byte` | Three computed byte endpoints; exact data/mismatch/not-found/broken statuses; changed XOR operand changes guest and persisted data; full-size verification | Sum checksum detects ordinary byte corruption, not all same-sum changes |
| Delete/Dir, functions 7/8; BupDir | `bup_delete`, `compact_copy`, `bup_dir` | First/middle/last/full deletion, moved-file retention, prefix matches, zero-size files, negative insufficient-table count, 36-byte structure stride and untouched output guard | Unused raw bytes after directory terminator are ignored |
| GetDate/SetDate, functions 10/11; BupDate | `bup_getdate`, `bup_setdate`, month/division helpers | Eight independent Gregorian/minute/weekday vectors, 1980 leap day, 1999 year-end, 2000 leap day, 2100/2200 non-leap centuries and 2235 upper year; SHC callee-save records | Valid documented BupDate fields, years1980–2235; invalid dates have no specified status in the void ABI |
| Persisted/corrupt media | Existing `BackupRam` plus guest validation/copy code | Cold reopen/export/import, repeated read-only probes and byte-for-byte retention; bounded corrupt marker/count/length/name/comment/language/redundant-count/checksum probes; native malformed-size/count extrema | Existing BK/PWRG reset-domain/import-atomicity tests remain; this row does not establish the full bus/reset umbrella |

Reproduce with `python3 src/hle/build.py --check` and
`python3 tests/fixtures/build_hle_backup.py --check` (both in CI).
The disc SHA-256 is
`aed258ed218fd4bcee94890e6582ee59185341842b70cf60aa1f6644c7b372db`;
the backup library image is
`992e935cf3a0bd64435ee7ec7c35afad05b3c8fcf8d7e2767914ca902bdc52b2`.
The original boot ROM fragment and all three existing demonstration media remain
byte-identical. BUPG is diagnostic media, not a replacement demonstration.
No mandatory Issue #17 umbrella checkbox is completed by this addition. All
remaining CPU/bus/scheduler/chip/service/demonstration evidence remains required.


## Standard TV graphics and dynamic INTBACK (TVG)

Primary references: [VDP2] §§2.1–2.5, tables2.1/2.3/2.4 and §8.1;
[SMPC] table1.1 (including PAL PLL frequencies) and §3.2 collection optimization.
The original MIT [builder](../tests/fixtures/build_display_modes.py),
[guest source](../tests/fixtures/display-modes.s) and
[ROM](../tests/fixtures/display-modes.hex) have expanded SHA-256
`59d477a20ca27b0f38b5d261247033843d4ec71b52e5bacc2057b4c1b8696184`.
ROM word100 is a guest-read TVMD operand, not a host write to emulated state.

| Behavior | Implementation | Native / actual browser Wasm evidence | Remaining required behavior |
| --- | --- | --- | --- |
| Standard TV graphic dimensions and source rows | `Vdp2::dimensions/validate/render`, `Raster::configure/display_lines` | [display_modes.rs](../tests/display_modes.rs) and [browser-smoke.mjs](../scripts/browser-smoke.mjs) execute all 60 valid NTSC/PAL, A/B normal/high-resolution, line-count and density combinations; eight authored corner colors plus 511/512 wrap pixels; production Canvas 704×480 | Complete exclusive-monitor multiplexing, per-format bank restrictions and VRAM arbitration |
| Field/status/counter transitions | `Raster::advance/counters/odd`, `Vdp2::scan_field/read16`, bus raster events | Same guest polls successive VBlank transitions and latches VCNT; literal PAL/ODD/double-density counter parity assertions; `tv_modes_have_independent_blank_field_and_counter_vectors` compares chunk-independent event timestamps | Half-line phase and blanking counter discontinuities; RASTG adds historical scanline pixels; full table/window/reload latching remains |
| INTBACK collection in every tested mode | `Smpc::vblank_out_after`, bus supplies PLL-converted active interval | Guest compares measured first collection and optimized second collection, both digital and ID15 packets; `optimized_intback_uses_selected_display_interval_and_reset_cancels_it` checks deadline−1/deadline/reset cancellation | Complete continue/break/clock/reset/latch boundary integration still required |
| PAL fixed domains and reset | `SystemClock::set_pal/advance`, `Bus::advance_fixed_clocks/set_pal/reset_volatile/system_reset` | Rational-phase vectors and `pal_reference_ticks_without_main_edges_still_produce_every_audio_sample`; guest checks physical PAL retention on reset | Full transaction scheduling and reset-cycle evidence remain required |
| Blank border and horizontal windows | `Vdp2::render/window` | `blank_border_uses_lowermost_back_color_and_hires_windows_keep_dot_precision`: opaque black/back-color selection and inclusive normal/hires window edges | Complete raster-time window evidence remains required |

Hardware Lab's advanced-video acceptance now compares fourteen independently
specified pixels in native/Wasm and production Canvas. Mutating only the disc's
A-table My literal changes the quarter-turn pixels; mutating only its first
line-window start clips two previously visible pixels. Diagnostic results and
all pixels are reasserted after guest Start restart. The guest now uses CD
Get-and-Delete, so restart consumes a fresh sector and receives a fresh CD IRQ
instead of reusing buffered data. Updated original disc SHA-256:
`6eef0cafaf4dc14575ce692a8892937cc140b9e10502de1b6ed6d03d6cd7c6d6`.
Audio/CUE hashes and licenses are unchanged. No umbrella completion is claimed;
all other mandatory review findings and Issue #17 criteria remain in force.


## VDP1 high-speed shrink and EOS (HSSG)

[VDP1] §6.3/fig.6.5 and §4.2 define HSS and EOS. The current
[sampling contract](vdp1-sampling.md) and [Issue #57 validation](validation-issue-57.md)
supersede the initial endpoint-interpolation oracle. The original HSSG checkpoint
implemented EOS register capture and original-source parity, plus HSS's
all-magnification end-code rule, but its ordinary scaling and post-interpolation
parity adjustment did not reproduce every Figure 6.5 row.

Current [literal framebuffer vectors](../tests/vdp1_sampling.rs) cover all ratios
3/8 through 10/8, both EOS selections, flips and both command kinds. The extended
MIT [builder](../tests/fixtures/build_sprite_shrink.py),
[source](../tests/fixtures/sprite-shrink.s) and
[ROM](../tests/fixtures/sprite-shrink.hex) execute twelve real draws, poll CEF,
record 53 framebuffer words and compute a checksum. [Native integration](../tests/sprite_shrink.rs)
and [browser Wasm/Canvas](../scripts/sprite-shrink.mjs) retain the HSS-clear
operand control and add changed-texture/omitted-draw controls with cold boots.
Current hashes and actual check results are in the #57 validation record.
The earlier seven-draw/28-pixel ROM hash
`f21ddc6c195a0202cf6e77469225a8e5a878a728d0da26fc6932e23286b1c79b`
is historical and no longer identifies the current fixture.

Physical source-walk bandwidth, skipped-texel end-code acquisition, vertical
scaling and complete edge rules remain required under #17. RSCG below adds
interlaced/rotated scanout and the EOS framebuffer-change latch. The original
Issue #17 acceptance criteria remain unchanged and incomplete.


## BSC intercycle turnaround and SCU DSP DMA execution

Related to #17. The full umbrella remains incomplete. Primary references:
[SH7604][H] §§7.2.3/7.4.1/7.9, figs.7.7/7.47; [SCU][SCU] §§4.3–4.5,
tables4.6/4.7 and pp.89/134/135/136/138/140; [SCU final corrections][SCUFINAL]
#27. Both new diagnostics are original sources and generated media covered by
[the fixture MIT license](../tests/fixtures/LICENSE).

**IDLEG** links [builder](../tests/fixtures/build_bsc_idle.py),
[BIOS](../tests/fixtures/bsc-idle.hex), [golden timer/data records](../tests/fixtures/bsc-idle.tsv),
[native guests](../tests/bsc.rs) and actual Wasm execution in
[browser-smoke.mjs](../scripts/browser-smoke.mjs).

| Required behavior | Implementation | Independent evidence | Remaining limits |
| --- | --- | --- | --- |
| WCR intercycle IW0–3 | `Bsc::access_waits` remembers the preceding physical read's CS area/direction, applies that area's IWn, and enforces the minimum one-cycle read/write gap even with IWn=0 | `turnaround_uses_prior_read_area_minimum_write_gap_and_existing_empty_cycles` checks every source/destination CS, direction, legal IW setting and 0–4 empty clocks; reserved IWn=11 rejects without publishing | Full ordinary/DRAM/SDRAM/burst/board and WAIT protocols remain required |
| Already empty bus time | `Bsc::advance_bus` first consumes queued T1/T2/Tw/turnaround occupancy; only the remainder satisfies the next turnaround | Clock-chunk vectors and real bus tests distinguish occupied time from 0/1/2 empty clocks | Base opcode states and T1/T2 occupancy now bound independent CPU deadlines (TIMING below); subcycle CPU/DMA scheduling remains required |
| Cache and bus ownership | Only `CpuBus` physical beats enter history; cache hits/arrays leave it untouched; write-through hits and narrow beats participate. CPU/SCU ownership changes, reset and standby end old history | `bsc_turnaround_tracks_physical_cpu_beats_cache_hits_and_empty_bus_clocks`, refill/width vectors, cold-reset IDLEG | Actual arbitration latency, TAS locks and interleaving a stalled transaction with other CPUs remain required |
| Both CPUs' guest timing | IDLEG warms each CPU's instruction cache, executes sixteen TAS pairs through P1 and reads its own FRT | Authored deltas **10/10/12** for IW3=0/1/2, final TAS byte A5 and completion; native/Wasm compare both mailboxes and cold resets. Changed IW3=2→1 gives 10, changed guest byte 25→3C gives BC | This is a synthetic BIOS diagnostic, not full CPU timing or either required demonstration |

BWAIT and IDLEG now reset FRC/divider using documented FRT module stop before
each interval, so independent opcode deadlines cannot introduce a fractional
FRT phase into the goldens. BWAIT's eight CS3 reads yield **9+w**, **11+2w**,
**15+4w** for long, word and byte buses. IDLEG's warmed cached TAS sequences
yield **10/10/12** for IW3=0/1/2. Both builders derive these from authored state,
T1/T2 and turnaround counts. Endian/data/width/wait mutations and both independent
CPU traces remain enabled, including reset and browser execution.

**DSPD** links [builder and DSP sources](../tests/fixtures/build_dsp_dma.py),
[BIOS](../tests/fixtures/dsp-dma.hex), [golden records](../tests/fixtures/dsp-dma.tsv),
[native integration](../tests/dsp_dma.rs) and identical browser Wasm execution.

| Required behavior | Implementation | Independent evidence | Remaining limits |
| --- | --- | --- | --- |
| A-bus fixed-address DSP output | `Dsp::dma_beat` now accepts add0 and add1; add0 repeatedly writes the same longword and retains WA0 | `dsp_a_bus_output_fixed_increment_hold_and_register_counts_have_real_address_effects` checks add0/add1 × HOLD × immediate/M0/MC0 counts and follows with another transfer without rewriting WA0. DSPD inputs 7/11/17 then writes all three to one real RAM word, leaving its neighbor's canary intact | Full DMA arbitration/waits/device-boundary corpus remains |
| Register-count transfers | Actual DSP instructions read the counter from MC0, increment CT0, transfer work RAM into M1, and move the next M0 word into M2 | DSPD observes last fixed write=17, neighbor=ABCDDCBA and postincrement source=1234. Changing count 3→2 yields last word=0, changing data 17→53 yields 53 | Complete zero-count/counter-wrap/pipeline conflict vectors remain |
| Executed program-RAM DMA | DSP bootstrap at 0x40 uses register-count DMAH into PRAM and END; PRG destination begins at 0. SH-2 polls EX/T0 and starts the transferred code. A bootstrap at 0xA0 polls T0 and enters loaded code with DSP MVI PC; a third at 0xC0 transfers all 256 words and executes FF→00 | DSPD observes stopped PC=44, loaded-program PC=2 and result=42; changed MVI yields 99. Further authored records check the DSP branch delay slot, loaded result=57, stopped PC=C3 and PC=1/result=107 after wrap; shortening the last transfer to 1 word removes result 107 | Physical PRAM access timing and the complete pipeline inventory remain |

| Reproducible artifact | SHA-256 |
| --- | --- |
| `bsc-idle.hex` text | `520d1c5c7ecbf193e2026b84b9a02ebde9bae333008a5cb5b02cdd29581abce7` |
| `bsc-idle.tsv` | `963682704345a648e959033139e0174fbf422b0d5947a577da2015965e283c53` |
| `dsp-dma.hex` text | `fe17f7face81c0fdda685fa35701402e3fe8b8bf48637131492f7cab08196594` |
| `dsp-dma.tsv` | `d6e5b296cb234168d89c07467cd4b60735f1b251b762ffc5a6af725a2092333a` |

CI verifies both builders with `--check`. No guest computation or result is
substituted by host code. Existing demonstration media are unchanged. All unmet
CPU/bus/scheduler/reset, SCU/DSP, video/audio, CD/media, SMPC, BIOS/HLE and
complete inventory acceptance remains mandatory; these tests do not complete #17.

## Memory geometry, non-XA data and historical scanline output

| Contract / primary reference | Implementation | Independent native evidence | Wasm/browser evidence |
| --- | --- | --- | --- |
| SCSP MEM4MB, [SCSP] §4.2 table4.32/fig4.58 | `RamConfig`, `Sound::read_ram8/write_ram8`, `SoundBus`, slot fetch, SCSP DMA and DSP MRD/MWT use the same geometry. Chip0 is 128 KiB in 1-Mbit mode or 512 KiB in 4-Mbit mode; the second chip is absent. Configuration changes preserve stored bytes. | `memory_geometry_is_shared_by_main_and_sound_cpu_and_preserves_inaccessible_bytes`; `memory_configuration_controls_pcm_dma_and_executing_effects_memory_reads`; MEMG | MEMG's real 68k reads both configurations, verifies a discarded absent-chip write, programs DMA and DSP, and produces bipolar stereo. Changing its sample operands changes actual AudioWorklet peak by the authored ratio. |
| Backup selects and absent CPU/A-bus devices, [O] map / [SCU] §1.2 | `BackupRam::offset`, `Bus::unconnected`, `CpuBus::external_write`: eight backup aliases with odd lanes; absent board selects return FF bytes and discard writes; ROM CPU writes complete without a write-enable effect. Host ROM loader remains checked/read-only. Real-chip unsupported ports still fail. | `all_eight_backup_select_aliases_share_lanes_and_atomic_import_across_reset`; `absent_board_selects_return_high_data_and_cpu_rom_writes_have_no_storage_effect`; MEMG | MEMG checks eight aliases, absent cartridge data, ROM retention, repeated boot and changed sample operands. Existing BUPG import/export and reset/service tests remain enabled. |
| DIVU consecutive write/read, [H] §10.4.1 | `OnChip::instruction_break/read32/write16/write32` track instruction adjacency independently for each CPU. An immediately following read extends one cycle, subsumed by a longer active divide. Word writes preserve lanes without fabricating a read strobe. | `divu_consecutive_read_interlock_follows_instructions_not_time_and_word_writes_are_not_reads`: both CPU identities, word/long registers, 0–2 intervening instructions, 0/1/39/100 elapsed clocks, active divide and reset | Existing DIVG executes both CPU-local units and all arithmetic/IRQ records. CPUIO below adds scheduled CPU waits, real DIVU completion/IRQ boundaries and both-CPU peripheral timing guests; exhaustive instruction/pipeline evidence remains required. |
| Plain Mode2 data, [ECMA-130](https://www.ecma-international.org/wp-content/uploads/ECMA-130_2nd_edition_june_1996.pdf) §14 fig12 / §14.2; CD host view [CD] §5.4 | `Media::cue_with_mode2_format/raw_sector/decode_sector/cooked_sector`, `integrity::mode2_data`: validate sync/mode/BCD header and preserve all 2336 user bytes; no nonexistent XA EDC/ECC. Runtime 2336-byte FIFO uses the documented host layout. | `ecma_mode_two_data_preserves_all_2336_bytes_without_xa_parity_or_subheaders`: all byte fills, ambiguous subheaders, malformed header and rejected damaged XA; M2G compares every guest-copied word and sum | M2G mounts original mixed data/audio CUE, reads every byte through guest HIRQ/FIFO code, repeats reset and changes first/last payload bytes. |
| Historical scanlines, [VDP2] §§2.5/7.2/13.1 | `Bus::advance_main_clocks`, `Vdp2::scanline/finish_field/scanout/output_dimensions`: capture passed dots before video bus writes, finish rows at HBlank, publish at VBlank, retain alternate interlace rows and pair published dimensions with their pixels. | RASTG checks all 224 rows, transition-line old-color prefixes, blanking retention, next-field offset and reset. `bus::raster_write_tests` checks exact dot boundaries in normal/hires/exclusive monitors and palette/VRAM/color-offset changes. Existing static/game pixel tests wait for completed scanout. | RASTG's beam-polled bands survive later VRAM/offset writes in Wasm and production Canvas; a guest-color mutation changes only its band. Existing game/restart and advanced video checks remain enabled. |

CUE has no XA/plain sector-format bit. Auto detection inspects at most 75 stored
sectors for repeated subheaders, tolerating one damaged field. Explicit
`Mode2Format::Data/Xa` resolves ambiguous source bytes without rewriting them.
Once classified as XA, decode failure never falls back to unchecked plain data.
The bootstrap still requires 2048-byte logical sectors; raw lengths remain
available to runtime commands. This is a documented format ambiguity, not evidence
that damaged XA passed integrity checks.

The FF absent-device value is a deterministic functional policy, not a measured
floating-bus voltage or a claim that every real-chip hole is implemented.
Instruction-specific CPU/bus arbitration, refresh/WAIT ownership, remaining
memory/reset services, subscanline video fetch/cycle-pattern arbitration, exact
parameter reload strobes and other previously listed hardware requirements
remain mandatory. These changes do not complete the umbrella checklist.

The new original builders are checked in CI. SHA-256 of committed artifacts:

| Artifact | SHA-256 |
| --- | --- |
| `memory-contracts.hex` text | `10e2947b10ebd5ea149fb10acb954990bdea7c107e36b472fcb64ac662f84586` |
| `mode2.hex` text | `453bcbdf06a829313244ce5df446535d135ff18e0c42ef243de3240a73d85b27` |
| `mode2.bin` | `7503a70136dcd7c6aec44d6ac6a1bec849dd05ffb1d6a0dbd0edff751b065d9b` |
| `mode2-audio.bin` | `dee42ec6e0e7fba4a02375f99b04499fd6d481127c26c51acb31c8e20d36372f` |
| `mode2.cue` | `8832626090f6056d07589dd83e8f324db65f124863ef5fef5aaff0a259a831bf` |
| `raster-history.hex` text | `54aee2f4e0e10a6ba94f19e67ed06e5b1d7d55a187e2d546d367e1ee0d3304d8` |

FAMG now selects MEM4MB before its first stack access and again after RESET.
Its regenerated literal fault-frame records include the relocated code addresses;
all 74 family/EA vectors and ten fault records remain checked. Star Collector,
Prism Courier and Hardware Lab disc bytes are unchanged.

## VDP1 rotated/interlaced readout and rotation start reloads (RSCG)

Primary specifications: [VDP1] §1.2 pp.14–15, §4 pp.35/43–44/47;
[VDP2] §2.4 and §6.3 pp.152/158–159. All new builders, SH-2 sources,
synthetic ROMs and TSV vectors use [the fixture MIT license](../tests/fixtures/LICENSE).
These are additional diagnostics; they do not replace either required demonstration.

| Required behavior | Implementation | Independent native evidence | Actual browser evidence |
| --- | --- | --- | --- |
| Rotation16/Rotation8 sprite framebuffer readout | `Vdp2::sprite_data` uses A Xst/Yst, vertical deltas and horizontal deltas; signed 20/12-bit Q9 link precision, transparent out-of-buffer coordinates. The matrix, coefficients and parameter B do not move sprites. Sprite windows use the same transformed data. | [video_scanout.rs](../tests/video_scanout.rs): identity/quarter-turn, negative and last/outside row/column, ignored matrix/B/coefficients, discarded fractional bits, signed delta widths and rotated sprite-window pixels | [sprite_rotation.rs](../tests/sprite_rotation.rs)'s two identical [guest ROMs](../tests/fixtures/build_sprite_rotation.py) execute through [browser-smoke.mjs](../scripts/browser-smoke.mjs); authored Wasm/production Canvas boundary pixels and guest operand mutations |
| Independent start reloads | `Bus::advance_main_clocks` invokes `Vdp2::begin_scanline`; `read_rotation_line/rotation_start` hold Xst/Yst/KAst independently for A/B and accumulate current deltas. Issue #35 adds consumption of all six one-shot requests at the parameter read, including field initialization. Reserved RPRCTL bits fail. | [Rotation unit vectors](../src/vdp2/rotation.rs) now cover all 64 A/B combinations, automatic clear without a host clear, rearming, different/current deltas, signed widths, both field parities, debugger inspection and reset. RSCG guests retain no-request table writes and X/Y requests ahead of rows128/192, followed by accumulation. | Both depth guests poll real beam counters and change MMIO after line starts. All native pixels and extended browser boundary pixels use the corrected one-shot oracle. Blank-period writes cannot repaint a completed field. See [Issue #35 validation](validation-issue-35.md). |
| Double-density drawing and physical RAM | `Vdp1::plot` selects logical even/odd rows and packs Y/2 into physical framebuffer rows. CPU/erase addresses remain physical. `Vdp2::sprite_data` reads Y/2 while historical scanout retains the opposite field. | [video.rs](../tests/video.rs): both pixel depths, logical rows430–437 versus physical215–218, untouched guards and pending controls. [sprite_interlace.rs](../tests/sprite_interlace.rs) compares every pixel of the actual 320×448 guest frame and the next field. | [Original guest](../tests/fixtures/build_sprite_interlace.py) draws alternating texture rows into both buffers. Browser Wasm and production Canvas check both fields, 448-line dimensions, reset, changed DIL and the subsequent field. |
| EOS/DIE/DIL buffer-change latches | `Vdp1::vblank_out` captures draw controls only on a framebuffer change; register writes remain readable in MODR. Prohibited DIL-only settings fail atomically. | Existing HSS vectors now explicitly change buffers before drawing. Updated [HSS guest](../tests/fixtures/build_sprite_shrink.py) waits for field changes; all 28 original samples, MODR, flipped/scaled/distorted and mutation records are retained. | The existing full HSS native/Wasm/Canvas path now waits for its real latch, retaining all expected pixels and changed-HSS assertions. |

All three new synthetic BIOS images execute original SH-2 instructions and device
MMIO. Rotation guests draw polygons through VDP1 command lists, not host pixels.
Interlace uses a guest-authored alternating-row texture and both real buffers.
Changing the rotation guest's horizontal Y-delta at ROM104 changes the authored
column outputs; changing the interlace guest's initial DIL at ROM100 changes the
first physical-buffer trace and even-field pixels. Each path repeats cold reset.

The original Star Collector, Prism Courier and Hardware Lab media remain unchanged.
Physical draw/erase bandwidth, transfer-over, complete edge/effects corpus,
subscanline fetches, other table/window strobes, VRAM-cycle/bank arbitration,
half-line phase and all other outstanding Issue #17 requirements remain mandatory.
No umbrella criterion is marked complete by RSCG.

| Reproducible artifact | SHA-256 |
| --- | --- |
| `sprite-rotation16.hex` | `77d5f055037cb4ad6f90ae967497eed54178e8fe577fdec1c3aa710b0013e599` |
| `sprite-rotation16.tsv` | `81e30f69e8093bc349c452dbf75c6b2ab7504ab389b8369537a32a8033522748` |
| `sprite-rotation8.hex` | `553b8c1a1c3571bcf5f5a45894b405f8fbd32341cc0b6b68369f8b2f6b36da4f` |
| `sprite-rotation8.tsv` | `946db30774561bdcea9ddd6c93766c7dafd21cfc8b9542e459efd49c77e94b25` |
| `sprite-interlace.hex` | `8d6d795024a9c28ef2a9c7539e5aa29a0682f7991934b40d3d0720734a8642ec` |
| `sprite-interlace.tsv` | `65e193f1c8a01ce19c412c614f3de652cc12a7f8fa2eebb2e87f3ebce1184b00` |


## One-shot RPRCTL request consumption (Issue #35)

Primary contract: [ST-058-R2][VDP2] §6.3 printed pp.157–159 and the
[later HTML RPRCTL description/final paragraph](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p06_31.htm).
A request is consumed at the next parameter read; repeated line reloads require
repeated guest writes. Historical RSCG evidence above did not establish this
lifetime: its unit test manually cleared requests and its sprite oracle assumed
sticky requests. Those tests/guests now follow the primary contract while
retaining their substantive coverage. The sprite hashes above are the corrected
artifacts; interlace/record hashes and the three demonstration media are unchanged.

| Required behavior | Implementation | Independent native evidence | Production Wasm/Canvas evidence |
| --- | --- | --- | --- |
| Six independent one-shot requests | [read_rotation_line](../src/vdp2/rotation.rs) clears bits 0/1/2/8/9/10 at the existing raster latch, shared by backgrounds and sprites | Literal 10→100→101; all 64 A/B masks; table mutations, different/current deltas, consecutive accumulation/rearming, signed wraps, first line, both field parities, initial fallback/reset | [Original A/B guests](../tests/fixtures/build_rotation_reload.py) exercise no request, one request, changed operands/deltas and successive-line reissues; [browser extension](../scripts/rotation-reload.mjs) checks ten real guest records and every framebuffer/Canvas pixel for each cold boot and delta mutation |
| Request boundary ordering and debugger independence | [Bus](../src/bus.rs) keeps raster reads before equal-edge transaction completion; an emulator convention, not measured electrical timing | Both SH-2 scheduled adapters complete writes before/at/after a line read, chunks 1/3/17/10000, with/without debugger inspection; starts/pixels match; reset cancels queued/accepted writes before fresh accumulation | Guests poll actual raster counters and write real tables/MMIO; debugger-only paths never supply guest results or consume requests |
| KAst addresses and rotated sprite progression | §6.4 coefficients use latched KAst; §6.3 p.159 sprite readout uses A starts/deltas | [rotation_reload.rs](../tests/rotation_reload.rs) checks all 71,680 A/B background pixels; [sprite_rotation.rs](../tests/sprite_rotation.rs) checks both depths and all pixels after each X/Y request | Same diagnostic bytes, literal records and independently authored framebuffer/production Canvas expectations in [browser-smoke.mjs](../scripts/browser-smoke.mjs), repeated cold boots and ROM delta mutations |
| Distinguishing negative controls | No production test switch or alternate renderer | Actual unchanged-implementation failures: second Xst stays100 instead of101; guest sprite pixel(0,129) stays green instead of red | Production browser checks use the fixed core; actual native/guest negative-control details and commands are in [validation-issue-35.md](validation-issue-35.md) |

New text-fixture hashes:

| Artifact | SHA-256 |
| --- | --- |
| `rotation-reload-0.hex` | `285e6978f9b492e8e6e3d98b75a9a0fe1df6079c1f0c3e7b4855059ad18bfd5c` |
| `rotation-reload-1.hex` | `745a1cbf0dccbf128511864cd099d5a13fd36703fedf158bc267e2ceba270ac0` |
| Both `rotation-reload-*.tsv` | `0480eec1f3650982f8b1fb52e1b22f26093b020e8a5e83b627fe9d777f3c1114` |

Validation on 2026-10-24 passed formatting, locked build, all **858 native tests**
(none failed/ignored), native and Wasm Clippy with warnings denied, all **80
registered reproduction commands**, production web build, and the complete
extended browser suite in Chromium **145.0.7632.6**. No required regressions were
skipped. See [validation record](validation-issue-35.md) for the actual negative
controls, fixture reproduction and the precise historical sprite-oracle correction.
Full rotation VRAM acquisition timing, general VDP2 register-read semantics and
all other unmet #17 requirements remain mandatory. Issue #17 stays open; this
increment does not establish hardware completion or commercial compatibility.


## Clocked video transactions and exclusive monitors

**VTX**: [original MIT diagnostic builder/source](../tests/fixtures/build_video_transactions.py),
[native guest assertions](../tests/video_transactions.rs), and the same synthetic
BIOS guests in [browser Wasm/Canvas](../scripts/video-transactions.mjs), invoked by
the extended browser suite. CI verifies the three reproducible ROM fragments.
References: [VDP1] §2 (VRAM wrap and pixel clocks), §§4.2/4.4–4.8 (field change,
erase, ENDR and status); [VDP2] §2.1 p.13, tables2.1/2.3/2.4, §3.3 figs.3.2/3.6/3.7
and table3.5, §4.5 p.60 and table8.1. Expanded synthetic BIOS SHA-256:

- VDP1 progress (setup corrected for [Issue #25 capture timing](validation-issue-25.md)): `30e75e2872651275421b093131c235332fae45673940c71b300c15bf7a0b54bf`.
- Exclusive monitor: `65f0df1a183eb1dfea1ba024f0cd2f1d30d87247aaae50370126e5bd5efc7730`.
- VRAM grants: `4e984c634b4f5e600f003aad552676769921dd0a2e8081be4ae54d9c23679ea4`.

| Contract | Implementation | Independent native evidence | Guest/Wasm evidence and remaining limits |
| --- | --- | --- | --- |
| Dot progression, transparent/clipped dots, texture reads and stop | `Vdp1::raster_line/draw_dot/advance` | [progress tests](../src/vdp1/progress_tests.rs): partial pixels, texture changed between dots, 29/30-clock stop boundary, END/status/IRQ ordering, command wrap, reset cancellation | VTX observes a clipped 4096-dot line before it completes, forces stop, and compares computed status/IRQ/LOPR; guest color mutation changes only its pixel record. Full texture/edge/effect bus costs remain. |
| Erase and transfer-over | `erase_display_line/advance_erase/vblank_in/vblank_out` | Literal one/two-pixel word writes, 200-clock overhead, 58,812-word capacity, unchanged first unfinished word, field stop, both depths, scanline progression, manual cursor retention, reset/backup retention | VTX publishes authored framebuffer words, executes VBE/change, checks last erased/first untouched words, and repeats with a changed guest erase color. Subscanline display erase and late-enable timing remain. |
| Cycle words and CPU grants | `Vdp2::cpu_access_slots`, `Raster::vram_wait_clocks`, `CpuBus::observe_external` | [bank masks](../src/vdp2/access.rs), [both CPUs/cache/P1/reset](../src/bus/vram_grant_tests.rs), queued-refill waits, raster deadlines, reserved/atomic write vectors | VTX uses FRT to distinguish granted/blocked A/B ports, matching/mismatched split banks and ignored T4–T7 high-resolution slots; disabling screens releases the port. Waits still accumulate after atomic CPU execution; display-fetch bandwidth and SCU/DMA ownership remain unresolved. |
| Exclusive 320/352/640/704×480 | `Raster`, `Vdp2::background/window`, rotation display gating | Main-clock event vectors for 525/562-line fields and 852/848-clock rasters; counter/window layouts; native pixels at lines 0/479 and NBG1 scroll mutation | VTX verifies all four guest-selected modes, counters, buffer dimensions, exact adjacent RGB pixels, repeat reset and production Canvas. Full mode/effect/bus combinations and switching transient timing remain. |
| Normal-layer color-depth sharing | `normal_layer_available` | Eleven literal pixel vectors isolate NBG0/NBG1 color depth from priority/transparency and check the affected companion layers | Native decoder-boundary evidence; complete guest/Wasm boundary corpus remains required. |

These implementations address portions of the video and bus findings. They do not
establish complete Issue #17 acceptance. The outstanding CPU, scheduler, BSC,
68k microsequence, SCU/DSP, sound/DSP, CD/media, HLE and SMPC findings remain;
no umbrella criterion is checked off by this evidence.


## Independent SH-2 issue deadlines and clocked on-chip DMA

**TIMING** is the original MIT [builder](../tests/fixtures/build_sh2_timing.py),
[synthetic BIOS](../tests/fixtures/sh2-timing.hex), [golden states/results](../tests/fixtures/sh2-timing.tsv),
[native integration](../tests/sh2_timing.rs) and identical real browser Wasm guest
in [browser-smoke.mjs](../scripts/browser-smoke.mjs). The primary references are
[P] §§7.3–7.7/table7.2 and [H] §5.5/table5.9, §7.4 fig7.7, §7.9 fig7.47,
§14.3/table14.3. Source/ROM/vectors are covered by the [fixture MIT license](../tests/fixtures/LICENSE).

| Required behavior | Implementation | Independent native evidence | Wasm/guest evidence and limits |
| --- | --- | --- | --- |
| Base instruction states | [cpu/timing.rs](../src/cpu/timing.rs) `execution_states`; `Cpu::last_clocks` | Literal register/ALU/memory/system vectors, every displacement and T polarity for four conditional branches, all non-single-state families | TIMING warms seventeen actual kernels on both CPUs. FRT ticks/results: ADD 4/32, LDC.L 12/0, taken BF 5/8, untaken BF 3/8, delayed BT 4/8, STC.L 2/0. Replacing 16 ADDs by eight BRA/NOP pairs gives 5/16. Native and Wasm repeat cold resets. These are no-contention states, not full pipeline conformance. |
| Load WB/EX interlock, [P] §7.5/fig.7.9 | `LoadPipeline::issue`, `Cpu::execute/enter_exception` | All GPRs and byte/word/long, indexed, PC/GBR/displacement loads; source/destination conflicts, repeated loads, MAC forwarding exception, separated successors and exception flush | TIMING adds sixteen dependent load/ADD pairs (6 FRT ticks/result 1), independent pairs (4/0), repeated loads (4/0), and eight separated triples (3/1), on both CPUs in native/Wasm. Multiplier/IF/MA overlap remains required. |
| Independent CPU deadlines | [Machine::step_inner](../src/lib.rs), `cpu_ready`, [Bus::cpu_delay](../src/bus.rs) | `different_opcode_deadlines_allow_the_other_cpu_to_issue_independently`: cached NOP and BRA/slot produce authored per-clock retirement trace; `one_cpu_external_waits_do_not_charge_the_other_cpus_deadline`: master operand waits while slave retires four NOPs | Both TIMING processors execute independently. BWAIT/IDLEG preserve independent wait and byte-lane goldens. BUSG/DMABG now defer external CPU/DMA data phases and retain TAS ownership. Multiplier/IF/MA hazards and prefetch remain required. |
| External cycle occupancy and clocks | `CpuBus::observe_external`, `Bus::cpu_delay`, `SystemClock::deadline` | T1/T2 per physical beat; cache hits/arrays do not reserve external clocks; narrow/refill bus tests and original FRT guests; PLL vectors | BWAIT long/word/byte results above; IDLEG TAS minimum gap and mutations. CPU interval is max(base states, total external T1/T2 occupancy)+waits. This explicit functional serialization does not implement IF/MA pipeline overlap or all board protocols. |
| Clocked on-chip DMA | `Bus::prepare_dmac/grant_dmac/advance_bus_transaction`, `cpu_dma_errors` | `onchip_dma_progresses_on_clock_deadlines_without_cpu_issue_and_is_chunk_invariant`: no write before first deadline, actual transfer/count/TE, identical chunks 1/3/7/64 and no errors | Existing SH7604/DACK, DIVU and Hardware Lab guests retain DMA/IRQ assertions. DMABG adds both-CPU native/Wasm, actual BSC waits and unit/block ownership; complete request/refresh/display/sound arbitration remains required. |
| Exception and reset boundaries | `Cpu::enter_exception`, `Machine::reset`, timed SMPC/WDT paths | H table5.9 acceptance interval; existing address-error, slot, FRT, standby, watchdog and reset guests, independent deadline reset test | Original Wasm fixtures preserved. Complete interrupt synchronizers, memory-cycle exception sequencing and remaining reset domains remain mandatory. |

Clock-transition checks bound actual emulated time (two 100 ms transitions plus
setup), then scan complete fields before inspecting historical video. The sound
fixture's new cross-clock phase separates its power-on pending timer from the
programmed FD→FF edge (basic sound guest updated in #41): it executes two three-instruction IRQ handlers before
STOP (30 retired instructions), with separate guest IRQ counter=2 and unchanged
master first-result=1, waveform and exact stereo samples. No media/demo source
or generated demo bytes are changed by this scheduler revision.

| Reproducible artifact | SHA-256 |
| --- | --- |
| `sh2-timing.hex` | `0c77afd229efd034495c91cf8e810e45729518a347db325ef9a50b265636d699` |
| Expanded 512 KiB TIMING BIOS | `36ffed3fb5cf4489de57b096f3f0ca5fb1736b2032825d99fe058222c3c1bb6d` |
| `sh2-timing.tsv` | `4c6773d1b816b69e25ce98a511fa6d17a885a2009326876717b8764031f184b2` |

The new evidence removes fixed CPU issue turns; it does not resolve the remaining
pipeline/bus, CPU/peripheral/reset, SCU/DSP, video, SCSP, CD/media, HLE and SMPC
requirements or comprehensive umbrella acceptance. All remain mandatory in #17.

## BSC sharing and ISO/XA bootstrap and runtime files

Primary references: [H] §§7.2.1, 7.8, 7.10.3–7.10.4; [DISC] §3.2.4,
tables3.10–3.11 and §§4.1/4.3/4.5–4.7; [CD] §§6.2.1–6.2.3/table6.1,
§6.8 and §8.2.8; ISO9660 §9.1. All three new fixture builders and their
outputs use the adjacent [MIT license](../tests/fixtures/LICENSE). The discs
embed that license as a directory file. No security/area object or firmware
is included, and no host code writes a running guest's results.

| Behavior | Implementation | Independent native evidence | Same guest in browser Wasm |
| --- | --- | --- | --- |
| Valid BSTROM/PSHR initialization | [Bsc::write/shared_access/monitors_external_bus](../src/onchip/bsc.rs); [CpuBus::observe_external](../src/bus.rs); [OnChip::observe_access](../src/onchip.rs) | All four bit combinations and both MD5 pin states; private turnaround survives another shared owner; key/lane/reset tests retained | [BSC sharing guest](../tests/fixtures/build_bsc_share.py), [native integration](../tests/bsc.rs), [browser suite](../scripts/browser-smoke.mjs): both CPUs exchange real RAM writes, test external versus local UBC matches, and repeat reset. Changing only PSHR changes both external flags from 3000 to 7000; local flags stay 9000. This sharing fixture alone does not establish nibble sequencing; Issue #23 below adds fixed 16-bit CS0 and independently timed longword groups. |
| XA and extended-attribute metadata | [File::from_record](../src/cd.rs) | Authored unit/gap/file-number/attribute bytes, checked data-start offset and physical volume/track bounds; hidden ISO flags do not become reserved public attribute bits | [Filesystem guest/media](../tests/fixtures/build_cd_filesystem.py), [native execution](../tests/cd_filesystem.rs), browser: reads 12 actual FIFO bytes identifying FAD172, size6144 and metadata02020728. The authored raw XA Form1 disc includes EDC/P/Q, attribute block and independently numbered gap sectors. |
| Runtime interleaved data and offsets | `File::sector`, `CdBlock::drive_frame`, ReadFile in [cd.rs](../src/cd.rs) | [120 unit/gap/size combinations](../tests/cd.rs) include empty files, invalid unit=0/gap>0, last physical sector and out-of-volume gaps | Guest sums all payload words through real commands/HIRQ/FIFO; BB attributes and EE gap bytes are excluded. Changing MOV #0,R6 to MOV #1,R6 selects two sectors and removes exactly 1111×1024 from the checksum. Both reset repetitions agree. |
| File information windows | `CdBlock::directory_sector`, commands71/72/73 | [300-child directory](../tests/cd.rs): exactly254 children plus self/parent, ReadDir window256–301 with46 children, absent FID rejection; invalid parameters retain the previous window, while interrupted accepted acquisition invalidates it (Issue #21); uninitialized scope rejects as specified in [CD] §6.2.2 | Existing original directory BIOS still runs cooked and clean/correctable/uncorrectable raw inputs. Browser checks rejection status after failed initialization rather than interpreting undefined response fields as an empty valid table. The large-window boundary corpus is native evidence. |
| Filesystem selector/partition effects | `CdBlock::prepare_file_filter`, directory/read-file commands | Existing sector/partition/native file tests; selected destination is cleared, true route returns to its own partition, false route disconnects, FAD/file-number filtering is enabled | Filesystem guest starts a second read into the same partition without deleting the first read's stored sectors: resulting count is1, not4. It reads filter mode41, connectors03FF and CD connection0300 through actual commands. Full selector connection constraints and transfer overlap timing remain required. |
| Multi-area bootstrap and independent stacks | [disc::parse](../src/disc.rs); [hle::install](../src/hle.rs) configures the original reset literal pool from IP metadata | [Disc vectors](../tests/disc.rs) reject duplicate/invalid area strings and misaligned, IP-overlapping or first-read-overlapping slave stacks | [Boot-layout disc](../tests/fixtures/build_boot_layout.py): one/two/four area blocks select AIP at2E20/2E40/2E80; guest master/slave report FF000 and FD000/default1000 stack offsets, respectively. Reordered area symbols, changed fourth-sector data and repeated cold resets run in native and Wasm. Commercial region/authentication behavior is not claimed. |
| Interleaved first-read bootstrap | `FileExtent::from_record/sector_at/bytes` in [disc.rs](../src/disc.rs) | Checked logical/physical extent bounds; contiguous and interleaved files read only their required data sectors; interleaved attributes occupy a whole first unit before its gap | Boot-layout guest uses unit2/gap1, a two-sector attribute unit followed by the initial gap and four logical data sectors. Real SH-2 loads from the third/fourth loaded sectors compute99999999; a changed disc word changes that sum. The AIP and both processors execute ordinary guest instructions. |

| Original artifact | SHA-256 |
| --- | --- |
| `bsc-share.hex` | `dcae6692bd0661a8f694d4ebe90351b0bf08f3330f2025ccb0e5f1fe412383bb` |
| `cd-filesystem.hex` | `4d4659d3fe740a60893340cc86e37279a9e73d4d7473d33f4766f87ebcc67260` |
| `cd-filesystem.bin` | `5c67809a98b8678a507a5fb5192ae89baca3278a3b9000d72f03c2ff521d8d3b` |
| `boot-layout.iso` | `16ccece8c58c3eb8e4620318f6763cc057a84f619eb2d2478b894b20c31c3aed` |

These changes implement specific BSC, filesystem and bootstrap contracts. They
do not establish the remaining BSC/MCR or board protocols, transaction
arbitration/locking, CPU/peripheral/pipeline/reset, SCU/DSP, video, SCSP/DSP,
CD timing/connection/lifecycle, SYS_EXECDMP/CD-library or SMPC requirements.
All original Issue #17 acceptance criteria remain mandatory; no umbrella
criterion is marked complete by these fixtures.

## SCU DSP B-bus cycles and program prefetch

**DSPBUS** is the original MIT [guest builder](../tests/fixtures/build_dsp_bus.py),
[synthetic BIOS](../tests/fixtures/dsp-bus.hex), [authored word vectors](../tests/fixtures/dsp-bus.tsv),
[native integration](../tests/dsp_bus.rs) and identical guest in the extended
[browser Wasm suite](../scripts/browser-smoke.mjs). Sources and generated media
use the adjacent [fixture license](../tests/fixtures/LICENSE). References:
[SCU] §4.1 (bus/RAM widths), §4.3/fig.4.2 (prefetched successors),
§4.5 pp.134/136/138/140 (16-bit B-bus writes) and [SCUFINAL] #27 (B-bus input add2).

| Contract | Implementation | Independent evidence | Limits |
| --- | --- | --- | --- |
| Separate B-bus grants | `Dsp::dma_beat` retains an in-flight word across two external 16-bit cycles. Output samples DSP RAM once; input publishes RAM/count/address updates after the second successful cycle. | [Bus-spy vectors](../src/scu/dsp/tests.rs) check exactly one read/write per grant, eight output strides × HOLD, CT63→0 wrap, changed data between halves, partial faults and a second-half destination outside the allowed region. | BUSG now grants each SCU/DSP data phase with BSC/A-bus/VRAM waits. Full DSP bank-conflict/refresh/display/sound arbitration remains required. |
| Hardware deadlines, preemption and reset | `Bus::advance_main_clocks` and `Scu::advance_dma` retain the partial DSP word while another DMA channel owns grants; reset cancels it. | `dsp_b_bus_halfwords_progress_on_separate_deadlines_and_reset_cancels_the_tail` in [bus.rs](../src/bus.rs): authored snapshots at clocks7/8/15/16, chunks1/3/8/17; `higher_dma_channel_can_preempt_between_dsp_b_bus_halves_without_losing_the_word` in [SCU tests](../src/scu/tests.rs): actual RAM copy/IRQ between halves. | CPU/DMA transaction ownership, BSC/VRAM waits and prohibited concurrent-start behavior remain required. |
| Prefetched program versus PRAM DMA | `Dsp::step` latches the successor word. DMA writes update PRAM without changing that fetched word; LE and stopped host program writes invalidate the latch. Pause retains it. | `program_dma_cannot_replace_the_prefetched_successor_and_le_refetches_it`: actual PRAM DMA changes MOV7 to MOV9; the fetched instruction writes7, explicit LE writes9. `program_dma_starts_at_zero_and_full_count_wraps_without_changing_the_host_port` replaces the earlier incorrect host-cursor wrap oracle: it transfers 256 words starting at 0 and executes loaded instructions across FF→00. Existing jump/BTM/LPS/step and executed-PRAM guests remain enabled. | This establishes a one-successor functional prefetch, not the complete pipeline behavior or physical RAM-port timing. |
| Guest-visible B-bus routes | DSPBUS uses all eight output stride encodings into VDP1 RAM and real sound-RAM input, with CT wrap and polling of EX/T0. Both SH-2 and DSP execute ordinary guest instructions. | All **30** authored words agree in native and browser Wasm, repeated after cold reset. Changing eight guest data literals changes only authored data records; changing add1 to add0 overwrites the high half and leaves the next word's canary intact. | These are diagnostic fixtures, not substitutes for either required demonstration or complete SCU acceptance. |

| Artifact | SHA-256 |
| --- | --- |
| `dsp-bus.hex` | `a0036769763632ffa83e7595df7e7fa956da923e70f6c55aacd809fd5c95ec7c` |
| `dsp-bus.tsv` | `b9e7e5a52a3ef8c11b89e57a62f093af6f9650b9725d42847d4d8ff61c0014a0` |

Star Collector, Prism Courier and Hardware Lab retain their existing source and
media. The remaining CPU/peripheral/bus/reset, SCU/DSP, video, sound, CD/HLE,
SMPC and comprehensive acceptance requirements of #17 remain mandatory.


## Memory-interface, indirect-descriptor and SMPC boundary execution

**BCMEM** is the original MIT [memory-interface BIOS builder](../tests/fixtures/build_bsc_memory.py),
[ROM](../tests/fixtures/bsc-memory.hex), [authored timer/data vectors](../tests/fixtures/bsc-memory.tsv)
and [native guest](../tests/bsc_memory.rs). Both CPUs execute twelve cold-boot
configurations. [Browser Wasm execution](../scripts/hardware-boundaries.mjs)
compares the identical records, resets each configuration and changes guest data
operands to reject canned output. These guests program BCR1/MCR during initial
CS0 execution, before touching other areas. No SDK or firmware is used.

**SMPB** is the original MIT [SMPC boundary BIOS builder](../tests/fixtures/build_smpc_boundaries.py),
[ROM](../tests/fixtures/smpc-boundaries.hex), [authored packets](../tests/fixtures/smpc-boundaries.tsv),
[native guest](../tests/smpc_boundaries.rs) and identical browser Wasm guest in
[hardware-boundaries.mjs](../scripts/hardware-boundaries.mjs). Native execution
repeats both NTSC and PAL resets. Input changes target SMPC devices only.

**IDESC** identifies the register/bus vectors in [scu/dma.rs](../src/scu/dma.rs)
and `bus::bsc_memory_tests` in [bus.rs](../src/bus.rs). Existing **SCUROM** and
Hardware Lab continue to execute real DMA and IRQ handlers in native and Wasm.

| Contract / primary specification | Implementation | Independent executable evidence | Remaining limits |
| --- | --- | --- | --- |
| Six BCR1 memory selections and MCR SZ, [H] §§7.2.1/7.2.4 | `Bsc::interface/bus_width/access_timing`, `CpuBus::external_read_group` | `each_documented_memory_selection_uses_mcr_width_only_in_its_selected_areas`; BCMEM actual word/long accesses on both CPUs | Full board multiplex/address-pin/lane corpus and SDRAM mode-register commands remain required. |
| SDRAM reads and writes, [H] §§7.5.3–7.5.6, figs7.15–7.24/table7.4 | One activation/CAS interval per operand/cache burst; discarded read tail still occupies clocks; CS3 independent active banks; CS2 auto-precharges | `sdram_read_latency_and_empty_burst_tail_match_figures_7_15_through_7_17`; `sdram_banks_hold_independent_rows_and_cs2_is_always_auto_precharged`; `cache_line_is_one_sdram_burst_and_hits_issue_no_external_cycles_on_either_cpu`; BCMEM CL1–4, word/long, active-row guest records | CL4 is the manual's BSC latency setting for an intervening latch, not a claim about direct SDRAM CL4 support. CPU transactions still commit at instruction issue; complete shared arbitration remains missing. |
| DRAM page / pseudo-SRAM static-column timing, [H] §§7.6.3–7.6.5/7.7.2–7.7.4, figs7.31/7.34/7.38/7.41 | `Bsc::access_timing` uses TRP/RCD/WCR and BE/RASD row/group boundaries | `dram_page_and_pseudo_static_column_modes_observe_group_and_row_boundaries`; BCMEM word/long interfaces, page access, active row and computed RAM values | External WAIT pin sampling and complete board interface remain required. |
| Refresh and reset, [H] §§7.5.7/7.6.6/7.7.5 | Timer compare requests block subsequent accesses, close open rows and preserve one pending request; slave does not drive refresh; self-refresh suppresses timer requests and has exit recovery | `refresh_owns_real_bus_states_closes_rows_and_slave_never_drives_refresh`; `self_refresh_suppresses_timer_requests_and_exit_has_documented_recovery`; `sdram_bank_comparators_see_actual_pins_before_work_ram_mirror_decode` includes retained manual-reset configuration and closed rows | Refresh/CPU/DMA bus ownership and transaction cancellation remain required; forbidden accesses during self-refresh are not compatibility evidence. |
| Indirect descriptor bus cycles, [SCUFINAL] #25 / [SCU] §3.2 | `Table::fetch`, `Channel::beat/pending/cancel`: count/write/read on separate grants; retained partial table, no source transaction before complete descriptor | `indirect_descriptor_fetches_one_word_per_grant_and_never_resamples_completed_words`; `higher_channel_preempts_a_partial_table_and_low_channel_retains_sampled_count`; `indirect_dma_table_reads_have_separate_deadlines_and_reset_cancels_a_partial_descriptor` checks clocks8/16/24/32/40/47/48 under chunks1/3/8/17 | Eight-main-clock grant cadence remains a functional model; BSC/VRAM/SCSP waits and CPU locks are not integrated. |
| Descriptor bounds/count/error, [SCUFINAL] #24/25 | Whole 12-byte table bounds/alignment; twenty-bit indirect count on all channels; zero=1MiB; end marker in source bit31 | `indirect_table_alignment_and_whole_descriptor_bounds_fail_before_bus_access`; `partial_table_fault_and_cancel_never_publish_a_transfer_or_leak_latched_fields`; `every_indirect_channel_masks_twenty_bit_count_and_zero_means_one_megabyte` | Count decode vectors do not claim every maximal transfer/device combination executes correctly. |
| INTBACK zero-byte mode, [SMPC] figs3.11/3.13/table3.4 | `Smpc::peripherals` omits the entire disabled port record and packs later port data | `zero_byte_modes_pack_only_selected_ports_for_both_digital_and_analog_reports`; SMPB parses independently expected digital/ID16 packets, both single-port omissions and SR/SF | Standard built-in ports only; no optional multitap acceptance claim. |
| BREAK / collection timeout / route changes, [SMPC] §3.2/table3.2, §3.3, §3.4/table3.37 | `Smpc::write/end_peripherals/vblank_in`: cancel status-continue, VBlank wait or active collection; timeout invalidates optimization; DDR/IOSEL/EXLE disconnect cancels pending direct analog response | `intback_break_cancels_every_collection_phase_without_later_irq_or_stale_output`; `vblank_timeout_discards_incomplete_collection_and_remeasures_next_request`; `direct_analog_route_changes_cancel_pending_edges_only_on_the_affected_port`; SMPB executes all three BREAK phases in native/Wasm | Remaining system-control/reset/clock/latch and controller mode-switch boundaries remain mandatory. |

Expanded BIOS SHA-256 values (524288-byte images, zero-filled omitted words):

- BCMEM: `b514527ef5990dbfb85f6eb80b9c1452addef841df430e56de12df62e2779719`.
- SMPB: `e20403f0a3b029619abf3f729373410b6645974f62feeb47fb06b772f4187fcf`.

Both new synthetic BIOSes use [the fixture MIT license](../tests/fixtures/LICENSE).
These changes address concrete portions of review findings 1/3/4/11/12. They do
not resolve all review findings or satisfy Issue #17's full acceptance checklist.
The outstanding MC68EC000 sequencing, complete CPU/shared transaction scheduling,
SCU/DSP, video, SCSP/audio, CD/HLE and remaining SMPC contracts are unchanged
requirements. No completion checkbox or Issue state is changed by this evidence.


## MC68EC000 operand cycles and guest-executed CD player

**68BUS** identifies [the original operand-cycle vectors](../src/m68k/bus_tests.rs)
and the expanded [sound-bus guest](../tests/fixtures/m68k_faults.py),
[authored frames](../tests/fixtures/m68k-faults.tsv),
[native integration](../tests/m68k_families.rs) and identical browser Wasm execution
in [browser-smoke.mjs](../scripts/browser-smoke.mjs). The guest now records twenty-four
complete access-error frames. Ten cases execute destination-read faults
for Scc/MOVE SR and source faults in every size of ADDX/SUBX plus ABCD/SBCD.
Four further cases exercise the MOVEM terminal-read sequence documented below.

**DPLAYER** is the original [SYS_EXECDMP guest service](../src/hle/player.s),
[font/build source](../src/hle/build.py), [caller/media builder](../tests/fixtures/build_hle_player.py),
[native tests](../tests/hle_player.rs) and [browser tests](../scripts/hle-player.mjs).
The work-RAM pointer is installed only by [HLE startup](../src/hle.rs), never by
ExternalBios. The guest retains its own state at 06000F00 and its actual FIFO-read
TOC at 06000D00; host code only observes results or supplies controller/media inputs.

| Contract / primary specification | Implementation | Independent native and guest/browser evidence | Remaining limits |
| --- | --- | --- | --- |
| Scc memory destination read, [68] p.4-173; [68UM] table8-6 | `M68k::execute` reads the destination byte before writing, with one EA calculation | `scc_and_move_sr_read_the_destination_before_writing_with_one_ea_update` checks indirect/postincrement/predecrement, widths, CCR, fault frame and suppressed writes. `scc_all_conditions_and_register_forms_preserve_ccr_and_never_read_memory_data` checks 16 conditions × 32 CCR values × register/memory. 68BUS native/Wasm stacks source-authored read-error frames | Instruction-specific prefetch placement, bus arbitration and exact cycle placement remain required. |
| MOVE SR memory destination read, [68] p.4-125; [68UM] table8-12 | `M68k::group4` performs the otherwise unused word read before writing SR | 68BUS checks successful word cycles, one EA update, preserved CCR and read-error SSW/IR. Native/Wasm mutations replace it and Scc with ordinary MOVE writes: only the authored SSW/IR fields change | These operand-cycle tests do not establish all MC68EC000 microsequences. |
| Extended memory arithmetic operand order, [68] ABCD/ADDX/SBCD/SUBX entries; [68UM] §§5.1/5.4.1/6.3.9.1 | `M68k::alu` completes source read before decrementing or reading destination | Bus spies check byte/word/long cycles, both longword fault positions, retained address updates and low-word-first writes; alias tests use all eight address registers, including A7 byte stride. 68BUS executes source faults on the real sound bus in native/Wasm | Shared sound-bus arbitration and full per-instruction pipeline/timing evidence remain required. |
| SYS_EXECDMP, [System Program](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-162-R1-092994.pdf) §2.2 function6.1 | Pointer 0600026C -> original SH-2 ROM18000; independent stack; guest SMPC/clock/CD initialization; non-returning CD player | DPLAYER caller leaves an unfinished TOC transfer and INTBACK continuation, changes PLL and calls through the ABI. Native/Wasm check no return, real TOC tracks, persistent power-clear/backup, cold reset and absent HLE installation in external BIOS mode | Complete CD-library/system/startup ABI, all hardware-state entry combinations and reset/timing contracts remain mandatory. Original menu graphics are not Sega firmware. |
| Player controller packets, [SMPC] §3.2/tables3.19/3.37 and [STN43] | Guest INTBACK reads common button bytes for digital/ID15/ID16; analog X generates left/right edges with neutral between | Native ID15/ID16 tests and synthetic browser Gamepad API test held axes, neutral, data-track skipping and actual A/B packet controls; keyboard A/Start and direct B inputs also execute | Complete SMPC hardware acceptance still requires the remaining system/reset/latch edges in its inventory. |
| Player video/audio/media, [VDP2] bitmap mode, [CD] §§3.4/8.2 and [SCSP] table4.31 | Guest glyph rasterizer, RGB555 cursor state, TOC FIFO/END transfer, play/pause/resume/end, EXTS stereo routing | Native compares every rail pixel and exact original stereo pairs. Wasm/production Canvas match authored pixels. AudioWorklet instrumentation checks rendered nonzero stereo and expected peak; changed audio changes samples; data-only media stays silent | The player is a system-service diagnostic; it does not replace either required demo or complete outstanding chip behavior. |

All player code, font/audio assets, caller disc and generated fixtures are original
MIT works under [the HLE license](../src/hle/LICENSE) and
[the fixture license](../tests/fixtures/LICENSE). No firmware/SDK routines,
third-party core, game detection, host-written running guest state or canned
frame/audio results are used. The [provenance record](homebrew-provenance.md)
identifies the build commands. Reproducible hashes:

| Artifact | SHA-256 |
| --- | --- |
| HLE player ROM fragment | `435c8ea8c4e6eb8378d3f8b2f7daf537f96fbf8796c9c9ad830c7a33dd35d6f9` |
| Caller ISO, with embedded MIT license | `6c3b3e4522fbd244e30c54d29f4c70271530ef73c338cf2a22a06b5d71f47727` |
| Caller CUE | `60386b4595cbd15efb0f8e1665ed35c9354251616e9fc5ca552d88ed5cbe63db` |
| Original stereo BINARY tracks | `5d1dc5665f447cb6370f674918e0e6042aba8a6e8bc42f0952ddef80a394338e` |
| Expanded 512 KiB 68BUS synthetic BIOS | `fdfbc9457de3504e4a979ab29f26a22ae7b5b2f0c1f543d67161dd75f8df6733` |

These are additional implemented contracts, not complete Issue #17 acceptance.
All remaining CPU/SH7604, shared bus/scheduler/reset, SCU/DSP, video, sound,
CD/HLE and SMPC requirements remain on this same Draft PR and Issue. No umbrella
checkbox is checked, and no tracker is closed, replaced or reopened.


## SCU A-bus timing register evidence

The [original MIT ABUS synthetic BIOS builder](../tests/fixtures/build_abus.py),
[ROM](../tests/fixtures/abus.hex) and [authored vectors](../tests/fixtures/abus.tsv)
use [SCU] §3.2 pp.62–72, figs.3.24–30 and tables3.10–24.
[Native integration](../tests/abus.rs) and [production Wasm](../scripts/abus.mjs)
execute cached SH-2 kernels against the unpopulated CS0 select; physical data
pins read high. There is no synthetic cartridge or host-injected result.

| Behavior | Implementation | Independent evidence | Limits |
| --- | --- | --- | --- |
| ASR0/ASR1/AREF write-only programming | `Scu::write32`, [abus.rs](../src/scu/abus.rs) | Both CPU views accept documented fields; reads and reserved AIAK writes remain errors | Standard console external WAIT is inactive; external device pin/stretch conformance remains required |
| Normal/burst wait fields, narrow bus, sequential preread and boundary, precharge | `Abus::access/advance`, `CpuBus::reserve_external`, `Bus::grant_bus_transaction` for SCU/DSP DMA | Literal four-select/width/wait/boundary vectors; ABUS FRT ticks 4/10/24/10/6/6, sampled words FFFFFFFF, reset repetition; changing NW3 to NW4 gives 12 ticks in native/Wasm | BUSG now defers CPU/SCU data publication until bus completion; full display/on-chip-DMAC ownership and bus-lane/prefetch contracts remain required |
| Refresh and DMA deadlines | `Abus::request_refresh`, master `Bsc` refresh output, `Bus::advance_main_clocks`, `Bus::grant_bus_transaction/advance_bus_transaction` | Refresh enable/disable, all 16 wait lengths, elapsed precharge; existing DMA/DSP guest assertions remain enabled | SCU data phases now include BSC/A-bus/VRAM waits before completion; full refresh/display/on-chip-DMAC arbitration and external WAIT timing remain required |

`abus.hex` SHA-256 is `c1c8fbe99eeacb29172bfe50a0b90a9c5e0c7ed83cad6d985d7f586c5ad1b7a0`;
`abus.tsv` is `9802fb7594d1d344b2676e786d47651f72a043df4e21588d6197aff6de926d36`.
The fixture source and generated data use the adjacent [MIT license](../tests/fixtures/LICENSE).

## MC68EC000 MOVEM load bus sequence

Related to #17. [MC68000UM][68UM] §8.9/table8-10 specifies one additional
read cycle for the memory-to-register direction, independent of mask population
and word/long size. [M68000PRM][68] MOVEM defines register order, sign extension
and the postincrement base-register override. UM §6.3.9.1/fig.6-7 defines the
access-error context. This revision implements the terminal operand-word read
before the following instruction fetch. It does not establish complete
instruction-specific sequencing for every other opcode.

| Contract | Implementation | Independent native evidence | Guest/Wasm evidence |
| --- | --- | --- | --- |
| Operand burst followed by a terminal data-word read, including empty masks | `M68k::movem` discards the extra word's data while retaining device effects/errors; no extra base update | `movem_load_reads_the_terminal_word_before_fetching_the_next_instruction` in [bus_tests.rs](../src/m68k/bus_tests.rs) checks complete bus traces for empty/single/sparse/all-register lists, both sizes, read counts, order and unchanged SR | [Original fault programs](../tests/fixtures/m68k_faults.py) read the last connected sound-RAM word/long, then fault at 080000; an empty list also faults. Native and browser compare all 24 authored frame records |
| EA extension fetches precede the burst; the following instruction fetch waits for the terminal read | `M68k::next_word` honors the MOVEM load's deferred-prefetch state; `movem` refills after successful completion | `movem_load_ea_extensions_precede_operands_and_terminal_read_has_no_base_update` checks indirect, displacement, index, absolute word/long and both PC-relative modes | Existing SIOG and FAMG MOVEM fault PCs now point just past the consumed mask rather than an unissued successor fetch; native/Wasm check the literal assembled boundaries |
| A terminal BERR retains completed loads and reports a data-read fault | `M68k::read/movem` preserves register writes and the completed postincrement pointer; error dispatch uses the real faulting word address | `movem_terminal_read_fault_preserves_every_loaded_register_and_base_progress` checks all eight base registers, five masks, both sizes, loaded base override, complete SSW/address/IR/PC fields and absence of successor fetch | Long, word, empty and listed-base terminal cases execute through the real sound bus and recover through the guest's vector-2 handler/RTE |
| Operand-dependent result and reset | No game/fixture detection or host-written running guest state | [m68k_families.rs](../tests/m68k_families.rs) changes the longword terminal case to a word: its final read moves from disconnected 080000 to connected 07FFFE, so only that frame disappears | [browser-smoke.mjs](../scripts/browser-smoke.mjs) repeats the same mutation in actual Wasm, checks all other records and repeats cold boot |

Source, generated hexadecimal ROM and TSV vectors use the existing
[fixture MIT license](../tests/fixtures/LICENSE). Reproduce with
`python3 tests/fixtures/build_m68k_families.py --check` (already required in CI).
Expanded ROM SHA-256:
`fdfbc9457de3504e4a979ab29f26a22ae7b5b2f0c1f543d67161dd75f8df6733`.
The three demonstration discs and all other guest artifacts are unchanged.
`movem_following_program_fetch_fault_occurs_after_all_operand_reads` separately
checks completed loads, the terminal read and subsequent program-fetch failure,
including program function codes and its distinct internal PC.
CPU/DMA bus arbitration and the other mandatory umbrella requirements remain
required; no Issue #17 acceptance checkbox is claimed complete by these vectors.

## Deferred CPU/SCU transactions and SDRAM mode writes (BUSG)

Related to #17. Behavioral references: [SH7604][H] table4.6 note1,
§§7.3–7.9/fig.7.27 and §8.4; [Programming Manual][P] TAS.B;
[SCU] §§2.1/3.2/4.5 and [SCUFINAL] #24/25. These changes address
transaction publication and selected memory/DMA findings; the umbrella acceptance
criteria remain mandatory.

| Contract | Implementation | Independent native evidence | Original guest / Wasm evidence |
| --- | --- | --- | --- |
| External reads sample and writes publish only after a grant's waits | [CPU journal and arbiter](../src/bus/transactions.rs), `Machine::issue_cpu`; interpreter replay reuses completed I/O and sampled values | `queued_reads_sample_at_completion_and_waiting_writes_never_publish_early`, CD FIFO replay without double consumption; `stalled_cpu_write_is_invisible_to_peer_until_its_data_phase_and_reset_cancels_it` in [lib.rs](../src/lib.rs); existing BWAIT/BCMEM/SH-2 timing vectors remain unchanged | [BUSG builder](../tests/fixtures/build_bus_transactions.py), [native](../tests/bus_transactions.rs), [production Wasm](../scripts/bus-transactions.mjs): two separately executing CPUs protect a shared counter, overlapping-critical-section detection, loop mutation gives 34 instead of 64, repeated cold resets |
| TAS retains ownership through its write; cancellation drops outstanding work | `Transactions::lock`, `Bus::cancel_cpu_transaction`, watchdog/SSHOff/system/cold reset domains | `tas_owns_read_write_interval_and_cancellation_releases_a_waiting_peer`, `tas_blocks_dma_grants_until_write_completion_and_instruction_release`, pending/granted slave-write cancellation, preserved-RAM SYSRES vectors and bounded long-DMA yielding | BUSG runs real TAS, shared RAM and peer polling under independently programmed wait states; no host-written guest results |
| SCU descriptor/data phases share CPU ownership and charge waits before publication | [SCU cycle preparation/completion](../src/scu/transactions.rs), `Bus::grant_bus_transaction`; BSC plus A-bus and VDP2 access-slot waits; device waits extend busy occupancy instead of becoming false idle time | All three channels, direct/indirect, literal pre-completion data/IRQ checks, chunks 1/3/17/64; existing descriptor/A-bus/B-bus/reset tests now use completion deadlines | BUSG executes indirect channel0 and direct channels1/2, guest IRQ handlers, work/sound/VDP2 RAM transfers and concurrent slave reads; native/Wasm compare six transferred words and all three IRQ counters |
| DSP DMA samples CT/output at grant and retains a word across B-bus halves | `Dsp::grant_dma_cycle/dma_beat`, `Dma::latch` | `dsp_output_latches_at_grant_before_later_host_writes_and_keeps_both_halves`; existing DSP halfword/preemption/program-RAM vectors | Existing DSP/DMA and Hardware Lab native/Wasm guests retain their authored arithmetic, program-load and stereo expectations |
| SDRAM MRS uses address pins, ignores word data and precharges both banks | `Bsc::mode_states/write_mode`, `CpuBus::write16`, `Operands::write`; Tp/Tmw takes 2 or 3 main states according to TRP | Six documented encodings at both A0 values, both CPUs and TRP settings, row/burst invalidation and invalid-write atomicity; MRS is exempt from word-alignment address errors | BUSG executes all six MRS encodings, including odd addresses, on both CPUs; each records six completed writes with no address exception |
| Zero-length deadlines after a fractional PLL tick | `SystemClock::deadline` uses saturating phase subtraction | Full clock/display-mode native suites; cached timing and FRT compare deadlines count from the actual timer-start write | Corresponding PLL, TV-mode and FRT browser guests; sound guest now follows setup23 + IRQ6 + STOP2 + BRA1 = 32 retired instructions, with unchanged exact stereo samples |

The original BUSG source and generated ROM carry the adjacent
[MIT license](../tests/fixtures/LICENSE). Reproduce with
`python3 tests/fixtures/build_bus_transactions.py --check`, also required in CI.
Expanded ROM SHA-256:
`a27982aa42f2fd6ccbeaea9a6670507613731c4ecc7e21c15eca75ce4085d471`.
Generated hex SHA-256:
`b9319e555a875d4529baee69d132617bcfa07f2c3d181c617a7e82734e6e395d`.
The three demonstration discs are unchanged.

Remaining mandatory behavior includes full SH-2 IF/MA/multiplier overlap,
on-chip DMAC request/cycle-steal transactions, display-fetch/refresh and sound
RAM ownership, SCU DSP bank conflicts, full instruction-specific 68k sequencing,
remaining video/sound/CD/SMPC contracts and complete HLE startup/CD ABI. The
listed implementation and executable evidence do not waive those findings or
establish full Issue #17 completion.

## On-chip DMAC bus transactions (DMABG)

Related to #17. Sources: [SH7604][H] §§7.3/7.5/7.11.2 and
§§9.1.1/9.2.6/9.3.3–9.3.8, figs9.2/9.4/9.5/9.10/9.52.
The production runner replaces the eight-clock immediate-copy cadence with
separate bus-granted source and destination phases. Both SH-2s have independent
DMA journals, separate from their CPU journals. Replay retains sampled data and
completed MMIO without issuing them twice. All device work remains guest-driven.

| Contract | Implementation | Independent native evidence | Guest / browser evidence |
| --- | --- | --- | --- |
| Data, TCR/TE and IRQ become visible at their actual completion boundaries | `Bus::prepare_dmac/grant_dmac/advance_bus_transaction`, `Dma::transfer` | `dmac_samples_reads_then_accepts_buffered_writes_before_external_publication`: both CPUs, source mutation before/after sampling, exact 2-state read + 3-state external write, buffer acceptance/count/IRQ at the intervening internal boundary; destination remains unchanged until external completion (IFBUF below) | [Original builder](../tests/fixtures/build_dmac_bus.py), [ROM](../tests/fixtures/dmac-bus.hex), [authored vectors](../tests/fixtures/dmac-bus.tsv), [native](../tests/dmac_bus.rs) and [production Wasm](../scripts/dmac-bus.mjs): both CPUs independently execute eight transfers and eight IRQ handlers |
| BSC width/waits apply to actual DMAC physical beats | `CpuBus::reserve_external/external_read_group/external_write`, separate DMA journal indices | `every_narrow_bus_and_software_wait_delays_publication_after_buffer_acceptance`: both CPUs × 8/16/32-bit buses × four wait settings, literal `(2+Tw)*beats*2+1` deadlines; earlier clock-chunk test now expects the reset BSC's 18-state first longword unit | DMABG runs with guest-programmed waits and a real shared external bus, changing only its own MMIO/RAM |
| A 16-byte unit performs four reads before any writes; final TCR=1/2/3 emits only that many writes | `Dma::transfer` samples four longwords; `Memory::dma_read32` preserves source-burst first/last signals; source advances 16 regardless of SM | `sixteen_byte_units_read_the_whole_source_before_writing_even_for_short_final_counts`: both CPUs, all four final counts, overlapping source/destination, no premature writes or lost canary; `zero_tcr_means_sixteen_million_longwords_and_standby_discards_burst_state` checks 24-bit TCR wrap | DMABG's TCR=3 overlaps by one longword and preserves the fourth destination canary. Interleaved reads/writes corrupt independently expected values and a four-write implementation destroys the canary |
| Unit ownership, cycle-steal yielding, burst retention, channel priorities and TAS exclusion | `Transactions::dmac_owner/dmac_yield`, `Dma::cycle_previous/next` | `cycle_steal_yields_to_a_waiting_cpu_while_burst_retains_the_bus_and_cache_hits_proceed`, `fixed_cycle_steal_alternates_and_burst_priorities_follow_the_programmed_mode`, `tas_excludes_dmac_until_its_write_and_instruction_release` | DMABG executes independent CPUs, five width/address/burst cases, SCI-to-SCI/RAM and DIVU-to-RAM, and a final peer-completion handshake |
| DREQ is accepted once per edge; a later edge survives the current unit; a burst edge activates the block | `Channel::external_edge/burst`, latched `Dma::selected` | `edge_requests_are_consumed_at_acceptance_and_burst_edges_start_the_whole_block`: unrequested wait, a second edge while a read is pending, cycle-steal stops after two units and burst completes three | Existing DACK native/Wasm guest still waits for real pins rather than receiving invented standard-board data |
| NMI finishes the accepted unit; DME=0 stops at the current cycle; reset cancels queued/granted work | `Dma::transfer`, `Bus::cancel_dmac_transaction`, slave/watchdog/cold-reset paths | `nmi_completes_the_accepted_unit_but_dme_abort_never_issues_another_cycle`, `reset_cancels_both_pending_read_and_granted_write_without_late_dma_or_irq`: read/write boundaries, retained counts and suppressed tail/IRQ | DMABG repeats cold boot, guest handlers acknowledge TE before subsequent work, and both CPU results repeat |
| DMA bypasses caches and cannot access cache/DMAC/BSC/UBC registers | uncached `CpuBus`, P4 exclusions, separate peripheral completion path | `dmac_cannot_access_its_own_bsc_ubc_or_cache_registers_or_change_them`, existing SCI/cache/UBC vectors | Existing PERIPH and cache guests remain required alongside DMABG |

The manual contains conflicting round-robin reset wording. DMAOR's PR bit
[H] §9.2.6 and fig9.5 specify channel1 first; a sentence in §9.3.3 says channel0.
This implementation retains channel1 first to follow the register description
and figure. Fixed priority starts channel0; fixed cycle-steal alternates when
both channels remain requesting. Tests distinguish all three modes.

Original source, generated ROM and authored results use the adjacent
[fixture MIT license](../tests/fixtures/LICENSE). Reproduce with
`python3 tests/fixtures/build_dmac_bus.py --check`, required in CI. Hashes:

- Expanded 512 KiB ROM: `7161e843e92dd4bd746694ffd31d58f92e02b937c4411c878d43ee5621b5ff0e`.
- Hex container: `9033d405620a0016ef6744d1b63490e3e8ebe4c48bc16f8572887e0034b085c9`.
- Authored TSV: `018eb3ada87198eeb9a8c96521cbb997741c6bb56b2cc5d937b1332d7c1b9a66`.

These contracts do not complete the DMA/hardware acceptance inventory. Required
remaining work includes exact DREQ sampling/acknowledge-pin timing, complete
refresh/display/sound ownership and preemption, CPU/internal-peripheral bus
arbitration (extended by IFBUF below), every manual-reset/partial-error
boundary, and the other CPU, SCU/DSP, video, sound, CD/HLE, SMPC and integration
requirements already recorded above. No umbrella criterion is waived or checked.

## Internal DMA, sound data phases and media validation

Sources: [SH7604][H] §§7.11.2–3, 9.3.5, 10.3–10.4;
[MC68000UM][68UM] §5.1 and chapter 8; [CD] §5.3 table5.1 and
function specifications 4.1/5.7–5.8; [DISC] §4.3.

| Contract | Implementation and independent native vectors | Guest/browser evidence |
| --- | --- | --- |
| CPU-local internal DMA can overlap another master's external access, including TAS ownership | [transactions.rs](../src/bus/transactions.rs) stores separate internal phases. [dmac_tests.rs](../src/bus/dmac_tests.rs) checks simultaneous read/write deadlines, both CPUs, sampled source mutations, count/IRQ publication, TAS exclusion and slave-reset cancellation. A negated external level request while TAS owns the bus is not accepted early. | Extended DMABG executes SCI-to-SCI, SCI-to-RAM and DIVU-to-RAM on both CPUs, eight real IRQs each, changed operands and cold reset in native/Wasm. |
| DIVU DMA waits on the DMA phase, preserving CPU instruction dependencies | `OnChip::dma_wait_clocks/dma_read32/dma_write32`; `internal_divu_dma_waits_for_division_without_premature_result_irq_or_cpu_waits` checks overflow IRQ at clock 6, internal read at 7 and RAM write at 9, literal quotient 47 and zero CPU waits. [peripheral_tests.rs](../src/onchip/peripheral_tests.rs) checks CPU-only consecutive-instruction dependencies across DMA. | DMABG's independently computed `99/3=33` is transferred by DMA and read by guest code in native/Wasm. |
| Sound CPU accesses publish at separate four-clock byte/word completions | [sound/transactions.rs](../src/sound/transactions.rs) retains completed MMIO through CPU replay; `Sound::clocks_to_event` includes CPU and MIDI deadlines. [sound/tests.rs](../src/sound/tests.rs) checks normal/predecrement longword order, a main-CPU change between source words, MIDI consumption once, cancellation of a pending word and clock-chunk invariance. | SNDR, SIOG, FAMG, SYNTH, Prism Courier and Hardware Lab execute through these phases, retaining native/Wasm register/fault/stereo assertions and actual WebAudio. SNDR's second IRQ wakes its first STOP after the scheduled SH-2 WB change below: setup23 + IRQ6 + STOP2 + BRA1 = 32; its last saved PC is $182. |
| CD connection errors preserve both outputs and filter-input exclusivity | `CdBlock::execute` validates both selected output destinations before mutation; `selector_connection_commands_validate_both_outputs_and_exclusive_inputs_atomically` checks indices 24–254, ignored arguments, disconnect/reconnect, ESEL and actual sector routing. | [CD drive guest](../tests/fixtures/build_cd_drive.py), [native](../tests/cd_drive.rs) and [browser](../scripts/browser-smoke.mjs) assert rejection, retained `0304` connections and explicit reconnection. Hex SHA-256: `8203401b2a7d7d76aef396b69bdb7caaf73ee1d8dd26eea4aea7eac39c23da95`. |
| Compact ISO size validation agrees with the core | [app.js](../web/app.js) accepts complete sectors from 18 to 450,000 sectors and delegates structural validation to the core. | Offline actual-picker regressions in browser-smoke execute original 19/21-sector AIP instructions (`R0=37`, 100 retired, no exceptions); reject short/partial files and oversized metadata probes, and parse a malformed size-valid 18-sector image. Standard-size executable-media evidence is recorded below. |

The four-clock sound bus phases preserve the existing interpreter's access order;
they do not establish every instruction-specific prefetch sequence, RESET pin
timing, SCSP RAM arbitration or sub-sample DSP/DMA scheduling. Internal DMA
timing alone does not establish CPU/internal-bus arbitration or full write buffering;
CPUIO below adds CPU/internal-bus serialization and DIVU completion ordering.
Other mandatory inventory entries above remain pending with their original scope.

## SCU DSP program-DMA origin and data-bank restrictions

Sources: [SOA Technical Bulletin #8][SOA8], §§1.4/4.1/5.6, and
[SCU] §§3.3/4.3/4.4/4.5. SOA #8 specifies program-RAM location zero as the
destination of PRG DMA. The previous implementation incorrectly reused the
CPU's program upload cursor; an earlier native wrap vector encoded that same
incorrect assumption. The independent cursor and replacement vectors below
correct both. This entry supersedes the earlier host-cursor PRAM wrap claim.

| Contract | Implementation | Independent native evidence | Guest / browser evidence |
| --- | --- | --- | --- |
| PRG DMA starts at zero, independent of host upload/PC | `Dma::program_address`, `Dsp::step/grant_dma_cycle/dma_beat` in [dsp.rs](../src/scu/dsp.rs) | `program_dma_starts_at_zero_and_full_count_wraps_without_changing_the_host_port` and `program_dma_count_forms_and_hold_use_a_private_zero_based_cursor` in [tests.rs](../src/scu/dsp/tests.rs): immediate/M3/MC3 counts 1/2/63/64/65/128/255/256, HOLD, low-eight-bit selection, CT63→0, host cursor unchanged and untouched PRAM suffix | Expanded [DSPD source](../tests/fixtures/build_dsp_dma.py) runs bootstraps at 0x40/0xA0/0xC0, then executes actual transferred instructions at0 and FF→00; [native](../tests/dsp_dma.rs) and [production Wasm](../scripts/browser-smoke.mjs) compare 13 authored words and repeat cold reset |
| DSP control-flow entry into loaded code | Existing delayed `MVI PC` now reaches correctly placed DMA output | Existing delayed-branch/prefetch unit vectors remain enabled | DSPD's A0 bootstrap polls T0, executes MVI PC0 and its MC2 delay-slot write, then computes MC3=57 from the loaded program. Changing the slot and loaded instruction independently yields 11 and 77 |
| Complete transfer and operand dependence | DMA count and program data drive actual PRAM writes, not a game/result hook | Native count-form matrix verifies every transferred word and retained suffix; retained-successor test still observes the old instruction before LE refetch | Guest SH-2 initializes all 256 source words through stores. Count 0 loads the last MVI at FF, yielding 107; count 1 leaves FF as NOP, yielding 0. Changing the last MVI yields 125. All other 13-word record fields retain their authored values |
| Permitted parallel work and prohibited DMA-bank instructions | `Dsp::check_dma_bank` checks only selected X/Y/D1 sources, RAM destinations and taken immediate stores before changing state; `ScuError::DspDmaConflict` is explicit | `dma_bank_conflicts_are_diagnosed_before_any_parallel_instruction_side_effect`: all four banks, both DMA directions, ten source/destination forms, other-bank work, inactive fields, untaken conditions, independent CT register writes and bank availability after DMA completion | DSPD copies MC0→MC2 while M1 DMA runs, retaining 1234. Mutating that instruction to MC1→MC2 gives the expected bounded bank 1 / PC 02 diagnostic in native and Wasm, no result write, sticky retries and reset repetition |

The bank restriction is a documented programming constraint, not a newly claimed
automatic hardware stall. The emulator diagnoses violations instead of inventing
a winner between simultaneous RAM accesses. This change does not restrict the
stopped host data port or establish its physical arbitration. Complete physical
RAM-port/DMA/pipeline timing and the other mandatory #17 acceptance entries remain
required; no umbrella checkbox is checked.

The expanded source, generated [ROM](../tests/fixtures/dsp-dma.hex) and
[vectors](../tests/fixtures/dsp-dma.tsv) use the adjacent
[MIT license](../tests/fixtures/LICENSE). The existing CI command
`python3 tests/fixtures/build_dsp_dma.py --check` reproduces both files.
Current hashes are recorded in the DSPD table above. The three licensed
demonstration discs are unchanged.

[SOA8]: https://github.com/luckyseoul/saturn-docs/blob/main/files/Sattechs.pdf

## SH7604 CPU/internal-bus completion and DIVU ordering (CPUIO)

Sources: [H] §§7.11.2–7.11.3, 9.3.3–9.3.5 and 10.4. CPU peripheral
accesses now use the same per-processor internal/peripheral bus as on-chip DMA.
Previously the CPU performed those MMIO operations immediately while DMA used
scheduled phases. This also allowed a CPU result read to finish DIVU and assert
its overflow interrupt before hardware time reached the division deadline.

| Contract | Implementation | Independent native evidence | Native / Wasm guest evidence |
| --- | --- | --- | --- |
| CPU byte/word/long MMIO publishes or samples at completion; reads include the extra determine state | `CpuBus::read8/read16/read32/write8/write16/write32`, `Bus::grant_cpu_internal/complete_bus_transaction` in [bus.rs](../src/bus.rs) and [transactions.rs](../src/bus/transactions.rs) | `cpu_peripheral_byte_word_and_long_writes_have_one_completion_and_reads_have_two_states` in [dmac_tests.rs](../src/bus/dmac_tests.rs): SCI, INTC and UBC registers on both CPUs; late sampling and stable replay in `cpu_peripheral_reads_sample_at_completion_and_share_only_the_local_dma_bus` | Expanded [TIMING builder](../tests/fixtures/build_sh2_timing.py) executes 32 SCI reads and 32 writes on each CPU; independent FRT results are 8 and 4 ticks. [Native](../tests/sh2_timing.rs) and [browser Wasm](../scripts/browser-smoke.mjs) compare all 35 words, cold-reset repetitions, changed arithmetic/control flow and a changed SCI write operand (FF→5A) |
| DMA read phases hold the local internal bus; a buffered write releases it for peripheral work while external burst ownership continues; the other processor remains independent | `Bus::grant_bus_transaction/grant_cpu_internal` | `local_external_read_holds_internal_bus_until_completion_but_not_the_other_cpu_bus`, `external_dma_buffer_releases_local_internal_bus_while_external_burst_continues`; explicit completion/count/IRQ boundaries, including simultaneous pending CPU/DMA requests | Existing [DMABG](../tests/dmac_bus.rs) and [production Wasm](../scripts/dmac-bus.mjs) execute SCI↔RAM and DIVU→RAM, all transfer widths, burst/cycle-steal and eight IRQs on each CPU. TIMING independently exercises both CPU peripheral paths |
| Reset/cancellation cannot publish a canceled CPU phase or cancel another owner's phase | Owner checks in `Bus::cancel_cpu_transaction/cancel_dmac_transaction` | `cpu_internal_transactions_do_not_acquire_external_bus_and_cancel_by_owner` checks pending/granted writes, DMA-only cancellation, external TAS retention and subsequent bus release on both CPUs; existing reset-domain vectors remain enabled | TIMING and DMABG repeat cold resets; existing SYSRES/standby/slave-reset guests retain their native and browser assertions |
| DIVU wait consumes actual hardware time, retains its completion/IRQ deadline and does not charge the CPU twice | `OnChip::cpu_peripheral_wait`, `Bus::grant_cpu_internal` | `cpu_divu_interlock_keeps_irq_and_other_devices_on_real_clock_boundaries`: both processors, three clock chunk sizes, no early overflow/IRQ, six-clock completion and later read completion, zero residual wait charge | Existing [DIVG/PERIPH](../tests/sh7604.rs), [memory contracts](../tests/memory_contracts.rs), TIMING and their browser equivalents remain enabled |

FRT oracles in BWAIT/IDLEG/BCMEM/ABUS now include CPU peripheral determine/read
states and the resume write's completion boundary. The source builders document
the state arithmetic; affected native/browser mutation expectations use the same
new intervals. Hardware/device operands and computed data expectations remain
independent of measured output. Existing CI reproducibility commands cover all
changed generated fixtures; no new proprietary data or host result injection is
used. Sources and generated fixtures retain the adjacent [MIT license](../tests/fixtures/LICENSE).

Current TIMING hashes:

| Artifact | SHA-256 |
| --- | --- |
| `sh2-timing.hex` | `734cf7e542e51bcad864e2725bbeac2e26b571b59ac6d20610d7fead9658c18e` |
| `sh2-timing.tsv` | `a61acc6a88475b77f549d33efdfd4111caa85fc04ef05dd64807e19898de4b74` |

This entry supersedes earlier immediate CPU-peripheral and deferred DIVU-wait
behavior. It establishes these internal-bus boundaries, not complete SH-2
IF/MA/multiplier overlap and speculative pipeline fetch (IFBUF below implements
aligned longword fetch and BSC buffering), refresh/display/sound ownership, or the other pending mandatory #17 contracts.

## Granted refresh, indivisible CPU groups and self-refresh (RFSHG)

This entry supersedes the earlier normal-refresh countdown and missing
refresh/CPU/DMAC ownership notes. Primary specification: [H] §§7.2.7, 7.5.7
(figs.7.25–7.26), 7.6.6, 7.7.5, 7.10 (pp.202–203), and 9.3.3 (p.257).
External UBC monitoring also follows §§6.3.4/6.3.7 and §7.10 (p.203).
Refresh is a third BSC bus master. A timer match latches a request; it does not
perform a concurrent cycle or close RAM rows before receiving ownership.

| Required programming contract | Implementation | Independent native evidence | Guest / browser evidence |
| --- | --- | --- | --- |
| Request retention, CMF independence, coalescing and master-only output | `Bsc::advance/grant_refresh`; pending request survives elapsed time and CMF acknowledgement; PALL closes both banks at grant | [BSC vectors](../src/onchip/bsc.rs), [bus vectors](../src/bus/refresh_tests.rs): t4/t8 matches coalesce, a stopped timer leaves the request pending, and slave matches do not drive refresh | [Original guest builder](../tests/fixtures/build_refresh.py): master/slave own timer reads, independent arithmetic sums and the same program with timer-enable operand changed |
| Owned refresh states and simultaneous-request priority | `Bus::grant_bus_transaction/advance_bus_transaction`, `Owner::Refresh`: grant before unaccepted CPU, on-chip DMA or SCU; accepted data completes first; SCU A-bus refresh output moves to the real grant boundary | Six literal main-clock states with no early write; each of CPU/DMAC/SCU contenders; chunks1/3/7/64; peer internal SCI completes during refresh | [Native RFSHG](../tests/refresh.rs), [production browser Wasm](../scripts/refresh.mjs): enabling master refresh increases each real CPU's FRT-measured RAM-read interval; cold resets repeat |
| Retained CPU narrow operands and cache fills | `Transactions::cpu_group`, grant/complete/cancel paths: retain every beat until the last successful beat, error or cancellation | Both CPUs, ordinary byte-bus longword and four-longword cache fill; queued peer write cannot tear data; long host gaps do not expire an ungranted request | All existing memory/cache/dual-CPU guests remain in the mandatory native/browser suites; RFSHG warms its actual instruction cache |
| TAS and DMA burst deferral | Existing TAS lock and DMAC unit/burst ownership gate refresh grants | Accepted TAS read/write and two-unit burst finish before the single pending refresh; partial slave write cancellation releases its group, preserves completed bytes, and cannot cancel a refresh owned by the board | BUSG/DMABG guests continue executing actual TAS/DMA/IRQ programs; RFSHG checks the shared refresh effect on both processors |
| Self-refresh bus hold and exit recovery | `Bsc::inhibits_external_bus/clocks_to_event`, board grant gate: external requests wait; local/cache accesses proceed; recovery has its own deadline | SDRAM three-state and pseudo-SRAM five-state exit recovery, chunks1/2/17/128; slave reset does not release the master's hold; previously accepted access completes | Master executes 32 cached ADDs between keyed MCR entry/exit; omitting its exit leaves completion absent and slave unstarted within bounded native/Wasm execution |
| External UBC monitor owns its processor's internal bus | `Ubc::monitors_external_bus`, `OnChip::monitors_external_bus`, board internal/external grant gates: synchronized SH-2+EBBE settings; full-share/MD5 eligibility; retained external groups continue monitoring between beats | [Monitor bus vectors](../src/bus/monitor_tests.rs): both CPUs × four BRCR modes × CPU/DMAC peripherals × peer CPU/DMAC/SCU; accepted peripheral completes before handoff; three-state programming latency/read-back barrier, partial-share exclusion, retained group/cancel boundaries | Original `monitor.hex` runs master RAM polling and slave cached SCI/FRT sampling; changing BRCR from3000 to1000 removes the measured penalty; changing SCI operand90 to60 changes the sum from2880 to1920, with identical timing; native/Wasm cold resets repeat |

RFSHG is wholly original source and generated data under the adjacent
[MIT fixture license](../tests/fixtures/LICENSE). It uses the existing original
assembler and Python standard library. `python3 tests/fixtures/build_refresh.py --check`
reproduces `refresh.hex`, SHA-256
`796fe1c07d8096ca35e6a2bd5ddbd9e94ebe88e261c10af848f2c68d587ac560`.
Guest operand mutation changes the sum from `26AF37A0` to `48D159C0` on both
processors; timer mutation removes the timing penalty without changing the sum.
No host writes a running guest result or recognizes a game.

The same builder/CI command reproduces the original `monitor.hex`, SHA-256
`9eaf0af4e452d2be04b19ecfb2f1a6e31102451f7f545e22b091df76b48dacab`.
It uses the same license, native integration file and browser script. Its
timing comparison is against a guest-programmed control mutation; it does not
use captured output as a timing oracle. Exact phase boundaries are asserted
separately by native bus-state vectors.

Remaining mandatory work includes full IF/MA/multiplier and exception sequencing,
WAIT/DREQ synchronization, board display/sound ownership, memory initialization
and complete reset-domain boundaries, all other pending device/service contracts,
and complete umbrella acceptance evidence. These vectors do not establish
physical pin timing, illegal self-refresh access results, or analog RAM decay.
The other Issue #17 requirements remain mandatory; no umbrella checkbox is checked.

## VDP2 display-fetch grants and line-table latches (FETCHG)

Primary reference: [VDP2] §3.3, tables3.2–3.5 and figs.3.2–3.7;
§5.3 (line-scroll read intervals), §6.1 (per-line rotation-parameter reads),
§6.2 (rotation banks), §7.1 (line color) and §8.1 (line windows).
This evidence implements additional Issue #17 video behavior; the umbrella
acceptance checklist remains unsatisfied.

| Contract | Implementation | Independent native evidence | Guest/Wasm evidence |
| --- | --- | --- | --- |
| Pattern-name commands 0–3, character/bitmap commands 4–7 | `Vdp2::name_slots/character_access`, `tile_map/background` in [access.rs](../src/vdp2/access.rs) and [vdp2.rs](../src/vdp2.rs) | Exhaustive addressed-bank tests for four layers, five formats, 0–8 grants and normal/high-resolution/exclusive timing ranges; literal table3.4 pipeline masks for each name timing; x1/x2/x4 name/character requirements and ZMCTL | [Original licensed builder](../tests/fixtures/build_vdp2_fetch.py), [native guest](../tests/vdp2_fetch.rs), identical [production Wasm/Canvas test](../scripts/video-transactions.mjs); removing grants, putting them in another bank and changing color depth changes actual historical pixels |
| VRAM split addressing and rotation ownership | `display_bank/rotation_bank/rotation_access` | A1/B1 cycle words ignored when unsplit, independent when split; every bank/rotation-role pair; RBG1's fixed B1 names/B0 characters retained | FETCHG switches the bitmap from A0 to A1, proves blank output until A1 receives grants, then recovers; RBG1/effects/Hardware Lab use explicit RBG0 bank roles |
| Vertical cell-scroll commands 12/13 | `cell_access`, `normal_coordinates` | Literal early-slot limits, NBG0-before-NBG1 and same-bank requirements; NBG1-only release | Existing effects guest exercises table setup; phase/latency guest coverage for this port remains required |
| Line-scroll interval retention | `Vdp2::read_line_tables/scroll_data` in [tables.rs](../src/vdp2/tables.rs) | Mid-line mutation leaves the current read intact; an interval of two retains line0's read on line1, reads the next record on line2, and reloads at the next field | Existing line-effects guest and Hardware Lab rerun through Wasm; mid-interval mutation has native evidence |
| Per-line/per-dot coefficient reads | `RotationRead::coefficients`, `read_rotation_line/coefficient/read_coefficient` in [rotation.rs](../src/vdp2/rotation.rs) | A coefficient read once per line stays unchanged after a RAM write; a per-dot coefficient requires RAMCTL role1 or CRAM and samples changed RAM on later dots | FETCHG's guest-written start/matrix selects X=1*kx. Writes of coefficients1/2 preserve the current line's red/green pixel and change the following line. Switching to per-dot coefficients loses the RBG0 pixel until B0 is assigned role1, and removing the port loses it again; native/Wasm compare both sample points and guest-color mutations |
| Line-window, back/line-color and rotation-parameter reads | `read_line_tables`, `rotation_long/rotation_word`, `render_region/window` | Mid-line scroll/window/back-table mutations preserve earlier reads while palette/color-offset changes remain live; all 24 rotation longwords, packed points and both parameter banks are retained until the next line | FETCHG rewrites the current and following line-window records after polling VCNT; the current window remains full-width and the next becomes narrow. The existing raster-history native/Wasm guest now checks the held whole line and the next-line change, with a changed guest color operand. Existing sprite-rotation and effect guests retain their expected pixels |
| Correctly programmed licensed media | Guest MMIO and original source builders | Four-layer tile guest now advances through two actual fields before checking pixels; A supplies four names, B supplies five character accesses. Native Prism Courier and Hardware Lab still complete/restart | Full browser suite checks all three discs, Canvas, guest input/results/restart and production WebAudio; exclusive-monitor diagnostics now use two non-overlapping 256-color surfaces, independently fetched from A/B |

FETCHG expanded synthetic ROM SHA-256:
`121eb0283aa2e6f0b677bc5a04755efd5e6df2f6b4810cc99d752e0b5ccd727b`.
Sources and generated `.hex`/`.s` files use the adjacent [MIT license](../tests/fixtures/LICENSE).
CI runs `python3 tests/fixtures/build_vdp2_fetch.py --check`.
Mutating all five guest color literals changes the rendered color in native and
Wasm without changing running result RAM, pixels or host device state.

A missing addressed-bank grant or inadequate bandwidth produces a transparent
layer in this deterministic model. The manual says such programming cannot
produce the correct screen; it does not specify golden invalid-configuration
pixels. Static `Vdp2::framebuffer/composite` debugger decoding intentionally
shows an unclocked register/VRAM snapshot for inspecting unfinished setup.
`scan_to/scanout`, including completed fields presented by the production app,
apply the display grants and retain line reads. Snapshot decoder vectors alone
are not bus/beam evidence. Per-word fetch phases and retention, coefficient/cell
port timing, half-line transitions and the other pending hardware contracts
remain required; these results do not establish complete VDP2 or Issue #17
conformance.

## SCSP slot monitor and MC68EC000 reset retention (SCMON / SRESET)

Related to #17. [SCSP] §4.1 fig.4.3 and §4.2 p.89 define the
write-only MSLC selector in bits15:11 and read-only CA in bits10:7.
One CA unit represents 4096 waveform **samples**, for either PCM width.
[MC68000UM][68UM] §§5.5/6.3.1 and fig.5-30 define the separate external
reset sequence; RESET the instruction instead pulses external devices.

| Contract | Implementation | Independent native evidence | Guest / browser evidence |
| --- | --- | --- | --- |
| Correct CA field and slot selection | `Scsp::read16(0x408)`, `Voice::call_address` in [sound.rs](../src/sound.rs) | `slot_monitor_uses_documented_ca_bits_all_selectors_and_independent_byte_lanes` in [sound/tests.rs](../src/sound/tests.rs) checks all 32 selectors and sixteen field values for PCM8/16, both byte lanes, zero write-only/reserved read bits and a low-byte write preserving MSLC | [Original SCMON builder](../tests/fixtures/build_scsp_monitor.py), [native integration](../tests/scsp_monitor.rs) and [production Wasm](../scripts/scsp-monitor.mjs): 68k programs all 32 actual PCM slots, polls 600 sample-IRQ flags, then records word/high-byte/low-byte/post-byte-write monitor reads |
| Current output pointer rather than next phase | `Scsp::sample` retains the first interpolation address after loop normalization and FM displacement, relative to SA, before incrementing phase | `slot_monitor_retains_the_current_output_address_across_pitch_and_loop_boundaries` checks the 4095→4096 boundary, no/forward/reverse/alternate loops, repeated reads, retrigger/reset and both sample widths; `slot_monitor_follows_the_fm_waveform_address_without_moving_the_phase_generator` checks a modulated boundary | SCMON uses independently authored one-sample loops in all fifteen nonzero CA blocks. A guest LSA/LEA mutation changes only slot31's monitor records and right-channel samples; a source-waveform mutation changes the left channel while preserving all monitor records |
| No canned monitor/audio results | Ordinary 68k instructions, sound RAM/MMIO and live PCM synthesis | Omitting the guest KYONEX write leaves all 64 records and generated audio zero; baseline and mutations repeat cold resets | Production AudioWorklet consumes newly generated guest PCM for the baseline and each mutation, checks the corresponding peak and observes eventual underflow. The unkeyed negative case renders zero samples with peak0; it is not claimed audible |
| External reset preserves unaffected architectural registers | `M68k::assert_reset/reset` in [m68k.rs](../src/m68k.rs) clears internal work/prefetch/IRQ, sets S/I and clears T, retains D0–D7/A0–A6/USP, reads SSP-high/low then PC-high/low, and reloads A7/SSP/PC | `external_reset_preserves_general_registers_and_usp_but_reloads_vectors_and_control_state` in [bus_tests.rs](../src/m68k/bus_tests.rs) checks user/supervisor origins, all CCR values, exact four-word trace, no exception-frame write, stopped/halted recovery and stale prefetch/IRQ removal | [Original SRESET builder](../tests/fixtures/build_m68k_reset.py), [native integration](../tests/m68k_reset.rs) and [production Wasm](../scripts/m68k-reset.mjs): one 68k entry initializes registers and stops; SH-2 executes actual SNDOFF, changes reset vectors and executes SNDON; the second entry records the retained registers and new stack before changing them |
| Sound reset domains and operand dependence | `Sound::set_enabled(false)` preserves the register file while canceling its transaction journal; SNDON reloads vectors; fresh machine/LSI initialization retains its deterministic zero-initialized policy | Existing word-cancellation/RAM/SCSP retention vectors remain enabled; the off/on vector now expects the preceding computed D0 rather than an invented reset zero | SRESET's changed pre-reset D3 immediate changes precisely its post-reset record. Both original and changed guests repeat cold resets, without host-written running CPU registers or result RAM |

The source and generated media are original works under the adjacent
[MIT fixture license](../tests/fixtures/LICENSE), using only the project's original
SH-2 assembler, authored 68k encodings and Python's standard library. CI requires
both new builders with `--check`. Expanded 512 KiB ROM SHA-256 values:

- SCMON: `25fdfdcd3110fda8dba5a1fa35ca9ccc28e8e2c40961501ff620e07f359e50ad`.
- SRESET: `e2ed71e78c7df51583e963e644d2e6d54058911e8107b84d15479cb941c39432`.

These fix concrete sound-register and CPU reset defects; they do not establish
complete audio/CPU or umbrella acceptance. The monitor currently retains its last
output address after a voice becomes inactive; inactive/noise-source values and
within-sample register sampling are not independently corroborated physical-chip
claims. Cold-power register contents remain a deterministic policy. Remaining
68k reset bus-cycle placement/arbitration, prefetch/instruction sequencing,
SCSP memory ownership/DMA/EG/effects timing, and every other outstanding Issue #17
requirement remain mandatory in the same Draft PR. No acceptance checkbox is
checked and no tracker is closed, replaced or reopened.

## Scheduled 68k reset/IACK and VDP1 local-memory ownership

Related to #17. This entry supersedes the immediate sound reset-vector snapshot,
immediate RESET device clearing, missing IACK cycle and atomic VDP1 command-read
behavior in the earlier entries. References: [MC68000UM][68UM] §5.5 fig.5-30,
§§6.2.5/6.3.1–4/6.3.8 and table8-14; [VDP1] §2.1 pp.19–20, §3.1 table-access
steps and §8.3 fig.8.3.

| Required behavior | Implementation | Independent native evidence | Guest / production Wasm evidence |
| --- | --- | --- | --- |
| Separate reset SSP/PC/prefetch reads | `Sound::set_enabled`, `Sound::advance_cpu_cycle` in [transactions.rs](../src/sound/transactions.rs): four vector words and two program words complete separately; no vector snapshot at SNDON | [reset_tests.rs](../src/sound/reset_tests.rs): authored writes on either side of each deadline, retained halves/prefetch, 24-bit pins versus architectural PC, partial-reset cancellation boundaries and fault halt/recovery; chunks 1/3/17/10000 | [Original builder](../tests/fixtures/build_m68k_reset.py), [native guest](../tests/m68k_reset.rs), [browser guest](../scripts/m68k-reset.mjs): SH-2 changes PC after timed SNDON returns. The actual 68k selects the new entry; omitting that guest store selects the failure entry. Operand mutations and cold resets repeat |
| RESET output interval and trace ordering | `Memory::m68k_reset_output`, `Operation::ResetOutput`, `Scsp::set_reset`: suspend execution for the output interval, hold device reset for 124 sound clocks, preserve RAM and previously produced host samples; finish pulse independently of CPU disable | Sound reset tests include before/at/after signal edges, ignored writes/MIDI while held, CPU disable during pulse, and no trace stack bus access until the pulse ends | Original `m68k-traced-reset` executes MOVE-to-SR, RESET and the actual trace handler. Native/Wasm require saved SR=A700, saved PC=20C, retained D3 and completion600D. The following failure store never runs; changing D3's immediate changes its record |
| Interrupt acknowledge before stacking | `Memory::m68k_interrupt_acknowledge`, `Operation::InterruptAcknowledge`, `M68k::step/exception`: four-clock cycle, latched accepted level, autovector/device vector or spurious vector24 on IACK BERR | [bus_tests.rs](../src/m68k/bus_tests.rs): levels1–7, exact IACK/stack/vector/prefetch traces, device vectors15/32/64 and spurious short frames. Sound test clears SCSP's source during IACK and requires accepted level3 and delayed first stack write | Existing native/Wasm sound, synthesis, envelope, sound-I/O, family, Prism Courier and Hardware Lab guests exercise SCSP IRQs through the scheduled acknowledgement path |
| VDP1 command-word retention | `Vdp1::advance_phase/command` in [vdp1.rs](../src/vdp1.rs): retain each word; +1E dummy is not read; PTMR discards unfinished reads | [progress_tests.rs](../src/vdp1/progress_tests.rs): CTRL/COLR changes on opposite sides of read boundaries, PTMR restart, existing partial draw/stop/reset/erase vectors | [Original builder](../tests/fixtures/build_video_transactions.py), [native integration](../tests/video_transactions.rs), [browser integration](../scripts/video-transactions.mjs): guest writes END after CTRL was fetched and still draws the authored line. Moving that store before PTMR correctly leaves sample pixels zero and COPR=0 |
| VDP1 local VRAM/framebuffer ownership | `Bus::vdp1_bus_ownership`, `Vdp1::advance_with_bus`: active CPU/DMAC/SCU data phase holds the selected local memory; VRAM fetches/textured drawing pause on VRAM access and draw dots pause on draw-framebuffer access | [vdp1_tests.rs](../src/bus/vdp1_tests.rs): both SH-2 CPU and DMAC owners, SCU internal activation, held partial XC read, delayed dots and canceled CPU write. Command reads and untextured draws continue on independent memory | Original `vdp1-memory-ownership` compares FRT intervals for the same poll loop reading framebuffer versus VRAM. Only the address operand differs; framebuffer access takes longer while both runs produce the same guest-drawn pixels. Native/Wasm require repeatable timing across color mutations and cold resets |

All new sources and generated fixtures are original works under the adjacent
[MIT license](../tests/fixtures/LICENSE). Existing CI commands
`python3 tests/fixtures/build_m68k_reset.py --check` and
`python3 tests/fixtures/build_video_transactions.py --check` reproduce them.
Expanded 512 KiB synthetic ROM SHA-256 values:

| Artifact | SHA-256 |
| --- | --- |
| `m68k-reset-phases.hex` | `150726e7384ebff8513ff90045799f3a1850042da1b29b63e7dbdafa946b8393` |
| `m68k-traced-reset.hex` | `3312189850063dfaec6b692d989a90d12fafafd35a449c6209a77a28602dae7e` |
| `vdp1-command-fetch.hex` | `27974ed4fc9c3048180f5d51353394a7f5d289122cf37ffc55bc9e40263f2c9e` |
| `vdp1-memory-ownership.hex` | `056639795344bed31055be8edaa4d1b9920bb30a27f23404240c4f2ce8fc38d3` |

The existing 32-clock uncontended VDP1 command budget is divided into word reads
and a final decode phase; this does not establish every command-dependent physical
access cost. Gouraud/lookup-table accesses, full texture/effect and erase/swap
timing, SCSP memory ownership and DMA/effects pipelines, general 68k instruction/
prefetch/exception timing, and the other reviewed SH-2/SH7604, bus/reset, SCU/DSP,
VDP2/raster, CD/HLE and SMPC contracts still require work. All original Issue #17
acceptance criteria remain mandatory; none is checked by these changes alone.


## Scheduled SH-2 system-register write-back interlocks

Related to #17. [P] §7.5 fig.7.9 and §7.7.6 figs.7.86/7.90 distinguish
write-back from a register-only transfer: `STS MACH/MACL,Rn` and
`LDS.L @Rn+,PR` have WB; `STS PR,Rn` and register `LDS` do not.

| Behavior | Implementation | Independent native evidence | Guest / production Wasm evidence |
| --- | --- | --- | --- |
| MACH/MACL-to-GPR and memory-to-PR WB dependencies | `load_destination/register_uses` in [timing.rs](../src/cpu/timing.rs); PR has its own dependency identity, including RTS, PR transfers and calls; BRAF and BSR displacement bits do not become false PR/GPR dependencies | `system_register_writeback_uses_the_actual_destination_and_implicit_pr` covers all 16 GPRs, direct/implicit uses, overwrites, repeated loads, separation and non-WB controls. `mac_register_writeback_and_loaded_pr_have_architectural_results_and_interlocks` in [CPU vectors](../src/cpu/tests.rs) executes all-register STS/ADD and LDS.L/RTS/delay-slot sequences, exact values, flags and deadlines | [TIMING builder](../tests/fixtures/build_sh2_timing.py) adds five authored kernels: MACL-use 6 ticks/result1, MACH-use 6/1, separated MACL 3/1, PR-use 6/0, separated PR 3/0, on both CPUs. Removing each dependency gives 4/0 in the corresponding kernel; all other records stay unchanged |
| WB wait completes before dependent operand cycles | `Memory::sh2_pipeline_wait`, `Cpu::execute`, CPU-local journal `PipelineWait` in [transactions.rs](../src/bus/transactions.rs); main-clock deadlines, retained replay, no external/peripheral bus acquisition, reset cancellation | `writeback_interlock_precedes_dependent_mmio_while_the_peer_keeps_running` in [scheduler vectors](../src/lib.rs): STS at t34, wait completes t36, dependent SCI write publishes t37; peer NOPs continue and no earlier write occurs. `pipeline_waits_are_cpu_local_replayed_once_and_canceled_by_reset` checks both CPU identities, independent peer SCI, exact deadline, completed replay, cancellation and SYSRES | [Native guest](../tests/sh2_timing.rs) and [browser suite](../scripts/browser-smoke.mjs) execute the same original BIOS, 35-word records, dependency/control/peripheral mutations and repeated cold resets |
| Existing cross-clock sound guest remains independently checked | The real SH-2 upload/control now uses the scheduled operand wait | [Sound integration](../tests/sound.rs) checks second timer IRQ saved PC=$182, after STOP at $17E, before BRA/STOP; setup23 + two IRQ3 handlers + STOP2 + BRA1 = 32, with unchanged exact stereo samples and mailbox results | The browser sound guest checks the same saved PC, count, stereo, reset repetition and production AudioWorklet path |

The shared FRT-kernel helper preserves its old default for the A-bus fixture;
only the PR kernels save/restore PR. The original assembler adds CLRMAC and
LDS.L encoding support. The 64 existing CI reproducibility commands cover the
changed original MIT ROM/TSV; all three demonstration discs remain byte-identical.
The updated TIMING hashes appear in its inventory above.

These implement the WB interlock, not the complete SH-2 IF/MA/multiplier
pipeline or every exception/peripheral/cache/bus boundary. All remaining original
Issue #17 requirements, including the other chips and full demonstration and
conformance coverage, remain mandatory. No umbrella acceptance checkbox is
claimed complete by this entry.


## SH7604 aligned instruction fetch and BSC write buffer (IFBUF)

Primary behavior: [H] §§7.3.1/7.11.2 and [P] §7.4.2, fig.7.7.
Instruction accesses fetch an aligned longword at both 4n and 4n+2 entries;
only the sequential second word is retained. Taken branches discard it even
when the destination equals the sequential address. Stores do not snoop it.
CPU reset discards it independently on each SH-2.

The BSC accepts one byte/word/long operand into a CPU-local buffer. The CPU
or DMAC receives internal-bus completion before the chip-external data phase
ends. Physical byte/word beats retain the original operand and lane order.
A second write waits for space; external reads (including a dummy read of the
written address) remain behind the buffer. Cache hits and peripheral accesses
can proceed while it drains. TAS retains its read/write lock through external
completion. CPU-only reset/cancellation cancels uncompleted beats and preserves
completed bytes; board SYSRES clears work RAM and retains backup storage.
This entry supersedes the earlier unbuffered-write and retained-DMAC-internal-bus
statements; it does not change the broader Pending CPU/bus acceptance status.

| Contract | Implementation | Independent native evidence | Executing native / production Wasm evidence |
| --- | --- | --- | --- |
| Longword fetch, 4n+2 entry, sequential word retention and bus splitting | `CpuBus::fetch16`, per-CPU `instruction_word`, existing `external_read_group` in [bus.rs](../src/bus.rs) | [fetch_tests.rs](../src/bus/fetch_tests.rs): both CPUs × three widths × CE=0/P1/ID-disabled paths × both word entries; literal physical address/width/first/last and 8/16/32-clock beat traces; self-modifying code, single-use retention and reset invalidation | [Original guest builder](../tests/fixtures/build_sh2_bus_pipeline.py), [ROM](../tests/fixtures/sh2-bus-pipeline.hex), [authored records](../tests/fixtures/sh2-bus-pipeline.tsv), [native](../tests/sh2_bus_pipeline.rs), [Wasm](../scripts/sh2-bus-pipeline.mjs). Both CPUs execute original RAM functions: first retained immediate=7, second call/refetch=42, modified branch target=42; operand mutation yields 61 |
| Taken-control-transfer and exception invalidation | `Memory::sh2_discard_prefetch`, replay `DiscardPrefetch`, `Cpu::execute/enter_exception` | `taken_branch_discards_the_word_fetched_beside_its_self_modifying_delay_slot`; `taken_conditional_to_the_sequential_address_still_refetches_its_target` | Same guest executes a modifying delay slot whose branch destination was fetched beside the slot; source mutation and repeated cold reset are checked |
| Independent write acceptance and publication, dependent-read barrier, bounded buffering | `Journal::pending_write/write_accepted`, `BufferedWrite`, `WriteAccept/WriteBuffer` owners, `Bus::accept_buffered_write/grant_buffered_write/complete_bus_transaction` in [transactions.rs](../src/bus/transactions.rs) | [write_buffer_tests.rs](../src/bus/write_buffer_tests.rs): both CPUs, write accepted at1 while RAM remains old, SCI read completes at3, external write publishes at8, dependent external read at16; chunk-invariant continuation, backpressure and each partial-byte reset boundary. [Scheduler test](../src/lib.rs) executes independent ADDs before publication and cancels pending data on reset | Each cold boot selects one physical width before any CS3 access. A warmed guest kernel measures high(2)+low(2)+write acceptance(1)+SCI read(2)+high(2)=9 clocks, hence FRT=1 at all three widths; the dummy read then returns the actual written value. A synchronous external write cannot produce the timing record |
| DMAC releases the internal bus after buffer acceptance; next internal source read overlaps prior external write | `Bus::prepare_dmac/grant_cpu_internal`, independent DMA journals and the shared BSC buffer | `dma_internal_source_is_sampled_while_its_previous_external_write_is_buffered`, updated [DMAC phase vectors](../src/bus/dmac_tests.rs): exact acceptance/TCR/TE/IRQ versus destination publication, byte/word/long physical waits and concurrent local peripheral reads | The guest starts SCI→RAM DMA, changes SCI after four NOPs and observes `31310000` from two retained source reads; changing its source immediate gives `52520000`. Its warmed DMA kernel also measures DMAOR write + four NOPs + a live FRT read at clock8 (1 tick). An isolated unbuffered-DMAC regression preserves every data record but changes this timing to 2 ticks and fails the native guest test |
| Local cache update and diagnostic/reset provenance | `CpuBus::write`, buffer context retained separately from instruction journals; `Cpu::deferred_write_fault`, `Machine::step_hardware_boundary` | `accepting_a_write_updates_only_the_issuing_cache_before_external_memory`; `deferred_diagnostics_keep_the_issuer_pc_and_exception_context_and_reset_discards_them`; existing sticky slave exception-entry and cold-reset tests | Existing browser BIOS faults keep the issuing PC and CPU identity after buffered retirement; external BIOS remains separate from HLE |

FRT oracles were revised from phase arithmetic, without changing guest operands:
BWAIT's retained IF removes redundant fetches/turnarounds; fixed 16-bit CS0
now requires two words per IF longword (8+Tw ticks; Issue #23 below).
BCMEM's write kernel overlaps ADD/acceptance with external work; the final latch
is at `10 + 7*max(2, external-write-states)` clocks, including the initial
low-byte load's WB/EXTU interlock. Read/data records and all three demo media
bytes are preserved. The Hardware Lab effects test captures a complete field
before searching for its unchanged stereo golden vector, independent of the
raster phase at guest completion.

Reproduce with `python3 tests/fixtures/build_sh2_bus_pipeline.py --check`
(required in CI alongside the 64 existing media/fixture checks). All new source
and generated media use the adjacent [MIT license](../tests/fixtures/LICENSE).

| Artifact | SHA-256 |
| --- | --- |
| Expanded 512 KiB BIOS | `4e17f25da75a5a60e060ebe0702d2ed5fc23d8055c28316ada02c997c8d3d415` |
| `sh2-bus-pipeline.hex` | `4c50f34c985bd9334999ec2ddb86c00eacd2f4fd2349584b96b0f3cd0fc7ed8a` |
| `sh2-bus-pipeline.tsv` | `176502fbd279a05f2b9959540ebebb56b04bb593ee2ebd1aa9f5165c1578777c` |
| Updated `bsc-memory.tsv` | `96366960dbb3a161ec84261b9f00663a6e7feae4ab640c7fab77b520b46a1e00` |

The existing sound guest now independently checks IRQ2's stacked return PC
`$17E`, before the first STOP, and setup23 + two IRQ3 handlers + STOP1 = 30
instructions. Longword IF and buffered upload/control work shift the fixed-domain
timer's phase relative to the sound CPU; this supersedes the previous `$182`/32
record without changing guest code or PCM goldens.

The complete IF/ID/EX/MA/WB/multiplier pipeline, all manual-reset/standby
boundaries and the other mandatory subsystem contracts remain subject to the
full Issue #17 acceptance checklist. This evidence does not establish those
unimplemented contracts or close the umbrella Issue.

## SH7604 watchdog reset bus completion (RSTBUS)

Primary specification: [H] §§4.2.2–4.2.3, table4.5, §7.11.1, §§8.4.6/12.3.1.
Power-on reset aborts pending external work. Manual reset completes the current
external operand, stopping cache refills and 16-byte DMAC requests at a longword.
A word on a byte-wide interface still stops at its word boundary. Reset does not
resume the interrupted instruction or issue the next DMA unit.

| Contract | Implementation | Independent native evidence | Executing guest / production Wasm evidence |
| --- | --- | --- | --- |
| Manual completion versus power-on cancellation | `Transfer::unit_end`, `Bus::hold_cpu_reset/grant_reset_drain` in [transactions.rs](../src/bus/transactions.rs); physical phases retain sampled write data independently of canceled CPU/DMAC journals | [reset_tests.rs](../src/bus/reset_tests.rs) covers both processors, all three ordinary bus widths, pre/post-beat reset edges, advancement chunks, a queued second operand, narrow word reads, cache refill and 16-byte DMA stopping at the current longword | [Original MIT builder](../tests/fixtures/build_sh2_reset_bus.py) and [ROM](../tests/fixtures/sh2-reset-bus.hex) execute a warmed guest write one clock before WDT assertion. Both CPUs read `12345678` after manual reset and zero after power reset; changed guest operands produce `89ABCDEF` only in manual mode |
| Reset initialization and retained domains | `OnChip::reset_from_watchdog/finish_watchdog_reset`, `Watchdog::reset_preserving_cause`, cache control reset at assertion | `reset_assertion_initializes_modules_and_power_reset_releases_self_refresh_before_pulse_ends` checks SCI/CCR/UBC and peer access during the pulse. [Peripheral vectors](../src/onchip/peripheral_tests.rs) and the updated [SDRAM comparator vector](../src/bus.rs) check retained BSC/open-row timing | RSTBUS checks distinct power/manual vector entries, retained/reset BCR2 (`7C`/`FC`), disabled cache, watchdog cause and completion on both CPUs |
| Pulse completion versus bus completion | Reset remains pending until the granted cycle and its operand tail finish; new CPU/peripheral/DMA requests are blocked | A real VDP2 grant stall lasts 1282 main clocks, beyond the 512-clock WDT pulse; no reset-vector release occurs at514 or1281. The peer SCI completes independently during the drain | RSTBUS waits through actual WDT reset and scheduled vector reads, without host advancement of guest result state |
| Arbitration, diagnostic and board-reset boundaries | Reset-drain ownership retains the current unit, observes external UBC ownership, respects a peer TAS lock and cancels on SYSRES/CPU disable | `retained_manual_reset_buffer_cannot_break_the_other_processors_tas_lock`, `reset_cancels_unaccepted_buffer_and_system_reset_cancels_manual_drain`, and `reset_drain_preserves_cpu_and_exception_diagnostic_provenance` | Existing SYSRES, DMA, refresh/monitor and shared-bus guests remain enabled in the complete browser suite |
| Scheduled watchdog vectors | `Machine::issue_cpu`, per-CPU `cpu_reset_vectors` in [lib.rs](../src/lib.rs); PC and SP use separate replayed bus reads after reset release | `watchdog_reset_vectors_sample_at_separate_bus_completions_and_peer_runs` checks physical power/manual vector addresses, retained completed PC, later SP sampling, SR/VBR initialization, zero premature retirement and peer execution | [Native RSTBUS](../tests/sh2_reset_bus.rs) and [Wasm RSTBUS](../scripts/sh2-reset-bus.mjs) run the identical original BIOS, manual/power selections, operand mutations and repeated cold resets |

UBC register values after manual reset are undefined in H §6.2. Retaining them
is an explicit deterministic policy, not a claim that hardware guarantees them.
The BSC's documented configuration and SDRAM row retention are implemented;
partial cache-fill contents remain subject to the existing deterministic policy.
Cold-boot, SSHON and external-BIOS SYSRES now share scheduled vector loading
(STARTV below). Full CPU pipelines and other reset/peripheral/board contracts and the other mandatory Issue #17
subsystems still require completion. No umbrella acceptance checkbox is changed.

The fixture and generated ROM use the adjacent [MIT license](../tests/fixtures/LICENSE).
`python3 tests/fixtures/build_sh2_reset_bus.py --check` is required in CI alongside
the other 65 reproducibility commands. Expanded 512 KiB ROM SHA-256:
`e0bc3e201436e2f3b6b48649dd91b9552f4537780024640471b0501da270b628`.
Removing manual-reset draining makes RSTBUS fail; extending a canceled cache
refill past its current longword makes its independent bus vector fail. Both
intentional regressions were rejected and the production sources restored
byte-for-byte before final validation.


## SH-2 startup vectors, VBR alignment and IF comparison boundaries (STARTV/VBRG/IFUBC)

Primary references: [H] §§4.2.2–3, 4.8.2–3, 6.3.2, 6.3.7 table6.4,
7.3/7.11; [SMPC] §§2.3.2/2.3.8. Related to #17.

| Contract | Implementation | Independent native evidence | Guest/Wasm evidence |
| --- | --- | --- | --- |
| All hardware PC/SP reset paths use scheduled data phases | `Machine::schedule_cpu_reset/issue_cpu/step_hardware_boundary` in [lib.rs](../src/lib.rs); cold external BIOS, SSHON, external-BIOS SYSRES and both watchdog kinds share the replayed bus | [Machine reset vectors](../src/reset_tests.rs): PC completes at clock8 and SP at clock16 under reset WCR; later ROM mutation affects unsampled SP only; neither value nor instructions publish early. Cached master progresses during slave waits. SSHOFF cancels queued vectors behind a master video cycle; a later SSHON samples a fresh pair. SYSRES/new boot/rejected boot cancel old journals | [Native reset/exception guests](../tests/sh2_reset_bus.rs), [existing dual guest](../tests/dual_sh2.rs), [SYSRES guest](../tests/system_reset.rs); [production Wasm](../scripts/sh2-reset-bus.mjs) asserts initial zero PC/SP/instruction/time, completed waits before first execution, boot cancellation, changed reset SP and repeated cold boots; existing dual/watchdog/SYSRES guests retain their results |
| Misaligned VBR enters vector9 after the original exception entry | `Memory::exception_read32` in [bus.rs](../src/bus.rs), `Cpu::enter_exception/address_error` in [cpu.rs](../src/cpu.rs) | `unaligned_vector_tables_complete_entry_then_dispatch_one_repairable_address_error` in [CPU vectors](../src/cpu/tests.rs): offsets1/2/3 and vectors4/11/64/127, both frames/PC/SR, no premature instruction, bounded vector9 suppression and VBR repair | Original `sh2-vector-error.hex` executes TRAPA on both SH-2s, records nested frames and resume labels, repairs VBR and RTEs through the original frame. Native/Wasm assert two exceptions, original stack restoration, the skipped first handler and completion; Wasm also mutates the actual reset SP |
| SH7000-compatible IF matching includes words never executed | `Ubc::fetch_longword/native_mode`, `OnChip::instruction_fetch/instruction_break`, `CpuBus::fetch16` | `compatible_ubc_observes_both_if_words_and_native_ubc_waits_for_execution` in [IF vectors](../src/bus/fetch_tests.rs): both CPUs, cache hit/miss, entry at 4n/4n+2, retained word, native/compatible mode and compatible flag behavior | Original `sh2-overrun.hex` enters at 4n+2. The unused first word triggers a compatible break and never a native pre-execution break. Both guest CPUs record saved PC5002, zero compatible match flags and pre-execution R12; changed executed opcodes alter actual results. [Native](../tests/sh2_reset_bus.rs) and [Wasm](../scripts/sh2-reset-bus.mjs) repeat each mode/operand across cold resets |

The two new source-authored ROMs use the adjacent [MIT license](../tests/fixtures/LICENSE).
The existing `python3 tests/fixtures/build_sh2_reset_bus.py --check` CI command
reproduces all three reset-boundary fixtures. Expanded 512 KiB ROM SHA-256:

- VBRG: `6f7bce545a8e2a9d8c588bd335b7aa981c75b1fd05db7fb5dfdb0468cc888f8f`.
- IFUBC: `dca735641007de22dc2962826f9a064eb22106f369600d5b5e6e73bfaefd25be`.

Unaligned vector-cycle data uses the same explicit exact-address byte policy as
invalid exception stacking. It is not a claim of physical invalid-bus data.
IFUBC covers actual fetched longwords, including their unused words; complete
advance IF/MA/multiplier timing and interrupt-transition speculation still require
implementation/evidence. These changes do not satisfy the remaining CPU/peripheral,
68k, board, SCU/DSP, video, audio, CD/HLE, SMPC and comprehensive acceptance findings.
No umbrella acceptance checkbox is changed.


## MC68EC000 UNLK stack-register alias and sound-bus sampling

Primary specifications: [Motorola M68000PRM](https://www.nxp.com/docs/en/reference-manual/M68000PRM.pdf)
p.4-194 (UNLK operation order), [68UM] §§5.1/6.3.9.1 (word data cycles and
fault frames), table8-10 (12-clock UNLK). Related to #17.

`M68k::group4` now loads the selected address register before incrementing SP.
When the selected register is A7, the increment applies to the newly loaded
pointer; the previous generic pop incremented the old pointer before overwriting
it. Other An encodings retain their results, flags and cycle budget. A failed
operand word cannot publish a partial An load or the final stack increment.

| Contract | Implementation | Independent native evidence | Executing guest / production Wasm evidence |
| --- | --- | --- | --- |
| All address registers, SP alias, wrapping, flags and privilege modes | `M68k::group4` in [m68k.rs](../src/m68k.rs) | `unlk_loads_the_frame_register_before_incrementing_the_selected_stack` in [bus_tests.rs](../src/m68k/bus_tests.rs): 1536 literal operation-order cases, both operand-word addresses and no extra memory cycle | Original `UNLK A7` program in [m68k_families.py](../tests/fixtures/m68k_families.py) saves/restores its actual caller stack, writes its own frame, executes UNLK and records 6204. [Native](../tests/m68k_families.rs) and [browser Wasm](../scripts/browser-smoke.mjs) change the frame literal and require 6304, with every other family/fault record retained and repeated cold boots |
| First/second-word faults preserve completed pointer selection | Same checked `M68k::read` and architectural exception entry | `unlk_operand_fault_keeps_the_frame_pointer_and_does_not_increment_sp`: all eight An, both privilege modes, both fault words; vector2, selected/active/user stacks, original CCR, SSW, IR, exact fault address and no retirement | Existing 24 guest-authored fault records remain checked alongside all 74 family/EA cases; the new UNLK fault corpus itself is native evidence |
| Independent word-completion sampling and retained high half | Existing four-clock sound transaction journal in [transactions.rs](../src/sound/transactions.rs) | `unlk_samples_each_frame_word_at_its_own_sound_bus_completion` in [reset_tests.rs](../src/sound/reset_tests.rs): real MOVEA/UNLK/MOVE/STOP instructions; high word at60 and low at64 sound clocks. A shared-RAM change between them produces 00066304, with no partial register publication at63, identical for chunks 1/3/17/10000 | The source-authored family program runs through this same production sound adapter in native/Wasm, retains its caller stack, finishes and produces guest-configured nonzero PCM |

The source, ROM and TSV use the existing [MIT fixture license](../tests/fixtures/LICENSE).
The already-required `python3 tests/fixtures/build_m68k_families.py --check`
reproduces the 74 cases and updated fault-PC labels. The fault programs retain
all instructions, data operands, exception kinds and register expectations; only
source relocation changes their recorded instruction addresses. No demonstration
media or public runtime assets change.

Expanded 512 KiB family BIOS SHA-256:
`fdfbc9457de3504e4a979ab29f26a22ae7b5b2f0c1f543d67161dd75f8df6733`.

- `m68k-families.hex`: `67dc21944dd12c126f5cda5b336e712cdc390bf0267d45a88d633497a7d78d77`.
- `m68k-families.tsv`: `95ff8106e1c39a7aad792007774dd0b448c12a8269bffe732ebd987ac958d7d5`.
- `m68k-faults.tsv`: `b48311bbe742a0fa3eff61430437292cd6b92c362e915af3c6780805ae3ad065`.

An intentional restoration of the former pop operation makes the native
original-guest mutation test return 6300 instead of 6304 and fail. The production
source was restored before complete validation. This entry addresses the UNLK
contract; the full mandatory CPU, board, SCU, video, audio, CD/HLE, SMPC and
comprehensive acceptance findings above remain open on this Draft PR. It does
not establish full Issue #17 resolution or change an umbrella checkbox.

## Vertical counter and VDP1/SMPC memory boundaries

Primary specifications: [VDP2 §2.5, Table 2.4](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/hard/vdp2/hon/p02_50.htm),
[VDP1 §5.3, Tables 5.2–5.3](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/hard/vdp1/hon/p05_30.htm),
[VDP1 §6.3, color calculations](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/hard/vdp1/hon/p06_37.htm),
and [SMPC §§2.3–2.4, OREG31/SF](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/hard/smpc/hon/p02_40.htm).
Related to #17.

| Contract | Implementation | Independent native evidence | Guest and browser evidence |
| --- | --- | --- | --- |
| VCNT bit layout | `Raster::counters` in [timing.rs](../src/timing.rs): noninterlace, single-density and exclusive modes return the line; double-density inserts even-field parity at bit0 | `vertical_counter_uses_v0_except_for_double_density_field_parity`: all horizontal encodings, four fields, seven counter values including bit8/9 boundaries. [MMIO regression](../tests/devices.rs) reproduces the clock1748 latch and checks retention. Existing both-port external-latch tests retain HCNT behavior | [60 NTSC/PAL mode guests](../tests/display_modes.rs) and [INTC guest](../tests/intc.rs) use the corrected expectations in native and browser Wasm. Raster-history, sprite-rotation and display-fetch builders now poll unshifted normal-mode lines; their original pixel expectations are retained |
| Gouraud table reads | `Drawing::shade_read`, `Vdp1::clocks_to_event/advance_phase` in [vdp1.rs](../src/vdp1.rs): fetch/retain A/B/C/D separately, only for shading; pause on VRAM ownership; framebuffer ownership is independent | [progress_tests.rs](../src/vdp1/progress_tests.rs): before/after-word mutations, literal top/bottom gradient pixels, VRAM/framebuffer contention, all four partial-fetch stop points and restart. [Real bus arbitration vectors](../src/bus/vdp1_tests.rs) additionally grant both CPUs, both DMACs and SCU while retaining unfetched versus captured vertices and the final pixel/IRQ deadline. Chunk sizes 1/3/17/10000 produce identical outputs | [Original effects guest](../tests/fixtures/build_video_transactions.py) draws 16 independent eight-dot polygons, measures the extra fetch time through FRT, reads pixels/EDSR/COPR/SCU status; [native](../tests/video_transactions.rs) and [Wasm](../scripts/video-transactions.mjs) repeat cold boot and mutate the actual guest color operand |
| Six-clock framebuffer calculations | `PendingPixel`, `Vdp1::plot/advance_phase`: shadow, translucency and Gouraud translucency retain the sampled background and physical target through the five remaining clocks; no-write shadow still consumes time | `framebuffer_calculations_take_six_clocks_and_retain_the_sampled_background`: RGB/palette backgrounds, retained read versus later memory write, suppressed shadow write and IRQ after final publication. Contention/cancellation vectors cover independent VRAM, blocked framebuffer, ENDR, PTMR and SYSRES | The effects guest compares replace/half-luminance, shadow/translucency, Gouraud and combined operations. FRT must distinguish 128 extra Gouraud clocks and 640 extra RMW clocks across 128 dots, within the guest polling interval. Pixel and IRQ results are independently authored |
| INTBACK command report | `Smpc::advance_clocks` in [smpc.rs](../src/smpc.rs) restores OREG31=10H when publishing status, preserving the code accepted at command start | `command_report_survives_status_publication_continue_break_and_rejected_writes`: seven command latencies, last-clock completion, rejected double issue, status-only/status+peripherals, CONTINUE/BREAK and reset domains | [Original command-report guest](../tests/fixtures/build_smpc_boundaries.py), [native NTSC/PAL execution](../tests/smpc_boundaries.rs) and [browser local-BIOS execution](../scripts/hardware-boundaries.mjs), repeated cold reset. Existing eight-area sound/CD status guest now expects10H rather than zero in OREG31 |

The Gouraud word cadence uses the existing two-main-clock VDP1 VRAM transaction
model. The manual establishes vertex ordering and six-times replacement cost for
shadow/translucency; this evidence does not establish every physical texture/LUT
fetch phase, shading rounding, repeated coverage edge, erase/swap boundary or
VDP2 half-line transition. Those review findings retain their mandatory scope.
No umbrella acceptance criterion is marked complete by this evidence.

The new guests and generated artifacts use the existing [MIT fixture license](../tests/fixtures/LICENSE).
The existing CI commands `build_video_transactions.py --check` and
`build_smpc_boundaries.py --check` reproduce both additions. Expanded 512 KiB ROM
SHA-256 values:

- VDP1 effects timing: `c5f756beede69e5596adf4f38e1795ba6fc69361808f63ec0eb03a31b50c5e7d`.
- SMPC command report: `96e77242bb186adf71fbf88d2b1efd44ae178e9e51b0d2db4c0dc18a9e0e0fbf`.


## SCU byte fringes and separate B-bus halfwords (FRINGE)

Primary references: [SCU][SCU] §2.1 p.16/fig.2.1 and §3.2 pp.44–45/
figs.3.6–3.8; [SH7604][H] §6.3.4 for external UBC address comparison.
A two-byte head/tail is two byte transfers, even when word aligned. A complete
SCU longword crosses the B-bus as two physical halfwords. These are separate
contracts: the low half of an already started longword is not a new byte fringe.

Implementation: `Channel::beat` and `Transfer::write_unit` in
[scu/dma.rs](../src/scu/dma.rs), with accepted phases completed by
[SCU transactions](../src/scu/transactions.rs) and
[board transactions](../src/bus/transactions.rs). `ScuUnit` retains each channel's
accepted width, sampled prefix and remaining physical beats. CPU-bus 8/16/32-bit
widths and A-bus 8/16-bit widths drive separate data phases, not atomic longwords.
Each B-bus read samples at its own completion; buffered data survives priority preemption. Each B-bus write
publishes separately. Only the final write signals the completion interrupt.
Cancellation discards an unfinished unit without resampling or later writes.

Independent evidence:

- `all_channels_direct_and_indirect_use_byte_fringes_for_every_alignment_and_short_count`
  in [scu/dma.rs](../src/scu/dma.rs) checks **7,296** direct/indirect transfers:
  all three channels, every source/destination alignment, counts 1–19 and all
  four work-RAM/B-bus routes. Independently constructed transaction lists check
  logical read units and physical B-bus write strides, copied bytes, untouched guards, pointer
  updates and exactly one completion.
- Three `scu_b_bus_*` / `scu_partial_b_bus_*` vectors in
  [board transactions](../src/bus/transactions.rs) check separate read sampling
  at clocks 10/18, destination publication and IRQ at 28, partial B-bus writes
  at 20/28, source mutation between phases, channel 2 preemption of channel 0,
  and SYSRES at seven phase boundaries. Four advancement chunk sizes agree.
  CPU-bus vectors separately check 8/16/32-bit
  publication at 68/36/20, source changes after the first sample, higher-channel
  preemption of a partial byte unit and reset followed by a different bus width.
  A-bus vectors check all three programmable selects, per-beat NW3 at 8/16-bit
  widths, exact completion at 56/34 and UBC comparison of the skipped A+3 address.
  The VDP1 ownership regression now accounts for two byte reads of count 2;
  its command read remains blocked until the actual second grant ends.
- `final_sampled_source_unit_can_end_at_the_map_boundary_and_still_drain_to_destination`
  in [SCU vectors](../src/scu/tests.rs) checks counts 1–8, all three channels,
  direct/indirect transfers ending at real work-RAM/framebuffer boundaries.
  `Channel::beat` validates the source only while unread bytes remain; its
  advanced pointer cannot invalidate already sampled data before the last write.
- Original MIT [builder](../tests/fixtures/build_scu_fringe.py),
  [synthetic BIOS](../tests/fixtures/scu-fringe.hex) and authored
  [goldens](../tests/fixtures/scu-fringe.tsv) execute **240** scenes through
  [native integration](../tests/scu_fringe.rs) and the production
  [browser Wasm helper](../scripts/scu-fringe.mjs), required by the complete
  [browser suite](../scripts/browser-smoke.mjs). Work RAM, sound RAM and framebuffer sources
  transfer to work/sound RAM on all three channels, direct and indirect.
  UBC compares source/read and destination/write byte addresses that an
  incorrect word transfer skips. The guest checks both flags, executes 480 UBC
  and SCU IRQ handlers, and copies data from its own literals. A complete-longword scene additionally
  requires the programmed 16-bit work-RAM bus to expose the second halfword
  to UBC. Changed literals
  and repeated cold boots retain all independent register/data expectations.
- Restoring word-sized fringes deliberately fails that native guest. Replacing
  separate physical bus phases with an atomic longword deliberately fails
  the B-bus read-phase vector. The same restoration independently fails
  the expanded native guest. Restoring the exhausted-source validation also
  fails the independent boundary vector. Production sources are restored
  before final checks.

Reproduce: `python3 tests/fixtures/build_scu_fringe.py --check`.
The adjacent [MIT license](../tests/fixtures/LICENSE) covers source and generated
media. Expanded 512 KiB BIOS SHA-256 and performed checks are recorded in
[validation](validation-issue-17.md). All three original demonstration discs
are preserved.

This establishes the specified byte-fringe and B-bus-unit behavior. Complete
shared sound-memory ownership, external WAIT sampling, descriptor constraints, trigger/reset/arbitration boundaries and
full SCU/DSP acceptance still require implementation/evidence. It completes no
umbrella checkbox and does not establish full hardware conformance.

## Retained VDP1 tables and characters, 68k EA waits and CD transfer completion

Related to #17. Primary references: [VDP1] §2.1 (independent local memories),
§3.1 steps5–6 (table loading before raster), §§5.1–5.2/fig5.1 (physical words,
character packing and the sixteen-entry LUT); [68UM] table8-1, tables8-2/8-11
(EA and multiprecision clocks); [CD] §3.2 table3.2 (command-end flags),
§5.3.3 and §8.2.7 function7.5 (host/filter connection and early-ended writes).

| Programming effect | Implementation | Independent native evidence | Executing guest / Wasm evidence |
| --- | --- | --- | --- |
| Lookup-table loading precedes character drawing | `Drawing::tables_pending`, `Vdp1::advance_phase/draw_dot` in [vdp1.rs](../src/vdp1.rs) retain sixteen separately read LUT words per command, alongside the existing Gouraud words | [table_tests.rs](../src/vdp1/table_tests.rs): 544 capture/flip/shade/chunk cases, all sixteen word boundaries; partial fetch ownership, stop/restart cancellation and exact modeled END deadline | Original [lookup guest](../tests/fixtures/vdp1-lookup-fetch.s) changes the palette while a 504×4 sprite draws. The current command retains old colors; the following command uses all sixteen new entries. [Native](../tests/video_transactions.rs) and [production Wasm](../scripts/video-transactions.mjs) repeat cold boots with changed guest palette operands |
| Character reads retain the complete physical word | `Drawing::texture_address/texture_pending`, `Vdp1::texture_coordinates/advance_phase/draw_dot` separately fetch a VRAM word and publish framebuffer dots; a word carries four nibbles, two bytes or one RGB dot | 864 capture/format/flip/chunk cases in [table_tests.rs](../src/vdp1/table_tests.rs); mutation before/after fetch, independent VRAM/framebuffer owners, retained RGB translucency background and next-word blocking. [progress_tests.rs](../src/vdp1/progress_tests.rs) preserves unfetched-word mutation and prior stop/reset/erase vectors | Original [character guest](../tests/fixtures/vdp1-character-fetch.s) changes a selected texture word during scaling. Existing captured nibbles remain old; a following command reloads both changed words. The same native/Wasm helper checks pixels, EDSR, COPR, draw IRQ, changed texture operands and cold resets |
| Indexed and predecrement EA internal clocks precede operand sampling | `Memory::m68k_internal_wait`, `M68k::index/get/internal_wait`, replayed `InternalWait` in [sound/transactions.rs](../src/sound/transactions.rs); waits drive no memory cycle, paired predecrements share one interval, MOVE write-only destinations add none | [bus_tests.rs](../src/m68k/bus_tests.rs): 1,200 family/register/width/EA cases place the wait before the first operand; [sound-clock vectors](../src/sound/reset_tests.rs) execute MOVEA/MOVE/STOP, mutate shared RAM immediately before the actual read at clocks62/66, and cancel five partial waits/reads through SNDOff/SNDOn | Existing original [family/fault guest](../tests/m68k_families.rs) runs through the changed adapter natively and in the complete browser suite, retaining all family/EA results, exception frames and nonzero PCM |
| Early DataEnd retains all reserved PUT sectors and routes them through the selected filter | `CdBlock::execute` command06 and `route_from` in [cd.rs](../src/cd.rs); transferred prefix retained; unspecified unwritten bytes use deterministic zero fill; complete reservation released on reset | [CD vectors](../tests/cd.rs): four sector lengths × eight short/full transfer boundaries, all three reserved sectors, alternate DATATRNS word addresses, target partition7 rather than filter3, exact prefix/count/guards/free capacity, get-delete reclamation | Expanded original [CD drive builder](../tests/fixtures/build_cd_drive.py) writes only one word of a two-sector reservation, reads both through real FIFO and checks routing/capacity/counts. [Native](../tests/cd_drive.rs) and [browser](../scripts/browser-smoke.mjs) repeat cold boots and mutate the actual guest data operand |
| Completion IRQ follows command class | Explicit `Transfer::host_io`, not payload length, controls EHST; TOC/Q/file-info transfer end clears DRDY without raising sector-I/O completion | Independent register vectors cover no-read and partial TOC, Q, single/all-file-info ends plus GET/GET-DELETE completion | The same CD guest clears EHST, ends a short TOC, then ends PUT; it must record zero and80H respectively, along with the actual word counts |

The table/character fetch model uses the existing two-main-clock VRAM-word
cadence; replacement publication takes one clock and framebuffer RMW six.
These tests establish retained data and independent memory ownership in that
model, not every physical VDP1 prefetch overlap or command-dependent bandwidth.
Character retention is per raster line. Scaling/end-code/preclip/duplicate-edge,
erase/swap and half-line boundaries remain part of mandatory video acceptance.
Additional [board vectors](../src/bus/vdp1_tests.rs) grant both CPUs, both
DMACs and SCU access while lookup/character reads are active, with four
advancement chunk sizes per owner. They assert retained versus unfetched LUT
entries, texture loading during framebuffer ownership, all eight final pixels
and the exact modeled END/IRQ boundaries. SCU byte fringes interleave independent
work-RAM writes; the test preserves the resulting overlap with VRAM reads.
The two-clock 68k EA intervals do not complete instruction-specific prefetch,
DIV microsequencing, exception/IRQ phases or shared sound-memory arbitration.
CD command-specific operation timing, copy/move failure handling, concurrent
transfers, subcode/media and HLE-library requirements remain mandatory.

Reproduce both new VDP1 fixtures with
`python3 tests/fixtures/build_video_transactions.py --check`, and the expanded
CD guest with `python3 tests/fixtures/build_cd_drive.py --check`. All original
source and generated fixtures use the adjacent [MIT license](../tests/fixtures/LICENSE).
Expanded 512 KiB synthetic BIOS SHA-256 values:

- Lookup: `5a219c44761a94ae832e1537b9eb41ebf9ae76e9e6800e422639f19324304c5f`.
- Character: `332b1cdf5325cac6643131c8919d8b5a34a14464c7996d710230dfdfc867ab0f`.
- CD drive: `9fae535596f08c4744b6f93be2071bdaff6aa0cb81e4a4ffc91e1b0d9e7756c2`.

All three demonstration discs retain their original source, bytes and licenses.
The full CPU/peripheral, bus/reset, SCU/DSP, video, SCSP/DSP, CD/HLE, SMPC and
coverage criteria remain mandatory; this evidence does not mark an umbrella
checkbox complete or establish that Issue #17 can close.


## Scheduled CD selector transfers and independent command classes (CDSEL)

Primary behavior: [CD] §3.2 tables3.6–3.7 (independent command-end classes),
§3.4 (host FIFO), §8.2.7 functions7.5–7.8 (PUT/copy/move/copy error).
The later original [function specifications](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/prgg/sysg/cdif/hon/p08_17.htm)
clarify insufficient-space WAIT, errors during processing and retention until
another accepted copy/move. Interface values COPY_OK=0, COPY_NG=1 and
COPY_BUSY=FF follow the original SEGA_CDC.H declarations; no SDK routine is used.

| Programming behavior | Implementation | Independent native evidence | Guest / production Wasm evidence |
| --- | --- | --- | --- |
| Copy/move consumes hardware time and ends with ECPY | `CdBlock::execute/copy_sector/clocks_to_event/advance` in [cd.rs](../src/cd.rs); immutable selected sector identities, raw 2352-byte copies, no extra allocation for moves | [CD vectors](../tests/cd.rs), `selector_copy_move_deadlines_flags_and_raw_data_cover_every_source_and_target`: 24×24×2 routes, self-targets, both sector payloads, complete raw bytes, no early/intermediate IRQ, final IRQ and BUSY/OK | Original [SH-2 builder](../tests/fixtures/build_cd_copy.py) and [ROM](../tests/fixtures/cd-copy.hex) run in [native](../tests/cd_copy.rs) and [browser](../scripts/cd-copy.mjs); 32-sector copy and move, actual FIFO words, repeated cold boot and changed guest source operand |
| Capacity, partial failure and error retention | Initial lack of space returns WAIT without changing old error or signaling ECPY; later exhaustion/disconnection/missing source ends with NG and retains the completed prefix and unprocessed source | Capacity reservation race, exact simultaneous drive/copy deadline in four advancement chunk sizes, full 200-sector move, target disconnect/reconnect, source reset, accepted-job exclusion and sticky error vectors; all deadlines use exact clock bounds | Guest creates a full pool, observes copy WAIT, moves32 sectors without allocating, detaches the target, cancels a job by software reset, and forces a concurrent PUT reservation failure |
| Independent command classes and retained FIFO work | ESEL/ECPY/drive/query commands may run during DATATRNS; competing FIFO/EHST requests return WAIT. PUT retains accepted length; GET/DELETE selects sector identities rather than shifting indices | Four length-change vectors check actual bytes/count/capacity and competing requests. A move of an earlier source sector cannot make GET/DELETE remove the wrong neighbour | Guest opens PUT during copy, changes the next PUT length, polls actual HIRQ/error/count registers, and later reads the retained PUT bytes and both copied/moved payloads |
| Cancellation and advancement invariance | Software reset cancels the job and resets its error; CD power reset cancels all transactions; media mount cancels the host-side selection job | 42 software-reset/chunk boundaries plus 12 power/media boundaries, source removal, no late append/IRQ; existing CD power vectors stay enabled | Guest resets a running copy then queries error and empty destination; complete existing native/Wasm CD and demonstration fixtures remain required |

**Timing policy:** one sector completes every 2352 reference clocks. The manuals
specify asynchronous completion and software-visible failure, not the physical
copy-engine bandwidth. Drive allocation precedes a simultaneous selector-copy
allocation; command effects follow both. This explicit deterministic policy is
not a claim of measured decoder clocks or complete physical contention.

All source/generated bytes use the adjacent [MIT fixture license](../tests/fixtures/LICENSE).
Reproduce with `python3 tests/fixtures/build_cd_copy.py --check` (also in CI).
Expanded 512 KiB ROM SHA-256:
`f05f0dee329351be8e1408679e700d3c097d4eff8b339e09cc25baad463916d6`.

This implements additional CD selector/host contracts. Full CPU/peripheral,
board/reset, SCU/DSP, video/audio, CD/media/HLE, SMPC and comprehensive acceptance
remain mandatory. No umbrella checkbox is satisfied by this fixture alone.

## Display blanking, VDP1 pre-clipping and retained VDP2 words

Related to #17. Primary references: [VDP1] §6.3 p.83/fig.6.6 (pre-clipping),
[VDP2] §3.3 tables3.2–3.5 (display paths) and
[VDP2 §2.5 TVSTAT](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/hard/vdp2/hon/p02_50.htm).

| Programming behavior | Implementation | Independent native evidence | Executing guest / Wasm evidence |
| --- | --- | --- | --- |
| DISP=0 forces TVSTAT.VBLANK=1; raster events continue independently | `Vdp2::read16` in [vdp2.rs](../src/vdp2.rs) derives the status bit without changing `Raster` or SCU signaling | [vdp2_status.rs](../tests/vdp2_status.rs): exact clock100 reproduction, HBlank, before/at VBlank-in/out, two fields, four advancement sizes, matching counters and SCU interrupt edges | Original [builder](../tests/fixtures/build_vdp2_status.py), [native](../tests/vdp2_status.rs) and [Wasm](../scripts/vdp2-status.mjs): guest compares display-on/off status with actual SCU status, polls VCNT during forced blank, changes the DISP operand and repeats cold boots |
| Pre-clipping rejects separated raster lines, truncates horizontal/vertical traversal and may invert its direction | `Drawing::clip`, `RasterLine::preclip`, `Vdp1::raster_line/texture_address/draw_dot` in [vdp1.rs](../src/vdp1.rs); UV/shading endpoints follow inversion, interpolation retains the original denominator, rejected lines acquire neither character nor framebuffer memory | [preclip_tests.rs](../src/vdp1/preclip_tests.rs): -4096/4095 endpoints in four directions, exact modeled completion, unchanged visible pixels, four separated diagonals, user/system intersection, hidden end-code pair and independently authored Gouraud pixels. [Bus vector](../src/bus/vdp1_tests.rs) checks earlier END/IRQ after a newly captured XC=0 | `vdp1-preclip` from the existing [video builder](../tests/fixtures/build_video_transactions.py), [native](../tests/video_transactions.rs) and [Wasm](../scripts/video-transactions.mjs): hidden end codes, four literal visible pixels, clipped separate command, EDSR/COPR/draw IRQ, Pclp operand mutation and cold resets |
| Fetched packed words survive subsequent VRAM writes | `fetch_word/fetch_byte` and scheduled normal packets in [fetch.rs](../src/vdp2/fetch.rs); rotation retains its independent first-use serializers; layer/path latches reset at line start | Four-/eight-bit packed-dot mutations at each interior word boundary, unfetched neighbour, next-line reload, debugger snapshot independence, separate name/character/cell/layer paths and mirrored address in [fetch vectors](../src/vdp2/fetch.rs) | `vdp2-word-retention` from the existing [fetch builder](../tests/fixtures/build_vdp2_fetch.py), [native](../tests/vdp2_fetch.rs) and [production Wasm](../scripts/video-transactions.mjs): guest writes VRAM on line16 during 256× enlargement; all old packed dots remain, line17 reads the new word; changed original texture and cold resets |

The historical retained-word increment sampled on first display use. Normal
backgrounds now acquire through scheduled slots, as recorded in
[Issue #19 validation](validation-issue-19.md); rotation keeps the historical
serializer path. This does not complete all physical display-fetch ownership or
raster/bank transition contracts.
The existing functional VDP1 pixel/word cadence is retained: rejected lines take
one decision clock; these vectors do not claim measured pre-clipping overhead
or complete sloped/repeated-edge coverage. Other review findings and mandatory
umbrella acceptance remain unresolved; this evidence does not close #17.

The VDP1 progress/ownership fixtures explicitly set Pclp=1 where they deliberately
walk offscreen to test unfinished drawing and contention. The progress, shrink
and SMPC boundary guests now enable TVMD.DISP before polling actual blanking
transitions. Their previous disabled-display polls relied on the TVSTAT bug;
their existing framebuffer, packet and result goldens are retained.

All new source and ROM bytes use the adjacent [MIT fixture license](../tests/fixtures/LICENSE).
The three builders above run in CI; the expanded 512 KiB ROM SHA-256 values are:

| ROM | SHA-256 |
| --- | --- |
| `vdp2-status` | `2f75d635d32cfb2b6a9371b8709c004298d64f5e0e5a0a07a303bedf375aeb4f` |
| `vdp1-preclip` | `0a3a311632d5998f5c245313d73ffe172bbb1968d0b6a37131965ec8ba86d222` |
| `vdp2-word-retention` | `242a1e1f3e832525a12463da39937ba47a5f814f0e5d66a77e862f0edf4e9c69` |

## MC68EC000 instruction completion and trace/reset boundaries

Primary references: [68UM] §§6.3.1/6.3.2/6.3.8 (reset, interrupt and trace
ordering), §8.3/table8-4 (ALU/MUL/DIV clocks), §8.6/table8-7 (shift clocks),
and §8.12/table8-14 (exception clocks). [H] §11.2 defines the FRT's byte-wide
counter and temporary-byte latch used by the independent SH-2 guest.

| Programming behavior | Implementation | Independent native evidence | Executing guest / production Wasm evidence |
| --- | --- | --- | --- |
| Register results retire after the remaining internal instruction clocks | `Memory::m68k_finish_clocks`, `M68k::step/finish_clocks` in [m68k.rs](../src/m68k.rs), and `FinishClocks` in [sound transactions](../src/sound/transactions.rs) | [sound-clock vectors](../src/sound/reset_tests.rs), `register_results_and_retirement_wait_for_the_instruction_internal_clocks`: DIVU/DIVS, MULU/MULS, ADD.L and variable-count LSL.L at the preceding clock and completion, four advancement sizes; first MOVEQ retires at 44 after reset finishes at 40 | The original DIVU/DIVS guests below execute actual division instructions and preserve all inherited sound/demo guests |
| Trace frame writes and vector fetches follow completed computation | The completion barrier precedes `deliver_exception`; replay retains completed memory cycles without repeating MMIO | `traced_division_cannot_stack_or_fetch_its_vector_during_the_divide`: all frame words, first write deadline, computed result/CCR/PC, late vector change and exception completion. `irq_arriving_during_division_waits_for_retirement_and_its_own_exception_clocks` checks IACK, first stack write and the 44-clock IRQ boundary | Original MIT [builder](../tests/fixtures/build_m68k_reset.py), [DIVU SH-2 source](../tests/fixtures/m68k-retire-divu.s), [DIVS SH-2 source](../tests/fixtures/m68k-retire-divs.s) and literal 68k words. SH-2 waits using FRT then changes vector9 while the independent sound CPU is dividing. Both handlers record quotient/remainder, stacked SR/PC and their own marker in sound RAM. [Native](../tests/m68k_reset.rs) and [Wasm](../scripts/m68k-reset.mjs) require the late handler, changed dividend results, repeated cold boots and the old handler when the guest vector store is replaced by NOP |
| Reset discards unfinished computation and retains a completed result during trace entry | `M68k::retired_state` records the architectural completion; the adapter keeps a separate replay origin until exception bus work completes | `sound_off_discards_unretired_division_and_on_retains_only_completed_registers`: five pre-retirement reset boundaries. `reset_during_trace_keeps_the_already_completed_division_result`: eleven trace-phase reset boundaries. Three advancement sizes each; the restarted 68k stores its retained register through real sound RAM | Existing guest SNDON/SNDOFF and traced RESET cases remain in the same native and browser helper |

Reproduce with `python3 tests/fixtures/build_m68k_reset.py --check`, already
required by CI. Source and generated fixtures use the adjacent
[MIT license](../tests/fixtures/LICENSE); no external firmware or SDK routine.
Expanded 512 KiB ROM hashes:

- DIVU: `3667918fa9c06e6f18e042f02a5fab8720a2c811551b00dfe8c2e3e3f45d6b5e`.
- DIVS: `8f3e796705789dfef48a5f9e1533a8f36ea9e240c8a23a5730ee8a6492ec9d54`.

The completion barrier establishes these architectural boundaries, not every
instruction's physical prefetch/operand/exception microsequence or shared
sound-memory arbitration. DIVU/DIVS still use the manual's maximum functional
times, 140/158 clocks plus EA; their data-dependent physical timing remains
required. Already completed operand writes remain visible before retirement;
the barrier never rolls back or repeats them. Other mandatory CPU/peripheral,
bus/reset, SCU/DSP, video/audio, CD/HLE, SMPC and coverage work remains open.
This evidence does not complete an umbrella acceptance checkbox or resolve #17.


## SCSP shared DRAM DMA, refresh and CPU request priority

Related to #17. [SCSP] §3.2 pp.20–21, fig.3.2 specifies 128 memory
cycles per sample, two refresh cycles and the priority PCM/DSP > refresh >
internal DMA > main CPU > sound CPU. §4.2 pp.101–102, fig.4.66 and
tables4.39–40 define word transfers, increasing addresses, gate/direction,
DEXE=0 preservation and completion. This replaces the old one-word-per-sample
internal DMA implementation. It preserves the independently scheduled DSP.

| Contract | Implementation | Independent native evidence | Guest / production Wasm evidence |
| --- | --- | --- | --- |
| DMA grants and completion are separate DRAM-cycle boundaries; a word retains its address, direction, gate and RAM geometry; source data remains live until completion | `Scsp::memory_cycle`, `Word` in [dma.rs](../src/sound/dma.rs); `Sound::scsp_deadline/advance_domain` in [sound.rs](../src/sound.rs) | [DMA vectors](../src/sound/dma/tests.rs): four word deadlines, both directions, zero gate, source changes, address/geometry crossing and separate sound/main interrupt acknowledgement | [Original builder](../tests/fixtures/build_scsp_dma.py), [native guest](../tests/scsp_dma.rs), [Wasm/WebAudio](../scripts/scsp-dma.mjs): real SH-2 FRT times a 64-word RAM-to-COEF transfer; real 68k reads transferred COEF0, computes positive/negative PCM and configures its voice |
| DSP and refresh precede DMA; accepted words finish before the following owner; transfer progress/DEXE/IRQ publish only after completion | `Scsp::memory_cycle`, `Sound::ram_busy` | Literal DSP123/refresh124/DMA125/refresh126/DMA127 ordering; retained DSP read/IWT; exactly two refresh reservations per frame; no drift over four frames; reset cancels only unfinished work; SNDOFF leaves DMA running; a step127 word completes before the next PCM source capture | Guest data/coefficient changes alter the computed bipolar stereo. Omitting the actual guest DEXE store leaves the 68k polling, with no completion or sound. The guest rejects the old sample-paced DMA baseline (FRT=4864, outside the authored 32–99 interval) |
| SH-2/SCU beats wait while internal DMA owns DRAM; buffered main-interface requests precede simultaneous SCPU data phases | `Bus::sound_ram_request/advance_fixed_clocks`, [board transaction completion](../src/bus/transactions.rs), `Sound::main_ram_request`, [68k pending-cycle classification](../src/sound/transactions.rs) | [Board vectors](../src/bus/sound_tests.rs): both SH-2s, all SCU channels, retained writes, last-word/IRQ boundary; main grant43/completion48 precedes SCPU grant48/completion53 and retains that high word through the later low read. Internal SCSP reset prefetches do not request DRAM | Existing SIOG, bus, sound, reset, DSP, demonstration and audio guests continue executing actual memory/MMIO and IRQ handlers |

The original synthetic BIOS and generated SH-2 source use the adjacent
[MIT license](../tests/fixtures/LICENSE). Reproduce with
`python3 tests/fixtures/build_scsp_dma.py --check`, also in CI. Expanded
512 KiB ROM SHA-256:
`efc6a059e988a4e6ce1a98b9b642b622b33a8907457ba28b9c4d5f8af8fb0a99`.
Native/Wasm repeat cold resets and three source operands, requiring actual
transferred end words, computation markers, non-silent positive/negative stereo,
no CPU exceptions and deterministic results. Production AudioWorklet must report
the expected peak, rendered frames, explicit underflow and zero dropped frames.

Main-priority scheduling exposed two original guest races. The reset-vector guest
now publishes a separate completion flag after both halves of its result;
the sound-I/O and envelope guests clear stale timer requests **before** arming
the timer. No result/pixel/audio assertion was weakened. Their updated hashes
are recorded in the corresponding entries above, and their original builders
remain CI reproducibility checks.

The PCM implementation below supersedes the first-step source snapshot.
Refresh requests originate at cycles124/126; PCM/DSP can defer them, including
across frame edges. Exact physical CPU-interface request/grant phases, complete
synthesis/envelope/DSP/converter vectors and the other mandatory CPU, board,
SCU, video, CD/HLE, SMPC and per-feature demonstration/acceptance evidence
remain required. The DMA tests alone do not establish those contracts. CPU cycle admission and
retention are now implemented in the [CPU allocation increment](#cpu-sound-memory-allocation).


## Separately retained PCM operands and sound-memory ownership

Primary reference: [SCSP] §2.2 (first-order interpolation), §3.2 pp.20–21,
fig.3.2 (128 DRAM cycles/sample, two reads/slot, PCM/DSP priority, EG3FF
suppression), §4.2 p.72 (SDIR bypass), fig.4.30 (slot input/output order).
Implementation: [PCM slot/read pipeline](../src/sound/pcm.rs),
[shared DRAM grant/completion](../src/sound/dma.rs), and
[fixed-domain scheduling](../src/sound.rs), `sample_cycle`, `begin_slot`,
`grant_pcm`, `complete_pcm`, `end_slot`, `memory_cycle`, `scsp_deadline`.
No guest identification, host-written result, waveform snapshot or canned audio
is used.

| Contract | Independent native evidence | Executing native / production Wasm evidence |
| --- | --- | --- |
| Two separately granted/read operands; signed PCM8 and aligned PCM16; first sample survives later writes; second sees its own completion-time data | [phase vectors](../src/sound/pcm/tests.rs): slots 0/7/31, four advancement sizes, signed half-sample arithmetic, shared-word byte reads, accepted geometry versus later MEM4MB changes and retained SA | Original MIT [builder](../tests/fixtures/build_scsp_pcm.py), [SH-2 source](../tests/fixtures/scsp-pcm.s), literal 68k program, [native](../tests/scsp_pcm.rs) and [browser](../scripts/scsp-pcm.mjs). Real 68k stores alternate one waveform word; a half-rate one-sample loop produces the arithmetic midpoint only when a write separates the two reads. Three operand pairs, repeated cold boots and omitted-KEYON silence control |
| EG3FF suppresses PCM; SDIR bypass and unrouted PCM retain reads; noise/zero need no waveform bus | Native vectors check actual busy cycles, generated silence, continuing phase/EG/timer behavior and all source selectors | The second original [SH-2 guest](../tests/fixtures/scsp-pcm-arbitration.s) uses FRT to time 64-word internal DMA with 32 maximum-attenuation voices, 32 direct PCM voices and 32 noise voices. Independently bounded native/Wasm intervals require the PCM waits and their absence for quiet EG/noise; actual transferred coefficients and repeated resets are also asserted |
| PCM/DSP precede refresh/DMA/main/sound; accepted DMA word completes before the next grant; refresh is retained when both high-priority paths are full | Native PCM vectors cover 64 PCM grants/frame, full DSP traffic and deferred refresh, retained DMA data/IRQ, and completed MIXS routing. [Board vectors](../src/bus/sound_tests.rs) check both SH-2s and all three SCU channels publishing after the second read | The timed 68k waveform guest runs through the real sound-memory arbiter; FRT/DMA guest verifies the aggregate ownership in production Wasm |
| Reset cancels either unfinished operand and preserves completed RAM; exact stereo reaches the worklet | Native phase vectors reset after either grant/first completion, then start a new voice with distinct RAM | Both guests execute the external synthetic BIOS path; the browser checks production AudioWorklet peak, rendered frames, zero drops and explicit underflow for all three waveform mutations |

Expanded 512-KiB ROM SHA-256:

- PCM read guest: `d604f2162e8611354e160ee0bbc5ac767045b5055030add104b8b30b8d161ade`.
- PCM/EG/noise arbitration guest: `2c6ff0ef8edabb8c140c2342c9f95cf192d693ce3cfd3b10a0d72d0f37653841`.

Both are wholly original under the adjacent [MIT fixture license](../tests/fixtures/LICENSE).
`python3 tests/fixtures/build_scsp_pcm.py --check` reproduces both; CI runs it
alongside the existing fixtures. The Hardware Lab effects program now consumes
MIXS0 after slot 0's operand reads finish. Its exact wet/dry vectors are retained;
its rebuilt original disc hash is in the [demo README](../homebrew/hardware-lab/README.md).

The inherited atomic-read implementation fails the independent midpoint guest
with `missing midpoint 4000`. The fixed implementation must also pass all existing
synthesis, effects, sound-clock, DMA, reset and three-disc demonstrations.
The cycle origin and refresh request phase are deterministic functional policies;
the manuals do not define a power-on oscillator phase. This supplies PCM
ownership/operand retention, not full chip/CPU pin timing or completion of the
remaining Issue #17 acceptance criteria.

## SCSP effect register store semantics

Primary references: [SCSP] §5 fig.5.1 routes the previous accumulator through
the shifter into EFREG. [ST-228][SCDSPASM] p.31, the multiple-STR warning,
specifies that a later store to the same EFREG replaces its earlier value.
`Dsp::step` in [dsp.rs](../src/sound/dsp.rs) now performs that store, replacing
the inherited incorrect addition. Programs compute sums through the executing
multiply/add datapath before publishing them to the output register.

The independent [deadline vectors](../src/sound/dsp/clock_tests.rs),
`every_effect_store_replaces_at_its_own_deadline_including_zero_and_negative_data`,
cover all sixteen ports, positive/negative/zero values, an intervening CPU
overwrite, adjacent-port isolation and four advancement sizes. Existing
shifter saturation/wrap, parallel arithmetic, MRD/IWT, reset and clock vectors
remain enabled.

The original [fixture builder](../tests/fixtures/build_scsp_clock.py),
`build_effects`, emits [SH-2 source](../tests/fixtures/scsp-effects.s), literal
68k/DSP instruction words and the [synthetic ROM](../tests/fixtures/scsp-effects.hex).
The actual 68k uploads the operands and DSP instructions, polls a DSP-computed
marker and records the resulting registers. The actual DSP writes EF0 first
positive then negative, EF1 first negative then positive, and EF2 first positive
then zero. EF0/1 route independently to left/right; EF2 must read zero.

[Native execution](../tests/scsp_clock.rs) and [production Wasm/WebAudio](../scripts/scsp-clock.mjs),
`scspEffects`, use three source-authored operand/result pairs, repeated cold
boots and an omitted-final-EWT control that changes the left channel to the
earlier value. Fresh complete stereo buffers must contain only the expected
computed samples. Production AudioWorklet assertions require the expected peak,
rendered frames, zero drops and observable underflow. Neither host code nor
the sound CPU writes the DSP's result registers.

All source/generated bytes use the adjacent [MIT fixture license](../tests/fixtures/LICENSE).
Reproduce using `python3 tests/fixtures/build_scsp_clock.py --check`, already
required by CI. Expanded 512-KiB ROM SHA-256:
`dc409f5dcf612c02a98a37d080e807d57132413f80d1fdc3d4852c8039e29ca8`.
The independent guest rejects the inherited addition with `1000/1000` instead
of `F000/2000`; the negative control used an isolated checkout without changing
production sources. The original exact CLAB wet/dry vectors remain required.

This corrects EWT storage, not the full DSP converter/pipeline, synthesis,
physical sound-interface timing or other mandatory Issue #17 contracts.


## Scheduled normal-background VRAM acquisition (Issue #19)

Related to #19. Related to #17. [Implementation, exact manual references,
phase convention, native vectors, guest provenance/hashes and actual validation](validation-issue-19.md)
replace the previously recorded normal-background first-use acquisition gap.

| Contract | Implementation and evidence |
| --- | --- |
| T0–T7 / T0–T3 hardware-time acquisition | [Raster deadlines](../src/timing.rs), [Bus ordering](../src/bus.rs), [normal packets/serializers](../src/vdp2/fetch.rs); no render-time miss filling |
| NBG0–3 names/characters, supported bitmaps and vertical-cell reads | Independent address/data vectors for formats, name widths, dependencies, bank modes, scrolling/reduction and separate layers/paths in [fetch.rs](../src/vdp2/fetch.rs) and [access.rs](../src/vdp2/access.rs) |
| CPU, on-chip DMAC and SCU completion boundaries | [Board vectors](../src/bus/vdp2_tests.rs): both CPUs/DMACs, all three SCU channels, before/equal/after publication, IRQ order, separate banks, reset, chunk/inspection invariance |
| Writes after acquisition but before consumption | Original MIT [guest](../tests/fixtures/vdp2-cycle-slots.s), [native Machine](../tests/vdp2_fetch.rs), [production Wasm/Canvas](../scripts/video-transactions.mjs); source/slot mutations, omitted store, cold resets, whole-row literal pixels, failing inherited-implementation control |

The two-cycle horizontal lead is an explicit emulator convention. Rotation
acquisition timing and the other mandatory #17 hardware/comprehensive acceptance
criteria remain open. This entry does not claim full hardware completion or
commercial-game compatibility and does not reopen historical trackers.

## Scheduled CD directory acquisition (Issue #21)

Related to #21 and #17. [Implementation, specification, timing policy, complete
validation and remaining scope](validation-issue-21.md) supersede earlier
synchronous directory-response evidence. Runtime ChgDir/ReadDir acquire actual
descriptor and directory sectors at 75/150 Hz drive deadlines, using guest ECC and
retry settings. One shared 200-sector pool workspace coexists with host
reservations and unrelated partition data; completion/cancellation releases it.
Scope remains unavailable until all records validate. Invalid command parameters
reject before mutation; acquired-media failures end with drive ERROR and EFLS
without changing a retained command response. AbortFile invalidates interrupted
acquisition and preserves partition data/selector settings.
Bounded CD Play retains normal range-end PEND and finite/infinite repeat handling
while directory sectors are acquired. A selected range ending obstructs an
unfinished job without reporting a media error; resuming at its required FAD
continues acquisition with the same workspace and partial records.
The unreadable-root vectors cover first-descriptor leadout and audio-only media:
both terminate at the first scheduled read with ERROR/EFLS, unavailable scope
and released workspace, preserving acceptance and allowing another ChgDir.

The [new vectors and original guest](../tests/cd_directory_schedule.rs) cover
literal event ordering at 1×/2× with chunks 1/3/17/10000, retries, pool capacity,
independent transfers and cancellation before/between/immediately before the last
sector. The same reproducible MIT media/guest runs in production browser Wasm,
measuring FRT progress and handling masked/unmasked SCU completion. Changed
navigation operands, command omission, mid-operation abort and repeated cold
boots are controls. The inherited synchronous implementation fails the guest's
EFLS-at-CMOK assertion (512 versus 0). Existing malformed/media/window/XA tests
retain their substance with separate CMOK/EFLS waits. These results do not
establish Issue #17 completion or commercial-game compatibility.


## Fixed CS0 word bus and burst ROM (Issue #23)

Related to #17. The independent [VA0.5 board drawing](https://consolemods.org/wiki/images/1/1b/Sega_saturn_va0.5_schematics.pdf),
sheet 1, shows MD4=0/MD3=1 for both processors and a word-output boot ROM.
[SH7604][H] §3.3/table3.9 specifies the strap; §7.3.1/fig.7.3 specifies the
big-endian word phases; §7.8/figs.7.44–46 specifies nibble grouping and its
zero-wait exception; §§7.11.1–2 specifies reset, IF/cache and buffering boundaries.
This supersedes earlier 32-bit CS0 adapter claims and two-phase reset oracles.

| Contract | Implementation | Independent evidence |
| --- | --- | --- |
| Fixed 16-bit CS0, other areas independent | [Bsc::bus_width](../src/onchip/bsc.rs), physical splitting in [bus.rs](../src/bus.rs) and [transactions.rs](../src/bus/transactions.rs) | [CS0 vectors](../src/bus/cs0_tests.rs) on both CPUs: literal byte/word/longword addresses and values, P1, ROM/low-RAM, mirrors, SMPC bytes, backup odd lanes and ROM protection; existing BSC mode matrix still checks independently configurable CS1–3 |
| BSTROM longword pairs, including zero waits | `Bsc::access_timing` receives actual beat width; request flags/address identify the second word without persistent burst state | CS0 vectors require ordinary reset-wait longword deadlines 8/16, BSTROM one-Tw 3/5, two-Tw 4/7, and zero-Tw 2/4. Independent adjacent words each restart T1. Eight-word fills require 3/5/8/10/13/15/18/20 with one Tw. Mutation between word phases proves retention and future sampling; every listed board trace repeats with chunks 1/3/17/10000 |
| IF/refill, buffering, DMA and ownership | Existing replay journals, per-CPU successor retention, single-entry BSC buffer and indivisible CPU groups; self-refresh checked after accepted CPU groups | Both IF word entries, retained successors, complete refills and changed unsampled data; both DMACs and channels, ROM/low-RAM longword and 16-byte reads, per-longword pair restarts, retained writes, SAR/DAR/TCR/TE/IRQ ordering; low-RAM TAS/peer exclusion and refresh six-state grant after the two-word operand. Existing SCU prohibited-route checks remain unchanged |
| Four sampled reset words, cancellation and reset kinds | Existing Machine reset journal and physical manual-reset drains | [Machine reset tests](../src/reset_tests.rs) require cold clocks 8/16/24/32, no early PC/SP/instruction publication, cancellation/new boots at every vector word, external-BIOS SYSRES and SSHON/SSHOFF; [scheduler vectors](../src/lib.rs) mutate each watchdog vector word on both CPUs while the cached peer advances. CS0 vectors exercise both watchdog kinds at every affected refill/16-byte read word and buffered-write boundaries; manual reset drains the current longword, power reset cancels it, no partial line is installed and no late destination write survives |
| Actual dual-SH-2 guest timing/computation | [Original MIT builder](../tests/fixtures/build_cs0.py), generated sources `cs0-*.s` and [ROM](../tests/fixtures/cs0.hex), adjacent [license](../tests/fixtures/LICENSE) | [Native Machine](../tests/cs0.rs), [production Wasm](../scripts/cs0.mjs): both CPUs read ROM/low-RAM, XOR their payloads, record FRT and exchange 51/A3 synchronization. W0=0/1/2 gives 16/20/28 ticks; omitted-ROM-read controls give 10/12/16. Two payloads and two cold boots per variant. The inherited four-byte selection fails the real guest with FRT=8 instead of 16 |

Expanded 512-KiB CS0 BIOS SHA-256:
`5ae737dece64b91d567a917fef0f33c67ade43cad27d5cefdbf398ab94a025a0`.
Generated hex SHA-256:
`b63fe096b9ac4dfbfe7aab10b74b46657d9e55cb40e28966c67ed5aa7f2bbf79`.
The [validation record](validation-issue-17.md#issue-23-fixed-cs0-and-bstrom-validation)
records trace-derived historical fixture corrections, additional hashes and actual checks.

The fixed strap models the standard console represented by the independent board
drawing; it adds no configurable board variants or electrical pin simulation.
External WAIT pin input, exhaustive board bridges, full arbitration/reset and the
remaining CPU/peripheral, SCU/DSP, video/audio, CD/media/HLE and SMPC acceptance
remain outstanding under #17. These synthetic contracts establish neither
umbrella completion nor broad commercial-game compatibility. No personal firmware,
physical console, human approval or live Pages inspection is required.


## VDP1 erase-parameter capture (Issue #25)

Related to #25. Related to #17. [ST-013-R3 chapter 4 register update timing](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_10.htm)
places EWDR/EWLR/EWRR capture at framebuffer switching; [§4.2](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_12.htm)
distinguishes automatic changes, manual erase and manual change. [§4.4](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_14.htm)
defines sixteen-byte X units, inclusive lower Y, packed erase bytes and budgets.
[§4.9](https://www.infochunk.com/saturn/segahtml_en/hard/vdp1/hon/p04_19.htm)
requires MODR to report written register settings, independently of internal state.
This supersedes earlier evidence that did not separate written erase settings
from the settings captured before an erase was accepted.

| Contract | Implementation | Independent evidence |
| --- | --- | --- |
| Written versus captured erase settings | `Vdp1::erase/erase_latched`, `vblank_out/erase_area` in [vdp1.rs](../src/vdp1.rs); automatic/manual changes copy all three, held/manual-erase fields do not | [erase_tests.rs](../src/vdp1/erase_tests.rs): explicit overlapping A/B/C rectangles, repeated pending writes, automatic and manual sequences, 16-bit and both supported 8-bit geometries, distinct even/odd bytes, edges and canaries |
| Accepted erase retains color, bounds and destination; field end/reset cancels work | Existing retained `EraseArea/Erasing`; [progress_tests.rs](../src/vdp1/progress_tests.rs) retains exact partial budgets and scanline assertions | New erase vectors change registers during accepted jobs and check both physical buffers; [bus tests](../src/bus/vdp1_tests.rs) reset unfinished A plus pending B and physically in-flight C, then explicitly initialize D without stale writes |
| Register completion/capture ordering | Existing [Bus raster-before-data order](../src/bus.rs), with no new timing phase | Both SH-2 replay adapters write each of EWDR/EWLR/EWRR at completion edges -1/0/+1, chunks 1/3/17/10000, with/without debugger reads. Physical acceptance is distinguished from external completion. Equal-edge capture-before-write is an emulator convention |
| Real MMIO guest and browser pixels | [Original MIT builder](../tests/fixtures/build_vdp1_erase_latch.py), [source](../tests/fixtures/vdp1-erase-latch.s), [ROM](../tests/fixtures/vdp1-erase-latch.hex), adjacent [license](../tests/fixtures/LICENSE) | [Native Machine](../tests/video_transactions.rs) and [production Wasm/Canvas](../scripts/video-transactions.mjs) execute identical bytes. Guest initializes both buffers, synchronizes A/B capture, computes whole-region checksums, reads edge pixels/status and publishes output. Colors/coordinates mutate, B writes can be omitted, and every variant repeats cold boots |

[Actual checks, inherited-implementation negative controls, fixture corrections
and artifact hashes](validation-issue-25.md) document this partial #17 increment.
The new builder's `--check` is required by existing CI. VBE sampling-phase
correction, half-line timing, additional display modes and rasterizer changes
remain separate work. CPU/peripheral, multiprocessor/bus/reset, SCU/DSP,
video/audio, CD/media/HLE, SMPC and comprehensive acceptance remain outstanding.
These results do not establish hardware completion or broad commercial-game
compatibility. Issue #17 remains open; historical #11/#13/#15 are not reopened.


## SCU DSP arithmetic status lifetime

Related to #27. Related to #17. Program-control reads return the current status,
then clear **E and V**. ADD, SUB and AD2 replace V with each operation's
signed-overflow result;
non-overflowing arithmetic clears V. NOP, logical/shift operations and transfers
preserve V. S/Z/C describe the ALU result even when it is only transferred to
memory, the accumulator is retained, or CLR A executes in parallel.

The corrected [Issue #27 criteria](https://github.com/takahirox/saturn-rust/issues/27)
supersede the original read-preserved-V requirement with documented read-cleared V.
[SCU Final Specifications: Precautions, ST-210-110194, item 36, printed p.10](https://antime.kapsi.fi/sega/files/ST-210-110194.pdf#page=14)
explicitly requires read-cleared V. The same rule appears in
[Sega Technical Bulletin #10, item 36, PDF p.33](https://antime.kapsi.fi/sega/files/Sattechs.pdf#page=33).
SOA #8 §3 describes E acknowledgement without revoking this V rule, and the
simulator addendum describes arithmetic flag replacement without specifying
control-port reads. The implementation and repeated-read vectors satisfy the
corrected read-clear contract. This evidence-backed correction resolves the
technical-contract conflict; it is not a human test result or a waiver of review
or CI. Tests verify the documented contract, not independent measurements of
physical hardware.

Primary contract:

- [SCU DSP Simulator User's Manual Addendum, ST-240-B-SP1-052295, printed p.5, Bugs](https://manualzilla.com/doc/5874469/scu-dsp-assembler-user-s-manual-addendum): hardware replaces V after ADD/SUB/AD2; the simulator's sticky V is a documented defect. The mirror's title says Assembler, but the document cover and footer identify the Simulator addendum.
- [Sega Technical Bulletin SOA #8, §3 and §5.1](https://antime.kapsi.fi/sega/files/Sattechs.pdf#page=151): E acknowledgement, S/Z/C/V definitions and the instruction flag-effect table. §5.1 (PDF p.153) specifies flags and same-cycle memory results independently of MOV ALU,A; §§5.2–5.5 specify flag-preserving transfers.
- [SCU User's Manual ST-097-R5, §4.5, ADD/SUB/AD2 and X/Y/D1 transfer descriptions](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-097-R5-072694.pdf): 32-bit ADD/SUB, 48-bit AD2 and parallel result transfers.

[Dsp::control/alu](../src/scu/dsp.rs) keeps read-cleared E/V and fixes arithmetic
opcodes 4/5/6 to clear the previous V before assigning the new overflow result.
Arithmetic widths, carry/borrow, accumulator high-word
retention and scheduling retain their existing implementation and now have
additional literal checks. DMA/pipeline design and rendering are outside this
increment.

| Evidence | Independent expectation and coverage |
| --- | --- |
| [Native instruction vectors](../src/scu/dsp/arithmetic_tests.rs) | 17 literal A/P/result/S/Z/C/V rows, each with both initial V values and three accumulator choices (retain, clear, writeback), plus parallel ALL-to-memory. Both signed overflow directions, unsigned carry/borrow without signed overflow, zero/sign boundaries, and 32-bit versus 48-bit sign/zero behavior. No expected result calls a production arithmetic helper. |
| Arithmetic lifetime | Six two-instruction overflow/non-overflow sequences cover ADD/SUB/AD2 in both directions without an intervening status read. NOP, all logical/shift/rotate ALU operations, immediate, X/Y and D1 transfers preserve V between arithmetic operations. Separate END/ENDI, pause/resume, single-step and repeated-read vectors check read-cleared V, retain E acknowledgement and emit a new end event once after acknowledgement. Running END-program reads also clear V while retaining S/Z/C and EX. |
| Real 48-bit operand construction | MOV M0,X / MOV M1,Y; MOV MUL,P / CLR A constructs positive/negative 2^46 from signed 65536 × 2^30. Repeated AD2 reaches the 48-bit sign boundary, overflows in both directions, then clears V; literal full 48-bit results and flags are checked each step. |
| [Original MIT guest builder](../tests/fixtures/build_dsp_arithmetic.py), [SH-2 source](../tests/fixtures/dsp-arithmetic.s), [ROM](../tests/fixtures/dsp-arithmetic.hex), [license](../tests/fixtures/LICENSE) | Guest instructions upload operands/programs via 25FE0080/84/88/8C, execute twelve END kernels and two ENDI kernels, wait 100 SH-2 loop iterations without destructive EX polling, record three halted status reads per END (including EX=0), consume ALL/ALH results, and compute result+1 inside the real DSP-end interrupt handler. E is acknowledged through control reads; CPU interrupt entry acknowledges the SCU request, and the handler returns with RTE. No host result writes or calculations. |
| [Native Machine guest](../tests/dsp_arithmetic.rs), [production browser Wasm](../scripts/dsp-arithmetic.mjs) | Identical diagnostic ROM bytes and [71 source-authored golden words](../tests/fixtures/dsp-arithmetic.tsv), two operand mutations, an omitted-ADD control and repeated cold boots for every variant. Native step chunks 1/3/17/10000 with paused inspection/resume retain results. Exactly two vector-45 interrupts occur; additional advancement cannot duplicate them. Reset during an unhandled overflowing ENDI cancels V/E and the pending interrupt, then reruns the complete diagnostic. |
| Existing regression evidence | [SCU instruction/control/DMA vectors](../src/scu/tests.rs), [DMA-bank/prefetch diagnostics](../src/scu/dsp/tests.rs), [SCU guest](../tests/scu.rs), [DSP-DMA guest](../tests/dsp_dma.rs), [DSP bus guest](../tests/dsp_bus.rs) and their complete [browser checks](../scripts/browser-smoke.mjs) remain enabled. |

Expanded 512-KiB diagnostic ROM SHA-256:
`c145080fa0191c2ac785c744d7fe9470d1f4fa84a50bc0305ca1c9141fa6f38e`.
Reproduce all three fixture artifacts with
`python3 tests/fixtures/build_dsp_arithmetic.py --check`; this is mandatory in the
existing [CI fixture step](../.github/workflows/pages.yml). The shared TSV contains
literal source expectations, not captured emulator output. Two initial reset
words are followed by twelve five-word groups (three statuses, ALL, ALH), two
three-word interrupt-handler groups and interrupt count/pending/done records.
Status masks keep S/Z/C/V/E/EX and omit the incidental PC.

The earlier controls against a read-preserved-V expectation were invalid evidence
for that hardware claim and are superseded by the following controls, executed
on 2026-10-23 against the reconciled contract. Each mutation was temporarily
applied to the same source, checked with
`cargo test --locked --lib scu::dsp::arithmetic_tests` and
`cargo test --locked --test dsp_arithmetic`, then restored:

| Implementation control | Baseline guest observation versus literal contract | Result |
| --- | --- | --- |
| Sticky arithmetic V (inherited arithmetic defect; documented read-clear retained) | The two-ADD result is 80000001, but its first halted status is stale 00480000 rather than 00400000. Later reads clear V, so the first sample is essential. | Four of six focused unit tests and all three Machine tests fail. |
| Read-preserved V (rejected #27 assumption; arithmetic replacement retained) | Overflowing ADD gives 00480000 on all three reads rather than 00480000/00400000/00400000; the second handler read likewise incorrectly retains V. | Three of six focused unit tests and all three Machine tests fail. |
| Corrected implementation | Both distinguishing ADD results/statuses and every source-authored record match; the first read reports V, later reads clear it, E clears once, and exactly two end interrupts occur. | Six focused unit tests and three Machine tests pass. |

Validation on 2026-10-23 used macOS arm64, installed stable Rust 1.99.0,
wasm32-unknown-unknown, wasm-bindgen 0.2.129, Node 24.12.0 and Playwright 1.58.2.
The installed stable toolchain's bin directory was placed first on PATH for the
final checks and browser build (the default Homebrew compiler lacked Wasm core).

| Required check | Actual result |
| --- | --- |
| `cargo fmt --check`, `cargo build --locked` | Pass. |
| `cargo test --locked` | Pass: 808 tests, zero failures/ignored, including all existing SCU/DSP, games, audio, backup/controller and reset coverage. |
| `cargo clippy --locked --all-targets -- -D warnings` | Pass. |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Pass. |
| `sh scripts/build-web.sh` | Pass: all original media reproduced and release Wasm/bindings built. |
| All 76 fixture/media `--check` commands in the existing CI workflow | Pass, including the new diagnostic and every existing original guest/media check. |
| `npm run test:browser` | Pass: complete suite on Chromium 145.0.7632.6, including the new diagnostic, all existing native-equivalent DSP guests, Star Collector, Prism Courier, Hardware Lab, production Canvas/AudioWorklet, backup, both controller ports, resets, offline execution and local-file privacy assertions. |

The implementation and validation above satisfy #27's corrected arithmetic/status
criteria. This remains a partial #17 increment. CPU/peripheral, multiprocessor/bus/reset,
remaining
SCU/DSP, video/audio, CD/media/HLE, SMPC and comprehensive integration requirements
remain mandatory. The remaining DSP pipeline/physical RAM-port/conditional/DMA
and cross-device timing inventory stays pending, including item 36's warning that
control reads during execution can suppress the DSP-end interrupt. The guest
avoids that prohibited polling pattern; it does not establish its physical timing.
Green CI or these diagnostics
do not establish hardware completion or broad commercial compatibility. Genuine
BIOS and commercial-game compatibility remain not independently confirmed.
Required post-merge verification: none.

## Shared SCSP master-volume attenuation

Related to #29 and #17. `Scsp::push_output` in `src/sound.rs` now applies one
nominal 3 dB staircase after clipping the combined direct PCM, executing effects
DSP and EXTS/CD-DA mix. The [MVOL contract](scsp-mvol.md) gives all sixteen
coefficients, primary references (Sega sound FAQ Overview, SCSP §1.3,
ST-077-R2 pp.ii/35/80–88), deterministic Q15 conversion, signed truncation,
error bound and the unmeasured output-deadline latch convention.

`src/sound/tests.rs` covers all gains with literal bipolar PCM vectors and
independent source sums, asymmetric stereo, silence, pre-attenuation clipping,
continued muted PCM/DSP execution, byte lanes/MEM4MB/write-only reads, queued
samples, reset and 1/3/17/10000 reference-clock chunks. The original MIT
`tests/fixtures/build_scsp_mvol.py` guest/media runs through native Machine and
production browser Wasm in `tests/scsp_mvol.rs` and `scripts/scsp-mvol.mjs`:
real SH-2/68k byte writes, source-isolated PCM/DSP/runtime mixed-track CD-DA,
combined amplitudes at 0/1/D/E/F, waveform/coefficient/audio mutations, omitted
volume-write control, repeated boots and exact mute/resume progress. Production
AudioWorklet peak/render/underflow/drop and observable mute/blocked checks run
under `/saturn-rust/`. See the [validation record](validation-issue-29.md) for
artifact hashes, actual results and the inherited-linear negative control.

This is a numerical output-stage increment, with no analog/bit-exact DAC claim.
Envelope rates, 18-bit DAC operation, other sound timing gaps and remaining
CPU/peripheral, multiprocessor/bus/reset, SCU/DSP, video/audio, CD/media/HLE,
SMPC and comprehensive integration criteria remain mandatory under open #17.
Commercial-game compatibility remains unverified.

## Clocked SH-2 multiplier and MACH/MACL interlocks (#31)

Related to #31 and #17. This increment supersedes the earlier multiplier-hazard
limitation for the instruction families and boundaries below. It does not mark
#17 complete or establish complete speculative pipeline or commercial-game
compatibility. The [timing contract](sh2-multiplier.md) was authored from primary
SH-2 diagrams before implementation output was used as evidence; the
[validation record](validation-issue-31.md) records actual checks and controls.

| Primary behavior | Implementation | Independent evidence |
| --- | --- | --- |
| [P] §7.7.2 figs7.23/34/53/63: two/four-clock mm after final MA; contention until mm ends | `src/cpu/multiplier.rs::Multiplier::{access,wait,finish}`, retained per `Cpu` | `multiplier_all_producer_consumer_families_and_progressive_spacing`: every multiply/MAC and MACH/MACL transfer class, 0–3 separating NOPs; literal timing vectors exclude production timing helpers |
| [P] figs7.24/27/54/55/58: second-MA access overlaps short multiply; figs7.35/38/39/64–66: longer tails stall | Distinct first/second MA boundaries, first MAC operand precedes wait | Native consecutive MAC and saturation vectors; guest word/long MAC streams and separated MAC.L, computed products and FRT records |
| [P] figs7.88–91: CLRMAC, LDS and STS register/memory access the multiplier; STS GPR retains WB | Shared accumulator-port interlocks, existing `LoadPipeline` retained | Signed/unsigned product and transfer-direction vectors, MACH preservation, flag preservation, predecrement/postincrement, saturation; inherited all-register STS WB tests retained |
| [P] §§7.4.1–3, figs7.6–8 and explicit contention diagrams listed in the contract | Alignment-dependent multiplier IF/MA splits, even with warmed cache; first split delays second MA | `multiplier_if_ma_alignment_splits_and_documented_consecutive_mac_overlap`, aligned/unaligned original guest kernels; corrected inherited STS-separated FRT expectation retains its dependency coverage |
| [P] §7.2.3 and Renesas Rev.5 §7.2.3: multiplier advances in cycles, not slots | `Bus::main_clocks`, journaled `Operation::Clock`, main-clock CPU deadlines and sub-reference PLL boundaries | FRT guest on both SH-2s at both PLL selections; external/on-chip/cold-fetch wait regressions; peer CPU and DMA progress during local stalls |
| Suspended operand effects complete once at the permitted bus boundary | Existing instruction clone/journal plus local pipeline wait before dependent memory transfer | Dependent LDS samples changed memory after wait and increments once; STS.L leaves old SP/data until wait, publishes once, leaves preceding address untouched; slave-local DMA completes independently |
| [P] §7.2.3, figs7.63/7.71/7.73: LDS.L/STS.L MA completion reaches the final mm even without extra contention stalls | Journaled access classification and completion boundary: synchronous accesses wait through completion; scheduled reads overlap their full operand latency with mm; STS.L retains write admission timing | Existing cache/array MACH/MACL vectors retain 0–4 NOPs, boundary mutations and single updates. `dependent_scheduled_lds_overlaps_its_full_operand_latency_with_mm` and `dependent_external_lds_overlaps_short_and_long_bus_latencies_with_mm` cover both halves, 0–4 NOPs, 2/3/4/8-clock reads and operand-dominated completion; `operand_completion_bounds_are_cpu_local_replayed_and_canceled_by_reset` covers both CPUs, peer progress, replay sampling and reset cancellation |
| [P] figs7.64/7.65: MAC.L/MAC.W second MA extends through the older multiply's final mm | Journaled second-operand completion bound keeps synchronous and scheduled sampling at completion while retaining the independent first read | `mac_second_operands_sample_at_the_multiplier_boundary_and_replay_once`: both widths, cache arrays/resident reads, mutation at clock 41 observed at 42, first-read retention and single increments; the two-clock peripheral read also completes at 42 because its latency overlaps mm; `mac_operand_completion_preserves_successor_ex_overlap` retains both widths/alignments' EX overlap at MA completion |
| [P] §7.2.3, table7.2, figs7.89/7.91: the successor's EX overlaps aligned LDS.L/STS.L MA, including stretched operand slots | Retained final shared-slot clock recovers memory MA retirement overlap, including two-clock scheduled reads; alignment splits and waited successor fetches retain their costs | `mac_memory_transfers_overlap_the_successor_in_elapsed_clocks` checks synchronous and scheduled MACH/MACL loads/stores at both alignments; `multi_clock_mac_loads_preserve_shared_ma_ex_slot_and_replay_once` checks third EX at 35, read sampling and single address updates; waited-fetch controls consume overlap after both synchronous stores and scheduled loads; four dual-CPU guest kernels assert FRT 4 for sixteen transfer/NOP pairs on both PLLs |
| Hardware resets replace local state; exception entry preserves MAC registers while time advances | Existing cold/SMPC/WDT reset paths replace `Cpu`; eight-clock exception entry ages retained work | Pending-transfer cold boot, actual SSHOFF/SSHON, both WDT reset selections on both CPUs, direct reset-boundary vectors, IRQ/NMI stack/handler/RTE checks |

The new original MIT fixture is `tests/fixtures/build_sh2_multiplier.py`, with
reproducible `.s`, `.hex`, `.tsv` artifacts. Native `tests/sh2_multiplier.rs` and
production-browser `scripts/sh2-multiplier.mjs` execute the same bytes and check
both CPU records across repeated cold boots. Operand, spacing and omitted-
producer controls distinguish computed results from timing. Disabling only the
multiplier interlocks causes both the independent native sequence assertion and
the FRT guest assertion to fail; arithmetic alone still succeeds.

The inherited constant-three MAC cost represented a fetch split rather than the
no-contention EX minimum. Fig7.23/34 gives the two-slot baseline; the clocked
adapter supplies alignment-dependent IF/MA splits. The inherited STS-separated
kernel has four unaligned STS MAs, adding four clocks (28 rather than 24);
its final FRT latch is floor((6+28)/8)=4. The inherited LDS→STS→MMIO scheduler
vector includes an unaligned LDS, adding one IF split before its unchanged WB
hazard. Both corrections cite §§7.4.1–3/7.7.6 and retain substantive coverage.

General speculative IF/ID/EX/MA/WB completion, arbitrary future fetch latency,
full interrupt-transition speculation, complete board-level timing, video/audio,
CD/media/HLE and the other still-unmet #17 requirements remain mandatory. This
bounded multiplier increment does not use firmware, commercial media, physical
hardware, human testing/approval or live Pages inspection as acceptance evidence.


## Sector-addressed standard-size media bootstrap

Related to #17 and #33. This admission/bootstrap increment is implemented;
umbrella hardware completion and commercial-game compatibility remain pending.

[Validation record](validation-issue-33.md) links the primary sections, resource
policy, original MIT guest/source and deterministic full-medium hashes.
[Native evidence](../tests/large_media.rs) and the
[production browser extension](../scripts/large-media.mjs) execute a 270,000-sector
volume with first-read sectors 9003/9004/9006 and runtime CD sector 260000.
The guest drives CD commands, FIFO-to-SCU DMA, interrupt acknowledgement, sum,
SYSRES and CD-DA; the browser uses real local ISO/CUE selections under the project
prefix. The focused bootstrap reader trace excludes unrelated payload, and
independently generated Mode1/XA vectors distinguish unread damage from required
sector failure. Existing drive/command/sector/DMA timing tests remain registered.
No full BIOS initialization, HLE CD library, complete drive/error timing or
commercial compatibility is established by these results.


## Scheduled rotation image acquisition

Related to #39 and #17. The [transaction contract](vdp2-rotation-fetch.md) records
ST-058-R2 §§3.3/4.4–4.9/6.1–6.3, later HTML sections, applicable Sega bulletins
and the separate unmeasured rotation phase assumptions. This supersedes the
rotation-image deferral in the historical [#19 record](validation-issue-19.md).
The [#35 one-shot request evidence](validation-issue-35.md) remains applicable.

| Scope | Implementation and independent evidence |
| --- | --- |
| RBG0 tile/bitmap, RBG1 tile | [Dedicated acquisition](../src/vdp2/fetch.rs), [shared address decoder](../src/vdp2/rotation.rs), [bank roles](../src/vdp2/access.rs), [raster deadlines](../src/timing.rs) and [board event ordering](../src/bus.rs) |
| Retained names, raw words and address interpretation | [Literal vectors](../src/vdp2/rotation_fetch_tests.rs): both name widths, 8/16-dot characters, all five color formats, backwards tile crossing, address revisits, bitmap data, split/unsplit geometry, dual rotation, an independent normal companion and NBG0/RBG1 storage isolation across display switches. RAM/register mutation cannot change acquired dots; misses cannot read live RAM. High-resolution RGB888 is rejected per §4.5. |
| Publication/waits/reset | [Board vectors](../src/bus/vdp2_tests.rs): both SH-2 adapters and on-chip DMACs, all SCU DMA channels, before/equal/after deadlines, independent B progress, blanking release, last-write/IRQ ordering, cancellation and identical traces/pixels across chunks 1/3/17/10000 and debugger previews |
| Guest/native/Wasm/Canvas | Original MIT [builder](../tests/fixtures/build_rotation_fetch.py), [tile](../tests/fixtures/rotation-fetch-tile.s), [bitmap](../tests/fixtures/rotation-fetch-bitmap.s), [RBG1](../tests/fixtures/rotation-fetch-rbg1.s), [native tests](../tests/rotation_fetch.rs), [production browser checks](../scripts/rotation-fetch.mjs), adjacent [license](../tests/fixtures/LICENSE). All setup, timed writes and computed records execute as guest instructions. |

Expanded 512 KiB ROM SHA-256 (also retained beside each fixture; includes the
[#43 cached-NOP synchronization correction](validation-issue-43.md#historical-timing-assertion-correction)):

- RBG0 tile: `2d6c5891f3af1d897158e739cd5b1647bc641e9595e04b81dddea08d4b9ee069`.
- RBG0 bitmap: `4abe71651329aaf5b5e07eac7272b424cccf16a59f58a0407aec9171f4a981e0`.
- RBG1 tile: `8180ae8a81f1be6722bde22a27007c933adb37d25e97a137c6ba671a6687dc3c`.

[Actual validation and negative controls](validation-issue-39.md) record the
commands and results. Acquired image operands now progress independently of
rendering. This is a deterministic functional phase, not measured console
latency; coefficient/other table acquisition timing remains outside this
increment. Full CPU/peripheral/board/reset, SCU/DSP, video/audio, CD/media/HLE,
SMPC and comprehensive integration acceptance remain mandatory. #17 remains
open; genuine BIOS/commercial compatibility is not independently confirmed.


### High-resolution rotation pictures

Related to #53 and #17. The [primary-derived coordinate contract](vdp2-rotation-fetch.md#high-resolution-picture-coordinates-53)
reconciles the 1997 Japanese/English §2.1, §§6.1/6.3/6.4, §8.1 table8.1,
STN-24 and the STN-13/22/37 corrections. RBG0 A/B and RBG1 now advance matrix and
coefficient X once per output pair in standard-TV 640/704 modes. Output windows,
normal screens and VDP1 composition retain their independent coordinates.
Two-clock image acquisitions remain an explicit deterministic convention.

| Evidence | Coverage |
| --- | --- |
| [Literal pixel/address vectors](../src/vdp2/rotation_fetch_tests.rs) | Complete 320/352 source spans and 640/704 output rows; both RBG0 sets, tile/bitmap, fixed-bank RBG1; all four legal formats, both name sizes, 8/16-dot cells; signed/fractional image and coefficient increments, RBG1 coefficient MSB, odd-output parameter/layer windows, missing reads, field tags, independent NBG0/VDP1 priorities and coordinates |
| [Board vectors](../src/bus/vdp2_tests.rs) | Both SH-2s publish before/equal/after paired two-clock acquisitions; retained pixels, reset cancellation, passive previews and identical traces across chunks 1/3/17/10000 |
| Original MIT guests | [Builder](../tests/fixtures/build_rotation_high_resolution.py), adjacent [license](../tests/fixtures/LICENSE), tile/bitmap/RBG1/coefficient `.s`, `.hex`, `.tsv` and expanded-ROM `.sha256` artifacts; correct PLL/display setup, guest-created operands and arithmetic/readback records; CI reproduction registration |
| [Native](../tests/rotation_high_resolution.rs) and [browser](../scripts/rotation-high-resolution.mjs) | Identical bytes; both widths, every diagnostic pixel, guest records, changed source/translation/table-start operands, RBG0 A/B selection, omitted enables, repeated cold boots; actual production Wasm/Canvas under `/saturn-rust/` |

[Actual results and inherited-behavior negative controls](validation-issue-53.md)
record validation separately from specification and phase conventions. This is
partial under #17: physical rotation latency/coefficient bus scheduling,
half-line interlace and all other unresolved umbrella requirements remain open.
No hardware-completion or commercial compatibility claim is made.


## CPU sound-memory allocation

Related to #41 and #17. [SCSP] §3.1 pp.18–19 and §3.2 pp.20–21,
figs.3.1–3.2 require buffered interface completion and priority PCM/DSP,
refresh, internal DMA, main CPU, sound CPU. The
[transaction table and independent deadlines](scsp-cpu-memory.md) explicitly
separate those requirements from oscillator origin, request eligibility and
interface edge conventions. Existing PCM acquisition is retained; older rows
listing PCM ownership as absent are superseded by the PCM and CPU entries.

| Contract | Implementation | Independent evidence |
| --- | --- | --- |
| Finite CPU requests, admitted per-word payload/geometry, memory completion, retained read and later acknowledgement | [CPU controller](../src/sound/memory.rs), [sound scheduler](../src/sound.rs), [68k journal](../src/sound/transactions.rs), [board bridge](../src/bus/transactions.rs) | [Allocation vectors](../src/sound/memory/tests.rs): literal grants/completions, all owner priorities, before/at/after arrival, late higher priority, finite traffic, byte lanes, split longwords and geometry |
| Both SH-2s, both DMACs, three SCU channels and SCU-DSP DMA preserve operands and publication order | [Board vectors](../src/bus/sound_tests.rs), [bridge vectors](../src/bus/bridge_tests.rs), SCU halfword vectors in transactions.rs | Chunks1/3/17/10000, both PLLs, debugger invariance, retained reads through later CPU acknowledgement, SCU IRQ after final destination and repeated real guest cold boots |
| Queued/admitted cancellation does not replay completed words | Controller cancellation and retained partial external requests; existing volatile reset domains | SNDOFF, SCSP RESET queued/granted/partial-longword vectors, both SH-2 manual/power reset drains; existing SYSRES/cold/reset suites |
| Guest main traffic consumes actual memory cycles while 68k/PCM/DSP execute | [Original builder](../tests/fixtures/build_scsp_cpu_memory.py), [native guest](../tests/scsp_cpu_memory.rs), [production browser guest](../scripts/scsp-cpu-memory.mjs) | 512-cycle minimum independent of emulation output, shared-RAM records, three operand pairs, omitted initiating operation, repeat cold boots, exact PCM/DSP stereo and production AudioWorklet rendering/underflow/blocked checks |

Original content uses the adjacent [MIT license](../tests/fixtures/LICENSE).
Expanded 512 KiB CPU allocation ROM SHA-256:
`188649564bbf678b1fd97cdd1bec829e5234a7d28b4bf88661e5cbfa71b27b24`.
Reproduce with `python3 tests/fixtures/build_scsp_cpu_memory.py --check` (registered
in CI). The [validation record](validation-issue-41.md) records actual check
outcomes, the inherited-admission negative controls, production Wasm hash and
remaining deviations. Electrical interface timing, general 68k sequencing,
envelope calibration, DAC modes and full synthesis/DSP corpus remain required.
All other unmet CPU/peripheral, board/reset, SCU/DSP, video/audio, CD/media/HLE,
SMPC and comprehensive #17 integration criteria remain open. Green CI or this
increment does not establish hardware completion or commercial compatibility.


The basic sound diagnostic now explicitly selects timer A startFD so its second
IRQ cannot merge with the power-on request during acknowledgement. Its exact
IRQ count, STOP saved PC and stereo assertions are preserved. Current expanded
ROM SHA-256: `0cea305717c24d7a62118bc9291c1609b4724c503b16d70c52e74df3be6d0994`.
Other sound guests and demonstration-disc bytes are unchanged. The PCM read
guest adds one phase NOP while preserving every arithmetic midpoint assertion;
its current hash is recorded in the PCM entry and the #41 validation record.

## Scheduled SH-2 video-memory write phases (#43)

Related to #43 and #17. [The transaction contract](sh2-video-writes.md) reconciles
SCU ST-097-R5 §3.2 printed pp.43–45/figs3.6–3.8 (32-bit bridge, 16-bit device),
ST-210 §3 item32 (requested CPU write size) and item03 (VDP1 register restriction),
SH7604 §7.11.2 pp.210–211 (buffer acceptance/drain), VDP1 §2.1 ownership and
VDP2 §3.3 display access allocation. Ascending words, per-word BSC envelopes and
equal-edge display-before-publication ordering are explicit model conventions;
they are not measured CPU-write waveforms or full board timing.

| Requirement | Implementation | Independent evidence |
| --- | --- | --- |
| Retained longword, separate device-word publication | [Buffered transfers](../src/bus/transactions.rs) split only scoped video memory; retain original payload and CPU-facing external UBC unit; [adapter](../src/bus.rs) keeps endian/alignment/cache policy | [Native vectors](../src/bus/video_write_tests.rs) observe1122CCDD at8 then11223344 at16; change completed high word to prove no replay; byte lanes, words, little endian, narrow BSC, every endpoint/mirror/P0/P1 and both processors |
| Both SH-2 DMACs, both channels, byte/word/longword/16-byte units | Existing early buffer acceptance/TE/IRQ remains independent of final publication; sound allocation and SCU/DSP paths unchanged | Literal source/destination/payload/deadline vectors, internal-source queued backpressure, TE before final word, retained source changes and P1 byte lanes |
| Target ownership and display acquisition | Each downstream word independently reserves the existing target | VDP1 selected-memory ownership with independent untextured drawing; VDP2 denied/revoked grants, delayed second word, before/equal/after acquisition at32, identical retained traces/pixels across chunks 1/3/17/10000 and previews |
| Reset and replay | Accepted manual-reset longword drains in existing reset domain; power reset cancels pending words; system reset clears volatile state | Both CPUs/DMACs, queued/accepted/partly completed writes, peer retention, no delayed canceled publication, completed-word retention and existing sound/reset regressions |
| Executing guest and rendering | Original [MIT](../tests/fixtures/LICENSE) [builder](../tests/fixtures/build_video_writes.py), [assembly](../tests/fixtures/video-writes.s), [ROM](../tests/fixtures/video-writes.hex) | [Native Machine](../tests/video_writes.rs) and [production browser Wasm/Canvas](../scripts/video-writes.mjs): both CPUs/DMACs compute readback/FRT; two-dot black interval between separately published CRAM words, changed operands, omitted-write controls and repeated cold boots |

Expanded ROM SHA-256: `3c885b12e0b8a8a5f810e65a5f8f9b6bf56875349fb73be26ac5781256354803`.
[Validation](validation-issue-43.md) records actual positive/negative checks and
the historical timing corrections. The original guest's pixels and
records are never supplied by host code. Existing memory geometry/aliases and
MMIO compatibility remain unchanged: this conversion does not make forbidden
VDP1 register sizes hardware-conforming. Sound-memory allocation evidence from
[#41](validation-issue-41.md) remains in force.

This completes a scoped video-write increment only. Comprehensive CPU/peripheral,
multiprocessor and board timing/reset, SCU/DSP, video/audio, CD/media/HLE, SMPC
and full integration acceptance remain mandatory under #17, which stays open.
Commercial-game compatibility and physical hardware timing equivalence remain
unverified. No personal firmware, commercial media, human testing/approval,
live Pages inspection or external Issue closure investigation is required.


## CD report validity and retained position

Related to #45 and #17. The [primary-derived contract](cd-drive-reports.md)
reconciles later Sega §7.2.5 reports/§7.2.4 Q with ST-162-062094
§7.2.4 pp.59–61/§7.2.5 p.62, plus §4.2 pp.37–38 and §4.3 p.39.
[Validation and negative controls](validation-issue-45.md) record actual results.

| Contract | Implementation and independent native evidence | Executing guest evidence |
| --- | --- | --- |
| OPEN/NODISC/ERROR and Seek Home expose FF report fields while retaining recovery data | `CdBlock::report`, `invalid_report`, `at_home` and `pause_position`; seventeen [register/Machine tests](../tests/cd_reports.rs) use literal fields, both standby origins, FAD 181/182, repeat notifications 1/2, retained finite maximum/range across shorter and longer replacement media, whole-disc reset range at a noninitial pickup, CD power-reset defaults before/after media replacement, forward/reverse scans of empty retained ranges with literal report/Q words and preserved play settings, cancellation outside either range bound, and retry/error recovery | **CDRPT**: original [MIT builder](../tests/fixtures/build_cd_reports.py), [native execution](../tests/cd_reports.rs) and [production Wasm runner](../scripts/cd-reports.mjs) issue seek/home/pause/open/error/recovery commands at FAD 181/187/173 |
| Q works in every supported drive state and agrees with the report | Native five-word vectors for NODISC, PAUSE, SEEK, PLAY, SCAN, both standby origins, OPEN, RETRY and ERROR; forward/reverse scans bounded by shorter replacement media without overwriting range/repeat settings; track-AA lead-out position and audio sample restart after home | Guest transfers six complete Q records through DATATRNS and computes sums; host reads records only. Two original media payloads and omitted-stop control change independently predicted results; two cold boots per variant with and without CD debugger previews |
| Command/periodic protection and cancellation | Before/equal/after command, seek and periodic deadlines; CR4 release, SCDQ masks/acknowledgements, pending command protection, software reset and power cancellation; chunks 1/3/17/10000 with passive CR4/FIFO previews through the Bus/Machine/Wasm debugger route; existing SMPC CDON/OFF and complete regressions retained | Rebuilt release Wasm under `/saturn-rust/`; inherited report/home negative module fails the same guest assertions as independent native vectors |

New CDRPT expanded 512-KiB ROM SHA-256:
`3d2bb8c8dd3a4f92120ff71030bcce1d7257b055954aa9b14a361bfa7230f9b1`.
Original/changed 100-sector XA media SHA-256:
`f1d0d0927f59d65e24e4801053f5a6b9554a41b489271bd27a8268d6ab5ec304` /
`8f40fffd5a354f01666aaef83a74fafc416cdb9b593b1662f851bd39e06e93cf`.
The inherited ECC/retry guest now polls ERROR `09FF`, with invalid public FAD,
while retaining every received/corrected sector and retry/recovery assertion.
Its current expanded ROM SHA-256 is
`7c07a24da7db28b6235e2f42c7fa21ab1803587d8299deb4239102ac209d32ce`;
older validation records/hashes describe historical artifacts.

BUSY transitional validity and FATAL contents are not assigned invented exact
values. Functional timing envelopes, PAUSE flag conventions and invalid Q
serialization are explicitly bounded in the contract. Full mechanical timing,
R–W decoding, additional formats, remaining CD/HLE and all other #17 hardware
requirements remain open; this increment demonstrates no commercial compatibility.


## Slave DCC raster interrupts (RASTER)

Related to #47 and #17. [STN-28 attachment, Dual CPU Usage Ver.1.1 §2.3](https://www.infochunk.com/saturn/segahtml_en/info/hon/stn28_1.htm)
specifies independent slave IRL2/vector `0x41` and IRL6/vector `0x43` levels throughout
HBlank/VBlank. [SH7604][H] §§4.4/4.6/4.7, table5.4 and §5.3.8 establish
SR.I acceptance, deferral, stack/vector entry, external-before-equal-internal
priority and normal autovectors. [VDP2 ST-058-R2 §2.5](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p02_50.htm)
distinguishes live blanking from DISP=0's forced TVSTAT VBLANK readback.

| Contract | Independent native evidence | Executing guest / browser evidence |
| --- | --- | --- |
| Live edges, IRL2/0x41 and IRL6/0x43, VBlank wins simultaneous HBlank | [Nine board/CPU/scheduler tests](../src/bus/slave_raster_tests.rs): before/at/after boundaries, NTSC/PAL, both PLLs, 224/240/256/480 lines, normal/high resolution/exclusive modes, two interlace fields and literal main/reference-clock deadlines | [RASTER guest](../tests/slave_raster.rs) wakes the slave from SLEEP for both sources |
| SR.I thresholds, level re-entry, no stale masked event, internal arbitration | SR.I 0/1/2/5/6/7/15, RTE/slot, unmask within/after HBlank, FRT above/equal/below VBlank | Two H entries within one line and two V entries within one line; handlers edit stacked SR to suppress later requests and return to ordinary guest computation |
| SCU and DISP independence | Slave acknowledgement retains IST; master IMS changes/acknowledgement do not remove slave levels; DISP=0 throughout edge vectors | Guest keeps master SCU masked and explicitly configures slave ICR.VECMD=0 |
| Replay and reset domains | Pending exception across HBlank deassertion enters once, retires no instruction, retains exact PC/SR/SP; delayed branch/control successor; pending CPU-boundary cancellation by SSHOFF/cold/SYSRES/both PLL commands; timed SSHON unmask observes current VBlank after each reset | Two cold boots for normal/changed arithmetic and masked/omitted-COMREG controls; identical records for chunks 1/3/17/10000, with/without passive inspection |

The original [builder](../tests/fixtures/build_slave_raster.py),
[assembly](../tests/fixtures/slave-raster.s), [ROM](../tests/fixtures/slave-raster.hex)
and adjacent [MIT license](../tests/fixtures/LICENSE) use no firmware or SDK.
Expanded 512-KiB ROM SHA-256:
`a33a1b1623df8430aee0616516266e21b302c694b26b4263801c7fd16cf450af`.
The [browser check](../scripts/slave-raster.mjs) runs the same bytes through
production Wasm under `/saturn-rust/`; CI reproduces the fixture and runs the
complete browser suite. [Validation](validation-issue-47.md) records actual checks,
negative controls, fixture corrections and remaining scope.

DCC sampling uses the existing raster event phase and CPU boundary journal;
no synchronizer/noise-canceler latency or measured electrical phase is claimed.
Normal IRL autovectors are supported. Slave VECMD=1 board-vector acquisition
remains unsupported; no D7–D0 value is invented and no raster vector is delivered
in that mode. Half-line interlace remains outstanding; HLE interrupt initialization
now has the separate [#49 contract](hle-dual-startup.md) and evidence below. Full CPU/peripheral, bus/reset/timing, SCU/DSP, video/audio, CD/media,
SMPC and integration conformance remains pending. Commercial compatibility is
unverified and #17 remains open.


## HLE dual-CPU interrupt startup (HLEDUAL)

Related to #49 and #17. [The primary-derived contract](hle-dual-startup.md)
reconciles System Program/STN-28 initialization with SH7604 register encodings,
widths and acknowledgement. Later entry94/capture priority15 supersede the older
ST-202-R1 printed p.3 entry164/level1; entry registration is distinct from slave
capture-handler registration at master index164.

| Contract | Implementation | Independent evidence |
| --- | --- | --- |
| CPU-local capture enable, priority15/vector64; other internal priorities zero; no phantom request | Original guest [trampoline](../src/hle/boot.s), [reproducible ROM](../src/hle/build.py); hardware reset and external BIOS untouched | [Literal boundary checks](../src/hle/startup_tests.rs), including reset values before/after installation, masked pending capture, unmask, frame, guest read-before-clear, RTE and reassertion; both CPUs' entry register records |
| Master external SCU vectors, slave normal raster autovectors41/43 | Separate ICR writes in the trampoline; existing board/interrupt mechanism unchanged | [Original HLE disc](../tests/fixtures/build_hle_dual.py), [native Machine](../tests/hle_dual.rs), [browser Wasm](../scripts/hle-dual.mjs): bidirectional SLEEP/handler/RTE arithmetic, two successive captures, H/V level re-entry, SR masks, master SCU masking, independent raster-only entry |
| Retained vectors/shared SDRAM and metadata stacks across slave/clock restarts | SSHON reruns guest initialization without reinstalling work-RAM vectors | Full 512-byte slave table compared at four entries; changed entry operand; default/custom SP and stacked SP; restarts after both clock selections; SYSRES/cold power-clear and backup retention |
| Independent reproduction and controls | Original MIT source/disc/TSV, registered in existing CI | Changed master/slave/request/raster operands, omitted-strobe and masked-raster controls, repeated boots, native chunks1/3/17/10000 with/without inspection; same authored records under production Wasm at `/saturn-rust/` |

[Validation #49](validation-issue-49.md) records actual full-suite results and
independent failures against the inherited bootstrap. Existing #47 board timing,
SCU-independence and reset/journal assertions remain intact. Slave VECMD=0 is the
HLE configuration that provides the specified vectors, not a Sega ROM register
dump. Slave external board-vector acquisition, physical synchronizer timing,
half-line interlace, full BIOS/CD-library initialization and all other unmet #17
requirements remain outstanding. This is a partial #17 increment; hardware
completion and genuine BIOS/commercial-game compatibility are not established.

## SCU A-to-B startup interlock

Related to #51 and #17. The [startup contract](scu-dma-startup.md) implements
ST-210 §3 item12 (printed p.5) through the existing transaction scheduler:
pending writes from either SH-2 defer A-to-B data activation; active transfers
retain qualification across physical words, preemption and indirect descriptors.
Other routes and descending DMA-level priority retain existing arbitration.
Eight [native transaction tests](../src/bus/dma_startup_tests.rs), including a
72-combination publication/source-count matrix, cover startup, continuity,
isolation, cancellation and advancement/inspection repeatability. An original
[MIT](../tests/fixtures/LICENSE) mixed-disc
[guest](../tests/fixtures/build_dma_startup.py) runs in
[native integration](../tests/dma_startup.rs) and the production
[Wasm/browser suite](../scripts/dma-startup.mjs), with Timer0/CD FIFO/video DMA,
real DMA-end handler, guest sums/counts, changed operands, omitted trigger,
repeated cold boots and actual Canvas output. The ISO SHA-256 is
`d04cf218087823c9a2d35c63b720c204f6e3bb14d575f10a2c71bf9b2749fd05`.
[Validation](validation-issue-51.md) records actual results and negative controls.
Equal-edge/continuous-write and descriptor boundaries are documented emulator
conventions. Electrical hang behavior and prohibited transfer/concurrency modes
are not modeled. Remaining CPU/peripheral, multiprocessor/bus/reset, SCU/DSP,
video/audio, CD/media/HLE, SMPC and comprehensive integration requirements remain
mandatory. Genuine BIOS and broad commercial-game compatibility remain unverified;
Issue #17 remains open and historical trackers remain administratively closed.

## VDP1 scaled/distorted horizontal sampling

Related to #57 and #17. The [sampling contract](vdp1-sampling.md) reconciles
ST-013-R3 pp.44/81–82, the later Japanese/English §6.3 Figure 6.5, §4.2 EOS,
§§7.5–7.6 and Chapter 9, plus the version-0 supplement and STN-15 Clip/Cmod
correction. All illustrated 3/8 through 10/8 rows now match, including ordinary
8→4 `[1,3,5,7]`, 8→3 `[1,4,6]`, and HSS/EOS=0 8→3 `[0,4,6]`.
HSS first selects a half-width parity source on reduced lines, then scales that
source. Unchanged/enlarged lines retain all source pixels. HSS ignores end codes
at every magnification despite the conflicting ECD summary row.

| Contract | Independent native evidence | Guest/browser evidence |
| --- | --- | --- |
| All three Figure 6.5 panels, EOS independence with HSS clear, both directions and command flips | [Literal framebuffer vectors](../tests/vdp1_sampling.rs), both command kinds; source widths 8/16/24/504, single-dot and enlargement cases, distorted rows with different spans, vertical flip | Extended [MIT guest builder](../tests/fixtures/build_sprite_shrink.py) executes twelve scaled/distorted/flipped draws and reads 53 actual framebuffer samples |
| Six texture formats, packed-word crossings, lookup data, transparency, ECD/HSS, mesh, Gouraud and clipping | Original authored nibbles/bytes/RGB/LUT words; original phase retained through pre-clipping reversal and user clipping | Guest constructs texture/command words through SH-2 MMIO, polls drawing, computes its checksum and publishes a distinguishing 8→4 row |
| Acquisition/retention, VRAM/framebuffer ownership, passive inspection, cancellation and publication | [Scaled transaction vectors](../src/vdp1/sampling_tests.rs), chunks 1/3/17/10000, writes before/during/after acquisition, retained packed samples, ENDR/PTMR/system reset and fresh drawing | [Native Machine](../tests/sprite_shrink.rs) and [production Wasm/Canvas](../scripts/sprite-shrink.mjs) use identical ROM bytes and literal records, changed-texture, omitted-draw and retained HSS-clear controls, two cold boots each |

[Validation #57](validation-issue-57.md) records hashes, actual complete-suite
results and the independent native/browser failures of the inherited sampler.
Generalization to ratios beyond the illustrated rows is explicitly inferred;
physical source-walk bandwidth, skipped-texel end-code acquisition, vertical
scaling and polygon coverage remain outside this horizontal-selection increment.
Existing two-clock texture acquisition and framebuffer publication timing are
emulator conventions. All unmet CPU/peripheral, multiprocessor/bus/reset timing,
SCU/DSP, video/audio, CD/media/HLE, SMPC and integrated acceptance requirements
remain mandatory under #17. This partial #17 improvement establishes neither
hardware completion nor commercial compatibility; historical trackers remain closed.
