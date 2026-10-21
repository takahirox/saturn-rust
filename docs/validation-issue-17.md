# Issue #17 implementation validation

**Result: partial implementation. Related to #17. No umbrella acceptance
criterion is complete; #17 must remain open.** Issues #11/#13/#15 are historical,
administratively closed trackers and must not be reopened.

## PR #18 VDP1, bitmap composition and original playable disc (2026-10-21)

The intended PR target remains **all of Issue #17**, with `Closes #17` retained
as the intended final outcome. The implementation is still incomplete; the PR
must remain Draft and must not be merged as umbrella completion. No mandatory
criterion is waived. The full-resolution Run requirement has not been achieved.

This revision preserves upstream checkpoint
`7285a713490ab20c2cb7b82af126a72d05c35a44`, including its unpublished original
68k/SCSP/WebAudio/SMPC work. Upstream main was rechecked and remains
`dbd981b030a46daa6923072ce240680dd2e58ebf`. The existing PR #18 branch is the
publication destination; no replacement PR/Issue, merge or Issue closure is used.

Implemented and exercised:

- Original clock-driven VDP1 command processor, command/texture VRAM and two
  framebuffers; PTMR, status/address registers, forced stop, field changes and
  erase windows; next/assign/call/return/skip/END, clipping/local coordinates,
  normal/scaled/distorted textures, polygons/lines/polylines, six texture formats,
  mesh, Gouraud and shadow/half-luminance/transparency calculations.
- Drawing completion actually sets CEF and triggers SCU source 13 and DMA
  factor 6. Independent clock vectors cover 1/3/8/17/32-clock advancement,
  completion/transfer ordering, acknowledgment and reset. A cyclic command list
  remains busy; prohibited commands produce explicit bounded faults.
- VDP2 NBG0/1 bitmap sampling and composition, palette/RGB formats, CRAM modes
  and mode-0 mirroring, bank/palette offsets, fixed-point scroll/zoom, mosaic,
  normal windows, priorities, weighted/additive blending, back-screen tables
  and signed color offsets. Color offset follows blending using the top screen's
  selection (ST-058 §13.1). Unsupported enabled modes/effects fail explicitly.
- Original MIT-licensed **Prism Courier** disc, assembly/asset/build source and
  embedded/public license. Guest SH-2 logic programs VDP1 and VDP2, polls SMPC,
  implements progress/collision/delivery/restart and handles VDP1 IRQ via its own
  VBR table. An independent guest-uploaded 68k computes PCM, programs SCSP and
  handles a timer interrupt. The browser has a packaged-disc play control.
- Native and browser Wasm execute the same disc and compare independent pixels,
  computed sound RAM, nonzero unequal stereo channels, guest IRQ counters, two
  input sequences, loss/win/restart and reset. Mutating its original texture
  changes native and Wasm output. The browser checks actual Canvas presentation
  and delivery of this disc's PCM through the real AudioWorklet.

The [conformance matrix](hardware-conformance.md#vdp1-and-vdp2) links primary
manual sections, implementations, 13 independent video-vector tests and guest
integration. Source: ST-013-R3 §§2–7; ST-058-R2 §§3–6,8–13. No emulator core,
manual program, proprietary BIOS/SDK, recording or third-party asset is included.

Prism Courier ISO SHA-256:
`2fd953d65b931383518f28f81919f5c89c3ea8974b742546e1606333c2064bee`.
The existing Star Collector disc and all five synthetic-ROM builder outputs remain
byte-for-byte reproducible. See the [guest README](../homebrew/prism-courier/README.md).

Validation on the final code in this revision:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  **177 native tests, zero failures and zero ignored tests**.
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`: pass.
- `sh scripts/build-web.sh`: pass with the documented Rustup stable toolchain
  and wasm-bindgen-cli 0.2.129, generating both original discs and licenses.
- Star Collector, Prism Courier and all five synthetic-ROM builders' `--check`
  commands: pass. CI now checks the new disc. License-copy comparison: pass.
- Full `npm run test:browser`: pass in Chromium **145.0.7632.6**, with the same
  packaged assets mounted under `/saturn-rust/`. Existing address-error, dual-CPU,
  SCU, 68k/audio, SMPC, gameplay, backup/input/privacy, responsive and fullscreen
  assertions remain active. The new Play control is covered at all three sizes.
- JavaScript syntax and Git whitespace checks: pass.

The first new video vectors exposed shadow-MSB halving and BEF/erase lifecycle
errors; these were fixed against the manual. Review also corrected color-offset
ordering and added a vector distinguishing top/second-screen offset selection.
Two browser-test failures were test assumptions: view navigation does not redraw
the Canvas, so the check now uses the actual Debug frame control; audio delivery
must wait for AudioWorklet initialization after the enable gesture. Final checks
pass without removing the pixel/audio assertions or skipping required tests.

Remaining mandatory work includes complete SH-2/SH7604 and MC68EC000 edge
conformance, full bus/reset/backup services and timing/arbitration, remaining
SCU/DMA/DSP behavior, VDP1 edge/scanout/timing and VDP2 tile/rotation/line/shadow/
display modes, full SCSP synthesis/effects DSP/CD-DA, runtime CD/mixed BIN/CUE,
BIOS/HLE service ABI and full synthetic boot/service pathways, remaining SMPC
system/clock/analog controls, the hardware exerciser disc, and complete linked
native/Wasm/browser evidence. Prism Courier supplies the first required new
demonstration; it does not replace the second or any hardware acceptance.

No real BIOS, human testing/approval, live Pages inspection, external-workflow
investigation or post-merge gate is required. Genuine BIOS/retail compatibility
remains not independently confirmed. This validation does not claim remote CI
or a production deployment passed. Required post-merge verification: **None**.

## PR #18 sound CPU, PCM and SMPC implementation (2026-10-21)

Related to #17. This revision implements review findings in the existing PR; the
operator's full-resolution objective **is not achieved**. Required missing systems
remain mandatory and are not waived or replaced with a continuation tracker.
No push, merge, Issue closure or external approval action is performed.

Started at review checkpoint `91fb527ec48e6a7a8df4da1153daa04e56d4d790`;
GitHub PR #18 head remains `0de971dee24036439472e4a9925c16f8459755d8` because this
node only commits locally. Upstream `main` was rechecked with `git ls-remote`
and remains `dbd981b030a46daa6923072ce240680dd2e58ebf`.

Executable additions:

- Original MC68EC000 integer interpreter with base effective-address modes,
  CCR/SR, USP/SSP, 24-bit pins, word-aligned long transfers, normal and seven-word
  bus/address exception frames, privilege/illegal/trap/line-A/line-F/trace,
  autovectored IRQ levels, STOP wake and RESET output. Native semantic vectors
  cover every applicable family, including 12,000 wide-arithmetic property
  vectors; the separate all-encoding safety test is not semantic proof.
- Shared 512 KiB sound RAM and SCSP byte/word buses. SMPC SNDON starts from the
  sound guest's own vectors; SNDOFF resets the sound CPU while retaining RAM and
  SCSP operation. The independent sound CPU and sample domains have repeatable
  integer deadlines across different advancement chunk sizes.
- SCSP 32-slot direct signed PCM8/16, key/loop/pitch/source-bit controls, stereo
  send/pan, clipping, stack, timers/IRQ, MIDI FIFO/status/overflow/byte lanes and
  a word DMA subset. Unsupported envelope/FM/LFO/effects-DSP settings produce
  errors. Exact opcode/bus timing, fractional PCM interpolation, complete
  synthesis, effects DSP and CD-DA remain unimplemented or unverified.
- Bounded native PCM queue and browser producer/AudioWorklet ring, explicit
  gesture enable, sample-rate adaptation, pause/mute, suspended-context blocking,
  underflow and drop counters. Chromium renders nonzero guest-produced audio;
  no human listening or proprietary firmware is used.
- SMPC IREG/OREG/SR/SF, deterministic BCD/Gregorian RTC, SETTIME/SETSMEM battery
  retention, timed INTBACK status/peripheral phases, continue/break, VBlank
  collection/optimization, SCU source-7 IRQ, reset-button mask/edge/status,
  and standard ID15 analog joystick reports. Synthetic browser Gamepads exercise
  both ports and focus release. Remaining system/clock commands, analog direct
  handshake and six-byte 3D-pad support remain required.

Primary sources: M68000PRM §§1–4, supervisor instruction entries and appendix B;
ST-077 §§1.2/4.1/4.2, tables 4.14/4.27/4.28/4.33–4.39 and figures 4.59–4.66;
ST-169 §§1.2/2.3–2.5/3.2/3.3, tables 3.18/3.19. Links and implementation/test
inventory are in the [conformance matrix](hardware-conformance.md). No emulator
source, CPU core, Sega SDK object, proprietary BIOS or manual example program is
included. Source-authored fixtures use the adjacent MIT license.

Original fixture SHA-256 values, after expansion to a zero-filled 524288-byte ROM:

| Fixture/source | SHA-256 |
| --- | --- |
| [sound](../tests/fixtures/build_sound.py) | `a6b824df82cba35aa7fa362e967474dc1b89ed6baa3e9ed00652cd20ba613f7c` |
| [SMPC](../tests/fixtures/build_smpc.py) | `9e4a267d6b225f07245b490667af26cd71f9e83ee95b659d59438263ccceafc2` |

The sound SH-2 guest uploads and starts an independently executing 68k program.
That program computes `[4096,-4096,-8192,8192]`, programs SCSP direct PCM with
pan 8, handles a timer IRQ and returns through RTE. Native and Wasm assert
stereo pairs `[256,4096],[-256,-4096],[-512,-8192],[512,8192]` and restart
repeatability. Mutating the guest's MOVEQ immediate halves both its computed
RAM waveform and actual stereo samples. This rejects canned audio and no-op cores.
The real browser producer/worklet then renders the resulting nonzero samples;
its instrumentation proves render/drain/underflow, mute and blocked states.

The SMPC guest sets time/SMEM, enables SCU SMPC IRQ, collects status, toggles
continue and waits for VBlank peripheral completion. It records both ports and
two IRQs in guest-written mailboxes. Native and Wasm compare independently
expected active-low button/X/Y/Z bytes. Browser focus loss produces released
buttons, centered X/Y and zero Z while retaining analog device identity.
These are diagnostic ROMs, **not either required demonstration disc**.

Validation performed on this revision:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  **161 native tests**, zero failures/ignored tests. Star Collector completion
  and restart, both pads, backup atomicity/retention and existing CPU/SCU guests pass.
- Native all-target and wasm32 Clippy with `-D warnings`: pass.
- `sh scripts/build-web.sh`: pass with Rustup stable and matching
  wasm-bindgen-cli 0.2.129. The first attempt selected the host Homebrew compiler
  without Wasm libraries; selecting the repository-documented Rustup PATH fixed
  that toolchain mismatch. No source workaround or test skip was used.
- All six reproducibility checks pass: Star Collector plus dual-SH-2, SH7604,
  SCU, sound and SMPC fixtures. CI now checks both new fixtures.
- `npm ci --ignore-scripts` and the full `npm run test:browser`: pass in
  Chromium **145.0.7632.6** at `/saturn-rust/`, including actual ISO gameplay,
  responsive/fullscreen/input/privacy assertions and all old/new Wasm diagnostics.
  An initial WebAudio wait read empty diagnostic counters. The wait now executes
  in the producer's main-world context and asserts the real consumer's nonzero
  rendering and drain states.
- `git diff --check`: pass. Generated bindings and manual extracts are not committed.

Unmet acceptance remains: SH7604/SH-2 edges and peripheral groups, full bus/reset/
backup filesystem and arbitration, remaining SCU/DSP/device interactions, VDP1,
advanced VDP2/composition, complete SCSP synthesis/effects DSP/CD-DA, runtime CD
and mixed media, BIOS/HLE service ABI/full original boot pathways, remaining SMPC
controls and analog model, both licensed demonstration discs and their complete
native/Wasm/browser coverage. Existing green checks do not satisfy those missing
features. Issue #17 must remain open and PR #18 is not ready to merge as its resolution.

## PR #18 SCU execution and raster timing (2026-10-21)

The full Issue #17 objective remains mandatory and **is not achieved**. PR #18
must remain Draft; this revision is progress within that PR, not a successful
umbrella completion. No historical Issue is reopened, no replacement Issue is
created, and no merge or Issue closure is performed. The operator requires the
PR's full target and intended final `Closes #17` outcome to be preserved despite
its current incomplete implementation.

Started at worktree checkpoint `6e0a446e225db835f1fec7b1c47db5b13b150c98`,
which includes the earlier SH7604 fixes and PR head
`a286e489f06c4044368b03a2e947f5bbef673bd7`. Upstream main was rechecked with
`git ls-remote` and remains `dbd981b030a46daa6923072ce240680dd2e58ebf`.
The earlier work is preserved.

New executable behavior:

- SCU IMS/IST, fixed vector/priority selection, master SH-2 interrupt acceptance
  and acknowledgment, A-bus acknowledgment gate, timer0/1 and reset state.
- Three SCU DMA channels execute direct and indirect bus transfers with counts,
  unaligned heads/tails, B-bus halfword strides, source/destination save/update,
  priority preemption, edge/GO triggers, one held retrigger, end/error handling.
  Final-spec indirect entries are count/write/read with the end bit in read bit 31.
- The original SCU DSP interpreter executes parallel X/Y/D1 operations, ALU and
  signed multiplication, immediate transfers, delayed branches, BTM/LPS loops,
  END/ENDI, pause/single-step, data ports and asynchronous external DMA/HOLD.
- NTSC 320-dot raster events drive TVSTAT HBlank/VBlank, SCU edges/timers and
  bounded field advancement. SCU DSP instructions and DMA advance by clock
  deadlines, including during DIVU waits/SLEEP. Native tests compare results for
  different clock chunk sizes. CPU issue/on-chip DMA still use fixed turns;
  full CPU/bus timing and other television modes remain required work.

Primary references are original **ST-097 §§2–4** and the superseding
**ST-210 final-specification corrections #1–27/30–36**, linked in the
[conformance matrix](hardware-conformance.md#scu-and-dsp). In particular, the old
DSTA/DSTOP functions are removed and IST writes prohibited in the final spec;
unsupported accesses fail rather than accepting inert writes. No emulator source,
third-party core, SDK objects, proprietary BIOS or manual sample program is used.

The original MIT-licensed [SCU guest source/builder](../tests/fixtures/build_scu.py)
generates [scu.hex](../tests/fixtures/scu.hex). Its zero-filled 524288-byte expanded
ROM has SHA-256:

```text
d8acbfdede3290fd581a2227e6f3eb8e6c1eb05a5edb38d415efe2b11f5d137a
```

Both [native integration](../tests/scu.rs) and the real
[browser Wasm test](../scripts/browser-smoke.mjs) boot that same synthetic ROM.
After boot, only guest instructions write guest/device state. The guest loads a
DSP program computing 7×6+42=84, performs direct DMA to VRAM, walks two indirect
entries, then loads another DSP program that DMA-loads 7/11/13/17 and sums them
to 48 before DMA-writing the result to high RAM. Guest IRQ handlers observe DSP,
DMA0/1 and timer0/1 completion, read the actual devices, preserve registers/stack,
and return through RTE. The eleven independently expected mailbox longwords are:

```text
00000002 00000002 00000001 00000054 00000007 00000030 0000600D
00000001 00040015 00000007 0000000B
```

There are six accepted interrupts; final SP=060FF000 and CPU=SLEEP. Cold reset
repeats the results. A negative test patches an original DSP instruction to
40000000 and requires an observable sticky bounded machine fault at DSP PC 00,
including through the browser UI. No-op DSP/DMA implementations cannot produce
the required computed/mailbox/VRAM results. This fixture is a CPU/device diagnostic
ROM and **does not satisfy either new demonstration-disc requirement**.

Validation performed on this revision:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  **117 native tests**, none ignored. Existing address-error, dual-CPU, backup,
  two-pad, ISO parser and game completion/restart tests all remain in the suite.
- Native all-target Clippy and wasm32 Clippy, warnings denied: pass.
- Star Collector, dual-SH-2, SH7604 and SCU fixture `--check` builds: pass.
  CI verifies the new SCU fixture alongside the preserved fixtures.
- `sh scripts/build-web.sh`: pass with the existing Rustup stable toolchain
  and matching wasm-bindgen-cli 0.2.129. No new Rust dependencies/core are added.
- `npm ci --ignore-scripts`, `npx playwright install chromium`, and the complete
  `npm run test:browser`: pass in **Chromium 145.0.7632.6** under `/saturn-rust/`.
  New SCU output/reset/fault assertions run alongside all previous UI, privacy,
  file/error, input, gameplay, backup and synthetic BIOS checks; no required check
  is skipped. Native/browser initial runs exposed a fixed old gameplay-frame
  budget assumption (position 252, four stars after 48 fields). With the new clock
  deadlines, tests now hold input until the actual guest win state within 80
  fields, preserving all position/score/pixel/completion/restart assertions.
- Browser-script syntax, source whitespace and documentation relative links: pass.

Remaining mandatory implementation includes MC68EC000, VDP1 and advanced VDP2,
SCSP/effects DSP/CD-DA/WebAudio, runtime CD/BIN/CUE, complete BIOS/HLE/backup and
SMPC/analog services, SH7604/CPU/reset/bus edges, SCU arbitration/device routes and
complete DSP transfer/condition/bank-conflict evidence, and both required original
licensed guest demo discs. The matrix keeps these pending; passing this subset is
not full hardware conformance. Genuine Sega BIOS/retail compatibility is not
independently confirmed and remains optional. No human-only approval/testing,
external workflow investigation, personal firmware or live Pages gate is added.

## PR #18 review fixes: SH7604 and cache (2026-10-21)

Started from review checkpoint `1cd6385b801d7b5462a915794687a13df3eb9102`,
which preserves PR head `a286e489f06c4044368b03a2e947f5bbef673bd7`.
Upstream main remains `dbd981b030a46daa6923072ce240680dd2e58ebf`, verified with
`git ls-remote origin refs/heads/main`. Changes are local; no push or merge.

Implementation added in this revision:

- Misaligned exception stacking now completes the original exception and reaches
  vector 9, inhibiting recursive stacking errors as required by SH7604 §4.8.3.
  Undefined stacking data use explicitly documented big-endian bytes at the exact
  SP. Native vectors cover offsets 1/2/3; the shared guest repairs its stack.
- CPU-local WDT interval/reset modes, keyed writes, divisors, observed-one/clear
  protocol, IRQ vector/priority, reset-cause retention and 512-clock internal reset
  hold (H §§12.2–12.3). Reset vectors use physical 8/12 for manual reset and
  preserve shared RAM and battery backup. Browser Step/Run remain usable while
  reset completion is pending.
- CPU-local SCI registers, internal-clock transmit timing, status, receive errors,
  external-character adapter, interrupt sources, and real RXI/TXI DMA handshakes
  (H §§13.2–13.3). No implicit loopback or invented incoming data.
- CPU-local signed 32/32 and 64/32 DIVU, dependent-access interlocks, completion
  timing, DVCR/VCRDIV and overflow IRQ (H §§10.2–10.4). **The six-cycle overflow
  intermediate is not implemented:** overflow output is deterministic saturation
  plus input-high preservation, including OVFIE=1. This remains an explicit gap.
- On-chip DMAC channels 0/1, transfer widths 1/2/4/16, counts/address updates,
  auto/DREQ/SCI triggers, fixed/round-robin channel selection, end IRQs, flag
  acknowledgement, NMI cancellation and CPU vector 10 for address errors
  (H §§9.2–9.3). Single-address DACK mode is rejected explicitly. Burst/cycle-steal
  CPU arbitration and bus-cycle timing are not implemented.
- IPRA/IPRB and VCRA–D/VCRWDT internal source arbitration, using programmed
  priority and the manual table 5.4 tie order. Peripheral flags remain guest
  acknowledged. External ICR/IRL, refresh and UBC sources remain missing.
- Independent 4 KiB write-through caches, CCR ID/OD/CE/CP/TW, pseudo-LRU,
  associative purge, address/data arrays, two-way RAM and TAS external reads
  (H §8, tables 8.2–8.4, figs.8.10/8.11). Slave/DMA writes do not snoop master
  cache; the guest must purge it. Hardware fill timing/contention remain absent.

The original MIT-licensed [sh7604.s](../tests/fixtures/sh7604.s), built by
[build_sh7604.py](../tests/fixtures/build_sh7604.py), executes the same guest code
natively and in Chromium Wasm. Its expanded 524288-byte synthetic BIOS SHA-256 is:

```text
a45486b44108ca2db9abd4df0d05fe030c75f01ba73fbc4e64ab64eae9964880
```

Guest instructions calculate signed quotient/remainder, initialize/copy four
RAM words with DMA, acknowledge DMA/WDT interrupts, recover from a DMA address
error at SR.I=15, transmit through SCI, prove stale-cache/purge behavior, trigger
manual reset, observe CCR reset/cause retention, and reach the address-error
handler after misaligned TRAPA stacking. The sixteen golden mailbox words at
06003000 are independent test constants:

```text
FFFFFFF2 FFFFFFFE 55555555 00000001 00000001 00000001 FFFFFF84 00000001
FFFFFFFF 00000001 0000600D 00000000 00000000 12345678 00000000 00000001
```

They encode q32/r32, q64/r64, DMA IRQ, WDT IRQ, SCI status, manual-reset count,
reset cause, stacking-error handler, completion, pre-DMA cache value, stale value,
purged value, reset CCR and DMA address-error handler. After reset there are two
exceptions (TRAPA then address error), saved PC=00001C00, SP=060FF000 and no
pending address error. The four DMA destination words are 12345678–1234567B.
Cold reset repeats the result. This ROM is a peripheral diagnostic, **not** either
required new demonstration disc.

Checks performed on this revision:

- Formatting, locked native build and all **100 native tests** pass; none ignored.
- Native all-target Clippy and Wasm Clippy with warnings denied pass.
- Star Collector, dual-SH-2 and new SH7604 fixture reproducibility checks pass.
  CI now verifies the new fixture too; the existing ISO and dual guest are unchanged.
- `sh scripts/build-web.sh` passes with the existing rustup stable toolchain and
  wasm-bindgen-cli 0.2.129. An initial attempt selected Homebrew Rust without its
  Wasm target; selecting the installed rustup toolchain fixed that environment
  mismatch without source/dependency changes.
- `npm ci --ignore-scripts`, `npx playwright install chromium`, and the complete
  `npm run test:browser` pass in Chromium 145.0.7632.6 under `/saturn-rust/`.
  The new guest assertions include repeat execution and usable controls during
  watchdog reset, alongside all existing game/input/backup/privacy checks.
- JavaScript syntax and Git whitespace checks pass.

The full operator objective is **not achieved**. CPU/SH7604 edge cases, BSC/UBC/
standby, full mapping/backup services and hardware-time scheduling remain pending,
as do MC68EC000, SCU/DSP, VDP1/advanced VDP2, SCSP/DSP/CD-DA/WebAudio, runtime
CD/BIN/CUE, complete BIOS/HLE and SMPC/analog behavior and both required new
licensed demo discs. Passing these tests does not satisfy those missing criteria.
No human-only gate, proprietary file, new Issue or historical Issue reopening is
needed to continue implementation. See the [conformance matrix](hardware-conformance.md).

## Earlier dual-SH-2/FRT validation (historical)

Read live [Issue #17](https://github.com/takahirox/saturn-rust/issues/17) through
the GitHub connector. `git ls-remote origin refs/heads/main` returned
`dbd981b030a46daa6923072ce240680dd2e58ebf` on 2026-10-21, the preserved PR #16
checkpoint. The assigned worktree started at
`850d3c1f9112d056281cfd967e3730320be4e1c8`; no technical diagnosis was supplied.
No push, Issue change/comment, PR creation or merge is part of this node.

## Implemented scope and primary references

- `Machine` owns an independent slave `Cpu`, initially held in reset. SSHON
  starts its common power-on vectors and SSHOFF resets the slave CPU/FRT,
  preserving shared RAM, master CPU and backup. Sources: SMPC User's Manual
  ST-169 table 1.1 and §2.3 pp.23/24; Dual CPU User's Guide ST-202 §1.1;
  SH7604 Hardware Manual §4.2.2 and table 4.3.
- Both CPUs execute the original interpreter against shared external memory and
  independent FRT/INTC state through `Bus::for_cpu`. CPU-specific errors stop
  the entire machine until reset; a failed slave exception entry is observable
  in the browser and cannot be bypassed by calling Run again.
- Original FRT implementation covers FRC/OCRA/OCRB/ICR, TIER/FTCSR/TCR/TOCR,
  shared TEMP, internal divisors, compare/counter-clear/overflow flags, computed
  output levels and observed-one/write-zero acknowledgment. IPRB FRT priority,
  VCRC/VCRD and level-sensitive IRQ dispatch are integrated. Sources: SH7604
  §§5.3.2/5.3.6/5.3.7 and §§11.1–11.5, tables 11.2/11.3.
- Word strobes at 21000000/21800000 signal slave/master FRT capture, as in
  ST-202 §1.2 and §§3/4. P0 aliases also use the shared external bus. No canned
  mailbox/result writes or CPU/game identification hooks exist in the host.
- SMPC byte SF/COMREG/OREG31 sequencing implements SSHON=02, SSHOFF=03 and
  NMIREQ=18 with a minimum 807-reference-clock delay. NMIREQ latches master NMI
  through instruction deferral and wakes SLEEP at SR.I=15. Source: ST-169 §1.2
  pp.5/6 and §2.3 pp.21/23/24/32 (30 µs minimum). Other commands fail explicitly.
- The browser keeps bounded execution available for a sleeping master with a
  possible hardware wake source, and reports slave faults as machine faults.
  Existing input release, file privacy and Star Collector behavior are preserved.

## Original licensed guest and independently expected results

[dual-sh2.s](../tests/fixtures/dual-sh2.s) is original guest assembly, covered by
[the adjacent MIT license](../tests/fixtures/LICENSE). The original project
assembler accepts a section base plus LDC/SLEEP/RTE; its existing Star Collector
output is unchanged. [build_dual_sh2.py](../tests/fixtures/build_dual_sh2.py) emits
[dual-sh2.hex](../tests/fixtures/dual-sh2.hex), checked for reproducibility in CI.
The expanded, zero-filled 524288-byte synthetic BIOS has SHA-256:

```text
d9eda155b46f6a8ce63c6533853075e0f7c5e057fe4483c06770d4becad7a616
```

Both CPUs begin at 00000200. Guest startup writes a shared flag, so the second
processor selects its own initialization without a host shortcut or BSC stub.
The master retains R14=42 and SP=06100000; the slave executes with R14=17 and
SP=060FE000. Guest instructions configure CPU-local FRTs, exchange an input of
35 and independently compute 35+7=42. Capture IRQs wake both CPUs and guest
handlers acknowledge their own flags and return with RTE. SSHOFF/SSHON causes
fresh slave execution while shared boot/IRQ counters accumulate. Finally the
slave is reset and a timed NMIREQ wakes the sleeping master at SR.I=15.

The golden mailbox at 06002000 contains fourteen longwords:

```text
00000001 00000002 00000023 0000002A 00000002 00001234 00000002
00005678 00000002 0000002A 0000600D 0000034E 000000F0 00000001
```

These are respectively startup flag, slave boots, input, result, master IRQs,
master captured count, slave IRQs, slave captured count, restart phase, retained
master marker, completion marker, NMI saved PC/SR and NMI count. The captures are
seeded independently at 1234/5678 and use external-clock selection, avoiding a
claim that fixed interpreter turns measure actual SH-2 instruction time.
Native tests and Chromium/Wasm compare these same expected values and check
re-execution after cold reset. A separate negative test corrupts only the slave
stack literal; exception vector 66 entry must fail on the slave, remain sticky
and disable browser execution. No host writes guest state after boot.

## Checks run

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  83 native tests after the timer review correction below, none ignored.
  Existing address-error/system fixtures, backup,
  both pads, parser, pixels and Star Collector completion/restart remain covered.
- `cargo clippy --locked --all-targets -- -D warnings`: pass.
- `python3 homebrew/star-collector/build.py --check`: pass, packaged ISO unchanged.
- `python3 tests/fixtures/build_dual_sh2.py --check`: pass, hash above reproduced.
- `sh scripts/build-web.sh`: pass with matching wasm-bindgen-cli 0.2.129.
- `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`: pass.
- `npm ci --ignore-scripts`, `npx playwright install chromium`, and
  `npm run test:browser`: pass in Chromium 145.0.7632.6 under `/saturn-rust/`,
  including the dual-CPU guest, visible/sticky slave fault, existing guest traces,
  all prior UI/gameplay/backup/input tests and local-file privacy checks.
- `node --check scripts/browser-smoke.mjs` and `git diff --check`: pass.

Native validation used Homebrew Rust 1.94.0. Initial Wasm attempts selected the
Homebrew compiler without a Wasm standard library, despite Rustup having the
target installed. Selecting the existing Rustup stable toolchain directory on
PATH fixed this environment mismatch: Rust 1.99.0, wasm32-unknown-unknown, no
source workaround or dependency change. No deployment/live Pages, personal
firmware, physical device, external workflow investigation or human sign-off was
needed or performed. Genuine Sega BIOS/retail compatibility is **not independently
confirmed**.

## Timer compare review correction

The review found that FRT compared the incremented count, making OCRA=3/CCLRA=1
clear one count edge early and OCRA=0 wait until wraparound. Compare flags,
output levels and counter clear now use the value held before the count edge,
per SH7604 §§11.4.2/11.4.3/11.4.6, figs. 11.6/11.7/11.11. OVF still denotes
FFFF→0000 (§§11.2.5/11.4.7, fig. 11.12), including simultaneous FFFF compares;
clearing from other values does not assert overflow or compare against zero.

Independent unit vectors cover the four-edge OCRA=3 period, compare=0 on the
first and repeated edges, simultaneous A/B matches and overflow boundaries.
The source-authored MIT-licensed [timer guest](../tests/fixtures/frt-compare.hex)
sets all timer/INTC registers through SH-2 instructions. Native and browser Wasm
tests check compare=0/3 IRQ timing, SLEEP wake, the stacked PC/SR, guest flag
acknowledgment and RTE. These verify the existing functional clock quantum,
not physical instruction timing. Both new unit tests and the guest test fail
when only the previous `increment` implementation is restored; restoring the
fix passes all 83 native tests.

After this correction, formatting, locked build/test, native and Wasm Clippy,
web build, both existing guest-media reproducibility checks, JavaScript syntax,
and the extended Chromium 145.0.7632.6 browser suite all pass. The existing
dual-CPU fixture and Star Collector ISO remain unchanged. No umbrella criterion
is completed by this correction.

## Remaining scope and continuation

The earlier CPU-control/FRT revision used the eight-reference-clock
interleave and legacy visible/blank turn budgets. They still do **not** satisfy hardware-time
scheduling, contention, scanline timing or cycle accuracy. FRT pin synchronization,
access waits, clock-switch edges, external clock wiring and output-pin routing
remain missing. The review-fix section above supersedes the earlier missing-cache,
manual-reset and non-FRT peripheral statements. BCR1 identity, BSC/UBC/standby
and complete hardware timing remain missing. SMPC still lacks all other
commands, RTC, INTBACK, system/clock/sound controls and analog input.

All broad CPU, bus/scheduler/backup, SCU/DSP, VDP1/advanced VDP2, MC68EC000/SCSP/
DSP/CD-DA/WebAudio, runtime CD/BIN/CUE, full BIOS/HLE services and two new guest
demo criteria remain open. This diagnostic ROM is neither required new demo and
does not substitute for them. The [conformance matrix](hardware-conformance.md)
records implementation/test links and deviations. Continue under #17 without
closing keywords, reopening old trackers or imposing human-only gates.
