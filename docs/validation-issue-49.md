# Issue #49 validation: HLE dual-CPU interrupt startup

Related to #49 and #17. This worktree increment supplies HLE capture startup
and the supported slave raster configuration. It does not establish hardware
completion or close #17. No push, PR, merge, Issue comment or required post-merge
verification is part of this checkpoint.

## Contract and implementation

[The startup contract](hle-dual-startup.md) records exact primary references,
register widths/encodings, enable/priority/vector ordering, SR masking,
acknowledgement, vector retention and the older manual discrepancies. It uses
System Program capture vector64/priority15 and the later STN-28 entry94;
ST-202-R1's printed p.3 entry164/level1 are reconciled explicitly. Capture-handler
registration at master index164 is a separate operation.

[boot.s](../src/hle/boot.s) performs the setup through original SH-2 instructions,
with the [reproduced binary](../src/hle/boot.bin) installed by the existing HLE
path. On each CPU, TIER=81, IPRA=0000, IPRB=0F00, VCRC=6465 and VCRD=6600.
Initialization disables requests before setup and reads/clears FTCSR before
enabling capture. It keeps SR.I=15 until application code chooses otherwise.
ICR.VECMD=1 supplies master SCU vectors; VECMD=0 supplies the supported slave
raster autovectors41/43. This is the documented HLE configuration, not a Sega
ROM register dump. The hardware-reset defaults, external BIOS path, interrupt
mechanism, scheduler, cache/coherence and reset domains are unchanged.

[Boundary vectors](../src/hle/startup_tests.rs) inspect both CPUs' architectural
reset values before/after HLE installation and after external-ROM execution.
Real trampoline instructions configure each CPU without a phantom capture.
Word strobes assert vector64/level15; SR.I=15 keeps it pending, SR.I=14 permits
entry, and exception acceptance leaves the request asserted. Literal frame
checks assert saved PC/SR and SP-8. A zero write without observing ICF cannot
acknowledge it; guest read alone also cannot acknowledge it. Guest read/zero
write, RTE and a second strobe exercise acknowledgement and reassertion.
A pending slave capture is discarded by SSHOFF; reset defaults return, SSHON's
trampoline configures capture again, and all initialized RAM vector bytes stay
unchanged.

## Original guest evidence

The original [MIT](../tests/fixtures/LICENSE)
[builder](../tests/fixtures/build_hle_dual.py) reproduces the
[assembly](../tests/fixtures/hle-dual.s),
[disc](../tests/fixtures/hle-dual.iso) and
[178 independent literal records](../tests/fixtures/hle-dual.tsv). The ISO carries
its license. Disc SHA-256:
`02eae6892c57724a3cb1df05d7a3dd398c39546080c5bc0b4c9276277dc213bc`.
No firmware, SDK, external emulator core or captured emulator
outputs generate guest code or expectations. The application never writes
TIER/IPRA/IPRB/VCRC/VCRD/ICR. Only the bootstrap supplies this configuration.

Both CPUs record startup registers, VBR, SP, entry operands and retained handler
pointers. Master uses index94 for slave entry and index164 for the capture
handler. Both CPUs' capture handlers change live arithmetic operands, record
hardware-stacked frames and SR, observe ICF, acknowledge it and return with RTE.
Ordinary code then computes shared-memory payloads. Actual word strobes carry
arbitrary values; payload/ready polling uses P1 aliases.

With master/slave seeds3/5 and request11, the first two bidirectional SLEEP wake
exchanges compute39/59. SSHOFF/SSHON selects a different original entry, which
adds17 to the slave seed; its exchange computes56. Restarts after each clock
selection also compute56. Changing seeds to9/17 and request23 computes69/89,
then86 at each restart. Omitting the actual strobe leaves both capture counts,
computed results and completion zero, and both CPUs asleep with no exceptions.
No host writes supply a result or interrupt outcome.

The native suite compares all authored records and raster samples across chunks
1/3/17/10000, with/without passive debugger reads, twice per combination. A
separate probe compares all 512 initialized slave-vector bytes at four actual
application entries, covering SSHOFF/SSHON and both PLL changes, for default
and custom slave stacks. Guest SP/frame records verify each metadata choice.
The [production Wasm check](../scripts/hle-dual.mjs) loads identical diagnostic
bytes, compares the same TSV, checks retained bytes/custom stacks, operands,
controls and repeated boots under `/saturn-rust/`.

The same disc registers H/V handlers using master indexes141/143, enables real
unified-cache instruction lines and handles two entries of each live raster
level. It records H seed+14, V seed+22 and final H+V+36; seeds3/5 yield17/27/44,
seeds23/41 yield37/63/100. Both H samples share one line and HBLANK; both V samples
share a line224–262 and VBLANK. The second handler edits stacked SR to suppress
later entries. SR.I=15 suppresses the entire stage. Default/changed variants use
master SCU IMS=FFFF/0000 with master SR.I=15; slave handling retains the master
IST H/V IN bits (literal masked result5). The raster-only original AIP selection
runs independently of FRT communication, so capture failure cannot hide a
raster failure. Existing #47 board timing, reset and arbitration tests are retained.

A SYSRES variant reruns the complete diagnostic after hardware/system-service
reinstallation. Power-clear value12345678 survives SYSRES and is zero on the
next cold boot. Imported backup bytes remain identical. Existing stack, backup,
NMI/power-clear, clock/reset-domain, input and local-file privacy assertions stay
in the unabridged native/browser suites.

## Negative controls and fixture adjustments

Temporarily replacing only `src/hle/boot.bin` with the exact inherited bytes
from `e92724d` made both independent tests fail with exit101:

- `independent_capture_handlers_compute_both_sleep_wake_rounds`: actual0,
  expected39 at the first ordinary bidirectional calculation.
- `independent_raster_entry_uses_boot_mode_and_guest_sr_mask_only`: actual all-zero
  raster records, expected counts2/2, arithmetic17/27/44 and completion600D.

The completed binary was restored in a `finally` block, and both independent
assertions passed again. Expected records and fixture bytes were unchanged in
these experiments. The old bootstrap's missing enable/priority/vector fields
and shared VECMD=1 are therefore rejected by separate guest-executed assertions.

The master trampoline now retires67 instructions before AIP, compared with23.
Existing disc/AIP assertions now expect71 total instructions for the four-opcode
AIP, and bounded startup waits allow200 instructions. The backup guest's exact
poll count advances by44 to preserve its original polling/backup assertions.
The browser's matching AIP count is updated identically; its UI pad-poll budget
also advances by44 (123 to167). Earlier full browser attempts exposed these
two inherited fixed-budget assertions before their correction. A focused
production-Wasm check verifies the corrected pad budget still produces the
authored button samples3E/3F/37/3F. These changes account
for real new initialization instructions; no substantive assertion is removed.
The new raster fixture reads EXTEN before VCNT to latch the real counter,
following VDP2 ST-058 §2.5 and the existing #47 fixture; no host counter is supplied.

## Reproduction and actual validation

Environment: macOS arm64, rustup stable Rust/Cargo1.99.0,
wasm-bindgen0.2.129, Node24.12.0, npm11.6.2 and locked Playwright1.58.2.
Checks use the rustup stable compiler/Cargo together; the shell also has a
separate Homebrew Rust1.94 installation. Initial focused tests also passed there.

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 949 tests across 88 result groups; zero failures/ignored tests. Final seven guest and two boundary tests also passed after the SCU-status addition |
| Native all-target Clippy, `-D warnings` | Passed, including final guest assertions |
| wasm32 Clippy, `-D warnings` | Passed |
| All 87 workflow-registered fixture/media reproduction commands | Passed; final HLE diagnostic also rechecked after its SCU-status assertion |
| `sh scripts/build-web.sh` | Passed; rebuilt release production Wasm and original demo media |
| `npm ci --ignore-scripts` | Passed; zero reported vulnerabilities |
| Focused production HLE browser check | Passed in Chromium145.0.7632.6 under `/saturn-rust/`; identical final disc/TSV, repeated boots, both metadata stacks, changed operands and all controls |
| Complete `npm run test:browser` | Passed in Chromium145.0.7632.6 under `/saturn-rust/`, no required skips; new HLE diagnostic plus Star Collector, Prism Courier, Hardware Lab, Canvas/audio, input, backup, standard-size media and local-file privacy assertions all passed |
| Inherited-bootstrap negative controls | Both independent native tests failed as intended; restored tests passed |
| Documentation local links and Git whitespace | Passed; rechecked before checkpoint |

The new fixture is registered in the existing workflow and setup reproduction
list; the browser helper is registered in the existing full smoke suite. Star
Collector, Prism Courier, Hardware Lab, Canvas/audio, input, backup and local-file
privacy coverage remain required. No personal firmware, commercial disc, human
approval or live Pages inspection is necessary.

## Remaining scope

This is a partial #17 increment. Slave VECMD=1 board-vector acquisition,
physical synchronizer/noise-canceler timing, half-line interlace, complete BIOS
initialization and other HLE services remain outside #49. All remaining #17
CPU/peripheral, bus/cache/reset/timing, SCU/DSP, video/audio, CD/media/HLE, SMPC
and comprehensive integration requirements remain mandatory. Genuine BIOS and
commercial-game compatibility remain independently unverified. Green checks or
administrative tracker closure do not establish hardware completion. Required
post-merge verification: none.
