# Issue #17 implementation validation

**Result: partial implementation. Related to #17. No umbrella acceptance
criterion is complete; #17 must remain open.** Issues #11/#13/#15 are historical,
administratively closed trackers and must not be reopened.

## Review fixes: CPU faults, interrupt vectors and peripheral latches (2026-10-21)

This work starts from review checkpoint `e7721083d3f14c76b89d8b307580a25f88d559e6`
and preserves existing PR #18. Upstream main was rechecked as
`dbd981b030a46daa6923072ce240680dd2e58ebf`. No push or merge was performed.

- MC68EC000 instruction lookahead survives writes, while branches, exceptions
  and reset refill or discard it. Fault frames capture internal fetch PC, IR,
  saved SR, function code and processing activity independently. Vector reads
  use supervisor data; group1/2 entry faults dispatch bus/address error; faults
  during reset/group0 halt. A traced trap precedes its trace exception.
  [Native vectors](../src/m68k/tests.rs) cover vector/handler failures, reset
  words, IRQ state, trace ordering and completed word transfers. Ten additional
  [original sound-bus guest cases](../tests/fixtures/m68k_faults.py) compare full
  frames, D0/D1/A0 and retained RAM in [native](../tests/m68k_families.rs) and
  [browser Wasm](../scripts/browser-smoke.mjs). Two new family cases distinguish
  a retained next opcode from a newly fetched extension. SIOG also records its
  actual PC/SR and repairs its frame using an explicit guest recovery label.
- SH7604 ICR now implements read-only NMIL, selected NMI edges and auto/external
  IRL vectors. SCU acknowledgement retains the actual source when SH-2 takes an
  auto-vector. The independent HLE initialization and original SCU/SMPC ROMs
  explicitly select external vectors; external BIOS retains the reset default.
- SMPC EXLE/DDR gates TH falling inputs into PAD IRQ and VDP2 external latching.
  PDR merges input pins and output bits per DDR. VDP2 exposes software/edge
  latches, held HCNT/VCNT and clear-on-read EXLTFG; non-interlaced ODD is set.
  MMIO write validation no longer reads side-effecting registers.
- The original [INTCG guest](../tests/fixtures/build_intc.py) exercises both
  vector modes, DMA IRQ acknowledgement, both latch ports and thirteen signed
  DIV1 quotient/remainder cases. Native semantic tests add 262 wider-arithmetic
  comparisons. Native and browser mutation of ICR's operand prevents the second
  stage from completing. Repeated resets and repeated TH levels are tested.

Validation: `cargo fmt --check`, locked build/test, native all-target and Wasm
Clippy with `-D warnings`, web build, all **32** CI reproducibility commands,
`npm ci --ignore-scripts`, JS syntax check and the complete extended
`npm run test:browser` passed. **362 native tests passed, zero failed/ignored**
(195 unit, 167 integration); Node 24.12.0 and Chromium 145.0.7632.6. Native and Wasm checks
used Rustup stable 1.99.0 and wasm-bindgen 0.2.129. All three existing demo media remain byte-identical.
The [conformance inventory](hardware-conformance.md) links primary manual sections,
implementation, guest cases, expected vectors and reproducible hashes. The first
new tests exposed old expectations that equated data accesses with I/N=1 and
injected handler instructions after they had already been prefetched; those
fixtures now prepare their code before execution and use the manual's frames.

This does **not** satisfy the requested complete Issue #17 resolution. Remaining
review findings include SH7604 overflow/peripheral behavior, instruction-specific
68k bus microsequences, full bus/CPU/DMA scheduling and reset contracts,
SCU/DSP arbitration, video modes/raster effects, SCSP/DSP conformance, remaining
CD/media behavior, HLE service/backup ABI and remaining SMPC clock/system controls.
No umbrella criterion is checked off, waived, or made dependent on human input.
The existing issue and PR remain the implementation boundary.

## Review fixes: DMA, sound CPU timing, envelope execution and ISO startup (2026-10-21)

Related to #17. This work starts at review checkpoint `4150e797024d8afb998be164c45a72b5c4d2ddfa`,
preserves PR #18 and all three demo media, and rechecks upstream main as
`dbd981b030a46daa6923072ce240680dd2e58ebf`. No push or merge was performed.

Implemented behavior and independent evidence:

- Single-address SH7604 DMA now dispatches explicit DACK pin endpoints, selecting
  direction/polarity, ignoring the unused address, advancing the memory address,
  consuming external requests, and reporting count/TE/IRQ/error behavior. Native
  pin-adapter vectors cover both channels and byte/word/long widths. An original
  SH-2 ROM checks armed-channel waiting and reset in native and Wasm execution;
  no requesting DACK device is connected on the standard board.
- MC68EC000 cycle costs use MC68000UM tables8-1–8-14 for EA, MOVE, arithmetic,
  conditional branches, MOVEM, control, shift and exception paths. Multiplication
  counts actual source bits/transitions, and reset first issue waits forty sound
  clocks. Independent native timing vectors cover all MOVE source/destination
  combinations, representative opcode/condition boundaries and source-dependent
  multiplication. DIV and register bit modifications retain documented maximum
  costs; prefetch/bus timing is still required.
- An original 71-case 68k program adds family/EA/CCR and exception/RTE evidence
  through actual SH-2 upload and sound-bus execution in native and browser Wasm.
  The browser changes ADD.B to SUB.B and verifies changed guest results without
  changing other cases. Existing PCM, effects DSP and CD-DA checks still pass.
- SCSP envelope writes now execute attack, decay1, decay2 and release, with
  EGHOLD, DL/KRS/LPSLNK, release retrigger and SDIR bypass. Native vectors cover
  32 independent slots, all rates/levels and stage boundaries. An original 68k
  timer handler keys off a voice; native/Wasm tests compare mathematical stereo
  vectors and mutated AR/D2R/RR/LPSLNK operands, and production WebAudio renders
  its output. Numeric rate/key-scaling/gain calibration is an explicit functional
  approximation, not independent chip-timing conformance evidence.
- Bootstrap and runtime CD directory parsing scan volume descriptor sets rather
  than assuming primary/terminator at fixed sectors. Hidden files, physical
  padding and logical-volume extent bounds are tested. A zero first-read address
  disables loading while original guest AIP instructions still execute. Native
  and Wasm guest checks cover both changes and malformed sets/extents.

Sources, implementation/test links, original MIT fixture provenance and all three
new reproducible ROM hashes are in the [conformance inventory](hardware-conformance.md#review-evidence-instruction-timing-dack-envelopes-and-iso-startup).

Validation completed on this combined worktree:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  **339 tests passed, zero failed/ignored** (180 unit, 159 integration),
  Rustup stable **1.99.0**. Native all-target Clippy with `-D warnings`: pass.
- `sh scripts/build-web.sh` and wasm32 Clippy with `-D warnings`: pass, using
  the installed Rustup target and `wasm-bindgen-cli 0.2.129`.
- All **29** CI reproducibility commands: pass, including the new envelope,
  DACK and 68k-family fixtures. Existing Star Collector, Prism Courier and
  Hardware Lab disc/audio/CUE bytes remain unchanged.
- `npm ci --ignore-scripts`, JS syntax check and the complete extended
  `npm run test:browser`: pass; Node **24.12.0**, Chromium **145.0.7632.6**,
  project prefix `/saturn-rust/`. No required tests were skipped. Existing
  Canvas/WebAudio, gameplay/results/restart, BIOS, input/focus and local-file
  privacy assertions pass along with the new guests.
- `git diff --check`: pass. No external CI, deployed Pages or proprietary BIOS
  compatibility result is claimed.

The first web invocation did not find the installed CLI on PATH; rerunning with
its existing path passed without installing a different toolchain. Initial new
fixture tests exposed an uncleared pre-existing timer request and incorrect
expectations for DMA incremented data; the fixture/reference vectors were fixed.
With documented 68k costs, the existing PCM guest accepts its pending timer IRQ
before its first STOP: its exact retirement assertion is now 23 setup + 3 handler
+ 1 STOP = 27, replacing an instruction-count threshold based on the old costs.

**The requested full Issue #17 resolution has not been achieved.** The remaining
review findings still require implementation, including SH-2/DIVU/BSC/cache/bus
contracts and hardware-time arbitration, backup filesystem and HLE service ABI,
SCU/DSP arbitration and edge coverage, complete video/raster modes, calibrated
SCSP/DSP timing, remaining CD/media contracts and SMPC system controls. No
mandatory criterion is waived or closed by these passing checks. Continue in the
same PR and Issue; no replacement tracker, historical reopening or human gate.

## Review fixes: UBC, power controls and CUE indices (2026-10-21)

Related to #17. This revision starts at `f0665c3f4e94438b1d3132ee410f6111a61ab23c`
and preserves PR #18's existing hardware/demos. `git ls-remote origin
refs/heads/main` again returned `dbd981b030a46daa6923072ce240680dd2e58ebf`.
The PR was not pushed or merged by this node.

Implemented and independently exercised:

- `src/onchip/ubc.rs` implements the H §§6.2–6.3 register/comparison model:
  both channels, masked address/data, operand size, CPU/DMAC/external master,
  sequential matching and sticky match flags. `Cpu::execute/step` distinguish
  before/after execution and preserve the existing delay-slot/interrupt deferral
  rules. A successful fetch is required for a before-execution match. UBC
  vector 12 wins the level-15 tie with external IRL15 (H table 5.4).
- `CpuBus` observes actual CPU data accesses, including cache hits, separately
  from physical external cycles. Cache-fill/P1 and SCU DMA/DSP beats can be
  observed by the other/external UBC; cached data cannot invent a physical bus
  cycle. DMAC accesses to CPU-only UBC registers are rejected (H §6.3.7).
- Software standby (H §§14.2/14.4 and table 14.3) freezes local clocks, resets
  FRT/SCI and DMA control, retains CPU/INTC/UBC/BSC and DMA operands, and wakes
  only after the WDT-selected NMI settling interval. SBY cannot be selected
  while the watchdog runs. Native vectors vary advancement chunks and test
  retained registers, stopped capture, resume and NMI at SR.I=15.
- SMPC SYSRES (ST-169 §2.3.8 p.29) has an emulated 100 ms deadline, clears
  volatile devices/memories and cache state, preserves backup/SMEM/RTC and the
  mounted physical media, and restarts the selected ExternalBios or HLE path.
  HLE disc startup shares its IP/first-read initialization with SYSRES; it does
  not run the slave trampoline against cleared work RAM. The original SYSRES
  guest executes as both a synthetic BIOS and an ISO first-read program.
- CUE INDEX02–99 preserve file-sector layout, translate to absolute FAD and
  drive track/index seeks, status and Q subcode (ST-162 §§7.2.4/7.2.5/8.2.2).
  Native golden vectors cover pregaps, shared files, absent indices and invalid
  number/time ordering/bounds. Browser Wasm boots Hardware Lab with INDEX02/99,
  checks unchanged computed hardware results and correct persistent-backup
  increments, and rejects malformed ordering.

All new guest source and expanded ROMs use the existing MIT fixture license.
The generated ROM hashes are reproducible from the adjacent assembly/builders:

| Fixture | Expanded 512 KiB ROM SHA-256 |
| --- | --- |
| `ubc.hex` | `33b17804f1d0b49c825a060a5db1538031270490830347bb073c4173affade73` |
| `system-reset.hex` | `b100144eaac62eae0983601b4fd61b28981a7f265b49b655f5a4c76630c97f07` |
| `standby.hex` | `7c232722489437e59e46a14997ec227b6a2bf69d3df393d4846bbd7efdb5d925` |

Validation: `cargo fmt --check`, locked native build/test and native Clippy,
Wasm Clippy, `sh scripts/build-web.sh`, all 16 CI reproducibility commands,
JavaScript syntax/whitespace checks and the complete Chromium browser suite.
The native suite has 257 passing tests (155 unit tests and 102 integration
checks), with no failures or ignored tests. Browser assertions include the new
UBC, SYSRES (both startup modes), standby/NMI and CUE vectors alongside Star
Collector, Prism Courier, Hardware Lab, actual WebAudio and privacy/input checks.
Native checks use the installed Homebrew Rust 1.94.0; Wasm checks use the
installed Rustup stable 1.99.0 and wasm-bindgen 0.2.129. The initial Wasm build
failed because the Homebrew toolchain lacked the target; selecting the already
installed Rustup binaries/target resolved it. The newer Clippy's constant-chunk
lint is fixed in an existing audio test without changing its sample assertions.
An added browser comparison initially compared a persistent backup counter to
its first-boot value; it now asserts the guest's increment and byte-lane result
explicitly across boots.

These changes do not establish umbrella completion. The existing conformance
inventory retains the mandatory unimplemented/unverified behavior, including
remaining CPU/SH7604 and timing edges, BSC/DACK/DIVU overflow, MC68EC000 prefetch,
complete bus/scheduler/backup services, SCU/video/audio/CD, BIOS/HLE services and
SMPC clock/system controls. UBC's configuration pipeline/compatibility overrun
fetches and standby's initial five-cycle NMI guard remain unmodeled. No acceptance
criterion is waived or replaced with this validation record, and no completion
or Issue-closing claim is made.

## VDP2 rotation/line effects and timed analog protocols

Related to #17. The complete Issue #17 target and all mandatory criteria remain
in force. PR #18 remains Draft with `Closes #17` as its intended final outcome;
this implementation does not resolve the umbrella Issue. The active worktree
started at `cc1a22a1fa90e2afaf26240168a1210b32523a22`, preserving `cc50fdc` and
all useful existing PR work. Rechecking upstream returned
`dbd981b030a46daa6923072ce240680dd2e58ebf`; the existing remote PR head before
publication was `bda7db4a42299b6658e90e9522bc3488ce269336`.

Implemented behavior:

- `Vdp2::normal_coordinates` reads packed enabled X/Y/zoom fields, relative
  scroll values, programmed line intervals and fractional coordinates. Vertical
  cell tables support NBG0/1 interleaving and mosaic precedence.
- `Vdp2::window` reads W0/W1 horizontal line tables from VRAM, including
  inclusive edges, reversed ranges, inverted windows and wrapped table addresses.
- The independent fixed-point RBG0 renderer executes the ST-058-R2 §6.1 p.147
  matrix/perspective equations using signed parameter widths from figs.6.2/6.3.
  It handles A/B tables, viewpoint/center/Z and translation, scale, tile/bitmap
  reads, sixteen-plane maps, screen-over controls, one/two-word coefficients,
  all four coefficient modes, per-dot/line addresses, coefficient transparency,
  coefficient/window A/B selection and priority/blending/offset composition.
- `Analog` implements ID16 six-byte reports (digital/X/Y/AR/AL), alongside the
  preserved ID15 joystick. STN-43's L/R diagram sets trigger-on at 146 and
  trigger-off at 85, retaining state in between. `Handshake` implements direct
  TH/TR/TL acquisition, a deterministic 54-clock response deadline, latched
  transaction data, end/abort and independent ports. No wall-clock wait is used.
- Standard-mapped browser Gamepads supply ID16 circular X/Y and independent
  trigger levels; other mappings preserve ID15 X/Y/Z. Pause/focus loss releases
  buttons, centers axes and clears both triggers while retaining device type.

Independent evidence:

- Fourteen [advanced video vectors](../tests/video_effects.rs) assert packed field
  subsets on both layers, relative/fractional scroll, table intervals, vertical
  cells/mosaic, line-window wrap/edges, identity/quarter-turn/negative transforms,
  center/viewpoint/Z terms, both coefficient sizes/modes, A/B switching, sixteen
  planes, screen-over behavior, priority/blending/offset, ignored bits and bounded
  extreme values. Golden colors and coordinates are source-authored constants.
- [SMPC unit vectors](../src/smpc/tests.rs) check 53/54-clock acknowledgment,
  mid-transaction input changes, abort, port independence, ID16 layout and both
  trigger thresholds/hysteresis. [Native guest integration](../tests/smpc.rs)
  exercises ID16 INTBACK and both direct ports through actual SH-2 instructions.
- Original MIT [effects.s](../tests/fixtures/effects.s) and
  [controller.s](../tests/fixtures/controller.s), their builders and expanded ROM
  hashes are recorded in the [matrix](hardware-conformance.md#original-advanced-video-and-controller-fixtures).
  CI checks both. The same ROMs execute in the browser Wasm suite. The video guest
  creates every texture and table through MMIO; changing My from 3 to 2 changes
  independently expected Wasm and Canvas pixels. The controller guest drives
  TH/TR, polls TL and stores received pin bytes; changed physical axis input
  changes those bytes. JavaScript never writes a result mailbox.
- Browser tests retain the ID15 synthetic Gamepad path and add standard-mapped
  ID16 trigger/axis reports and focus neutralization, plus direct ID15/16 guest
  transfers. All prior UI/input/privacy/Play/Debug, dual CPU, original ROM,
  WebAudio and both game completion/restart checks remain active.

Validation performed for this revision:

- Rustup stable **rustc 1.99.0**: `cargo fmt --check`, `cargo build --locked`
  and `cargo test --locked` pass: **214 native tests, zero failures and zero
  ignored tests**. Homebrew Rust 1.94 also passed the implementation suites.
- Native all-target and wasm32 Clippy with `-D warnings` pass. The first stable
  Clippy run flagged a new test's constant-size `chunks_exact`; it now uses
  `as_chunks`, and Clippy plus all fourteen affected tests pass again.
- `sh scripts/build-web.sh` passes with the existing matching
  wasm-bindgen-cli 0.2.129. Star Collector and Prism Courier ISO hashes are unchanged.
- Both demonstration-disc builders and all nine diagnostic fixture builders pass
  their `--check` reproducibility verification, including both new fixtures.
- The complete extended `npm run test:browser` passes in **Chromium 145.0.7632.6**
  at `/saturn-rust/`, with no required test skipped. It uses the production static
  assets, UI/Wasm, Canvas, Gamepad and AudioWorklet paths; local files remain local.
- JavaScript syntax, changed documentation relative-file links and Git whitespace
  checks pass. No remote CI or production publication result is claimed.

At that checkpoint the renderer used frame snapshots, and RBG1, coefficient
line-color/CRAM sources and sprite windows were pending; subsequent sections
record their implementation and executable evidence. Raster-time table writes,
rotation reloads, other display modes and cycle arbitration remain required.
Enabled unsupported parameter reloads still fail explicitly. The analog response deadline is a deterministic
functional 2-microsecond policy; complete pin timing, direct digital settling,
mode-switch calibration and external latches are not established.

No umbrella acceptance checkbox is completed. Remaining mandatory work includes
CPU/SH7604 and 68k conformance edges, complete mapping/cache/reset/backup services
and bus scheduling, SCU/DMA/DSP edge and arbitration coverage, remaining video,
SCSP synthesis and executing effects DSP, runtime CD/ISO/BIN/CUE/CD-DA, separate
BIOS/HLE services and synthetic ABI acceptance, remaining SMPC system/clock
commands, the complete original hardware exerciser disc and full linked evidence.
No human, proprietary-firmware, live-site, external-workflow or post-merge approval
gate is introduced. Nothing is merged or administratively closed.

## Review revisions: CPU bus effects, SH7604 controls and normal tile layers

Related to #17. These changes address concrete parts of review findings 1, 2, 7
and 13. They do not complete those findings or the umbrella acceptance criteria.
The repository's other mandatory implementation requirements remain in force;
no Issue/PR was created, closed, pushed or merged by this revision.

- SH7604 BCR1 processor identification now follows each CPU's mode pin and
  survives CPU resets. The BSC interval timer implements all seven clock
  divisors, RTCNT/RTCOR, keyed RTCSR writes, observed-one/write-zero CMF, CMI
  vector/priority arbitration and manual-reset retention. Changed memory-interface
  configurations remain explicit errors while bus timing is incomplete.
- SBYCR FRT/SCI module stops reset and gate those devices while retaining INTC
  vectors. DIVU and DMAC clock gates preserve their state; a pending DMA copy
  executes only after the gate is released. Oscillator standby/HIZ/MULT stop
  remain unsupported. Native register/reset/gate vectors and an original guest
  exercise effects rather than inert register readback.
- MC68EC000 long predecrement and stack writes issue low-word then high-word
  cycles. Fault frames identify the actual failed word address; completed writes
  and MOVEM loads/address updates survive a later failure. Four independently
  authored bus-spy tests cover successful/partial transfers and exception writes.
  Prefetch-related fault PCs and instruction timing remain incomplete.
- VDP2 now samples all four normal tile layers, one/two-word names, supplement
  modes, 8×8/16×16 characters, supported palette/direct color formats, flips,
  maps/planes/pages/wrap, integer NBG2/3 scrolling and normal-layer windows,
  mosaic, priorities and blending. Seven native tests cover independently
  expected pixels, all normal color formats and plane sizes, supplementary names,
  transparency, composition and guest ROM mutation. Rotation/line effects,
  cycle-pattern arbitration and other previously documented modes remain required.
- Two original MIT fixtures, `bsc.s` and generated `tiles.s`, execute through the
  ordinary external-ROM path in native and browser Wasm tests. The first uses
  BCR1 to select master/slave mailbox and stack, handles independent CMI IRQ/RTE,
  and repeats identical traces after reset. The second computes all VRAM/register
  writes from SH-2 instructions; changing its ROM palette literal changes
  native/Wasm pixels and production Canvas presentation. Their source builders,
  hashes and primary references are linked in the
  [conformance matrix](hardware-conformance.md#additional-original-review-fixtures).
  CI checks both builders. These diagnostic ROMs do not replace the still-required
  hardware exerciser disc.

Validation on Rust stable 1.99.0:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: passed,
  **197 native tests**, including Star Collector and Prism Courier regressions.
- Native all-target and Wasm Clippy with `-D warnings`: passed.
- `sh scripts/build-web.sh`: passed with the documented rustup stable PATH and
  matching wasm-bindgen 0.2.129. The initial shell selected Homebrew Rust without
  its Wasm target; selecting the already installed rustup toolchain resolved it.
- Both existing discs, all existing fixture builders, and the two new builders:
  reproducibility checks passed. The packaged disc hashes are unchanged.
- Extended `npm run test:browser`: passed in Chromium 145.0.7632.6, including the
  new BSC and tile/palette/Canvas assertions plus existing input/privacy/WebAudio,
  guest programs, Play/Debug, subpath packaging and gameplay checks. An initial
  new test incorrectly expected Debug Run to present a frame; it now invokes the
  app's production `advanceFrame` path before checking Canvas.

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
- Continued sound implementation with no-loop/forward-loop signed PCM8/16
  first-order interpolation, before stack writes/mixing. Two independent native
  vectors cover fractional pitch and loop/end neighbors. The original sound
  guest also programs half-sample pitch in native and browser Wasm, producing
  stereo values `[256,4096],[0,0],[-256,-4096],[-384,-6144],[-512,-8192],
  [0,0],[512,8192],[384,6144]`. Its RAM waveform is unchanged. Reverse/alternating
  fractional interpolation and the executing effects DSP remain required.

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
  **180 native tests, zero failures and zero ignored tests**.
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


## Runtime CD, mixed-disc guest and bus/audio review fixes (2026-10-21)

Related to #17. This revision preserves PR #18's existing CPU/device/demo work
and adds executable hardware behavior for the CD/media/audio/bus/demo findings.
It does **not** resolve every finding or satisfy the umbrella acceptance checklist.
Issue #17 must remain open; no continuation issue, historical issue reopening,
human confirmation or external workflow investigation is needed.

Implementation and objective evidence:

- `src/media.rs` retains cooked ISO or caller-resolved CUE/BINARY media, including
  cooked/raw Mode1, raw Mode2, shared/separate files, audio, INDEX00/01 and gaps.
  It performs bounded parsing with missing/ambiguous references and truncation
  errors. `tests/media.rs` includes original raw/cooked, TOC/gap and deterministic
  malformed-input mutation vectors. No filesystem or network resolver is called.
- `src/cd.rs` provides separate command/response latches, timed completion,
  HIRQ/masks/clear-zero, read-protected responses, FAD/track/index/TOC/session/Q,
  seek/read/play/repeat, a 200-sector pool/24 partitions, filtering/connection,
  transfers, copy/move, size/search, and simple ISO directory/file commands.
  The bus routes the CD FIFO to fixed-source SCU DMA; CD IRQ reaches source 16
  through the A-bus acknowledgement gate. Nine independent register/audio/media
  tests in `tests/cd.rs` cover the implemented contracts, including CDON/OFF.
- CD-DA is decoded from the actual selected track and mixed through SCSP EXTS0/1
  routing alongside the independently executing 68k PCM voice. Device/sample
  boundaries are included in the event scheduler. Exact stereo golden buffers
  test level/pan/muting. Changing only the CD audio track changes the mixed Wasm
  buffer; an already audible 68k voice cannot hide a disconnected CD route.
- Original Hardware Lab source and mixed media execute slave SH-2 computation,
  SCU DSP product/sum and IRQ, runtime CD-sector reads through actual SCU DMA and
  IRQ, guest checksum, 68k synthesis/timer IRQ, CD-DA, advanced VDP2 composition,
  backup words, SMPC controls and controller-driven restart. Its native and
  browser assertions reject a changed runtime sector via the computed checksum.
  The browser checks actual Canvas and AudioWorklet output and missing local
  CUE-file rejection. The guest writes all result mailboxes; JS only supplies
  local media/controller levels, reads diagnostics and presents pixels/audio.
- HLE disc boot installs an original MIT-licensed SH-2 reset trampoline so the
  slave handoff runs real reset instructions. External BIOS mode never installs
  this trampoline or redirects to HLE. General BIOS service ABI is still absent.
- BIOS/SMPC, low/high RAM, sound RAM, VDP1 framebuffer and VDP2 VRAM mirrors now
  decode to physical storage. Backup and SMPC wider cycles strobe odd lanes;
  disconnected even lanes return FF and ignore writes. Native boundary/lane
  tests and the guest word-backup path preserve atomic packed import/export.
- Reverse/alternating fractional PCM no longer falls back to sample points.
  Signed PCM8/16 half-pitch vectors independently test reverse wrap, alternating
  endpoint holds and traversal interpolation. The browser changes original 68k
  pitch/key-on immediate words and asserts an exact reverse stereo vector.

Reproducible original artifacts (MIT licenses adjacent to guest/trampoline source
and copied beside distributed media):

| Artifact | SHA-256 |
| --- | --- |
| Hardware Lab ISO | `e19437a750e10b24c5c6be7617f2879928bd68ba4cee139e15980a09318e6ed0` |
| Hardware Lab BINARY audio | `1c86e2d495e7d745dbfe1e4369afa104fb75eae9f042abb61e8fe56d8c67fd1d` |
| Hardware Lab CUE | `398e70e871f8b3ed6db6033bb9c34f08c9cd62dd59f87195cdb81e4ec3433f7e` |
| Independent HLE reset trampoline | `6df97e2390adac398df2cb58c05434034d998223ba41c31a36e7d32022d5a63d` |

Star Collector and Prism Courier images remain byte-identical to their existing
source builds. CI checks all thirteen reproducibility builders, including the
new Hardware Lab and HLE trampoline builders; the web asset allowlist includes
only their intended public files/licenses.

Validation performed:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  233 native tests, no ignored tests.
- `cargo clippy --locked --all-targets -- -D warnings`: pass.
- `sh scripts/build-web.sh` and Wasm Clippy with `-D warnings`: pass, using the
  installed Rustup stable 1.99.0 toolchain and wasm-bindgen-cli 0.2.129.
  Native checks use Homebrew Rust 1.94.0. The initial web build selected a PATH
  without the existing matching CLI; selecting the documented installed tools
  resolved it without changing dependencies or skipping checks.
- All thirteen workflow reproducibility commands: pass.
- `npm ci --ignore-scripts`, `npx playwright install chromium`, and the complete
  extended `npm run test:browser`: pass in Chromium 145.0.7632.6 under
  `/saturn-rust/`. Existing Star Collector/Prism Courier, CPU fixtures, UI/input,
  backup and privacy checks remain enabled. New CUE guest and exact reverse-loop
  audio checks run in the same suite. Early test failures exposed response-latch,
  guest DSP restart acknowledgement and test audio-drain/readiness mistakes;
  these were corrected before the successful full run.
- `node --check web/app.js`, `node --check scripts/browser-smoke.mjs` and
  `git diff --check`: pass.

Remaining mandatory implementation is still the complete scope in the
[conformance inventory](hardware-conformance.md): SH7604/UBC/standby/BSC/DIVU/DACK
and CPU arbitration/locking, full MC68EC000 prefetch/fault sequencing, remaining
map/open-bus/reset/backup filesystem and HLE service contracts, CPU/bus hardware
time contention, complete SCU modes/DSP evidence, VDP1/VDP2 modes/raster effects,
SCSP envelope/FM/LFO/noise and executing effects DSP, remaining CD drive/command/
error/initialization/scan/subcode/timing behavior, SMPC system/clock/latch controls,
the effects-DSP exerciser scene, full synthetic boot/service fixtures and complete
umbrella coverage. New passing tests establish implemented behavior only. No
proprietary BIOS, human listening/approval, physical console, live Pages or
post-merge check is required or claimed. Genuine BIOS/retail compatibility is
not independently confirmed. No push, merge or Issue closure was performed.


## Executing SCSP effects DSP and Hardware Lab scene (2026-10-21)

Related to #17. This revision preserves the PR and upstream runtime-CD work and
adds an original effects interpreter plus the missing guest scene. The full
Issue #17 target is **not achieved**; all remaining requirements in the
[conformance inventory](hardware-conformance.md) remain mandatory. PR #18 must
remain Draft and Issue #17 must remain open. No merge, replacement tracker or
historical Issue reopening is authorized or performed.

Implementation and independently expected evidence:

- `src/sound/dsp.rs` executes 128 parallel instruction steps per 44.1-kHz sample:
  signed multiplication/26-bit accumulation, previous-accumulator shift with
  saturation/wrap, TEMP circular addressing, MEMS/MIXS/EXTS, Y/fraction/address
  latches, effect writes and sound-RAM ring/table transfers with linear/normalized
  floating conversion. All programmer-visible DSP RAM, coefficient, address and
  program ports have actual effects; unsupported/reserved ports/instructions
  return bounded diagnostics. Slot IMXL/ISEL and output EFSDL/EFPAN now connect
  the processor to the existing stereo mixer. RBL/RBP are masked write-only
  controls. Byte lanes preserve adjacent register data.
- Nine independent numeric tests in `src/sound/dsp/tests.rs` check signed
  coefficients, accumulation, previous-value/read-before-write behavior, sample
  delays, all RBL sizes, ring wrapping/table addressing, read latch latency,
  simultaneous read/write, linear/float bit patterns, clipping/wrapping, mixed
  input overflow, exact stereo/muting and invalid instructions. The existing
  68k RESET test also checks DSP program/data/coefficient reset and retained RAM.
- The original MIT `tests/fixtures/build_sound.py` optionally generates a new
  68k routine for Hardware Lab only. Guest instructions write two coefficients,
  MEMS0 and eight actual DSP instructions. MIXS0, MEMS0 and both CD external
  channels independently traverse multiply/output pairs. Constant diagnostic
  EFREG1 is not an audio source. Both direct CD sends are muted, so CD data
  must pass through the DSP to affect the mixed output.
- Hardware Lab's SH-2 waits for sound CPU initialization, reads EFREG1 and stores
  its result at `06010028`. `tests/hardware_lab.rs` and browser Wasm assert 0900.
  Changing only the 68k coefficient immediate in the ISO to .25/zero changes
  this guest-computed result to 0480/0000. With a silent CD track, native and
  Wasm compare the exact four-frame bipolar waveform. At .5 gain it is
  `[2304,4096],[-2304,-4096],[-4608,-8192],[4608,8192]`; changing gain alters
  the wet left channel while retaining the direct right channel. Actual
  AudioWorklet rendering and changing the CD audio track remain asserted.
  No host-written result, guest identity detection or canned output is added.

Primary behavior references are SCSP User's Manual ST-077 §4.1 figs.4.5–4.7,
§4.3 table4.41, §5 fig.5.1 and dAsms ST-228 Reference1/2. These describe the
register/data-path and signal-flow model. Low-level encoding/converter/pipeline
edge corroboration, delayed RAM bus-cycle timing, DRAM/CPU/DMA contention and
complete DSP guest vectors remain required for full conformance; the manuals
omit some details. No third-party processor/effect/assembler implementation,
SDK routine or firmware was imported. Manuals were consulted outside the repo.

Reproducible original artifacts:

| Artifact | SHA-256 |
| --- | --- |
| Hardware Lab ISO with 68k effects routine | `8bc56657bf0376fece7dece1a094f9e159f0d365af94adb6f87cb2fa5dc7995c` |
| Hardware Lab BINARY audio (unchanged) | `1c86e2d495e7d745dbfe1e4369afa104fb75eae9f042abb61e8fe56d8c67fd1d` |
| Hardware Lab CUE (unchanged) | `398e70e871f8b3ed6db6033bb9c34f08c9cd62dd59f87195cdb81e4ec3433f7e` |

Star Collector, Prism Courier and existing synthetic ROMs remain byte-identical.
All original guest code/media remains covered by adjacent MIT licenses. The
new routine is included only when the Hardware Lab builder requests it.

Checks performed on this revision:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  **243 native tests, zero failures and zero ignored**.
- Native all-target and wasm32 Clippy with `-D warnings`: pass.
- `sh scripts/build-web.sh`: pass using the installed Rustup stable toolchain
  and matching wasm-bindgen-cli 0.2.129. Native checks use Homebrew Rust.
- All thirteen CI guest-media/fixture reproducibility builders: pass.
- `npm ci --ignore-scripts`, `npx playwright install chromium`, and the complete
  extended `npm run test:browser`: pass in Chromium 145.0.7632.6 under
  `/saturn-rust/`. No existing checks were skipped. The new DSP mutation and
  exact sample checks run alongside Canvas, real AudioWorklet, both games,
  hardware guests, local media, controller/focus and privacy assertions.
- JavaScript syntax, relative-file documentation links and Git whitespace: pass.

Initial development vectors caught incorrect negative float reconstruction and
fixture operand/waveform expectations; these were corrected before the complete
passing runs. Initial Clippy findings were fixed without suppressing warnings.
Passing local validation does not claim remote CI or production deployment.
Genuine Sega BIOS/retail compatibility is not independently confirmed and is
optional. No personal firmware, listening/human approval, live Pages inspection,
external-workflow investigation or post-merge approval condition is needed.

This is saved implementation progress, **not successful full resolution of the
Run objective**. Remaining CPU/SH7604, MC68EC000, map/reset/backup/HLE services,
hardware-time CPU/bus contention, SCU mode/pipeline corpus, VDP1/VDP2 modes and
raster effects, SCSP envelope/FM/LFO/noise and full DSP timing, remaining CD
commands/errors, SMPC clock/system/latch controls, complete synthetic service
fixtures and comprehensive umbrella evidence remain mandatory. Continue on
this same PR and Issue; do not mark it ready until all are implemented/validated.


## SCSP slot synthesis and RBG1 implementation (2026-10-21)

Continued the full Issue #17 target on PR #18, preserving the incoming UBC,
standby/NMI, SYSRES and CUE-index work. Upstream main is still
`dbd981b030a46daa6923072ce240680dd2e58ebf`, checked with `git ls-remote` and
`git fetch` before publication. The existing PR branch is an ancestor of this
worktree, permitting a normal fast-forward update with no force push.

Implemented real SCSP slot FM from recent/past sound-stack generations, the
five-slot producer/consumer pipeline, signed averaging/MDL and waveform guard
reads. Added deterministic per-slot ALFO/PLFO waveforms, published frequencies
and sensitivities, LFORE behavior, internal noise/zero sources, binary TL gain,
zero-rate silent attack and zero-rate EGHOLD. Timed envelope rates still reject
explicitly; the manual does not specify a noise polynomial, so the repeatable
LFSR policy is documented without claiming the physical chip's exact sequence.

Implemented VDP2 RBG1 cell sampling using parameter B, B0 characters/B1 names,
normal-layer exclusion, NBG0 format/window/priority/blending/offset controls and
coefficient-MSB semantics. Independent native vectors cover five color formats,
both name widths, separate matrices, screen-over, banks and composition. Invalid
bank fetches yield transparency in the functional model; physical bank/cycle
behavior and scanline reloads remain pending.

New original MIT fixtures execute through actual SH-2/68k instructions/MMIO in
native and browser Wasm. The synthesis fixture changes TL/MDL/ALFO/PLFO/source
operands and asserts independently expected stereo or mathematical tolerances,
repeatable reset, noise routing and actual AudioWorklet delivery. The RBG1 guest
writes both matrices and original cell/bitmap data, then tests mutate only its B
matrix literal and assert emulated and production Canvas pixels. See the
[conformance evidence](hardware-conformance.md#executing-scsp-slot-modulation-and-independent-guest-evidence)
and [RBG1 evidence](hardware-conformance.md#original-dual-rotation-guest-and-rbg1-evidence)
for sources, manual sections and reproducible hashes.

Final local checks:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  **271 native tests, zero failures, zero ignored**.
- `cargo clippy --locked --all-targets -- -D warnings`: pass.
- `sh scripts/build-web.sh` and Wasm Clippy with `-D warnings`: pass.
  Native Homebrew Rust/Cargo 1.94.0; Wasm rustup stable 1.99.0 with the installed
  wasm32 target and matching wasm-bindgen-cli 0.2.129. Selecting the Homebrew
  compiler initially failed because that compiler lacks the target; the documented
  rustup PATH resolved it, without changing dependencies or requirements.
- All **18** CI reproducibility builders: pass, including the two new fixtures.
  Existing sample discs, audio and ROMs remain byte-identical.
- `npm ci --ignore-scripts`, `npx playwright install chromium`, JavaScript syntax,
  and the complete extended `npm run test:browser`: pass in Chromium
  145.0.7632.6 under `/saturn-rust/`. No required browser checks are skipped.
- Relative documentation file targets and Git whitespace: pass. Initial new
  vector failures (arithmetic expectations, two-word map scaling and a fixture
  duration) were corrected before the final passing runs.

**The full-resolution Run objective is still unmet. PR #18 remains Draft.**
No mandatory umbrella criterion has been waived or declared complete. Remaining
work includes full SH-2/SH7604 and MC68EC000 edges/timing, bus/cache/reset and
hardware-time arbitration, backup filesystem and HLE system/backup/CD ABI,
SCU/DMA/DSP conformance, remaining VDP1/VDP2 effects/display/raster contracts,
timed SCSP envelopes and complete modulation/effects conformance, remaining CD
commands/timing/errors, SMPC system/clock/latch/controller contracts, and complete
native/Wasm/guest evidence for the full inventory and both demonstrations.
The [concrete unresolved gaps](hardware-conformance.md#concrete-unresolved-gaps-and-closure-gates)
remain the implementation backlog on this same PR and Issue. Closes #17 remains
its intended final outcome, not a statement that this validation completes it.
No proprietary firmware, human approval/testing/listening, live Pages inspection,
external-workflow investigation or post-merge approval gate is required. Nothing
is merged; local validation does not claim remote CI or production deployment.

## Review fixes: CI, CD drive, sprite composition and sound I/O

Related to #17. Upstream `main` was rechecked as
`dbd981b030a46daa6923072ce240680dd2e58ebf` on 2026-10-21.

- Replaced the six constant-size slice chunk calls rejected by Rust 1.99
  Clippy. Required lints/checks remain enabled.
- Implemented separate CD read/report deadlines, 60 Hz pause reporting,
  programmable pause-to-standby, directional scan with range termination and
  CD-DA attenuation/muting, tray/media retention and virtual close, empty
  subcode R–W FIFO WAIT and full-sector-pool pause/resume without dropping the
  blocked sector. Scan speed is unspecified in ST-162; the deterministic model
  advances ten sectors per 75 Hz drive tick. It does not claim measured pickup
  speed. CUE PRE/DCP/4CH CONTROL flags now reach TOC/status/Q metadata; malformed,
  duplicate and incompatible flags are rejected. This metadata support does not
  implement de-emphasis filtering or four-channel output.
- Implemented all sixteen VDP2 sprite field layouts, including shared selector/
  palette bits; normal 8-bit VDP1 scanout; sprite windows combined with normal/
  line windows; normal/MSB/transparent shadow controls and ordering after color
  calculation/offset; palette-MSB color-calculation conditions in RGB555/RGB888.
  Rotated scanout and other required display modes remain outstanding.
- Implemented automatic 31.25 kbit/s 8-N-1 MIDI output deadlines in the sound
  event domain. Host inspection retrieves completed characters without driving
  FIFO progress. Tests assert timing, independent sound/main interrupt clearing,
  reset cancellation and bounded output. Native and Wasm guests program SCSP
  DMA, poll MIDI completion and acknowledge three IRQ levels. Their real 68k
  instructions additionally compute BCD, MOVEP/MOVEM, multiply/divide/extend
  arithmetic, a partial-transfer bus-fault frame and TRAP/RTE results. Guest
  operand mutations change actual DMA output; no host writes set result state.

The three added synthetic ROMs share [the original MIT fixture license](../tests/fixtures/LICENSE).
They contain no proprietary firmware, SDK objects or third-party device cores.
Their builders are required by CI, alongside the preserved licensed discs.

| Original guest | Source/build | Expanded ROM SHA-256 |
| --- | --- | --- |
| CD drive | [build_cd_drive.py](../tests/fixtures/build_cd_drive.py) | `9f9860732832f22ff1f25038b53a1b3ae108680b83295ba42b0257428c543380` |
| Sprite effects | [sprite-effects.s](../tests/fixtures/sprite-effects.s), [builder](../tests/fixtures/build_sprite_effects.py) | `c640bb5a10cb281c0b0e6a86e067c085f5f8434505ccd9c4b2690cea68c7b1c1` |
| Sound I/O and CPU families | [build_sound_io.py](../tests/fixtures/build_sound_io.py), [original 68k encodings](../tests/fixtures/build_sound.py) | `740db49f8bd44c7ab574bd267a419ddc7d74ef7bb2527aa44a819d898e83600e` |

Actual local checks at this revision, using Rust 1.99.0 and wasm-bindgen 0.2.129:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: **287
  passed, zero failed/ignored** (164 unit tests and 123 integration tests).
- Native all-target and Wasm Clippy with `-D warnings`: passed.
- All **21** CI reproducibility commands: passed; existing Star Collector,
  Prism Courier and Hardware Lab media hashes remain unchanged.
- `sh scripts/build-web.sh`, `node --check scripts/browser-smoke.mjs`, and the
  complete `npm run test:browser`: passed in Chromium **145.0.7632.6**.
  The browser executes all three new guests, repeated resets, guest-operand
  mutations and real Canvas comparisons, alongside the existing games,
  Hardware Lab, AudioWorklet, input/focus, asset-prefix and privacy assertions.

These checks restore local mandatory CI parity and validate the implemented
contracts above. They do **not** satisfy the entire Issue #17 acceptance checklist.
The conformance inventory still identifies required SH7604/DIVU/DACK/BSC and
CPU/bus timing, MC68EC000 prefetch, complete bus/reset/backup services, SCU,
remaining VDP1/VDP2 modes/raster behavior, timed SCSP envelopes/DSP boundaries,
remaining CD/ECC/services and SMPC controls. The full source Issue remains the
completion target; no criterion has been waived, replaced with documentation or
converted to a human approval requirement. No push, merge or remote CI run was
performed by this fix node.

## VDP2 line, coefficient, special and extended color implementation

Related to #17; the full Issue remains the target of Draft PR #18. Upstream main
was rechecked as `dbd981b030a46daa6923072ce240680dd2e58ebf` on 2026-10-21.
This work preserves every prior CPU/device implementation, guest fixture and
demonstration disc. It adds hardware behavior rather than waiving acceptance.

Implemented in [VDP2](../src/vdp2.rs), [rotation sampling](../src/vdp2/rotation.rs)
and [sprite composition metadata](../src/vdp2/sprite.rs):

- Single/per-line LNCL palette tables, top-screen insertion enables, independent
  sprite enable, line/back ratios and top/second-image ratio selection.
- Rotation coefficients in upper CRAM, both sizes, signed scaling, ignored
  address offset, local wrap and coefficient line-color data with documented
  A/B/window/mode2/RBG1 selection.
- Character/dot special priority and special color calculation, bitmap/name/
  supplementary flags, code A/B masks, palette-MSB conditions and RGB restrictions.
- Three/four-image extended color calculation with raw lower-screen enables,
  CRAM/format restrictions and deterministic channel arithmetic. The 1997 Sega
  primary HTML table confirms the four-screen 2:1:1 ratio; the older PDF prints
  2:1:0. See the [traceable evidence](hardware-conformance.md#line-color-cram-coefficients-and-specialextended-composition).

The two original MIT-licensed SH-2 sources and ROMs reproduce using
`python3 tests/fixtures/build_line_color.py --check` and
`python3 tests/fixtures/build_special_video.py --check`. Expanded 512 KiB hashes:

| Fixture | SHA-256 |
| --- | --- |
| LINE | `e611e93e15b673ba915291bab6e9886af2f8700ccca7b19187fcbdaa0d4b9c1f` |
| SPECIAL | `118254a4eec38b8a2f4c0d10fcf47ebe9a251a61deeae361fc1395ff0d185923` |

[Native line/composition vectors](../tests/video_line_color.rs),
[special-function vectors](../tests/video_special.rs) and additional
[RBG0/RBG1 vectors](../tests/video_effects.rs) assert independent pixel values.
The guests write every image, matrix, coefficient, palette and control through
SH-2 instructions. Native and browser tests alter only ROM literal operands to
change coefficient scale, coefficient line-color bits, extended-calculation
enable, special code selection and bitmap flags. The real Wasm and production
Canvas must match the independently expected changed pixels; resets repeat.
No host result writes, game detection, firmware or external core is used.

Actual combined checks:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: **304 passed,
  zero failed/ignored** (164 unit, 140 integration), native Rust/Cargo 1.94.0.
- Native all-target and Wasm Clippy with `-D warnings`: pass. Wasm and additional
  native all-target Clippy use rustup stable Rust 1.99.0.
- All **23** CI reproducibility builders pass. Existing Star Collector, Prism
  Courier, Hardware Lab and prior diagnostic hashes remain unchanged.
- `sh scripts/build-web.sh` passes with Rust 1.99.0 and wasm-bindgen 0.2.129.
- `npm ci --ignore-scripts`, `npx playwright install chromium`, JS syntax and the
  complete extended `npm run test:browser` pass in Chromium **145.0.7632.6** under
  `/saturn-rust/`. No required test is skipped; both new guests/mutations, Canvas,
  all existing games and hardware guests, WebAudio, input/focus and privacy pass.

Initial test failures exposed obsolete unsupported-mode expectations and fixture
mutation assumptions; those tests were corrected before the final passing runs.
The renderer still samples frame snapshots. Gradation, display modes/interlace,
raster reloads and VRAM arbitration remain mandatory video work. SCSP timed
envelopes and DIVU overflow intermediates were investigated against primary
manuals but are **not implemented by these changes**. All other unresolved
CPU/bus/reset/backup/HLE/SCU/audio/CD/SMPC and comprehensive coverage requirements
remain in the conformance inventory. No umbrella criterion is marked complete.

PR #18 remains Draft with Closes #17 as the intended final outcome. The full
resolution objective is still unmet. No new tracker, reopened historical Issue,
human/firmware/live-site/external-workflow gate or waiver is introduced. This
node does not merge; local passing checks do not claim remote CI or deployment.


## 2026-10-21 runtime media review fixes

Related to #17. The reported 17-sector runtime-directory overflow now returns a
CD rejection response. Root and child records, volume/endian/identifier bounds
and file-range arithmetic are validated before directory state is published.
Failed navigation is atomic and mounting new media discards the old file table.
The original directory-command fixture runs against valid and malformed ISO and
clean/correctable/uncorrectable raw media in native and browser Wasm tests.

Raw Mode1/XA Form1 now executes bounded P/Q correction with EDC validation;
Form2 checks its EDC. Independent parity-matrix vectors, every protected Mode1
byte position, selected all-byte error deltas, XA subheader/Form-bit cases and
uncorrectable runtime error/pool/recovery behavior pass. The browser's original
raw fixture is generated with a separate parity-matrix encoder.

CD-DA PRE now selects a 50/15 µs digital de-emphasis filter before SCSP routing.
The original SH-2 CD-DA guest writes CD/SCSP registers and produces stereo PCM
matching independent reference values across sector boundaries. Browser tests
compare 1178 frames within 0.51 sample unit and deliver those emulated samples to
the actual AudioWorklet. Native tests also verify DC/Nyquist gain, unchanged-start
pause/resume history, and explicit-seek restart. All previous games, synthetic
BIOS fixtures and guest checks remain in the passing suites.

Checks actually completed on this revision:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  **315 tests passed, zero failed/ignored** (167 unit, 148 integration), Rust/Cargo
  **1.99.0**. Native all-target Clippy with `-D warnings` passes.
- `sh scripts/build-web.sh` and Wasm Clippy with `-D warnings`: pass, using
  rustup stable and `wasm-bindgen 0.2.129`.
- All **25** CI reproducibility checks pass, including both new MIT-licensed
  fixtures. Existing demonstration media hashes remain unchanged.
- `npm ci --ignore-scripts`, JavaScript syntax checks and the complete
  `npm run test:browser`: pass, Node **24.12.0**, Chromium **145.0.7632.6**,
  project subpath `/saturn-rust/`. No required browser tests were skipped.
- Changed documentation's local links and `git diff --check`: pass.

These changes resolve the concrete extent panic and implement additional media
and audio behavior. They **do not complete Issue #17**. Configurable CD decode
repetitions/retries/error-output policy and the other mandatory CPU, scheduler,
bus/backup, SCU, video, SCSP, BIOS/HLE, SMPC and comprehensive integration
requirements remain open in the conformance matrix. No Issue, PR state or
acceptance criterion was closed or waived; no push, merge, proprietary firmware,
human gate, model switch or allowance reset was used. PR #18 still requires
implementation and review against the full umbrella scope.


## 2026-10-21 CD decode configuration and guest-visible retries

Upstream main was rechecked as `dbd981b030a46daa6923072ce240680dd2e58ebf`
using `git ls-remote`. PR #18 remains the full Issue #17 implementation target.
This revision preserves the preceding extent/integrity/PRE fixes and all existing
hardware/demo work and implements runtime CD configuration instead of ignoring it.

The CD block now executes guest-selected one-to-six ECC passes, disabled ECC,
finite/infinite retries, error-sector output, XA Form2 retry and optional decode
subheader confirmation. RETRY consumes drive deadlines and holds the FAD without
publishing sectors; guest commands can cancel it. Initialization validates before
mutation, honors independent no-change fields, and software reset aborts an
unfinished transfer, restores host state and invalidates cached directories.
The [conformance entry](hardware-conformance.md#guest-configured-cd-correction-retries-and-initialization)
links primary specifications, implementation and independent evidence and states
the remaining timing/media deviations.

The new original MIT SH-2 diagnostic runs in native and real browser Wasm,
observes RETRY/ERROR, reads erroneous and corrected bytes from the same medium,
and recovers from infinite retry through an ordinary play command. It repeats
reset traces and media mutations, including a parity graph needing two passes.
No host writes guest results or device registers. The source, generated ROM,
license and reproducible hash are linked in the conformance entry and CI verifies
the builder. Existing demonstration media hashes remain unchanged.

Checks actually completed for the combined worktree:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  **323 native tests passed, zero failed/ignored** (169 unit, 154 integration),
  Homebrew Rust/Cargo **1.94.0**. Native all-target Clippy with `-D warnings`: pass.
- `sh scripts/build-web.sh`, wasm32 Clippy with `-D warnings`: pass using installed
  Rustup stable **1.99.0** and task-local `wasm-bindgen-cli 0.2.129`.
- All **26** CI reproducibility checks: pass, including `build_cd_decode.py`.
- `npm ci --ignore-scripts`, Chromium installation, JS syntax and the complete
  extended `npm run test:browser`: pass, Node **24.12.0**, Chromium
  **145.0.7632.6**, project subpath `/saturn-rust/`. No required tests skipped.
  Existing gameplay, synthetic BIOS, device guests, Canvas, WebAudio, input/focus,
  local-file privacy and distribution assertions continue to pass.

Initial new tests had an out-of-range guest conditional branch and incorrect
filter/scope expectations; these were corrected before final passing checks.
The initial web command lacked the matching CLI, which was installed in a task
local temporary directory. An initial Wasm Clippy selected the Homebrew compiler;
rerunning with the installed Rustup compiler/target passed. These were setup/test
failures, not skipped requirements. No external CI/deployment result is claimed.

**The full-resolution Run objective remains unmet.** Required CPU/peripheral,
bus/scheduler/reset/backup services, SCU, video, SCSP envelopes/DSP/timing, remaining
CD/subcode/media edges, BIOS/HLE ABI, SMPC and complete umbrella integration and
coverage remain in the conformance inventory. SCSP envelope and DIVU overflow
semantics were investigated against primary manuals but are not implemented by
this revision. No criterion was waived, no new tracker was created, no historical
Issue was reopened, and no human/firmware/live-site approval gate was introduced.
Keep the same PR Draft with Closes #17 as its intended final outcome. Do not merge
or mark ready based on these passing subset checks.

## 2026-10-21 VDP2 gradation and SH7604 cache continuation

Related to #17. Upstream main remains
`dbd981b030a46daa6923072ce240680dd2e58ebf`, rechecked with `git ls-remote`.
PR #18 retains the complete Issue #17 target and intended Closes #17 outcome.
The inherited DMA/DACK, MC68000 timing/family, SCSP envelope and ISO startup
changes are preserved, including their original guest/native/browser evidence.

VDP2 now executes BOKEN/BOKN gradation controls instead of rejecting them. It
collects the selected screen's horizontal colors, calculates the specified
1:1:2 filter and replaces the second image only when the selected screen is top
or second. Top-screen enable/ratio/offset/window behavior remains active;
line-color insertion and extended calculation are excluded while BOKEN is set.
Independent native vectors exercise all seven screen selections, priority and
hidden-source history. The new original SH-2 guest sets bitmap/palette/MMIO data,
repeats resets, and changes CCCTL operands; actual Wasm and production Canvas
pixels match authored RGBA values. The first two pre-display history samples,
transparent boundaries, expanded color precision and raster snapshots remain
explicit functional policies, not claims of independent physical conformance.

Cache ways0/1 now remain RAM in two-way mode even when their address-array tags
are valid. Reads and write-through updates exclude those ways. Refills now
perform the documented four individual longword transactions in wrapped order,
with the requested longword last. Independent vectors check every longword
offset and a failed refill with an occupied victim. A second test observes the
actual refill sequence through the other CPU's external sequential UBC and IRQ.
The new original P1 SH-2 guest proves RAM isolation, write-through, changed source
operands and repeated reset in native and browser Wasm execution. Ordered bus
transactions do not establish cycle-based arbitration or full cache conformance.

The [conformance entries](hardware-conformance.md#gradation-and-cache-refillram-implementation)
link primary specifications, implementation, native/browser checks, original
sources, MIT license and both reproducible ROM hashes. CI includes both builders.
Existing Star Collector, Prism Courier and Hardware Lab media remain byte-identical.

Checks actually completed for this combined revision:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked` and native
  all-target Clippy with `-D warnings`: pass using explicitly selected Rustup
  stable Rust/Cargo **1.99.0**. **348 native tests pass, zero failed/ignored**
  (183 unit and 165 integration). Homebrew 1.94.0 checks also passed during
  development; the final recorded full suite uses the managed toolchain.
- `sh scripts/build-web.sh` and wasm32 Clippy with `-D warnings`: pass using
  Rustup stable **1.99.0** and matching **wasm-bindgen-cli 0.2.129**.
- All **31** CI reproducibility checks pass, including the two new original MIT
  fixtures. The three existing demonstration media remain unchanged.
- `npm ci --ignore-scripts`, JavaScript syntax and the complete extended
  `npm run test:browser`: pass with Node **24.12.0**, Chromium
  **145.0.7632.6** and `/saturn-rust/` asset paths. No required browser test is
  skipped. Both new guests and operand mutations execute; gradation pixels reach
  the real Canvas. Existing gameplay, hardware, WebAudio, input/focus, local-file
  privacy and distribution assertions pass.
- Changed documentation local targets and Git whitespace checks pass.

Development-time vector/window assumptions, a temporary-borrow test compile error
and a copy-value lint were corrected before the final passing checks. The final
suite includes every new native case. No external CI/deployment result is claimed.

**Issue #17 and the full-resolution Run objective remain incomplete.** Required
CPU/peripheral, bus/scheduler/reset/backup services, SCU, video timing/modes,
SCSP calibration/DSP/timing, CD/SMPC and BIOS/HLE service/integration criteria
remain in the conformance inventory. DIVU overflow intermediates remain pending;
no unspecified arithmetic algorithm is presented as independently confirmed.
PR #18 stays Draft with Closes #17 as its intended final outcome. No criterion
is waived, no tracker is replaced or administratively closed, no historical Issue
is reopened, and no human/firmware/live-site/external-workflow gate is introduced.
This node updates the existing PR only; it does not merge or mark it ready.

## 2026-10-21 original guest-executed HLE system services

Related to #17. Upstream main was rechecked as
`dbd981b030a46daa6923072ce240680dd2e58ebf` with `git ls-remote`.
This work preserves the inherited `99dcf8a` CPU/interrupt/latch fixes and all
earlier PR #18 hardware, demos and evidence. The PR continues to target full
Issue #17 resolution and remains Draft with Closes #17 as the intended outcome.

HLE disc startup now installs independently authored, MIT-licensed SH-2 system
routines and executes the original master reset trampoline before disc AIP.
Nine callable services and two data interfaces implement VBR-relative interrupt
registration/readback, SCU C callbacks, atomic mask/shadow updates, shared
semaphore acquire/release, copied interrupt priorities and power-clear bytes.
SCU callbacks actually execute through guest wrappers, including nested interrupts
and complete caller-save register restoration. Private data writes update the
caller's write-through cache; shared semaphore operations use P1 and real TAS.
The external-BIOS path never installs or substitutes these routines.

The [conformance entry](hardware-conformance.md#original-hle-system-service-execution)
links primary ST-162-R1 and later system specifications, production SCU ST-210
restrictions, original source, ABI/layout, native assertions and actual browser
Wasm execution. Production IST writes remain prohibited; services use the
permitted mask/AIACK and ordinary interrupt acknowledgement paths.

The original [service diagnostic disc](../tests/fixtures/build_hle_system.py)
executes both CPUs, real SCU DMA/IRQ and all implemented interfaces. Its 46
authored main records, slave results, nested SR/mask records and reset traces
match in native and browser Wasm runs. Cases cover cached/uncached callers,
alternate VBR, zero/default restoration, semaphore endpoints and shared
contention, copied-table independence and R0–R7/MACH/MACL/PR/SP/SR restoration.
Changing only guest DMA data changes transferred values; changing the priority
operand changes the callback's actual SR. A separate original guest computes
retained bytes across NMI, requests timed SYSRES, then checks power-clear retention
and reset mask/callback/semaphore state. Repeated cold boots reset all eight
power-clear bytes. No host writes running guest results or detects game identity.

The disc and expected records use [the original fixture MIT license](../tests/fixtures/LICENSE),
and the ISO contains LICENSE.TXT. Service source and ROM fragments use
[the HLE MIT license](../src/hle/LICENSE). SHA-256 values:

| Artifact | SHA-256 |
| --- | --- |
| `src/hle/system.bin` | `20bdeed8c27a35fba5c0baa61c334383ac7812378a2774ee6f2e7bb26eaad745` |
| `tests/fixtures/hle-system.iso` | `20087f76e82f3aaafb049a7c1fbad46c9ea032e6b9638b3118cb0b30e472b0fa` |

Checks actually completed for the combined worktree:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass with
  explicitly selected Rustup stable Rust/Cargo **1.99.0**. **366 native tests pass,
  zero failed/ignored** (195 unit, 171 integration).
- Native all-target and wasm32 Clippy with `-D warnings`: pass.
- `sh scripts/build-web.sh`: pass using matching **wasm-bindgen-cli 0.2.129**.
- All **33** CI reproducibility commands pass, including the HLE ROM fragments
  and new service disc. Existing Star Collector, Prism Courier and Hardware Lab
  media hashes remain unchanged.
- `npm ci --ignore-scripts`, Chromium installation, JavaScript syntax and the
  complete extended `npm run test:browser`: pass with Node **24.12.0**, Chromium
  **145.0.7632.6** and project subpath `/saturn-rust/`. No required tests skipped.
  All new HLE traces/mutations and existing games, hardware guests, Canvas,
  WebAudio, input/focus, privacy and packaging assertions pass.
- Changed documentation's **336** local link targets and Git whitespace checks:
  pass.

Development failures exposed obsolete fixed startup budgets and cacheable SMPC
accesses in the new caller; those were corrected before final passing runs. The
additional 23 reset instructions are explicitly counted, preserving each test's
original AIP/game/pad assertions. An initial Wasm Clippy selected the Homebrew
compiler; selecting the installed Rustup compiler and target passed. No remote
CI or production publication result is claimed.

**The full Issue #17 and Run objectives remain unmet.** SYS_CHGSYSCK,
SYS_GETSYSCK, SYS_EXECDMP, backup filesystem/library and CD-library ABI remain
required, along with the other CPU/peripheral, bus/reset/scheduler/arbitration,
SCU/DSP, video/audio, CD/SMPC and comprehensive demonstration/coverage criteria
in the conformance matrix. DIVU overflow intermediates were checked against the
primary manual but are not implemented or independently confirmed here. No
acceptance criterion is waived or marked complete; this validation is not a
successful completion of the umbrella Run. Keep the same PR Draft, do not merge,
do not close or replace the tracker, and do not reopen #11/#13/#15. No human,
proprietary-firmware, live-site or external-workflow gate is introduced.

## SMPC clock transitions and digital selection timing (2026-10-21)

Related to #17. Rechecked upstream `main` with `git ls-remote`: it remains
`dbd981b030a46daa6923072ce240680dd2e58ebf`. Existing PR #18 work is retained;
this revision does not complete the umbrella requirements or resolve all review
findings.

Implemented CKCHG320/352 command deadlines and DOTSEL status, the documented
video/SCU/sound/slave reset domains, rational main/reference clock conversion,
normal 352-dot rendering and Canvas dimensions. Original guest SH-2 code supplies
SYS_CHGSYSCK and the SYS_GETSYSCK data interface. Both the original synthetic
BIOS and separate HLE disc start a real slave computation, perform clock changes,
observe retained work RAM/CD/backup and cleared device state, and verify the
slave stops with its shared result retained. Changed guest arguments change the
clock ABI value and status. Direct digital TH/TR selection now settles over
54 reference clocks independently on both ports; the original guest compares
early/settled reports and changed input levels. Both fixtures and their generated
media are MIT licensed. Primary references, implementation links, authored
register/pixel expectations and hashes are in the
[CLKG/DIGG conformance entries](hardware-conformance.md#smpc-pll-changes-guest-clock-abi-and-digital-input-settling).

Checks completed:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass,
  **375 native tests, zero failed/ignored** (201 unit, 174 integration).
  The strengthened slave-execution clock fixture also passes its targeted rerun.
- Native all-target and Wasm Clippy with `-D warnings`: pass. Rustup stable
  Rust/Cargo 1.99.0 was explicitly selected for the full final native run and
  Wasm checks. The new clock tests also pass with the installed native 1.94 toolchain.
- `sh scripts/build-web.sh`: pass with wasm-bindgen-cli 0.2.129. Existing Star
  Collector, Prism Courier and Hardware Lab hashes remain unchanged.
- All **35** CI reproducibility commands pass; both new fixture builders are
  included in CI. The strengthened clock fixture passes `--check` again.
- `npm ci --ignore-scripts`, JavaScript syntax checks and the complete extended
  `npm run test:browser`: pass with Node 24.12.0 and Chromium 145.0.7632.6,
  under `/saturn-rust/`. The final browser run includes the executing-slave clock
  fixture, argument mutation, 352-dot Canvas edge pixel and both-port digital
  timing/input mutation checks. Existing game, sound, input/focus, privacy and
  local-file/package checks remain enabled and pass.
- Changed documentation's local link targets and Git whitespace checks pass.

Development checks initially selected the Homebrew compiler for a Wasm build;
using the documented Rustup compiler fixed the missing-target error. A new
Rustup Clippy lint required array chunks in one test; it was corrected and the
native lint/test checks passed. No remote CI, production deployment or genuine
commercial BIOS compatibility result is claimed.

**The full Run objective remains unmet.** The remaining review findings still
require SH7604 overflow/BSC/peripheral effects, 68k bus sequencing, coordinated
CPU/DMA ownership, bus/backup filesystem services, SCU coordination/DSP coverage,
VDP1 and remaining VDP2 timing/modes, SCSP conformance, runtime CD/media behavior,
remaining BIOS/HLE services, complete SMPC/latch timing and comprehensive guest
acceptance evidence. Fixed CPU issue turns are unchanged by the PLL conversion.
No mandatory scope is waived, no tracker is closed/replaced, and no human,
proprietary firmware or post-merge approval condition is introduced.

## Ordinary BSC memory-interface execution (2026-10-21)

Related to #17. Rechecked upstream `main`: it remains
`dbd981b030a46daa6923072ce240680dd2e58ebf`. All inherited PR #18 work,
including the SMPC clock/digital-input revision, is preserved.

Implemented per-CPU WCR software waits/BCR1 long waits, BCR2 byte/word/long
external transfers and BCR1 CS2 endian conversion. Cache refills and larger
operands issue separate real narrow bus transactions; each adds its programmed
Tw clocks. TAS, write-through and cache-through accesses reach the external
bus; cache hits/arrays add no external waits. Reserved and unsupported register
configurations fail atomically rather than being accepted without an effect.

The original MIT BWAIT BIOS executes both SH-2s, initializes width/endian once
before non-CS0 access, measures seven wait configurations using independent FRTs,
computes endian read/write records and reconstructs physical bytes with guest
byte reads. Native and actual Wasm tests compare source-authored timer/data
vectors, repeat resets and mutate guest width, endian, wait and data operands.
Independent native vectors check cache-refill beat counts, opposite-CPU UBC
observations, physical byte strobes and all ordinary area wait/width selections.
Primary manual sections, implementation/test links, provenance and reproducible
hashes are in [BWAIT conformance](hardware-conformance.md#ordinary-bsc-software-waits-narrow-transfers-and-cs2-endian-conversion).

Checks completed for this combined revision:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass with
  explicitly selected Rustup stable Rust/Cargo **1.99.0**; **385 native tests**
  pass, zero failed/ignored (**208 unit, 177 integration**).
- Native all-target and Wasm Clippy with `-D warnings`: pass.
- `sh scripts/build-web.sh`: pass using wasm-bindgen-cli **0.2.129**.
- All **36** CI reproducibility commands pass, including BWAIT. Existing
  Star Collector, Prism Courier and Hardware Lab media bytes are unchanged.
- `npm ci --ignore-scripts`, Chromium installation, JavaScript syntax check and
  the complete extended `npm run test:browser`: pass with Node **24.12.0** and
  Chromium **145.0.7632.6** under `/saturn-rust/`, without skipping required
  tests. Includes every new BWAIT boot/mutation/reset trace and all existing
  hardware, games, Canvas, WebAudio, input/focus, packaging and privacy checks.
- All **330** local link targets in the changed development documents exist;
  Git whitespace checks pass, including the new fixture files.

Initial development runs exposed test assumptions equating all instruction
turns to eight clocks and fixed VRAM-clear startup budgets. FRT tests now locate
the same hardware deadline by elapsed clocks; game tests allow bounded guest
initialization and preserve their gameplay/pixel/restart checks. The build first
needed the already-installed matching wasm-bindgen CLI added to PATH, and the
browser run needed `npm ci`; both final runs passed. No remote CI, deployment or
genuine commercial BIOS compatibility result is claimed.

**The full Issue #17/Run objective remains unmet.** These changes add software
waits to the existing issue quantum; they do not implement full CPU bus-cycle
scheduling/arbitration. BSC idle/WAIT/memory protocols/board lanes, DIVU overflow
intermediates, 68k sequencing, backup filesystem/HLE/CD services and the other
mandatory chip/guest conformance requirements remain implementation work. Keep
the same PR Draft, preserve its intended Closes #17 outcome, leave #17 open and
do not merge. No scope is waived, no replacement tracker is created, no
historical Issue is reopened, and no human/proprietary-firmware/live-site or
external-workflow gate is introduced.

## Review correction: DIVU, UBC, peripheral deadlines and status

Related to #17. Implemented DIVU overflow intermediate quotient/remainder states,
OVFIE-dependent saturation, the UBC configuration delay/read barrier and slave
UBC interrupt acknowledgement. Added CPU-local FRT/BSC/SCI/DIVU/WDT deadlines and
fixed loss of clocks preceding watchdog reset assertion. SMPC now reports all
eight documented board regions and independent CD/sound reset signals. SCSP
DEXE=0 no longer clears an active DMA's busy flag early.

Original MIT synthetic guests execute both CPUs' DIVU and UBC paths, with 164
independent result words per CPU, repeated reset and changed-dividend checks.
The original SMPC status guest executes real commands for every region and
uploads an executing 68k STOP program. The existing original 68k DMA/MIDI guest
now writes DEXE=0 before polling completion. Native and browser Wasm assertions
exercise the resulting register, IRQ, stack and data effects. Primary sections,
implementation/test links and reproducible hashes are recorded in the
[conformance inventory](hardware-conformance.md#divu-ubc-peripheral-deadlines-and-smpc-status-corrections).

Validation on Rustup stable Rust/Cargo **1.99.0**, Node **24.12.0**:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  **395 native tests**, zero failed/ignored (**216 unit, 179 integration**).
- Native all-target and Wasm Clippy with `-D warnings`: pass. Two UBC style
  diagnostics found during validation were corrected before the passing runs.
- `sh scripts/build-web.sh`: pass with wasm-bindgen-cli **0.2.129** from the
  existing Rustup tool installation. Original Star Collector, Prism Courier and
  Hardware Lab media bytes remain unchanged.
- All **38** CI reproducibility commands, JavaScript syntax and Git whitespace
  checks pass; **273** local documentation link targets exist.
- Seven deliberate source regressions are rejected by the native tests:
  saturated overflow despite OVFIE, immediate UBC comparison changes, wrong-CPU
  UBC acknowledgement, discarded pre-reset clocks, fixed-Japan region status,
  missing CD-reset status, and early-cleared SCSP DMA busy. All source mutations
  were restored before final validation.
- The complete extended `npm run test:browser` passes in Chromium
  **145.0.7632.6** under `/saturn-rust/`, including both new guest fixtures,
  all 64-bit divisor sign combinations, region/reset reports, the revised 68k
  DMA guest, and all existing demos, Canvas, AudioWorklet, input/focus, packaging
  and local-file privacy checks. No required browser test is skipped.

The mandatory umbrella implementation and coverage remain incomplete. These
corrections do not establish coordinated CPU/bus arbitration, complete 68k bus
sequencing, full video/audio/CD/BIOS/HLE/backup services or the remaining SMPC
contracts. The existing mandatory inventory stays in force. No Issue is closed,
no scope is waived, and no human testing, personal firmware, live-site inspection
or external-workflow approval gate is introduced. No remote CI or deployment
result is claimed by these local checks.

## Original guest-executed HLE backup library (2026-10-21)

Related to #17. Rechecked upstream `main` and PR #18: main remains
`dbd981b030a46daa6923072ce240680dd2e58ebf`; the published head before this
update was `59c9722136bfa2ff53a54039034daa73da428a7b`. This revision preserves
all inherited work, including the subsequent DIVU/UBC/peripheral/status fixes.

Implemented BUP_Init and all ten expanded-library entry points as original SH-2
instructions. Init copies/relocates the 16 KiB library, publishes the documented
RAM354 vector table and returns three device configurations. The guest performs
partition/format/status, allocation/overwrite/read/delete/prefix-directory/
verification and Gregorian date conversions. All stored bytes go through the
existing backup bus. Primary specifications, exact API/status definitions,
private storage layout and limitations are linked from
[the BUPG inventory](hardware-conformance.md#guest-executed-hle-backup-library-bupg)
and [the HLE contract](../src/hle/README.md#independent-backup-library).

The independently identified SRBUP001 HLE filesystem does not claim compatibility
with Sega firmware's opaque storage layout. Unknown raw data remains untouched
and reports unformatted until an explicit guest Format call. External BIOS mode
does not install any HLE service. No SDK routine/object, emulator core, firmware
or third-party asset was copied. This is additional diagnostic media, not a
replacement for either required demonstration or any mandatory hardware scope.

The original MIT disc carries its license and executes the copied routines via
the public vector table. Native and actual browser Wasm compare **158** authored
ABI records, including full 510-block allocation, capacity/overwrite failures,
retained original data on rejected writes, empty files, three-file compaction,
prefix-directory short-table sign/guard/stride, all callee-save registers and
eight independent date/weekday vectors including the 2100/2200 non-leap years.
Changed guest XOR operands change computed/stored bytes; relocating the library
changes actual executed entry addresses with unchanged ABI results. Read-only
probes reopen cold-retained/exported/imported bytes, detect nine metadata/payload
corruptions and preserve every packed byte. Native probes add ten malformed
count/size boundary cases and remain bounded.

Checks completed on Rustup stable Rust/Cargo **1.99.0**, Node **24.12.0**:

- `cargo fmt --check`, `cargo build --locked`, `cargo test --locked`: pass;
  **401 native tests**, zero failed/ignored (**216 unit, 185 integration**).
- Native all-target and Wasm Clippy with `-D warnings`: pass. A new constant-chunk
  lint in the relocation test was corrected before the final passing run.
- `sh scripts/build-web.sh`: pass with matching wasm-bindgen-cli **0.2.129**.
  Existing Star Collector, Prism Courier and Hardware Lab media remain unchanged.
- All **39** CI reproducibility commands and JavaScript syntax checks pass.
- The complete extended `npm run test:browser` passes in Chromium
  **145.0.7632.6** under `/saturn-rust/`, including baseline, relocated and
  changed-operand backup discs, repeated persistence/corruption probes, and all
  existing hardware/demos/Canvas/AudioWorklet/input/privacy/packaging checks.
  No required browser test was skipped.
- Five deliberate guest-library source regressions are rejected: duplicate-write
  success without writing, positive short-directory counts, wrong century leap
  handling, lost R14 and ignored payload checksums. Original source/binaries were
  restored, reproduced, and all six backup integration tests passed again.

Development checks corrected the initial interface mapping against Sega's
published declaration facts before final validation. The full-capacity guest
required more than one million instructions; its runner now allows up to three
million in ordinary bounded 10,000-step calls, while preserving all expected
records. No source or expected result was weakened to accept a failing behavior.
No remote CI, deployment or genuine BIOS compatibility result is claimed.

**The full Issue #17/Run objective remains unmet.** Full CPU/bus scheduling,
68k bus sequencing, remaining SCU/DSP/video/audio/CD/SMPC behavior, SYS_EXECDMP,
CD-library/remaining boot services and comprehensive acceptance evidence remain
mandatory implementation work. All original criteria remain in force on the
same Issue and Draft PR, with Closes #17 as the intended completed outcome.
No Issue is closed/replaced, no historical Issue is reopened, and no human,
proprietary-firmware, live-site or external-workflow gate is introduced.
