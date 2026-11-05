# Issue #103: SMPC transaction investigation and byte-only boundary

## Finding and scope

Related to #103 and #17. The reported **one-byte** access at PC `0x0000025C`
remains unclassified: no instruction, direction, operand or earlier SMPC state
was supplied. This work does not identify that private firmware instruction or
claim that its startup now reaches a menu. No personal firmware or trace is an
acceptance dependency. No ROM identifier, PC-specific behavior or substituted
firmware UI is used. Required post-merge verification: none.

The inherited code at `2a323e79f12ba58333be46ae9d69c089f3a14676` routes
`0x0010001F` to COMREG. Its generic `UnsupportedDevice` can mean:

- Any COMREG read, because COMREG is write-only.
- A write without SF=1, or while a command/INTBACK acquisition is busy.
- An unsupported command code, invalid SETTIME calendar, or unsupported/invalid
  INTBACK parameters (including missing IREG2=F0).

The independent byte investigation below finds **no rejection of documented
master-issued byte commands**. An exhaustive command/parameter audit (which used
an implicit master issuer) and an additional original firmware guest pass
unchanged against inherited main and the checkpoint, including production
Wasm/Chromium. The original failure must not be labeled a COMREG read, an invalid firmware operation, or attributed to
a specific command. A synthetic test of each candidate distinguishes the rejection
paths; none is converted to unconditional success.

A separate concrete defect was reproduced: the inherited bus implements word and
longword SMPC operands as multiple byte register operations, even though ST-169
specifies byte-only access. A word store of 0009 to `0x2010001E`, after setting
SF, silently issues CDOFF. A longword store to `0x20100000` silently changes two
IREGs. Wide status reads also silently succeed. These are prohibited widths,
**not evidence explaining the reported one-byte fault**. The correction rejects
such operands before register mutation, cache fill or BSC decomposition. This is
a bounded emulator policy for accesses outside the documented contract; it does
not assert a physical Saturn bus exception or measured electrical response.

Review then identified a second concrete defect: bus dispatch discarded the
issuing processor and accepted slave INTBACK, SSHON/SSHOFF and clock changes
prohibited by ST-169 §2.2/table2.5. The original master-context audit could not
observe it. The issuer correction below preserves CPU ownership and rejects
those five stores before command/report/deadline mutation. This acceptance
defect also does not identify the private firmware transaction.

The scope is independently reconstructed SMPC access semantics, an executable
master-context acceptance audit, rejection diagnostics and proven
width/issuer-policy corrections.
The original byte failure is unverified. Keep #103 open and PR #104 in draft
under the partial publication scope; do not generate a closing reference from
this checkpoint. The review follow-up below supplies timing-valid byte-command
and post-deadline reset evidence; it does not classify the original symptom.
No private trace or human confirmation is required to finish that work. Broad hardware
completion remains in #17, and the unrelated #81 SCU endpoint finding is unchanged.

## Primary-source reconciliation

Sources were read directly from the original PDFs; their bytes are not added to
this repository:

- [Sega SMPC ST-169-R1-072694](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-169-R1-072694.pdf),
  §1.2, fig1.3 and printed pp.5–6; §2.2/table2.4/figs2.1–2.4,
  pp.13–16; §2.3, command descriptions pp.21–34; §2.4.2,
  pp.36–44; §2.5, pp.45–46; §3.1/table3.2, pp.48–60.
- [Hitachi SH-1/SH-2 programming manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/h12p0.pdf),
  table5.3 p.40 and §6.32 MOV pp.112–113: 6010 decodes MOV.B @R1,R0
  (byte read, sign-extended); 2101 is MOV.W R0,@R1 (word write).
  Effective addresses come from the actual bus operand, not a PC assumption.
- [Hitachi SH7604 hardware manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf),
  §4.3.1/table4.6, pp.71–72 (architectural address errors),
  §§7.3.1–2 (external operand cycles), §7.11.2, pp.210–211
  (cache-through access and writes completing to the CPU before the external bus).
- [Sega SCU ST-097-R5-072694](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-097-R5-072694.pdf),
  §1.2/fig1.3 (SMPC CPU-bus select and address map), §2.1,
  SMPC interrupt, p.33, and §3.2 IMS7/IST7 at 25FE00A0/25FE00A4.
  SMPC status readiness and SCU interrupt acknowledgement are separate events.

The linked [HTML command-issuance transcription](https://docs.exodusemulator.com/Archives/SSDDV1R1E/a/a03/hon/p013.htm)
returned HTTP 502 during investigation; the original ST-169 PDF was available.
Figures2.1/2.2 contain an English “Reads Command Code” label; the normative
COMREG W map and command-issuance prose specify a write. This translation label
is not a basis for making COMREG readable.

All offsets below are relative to physical `00100000`; `20100000` is its
cache-through alias. Existing 128-byte chip-select mirrors are retained.

| Register/path | Documented contract | Emulator behavior and evidence |
| --- | --- | --- |
| COMREG +1F | Byte, W; issue after SF=0 check and SF=01 write (§§1.2/2.2) | Accepted byte commands retain their timed effects. Reads give `WriteOnly`; missing SF, busy reissue, unsupported command and invalid parameters have separate reasons. No invented readback/open-bus value. |
| COMREG issuer | §2.2/table2.5 p.20 permits all sixteen commands from master; prohibits slave SSHON/SSHOFF/CKCHG352/CKCHG320/INTBACK | `ProhibitedIssuer` before command, OREG31 or deadline mutation. Direct and scheduled CPU/buffered/DMA writes retain the owning processor. The other eleven commands remain permitted from slave. Explicit host failure is a bounded policy, not a measured electrical response. |
| SF +63 | R/W; SH-2 may only set with 01; SMPC clears on completion; upper read bits undefined (§1.2 p.6) | Read returns bit0 with upper bits deterministically zero. Writes other than 01 fail `InvalidSf`; this boundary is policy, not a claim that hardware raises an exception. Setting SF alone does not start a command. |
| IREG0–6 +01…0D | Byte, W parameters; INTBACK IREG0 also continue/break (§3.1/table3.2) | Read fails `WriteOnly`. Input changes while busy fail except implemented INTBACK continue/break. Rejected writes do not change the pending command or deadline. SETSMEM copies actual guest operands. |
| OREG0–31 +21…5F | Byte, R results; OREG31 identifies command at processing start (§2.3 p.21/§2.4 p.35) | Writes fail `ReadOnly`. SF distinguishes active/completed command reports. Status publication preserves INTBACK code. |
| SR +61 | Byte, R, readable independent of command (§1.2); RESB sampled at VBlank-in | Writes fail `ReadOnly`; existing status/peripheral/reset-button behavior retained. |
| Word/longword | Only byte register accesses specified (fig1.3) | `ByteOnly` before any lane side effect, including CPU aliases and physical DMA bus paths. Ordinary CPU operand alignment errors still follow SH7604 table4.6 through the existing operand checker; exception stack/vector longwords are checked before invalid-cycle byte splitting. No fabricated guest exception for unsupported device behavior. |
| Disconnected even byte lane | No register on this lane; electrical value not specified by fig1.3 | Existing deterministic FF read/ignored byte write policy retained, confined to disconnected lanes. It does not apply to odd COMREG reads. |
| Type B/C commands | No SMPC result IRQ; SF/OREG31 report completion (table2.4) | Existing reset/power/RTC/SMEM effects and deadlines retained. CDOFF/CDON use 1075 reference clocks (40 µs rounded up), SETSMEM 1075, SETTIME 1882; CPU/NMI/reset-enable commands use 807 (30 µs). These are deterministic functional deadlines, not measured oscillator/internal-task latency. |
| INTBACK type D | IREG0=00/01, IREG2=F0; reserved port mode prohibited; status/peripheral readiness interrupts (§2.4.2 pp.37–44, §3) | Existing 8600-clock status bound, SF/OREG/SR publication, SCU bit7, VBlank collection, continue/break retained. SF completion is not SCU acknowledgement. Native/Wasm guest observes pending IRQ, then services it through the real SH-2 vector. |
| Reset/power | Power-on domains table1.1; SYSRES/clock commands §§2.3.8–10 | Cold reset cancels pending commands, snapshots and results. Existing battery/backup retention, SYSRES, PLL, sound/CD and dual-SH-2 tests remain the wider evidence; this change does not reclassify their timing approximations as full conformance. |

ST-169 does not define the electrical return value of a write-only register read,
invalid SF write, dual command issue, prohibited issuer or a prohibited wide access. Explicit,
sticky host failure remains the policy. It is not an SH-2 architectural memory
exception, and no open-bus assertion is inferred from the board address map.
The model still does not enforce every documented command-issuance window.
The issuer correction below now enforces table2.5 at COMREG. This checkpoint
does not claim full SMPC or stock-firmware conformance.

## Independent byte-transaction investigation

The follow-up examines the rejection predicates themselves. ST-169 tables2.1–2.3
(p.12) list exactly these sixteen command bytes: 00, 02, 03, 06, 07, 08, 09,
0D, 0E, 0F, 10, 16, 17, 18, 19, 1A. Table2.4 (p.13) divides them into
exception/reset, parameterless, parameterized and result/IRQ classes. All sixteen
are accepted by inherited `Smpc::write`; no listed command is missing from its
dispatch. IREG contents do not restrict parameterless commands or SETSMEM.

| Byte rejection at COMREG | Primary contract and independently checked boundary |
| --- | --- |
| Read | §1.2/fig1.3 pp.5–6 specifies W. SH-2 programming manual table5.3 and §6.32 distinguish MOV.B loads (6010 for @R1,R0) from stores (2100 for R0,@R1). No instruction/direction was supplied for the reported PC. A W map supplies no electrical read value; explicit unsupported-read policy remains justified. |
| Missing SF / busy | §2.2 pp.13–16 and command limitations p.19 require SF polling, SF=01, parameters when required, then COMREG, with completion before another issue. SF reservation alone has no pending command. All accepted commands enter pending processing or an INTBACK collection phase; completion/break releases them. A new byte-bus audit checks disconnected even lanes, four aliases, one clock before, at and after CDOFF's deadline, rejected reissue and subsequent CDON. |
| Unlisted command | All 256 byte codes are independently compared with the sixteen entries above. Each listed code accepts with valid parameters; the other 240 remain explicitly unsupported. Their electrical effects are unspecified here. |
| INTBACK parameters | §2.4 pp.37–38, 42–43 and §3.1 table3.3 p.54 require IREG0=00/01, IREG2=F0, nonreserved port modes (00/01/11), and PEN for peripheral acquisition. Status-only uses 01/00/F0. Every IREG0 byte, every defined IREG1 field combination (bits2/0 zero) and three IREG2 values are audited: 49,152 cases, including all 54 accepted combinations. Neither optimization choice nor 255-byte/zero-byte mode is accidentally excluded. No data requested (00/00/F0) remains outside the three documented acquisition forms. |
| SETTIME parameters | §2.5 p.46 defines BCD year/date/time, hexadecimal month 1–C and weekday 0–6. Every date in 2000/2001, all seven weekdays and both midnight/23:59:59 are accepted: 10,234 valid calendars, including leap day. Four nonexistent/non-BCD cases retain the explicit undefined-result policy. The firmware additionally reads back 1993/2000/2024/2099 and months A/B/C. Undefined calendars are not reclassified as unsupported command codes or physical bus errors. |

The original master-context [public-API audit](../tests/smpc_byte_contract.rs) has no dependency on the
new diagnostics or width correction. At the prior checkpoint, the exact source
and fixtures were copied into an isolated archive of `2a323e79f12ba58333be46ae9d69c089f3a14676`; all five
tests pass there. This includes the original MIT 512 KiB
[byte firmware](../tests/fixtures/smpc-byte-audit.s) and its independent
[expectation](../tests/fixtures/smpc-byte-audit.tsv) (then 111 records; now 201
with the raster checks below).
It executes reset vectors, polls SF, claims SF **before** writing IREGs, issues
SETTIME/SETSMEM/INTBACK through byte stores, reads real calendar/SMEM results,
and exercises all eighteen status-plus-peripheral port/optimization combinations.
Each status result leaves SF busy, SR.PDE reports readiness, and guest BREAK
releases acquisition. IRQ delivery/acknowledgement, actual peripheral collection,
reset/power/NMI and dual-CPU command effects retain the existing dedicated guests;
this audit does not replace those checks or infer their correctness from acceptance.

SH7604 table4.6 (pp.71–72) does not make this odd byte MMIO operand an alignment
exception. Its §7.11.2 (pp.210–211) distinguishes buffered external writes from
CPU completion and uses real addresses for cache-through operands. The new guest
uses 201000xx and, with cache disabled at reset, 001000xx; it does not read COMREG
to flush a write. SCU §1.2/fig1.3 maps the SMPC region at these aliases; it does
not prescribe readback for a write-only register. These sources give no basis for
changing a rejected COMREG read to zero, FF or a command echo. The existing
opt-in diagnostics distinguish buffered-store failures without re-reading ROM.

**Bounded conclusion (master-context audit):** every inherited byte rejection
predicate has a documented precondition or an explicitly bounded unsupported/undefined-operation policy;
the independent audits demonstrate no rejection of a correctly issued documented
master-issued byte command in this model. This rejection-only audit did not
cover prohibited slave acceptance; the issuer correction below addresses that
separate byte-path defect.
This substantiates the Issue's permitted no-defect outcome for the byte-command
acceptance investigation. It does not prove the unavailable firmware violated
the protocol, identify its transaction, or establish full SMPC timing/electrical
conformance. The dispatch audit alone excludes timing-window/CPU-issuer limitations,
cache-enabled MMIO and wider integrated acceptance. The timing-valid firmware
evidence below now covers command windows independently; it does not turn the
dispatch audit into evidence for timing enforcement or CPU-issuer restrictions.
For that master-context investigation, no byte correction, private-ROM startup
success or red-to-green byte control is claimed; the issuer follow-up below has
its own demonstrated byte acceptance defect and negative controls. Existing read/handshake/busy/invalid-parameter
negative controls remain active; the width defect has its separate red control.
Keep #103 open and PR #104 in draft under the contributor publication rule.

## Diagnostics and original guests

`BusError::Smpc(AccessFault)` captures the rejected access without an extra bus
read. Normal fault text includes register/path, direction, width, address, write
operand, reason, SF, busy and last accepted command. The explicit local
`Machine::smpc_fault_diagnostics()` / Wasm `emulator.smpc_fault_diagnostics()`
returns the CPU identity/issuing PC and the bounded snapshot: pending command,
remaining reference clocks, INTBACK waiting/phase flags, seven IREG values,
SR, OREG31, and main-bus clock. The remaining command deadline is in **reference
clocks**; do not add it to the main-bus clock across PLL changes. Waiting for
continue/VBlank has no numeric pending deadline and is distinguished from idle.
Already decoded instruction words are retained for synchronous operand errors;
late buffered writes have `instruction: None`. No firmware is reread to fill that
field, no ROM bytes or personal filename are dumped, and nothing is automatically
logged, uploaded or saved. Reset clears the fault. Observation does not consume
an IRQ or advance a clock.

Actual original-fixture examples from the native probe:

```text
PC 0000020C: COMREG 1-byte read 0010001F -> WriteOnly
  instruction=6010, SF=0, pending=None, main_clock=123
PC 00000218: COMREG 1-byte write 0010001F value=09 -> MissingSf
  instruction=None (buffered), SF=0, pending=None, main_clock=174
PC 00000248: COMREG 1-byte write 0010001F value=08 -> Busy
  SF=1, command=09, pending=09, remaining=966, main_clock=392
PC 00000226: COMREG 2-byte write 2010001E value=0009 -> ByteOnly
  instruction=2101, SF=1, command=00, pending=None, main_clock=225
```

These PCs belong only to the original fixtures, not the unavailable user BIOS.

[`build_smpc_transactions.py`](../tests/fixtures/build_smpc_transactions.py)
reproduces the source `.s` and sparse big-endian `.hex` representation of seventeen
standalone **524288-byte** ROMs, all under the adjacent [MIT license](../tests/fixtures/LICENSE).
Reset PC/SP are 00000200/060FF000. Expansion starts with zero-filled ROM bytes;
the fixture builder uses the repository's original assembler, no SDK/cross compiler.
The accepted guest [`smpc-transactions.s`](../tests/fixtures/smpc-transactions.s)
and independent expected [record fixture](../tests/fixtures/smpc-transactions.tsv)
exercise SF/command reporting, SETSMEM operands 12/34/56/78, CDOFF status,
INTBACK status/region/SMEM values, SCU pending bit7, real IRQ vector47 and RTE,
CDON and RESENAB. Only guest instructions write records at 0601A000 and the IRQ
counter at 0601A100. Ten additional guests isolate COMREG read, missing/incorrect
SF, busy reissue, unsupported command, invalid INTBACK parameters, word/longword
writes and word/longword reads. Each stores sentinel 55 at 0601A200 only if the
invalid operation silently proceeds. Native and browser assert the sentinel stays
zero, the fault stays sticky and reset removes its snapshot.

Native tests compare complete CPU state/clocks under run-call chunks 1/3/127/10000
and cold resets, with Debug observation between calls. Browser uses chunks
1/127/10000 and compares records, exceptions, clocks and instruction counts.
These are partitions of the same scheduler turns, not alternative hardware
latencies. Independent SMPC device tests actually partition clock advancement
into 1/3/127/10000 clocks and sample one clock before, exactly at and one clock
after deadlines. They verify rejected reads/reissues/parameter writes retain the
pending deadline; SF values 00/02/FF and read-only output writes are negative
controls. The reset follow-up below interrupts guest-issued SETSMEM, SETTIME
and INTBACK at three pending points and runs an observing guest beyond their
former deadlines. Existing SMPC, clock-domain, dual-CPU,
INTBACK/pad, SCU, HLE and browser guests cover the other exercised command classes.

## Negative control

An isolated `git archive` of inherited `2a323e7` ran a copy of the public-API
width test plus the four unchanged wide-access ROMs. Command:

```sh
CARGO_TARGET_DIR=target/issue-103/baseline-target cargo test --locked \
  --manifest-path target/issue-103/baseline-src/Cargo.toml --test issue_103_negative
```

All **five** assertions fail as expected: the host word COMREG write succeeds,
and each firmware word/longword read/write reaches sentinel `Ok(85)` (55 hex).
The corrected native/browser tests reject those operations before mutation and
never reach the sentinel. This is a negative control for the demonstrated width
policy defect only. The inherited read/missing-SF/busy/unsupported-command paths
already fail; their new tests establish diagnostic precision, not a previously
successful byte-command compatibility fix.

## Executed validation (prior checkpoint)

Executed on macOS arm64 with rustup stable Rust/Cargo 1.99.0, Node 26.12.1,
Playwright 1.58.2, Chromium Headless Shell 145.0.7632.6 and wasm-bindgen 0.2.129.
The repository setup PATH selects the paired rustup compiler/Cargo and matching
bindgen CLI. These are local concurrent-run timings, not hosted-CI measurements.

- `cargo fmt --check` and `cargo build --locked`: pass.
- `cargo test --locked` (wrapped by `python3 scripts/ci-time.py
  issue-103-native`): **1,172 pass**, zero failures/ignored, 104 harnesses;
  414.42 s summed harness time, 459.138 s wall time including compilation.
  The final additional invalid-register policy unit test was added while that
  run was active and separately passes with `cargo test --locked --lib
  smpc::tests::invalid_register_handshake_and_parameter_policies_do_not_mutate_command_state`.
  The current inventory is 1,173 tests; no single-run 1,173-test result is claimed.
  Final `cargo test --locked --test smpc_transactions` also passes all four tests
  in 0.07 s, including reset cancellation and full CPU/chunk comparisons.
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`: pass.
- `sh scripts/check-fixtures.sh`: all **104** registered reproduction commands
  pass, including all eleven new ROMs and unchanged HLE/homebrew/media fixtures.
  The CI-runner registry assertion now requires 104 and explicitly includes the
  new builder; no fixture command was removed.
- `sh scripts/build-web.sh`: production Wasm/bindings pass; final measured build
  2.253 s. The accepted expanded ROM SHA-256 is
  `9e3729ce1a2a80ceaf22e010939ed7a14052a65fa75398b0a803753bcafc20c2`;
  the word-write negative ROM is
  `3f48f80f32b46a4ff7078bdf815cce1b4a2bf4908c5da364ea6b3b4314155bd3`.
- Same-toolchain inherited-main negative control: **five expected failures**;
  unchanged control source linked to the corrected native library: **5/5 pass**.
- `node --test scripts/ci-runner.test.mjs`: **13/13 pass**, 13.005 s, including
  deliberate browser-failure propagation and corrupted-artifact barriers.
- `npm run test:browser -- --group=hardwareBoundaries`: passes; expanded group
  3.110 s, total 4.463 s, 258 assertions. It loads actual local synthetic BIOS
  files and production Wasm; it does not inject CPU registers or guest results.
- Complete `npm run test:browser`: **74/74 groups pass** in pinned Chromium,
  32 shared groups (413.224 s) and 42 scenario groups (358.732 s),
  413.486 s total wall time. The standard two-worker aggregator verifies complete
  coverage and the exact production web-artifact digest. Existing BIOS-only UI,
  dual-CPU, HLE, backup, ISO/CUE/BIN/CHD, audio and video checks all pass.
- JavaScript syntax, relative documentation link targets and Git whitespace
  checks pass.

Earlier development checks exposed an old generic error-type assertion and a
bus-test helper doing a prohibited wide SMPC Debug read. Assertions now use the
specific SMPC error and the helper observes byte transactions at byte width.
The first runner attempt also exposed the deliberate fixture-registry count
change. The first web invocation lacked bindgen on PATH; the documented rustup
and CLI PATH resolved it. Browser setup initially lacked pinned revision1208;
the official Playwright download completed but Node26 extraction stalled. That
installer was terminated, and the exact official pinned headless-shell archive
was CRC-checked and extracted with its original file modes. Final browser checks
use that pinned executable, not a different browser version. Initial failures
are not counted as passing checks.

Hosted GitHub Actions, publication, live Pages, proprietary ROMs and real devices
were not invoked. This node must not push, open a PR, comment or merge. The original
private-BIOS transaction and general BIOS/menu/retail compatibility remain
unverified; no unavailable owner investigation is made a mandatory gate.

## Executed byte-investigation validation

The review follow-up used the same Rust 1.99.0 / Node 26.12.1 /
wasm-bindgen 0.2.129 / pinned Chromium 145.0.7632.6 toolchain. The new fixture's
expanded 524288-byte SHA-256 is
`4d8fc3fcafc6c8fa44d85a1b56f2a7a2cd3b3238302f1f0b9202b5b915dc96a4`.
All firmware bytes and expected records are original and covered by the fixtures'
MIT license. No runtime behavior or external publication state was changed.

- `cargo fmt --check`, `cargo build --locked`: pass.
- `cargo test --locked --test smpc_byte_contract`: **5/5 pass**, final focused
  run 0.62 s after the Clippy parenthesis correction.
- `python3 scripts/ci-time.py issue-103-byte-native cargo test --locked`:
  **1,178 pass**, zero failures/ignored, 105 harnesses; 430.56 s summed harness
  time, 472.259 s wall time including compilation. The final parenthesis-only
  test-helper clarification also passes the focused rerun above.
- In an isolated `git archive` of inherited `2a323e7`, copy the new test and
  `smpc-byte-audit.{s,hex,tsv}`, then run:

  ```sh
  CARGO_TARGET_DIR=target/issue-103/baseline-target cargo test --locked \
    --manifest-path target/issue-103/baseline-src/Cargo.toml --test smpc_byte_contract
  ```

  **5/5 pass**, final run 1.73 s. An additional bounded observation of the native
  firmware execution reports 933356 clocks, 12667 instructions, zero exceptions
  for all chunks and resets. This is differential evidence of accepted byte
  operations in inherited code; it is not a claimed failing byte negative control.
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`: pass.
- `sh scripts/check-fixtures.sh`: all **104** reproduction commands pass. The
  existing registered transaction builder now also reproduces the twelfth ROM;
  registry size and all prior fixtures remain unchanged.
- `sh scripts/build-web.sh`: production build/bindings pass, 13.746 s wall time
  including compilation. The inherited archive was independently built with
  `cargo build --locked --release --target wasm32-unknown-unknown` and matching
  `wasm-bindgen --target web --no-typescript`. A temporary Chromium harness called
  each module's exported `Emulator.reset_bios`, `run_steps`, Debug reads and
  clock/instruction counters with the same original ROM. All six combinations
  (inherited/current × chunks 1/127/10000) produce the identical 111 records,
  **933356 clocks, 12667 instructions, zero exceptions**, agreeing with native.
- `npm run test:browser -- --group=hardwareBoundaries`: pass, 4.240 s group /
  4.674 s total, **276 assertions**. The production UI uploads the original ROM;
  all chunks and two cold boots compare its actual guest-published results.
  Existing invalid-operation/width/IRQ/peripheral controls also pass.
- Complete `npm run test:browser`: **74/74 groups pass**, 32 shared groups
  (417.617 s), 42 scenario groups (364.935 s), 417.923 s total wall time.
  The two-worker aggregator verifies complete coverage and the production web
  artifact digest; BIOS-only UI, media, backup, audio/video, HLE and dual-CPU
  regressions all pass.
- `node --test scripts/ci-runner.test.mjs`: **13/13 pass**, 14.662 s.
- JavaScript syntax, relative Markdown link targets and Git whitespace: pass.

The initial runner invocation preceded the first web build and timed out waiting
for missing Wasm assets; the complete rerun above passed after building. The new
guest initially attempted an unsupported SCU IST write in its test setup. Its
final readiness loop polls SR.PDE and uses the documented IREG0 BREAK path;
existing IRQ guests retain real CPU interrupt acknowledgement. Native Clippy
initially requested parentheses in the test's BCD helper; the corrected native
and Wasm checks pass. These initial failures are not counted as passing checks.

## PR publication reference correction

Review found that PR #104's Related Issues section used a closing reference for
this partial increment. Replaced it with `Related to #103.` and removed the
contradictory closure-target explanation. Exact GitHub body readback confirms
the replacement, open/draft PR state and unchanged published head
`0eb49111f9182e9aef63a1150c831ed03e395432`. Checkpoint publication must preserve
the plain reference, as recorded in [the contributor guidance](../AGENTS.md).
Keep #103 open and PR #104 in draft; this correction does not resolve the
remaining acceptance criteria or explain the original one-byte fault.

Validation for this PR-body and documentation correction: exact PR metadata
readback, local Markdown links/anchors and Git whitespace checks passed. Runtime
files are unchanged, so runtime suites were not rerun; their earlier results
remain recorded above. No push or merge was performed.


## Review follow-up: timing-valid guests and reset observation

Related to #103. The earlier byte guest did not observe command issuance windows,
and the earlier reset test stopped on an immediate unsupported read. These two
coverage gaps are now replaced with executable evidence, without changing the
emulator's device behavior or inferring the private firmware's transaction.

ST-169 §2.2, printed pp.17–19, figures2.5–2.7 requires commands later than
VBlank-in + 300 µs; INTBACK and status-to-peripheral continue must precede
VBlank-out. Other commands sharing an INTBACK frame also precede VBlank-out.
The guests execute on the master at the reset NTSC 320 clock, without SYSRES
or PLL changes, so the separate post-reset figure2.6 waiting rule and table2.5 issuer restrictions are not exercised here.

The original byte-audit guest now enables DISP (ST-058 §2.5: otherwise TVSTAT
reports blank even during active display), observes active display then the next
VBlank-in, and measures elapsed time with coherent FRT high/low reads (SH7604
§11.2, FRC/TEMP). Reset FRT phi/8 is retained. A 1200-tick wait nominally
spans 9600 clocks; allowing a full tick of quantization still gives at least
9592 clocks, above the 8063 clocks for 300 µs at 26.8741 MHz. Each of thirty
command windows publishes three checks: elapsed >=1200 ticks,
elapsed <=2000 ticks, and TVSTAT.VBLANK=8. SF claim, parameters and COMREG follow
immediately in the same blank, well before its 39-line/66612-clock end. All
SETTIME/SETSMEM/status commands and eighteen port/optimization combinations now
execute this sequence. The original 111 result records plus ninety window
checks produce **201 records**. The larger 2,000,000-turn budget is necessary to
execute the thirty real raster transitions rather than omit or accelerate them.

The additional original [timed peripheral guest](../tests/fixtures/smpc-timed-peripherals.s)
executes status-only, status-plus-peripheral (with a continue in the same blank),
and peripheral-only INTBACK, including both optimization settings and repeated
optimized acquisition after measurement. Its **79 records** check the same window
bounds, SF/SR/OREG31, Japan region, both real released digital-pad packets
(F1/02/FF/FF), IRQ counts and cleared SCU bit7 after actual SH-2 vector47/RTE.
Six actual IRQ entries occur. Native and production Wasm use chunks
1/3/127/10000, with and without passive Debug queries, and compare complete
records, clocks/instruction progress and exception counts across all partitions.
The native test additionally compares the entire CPU state. These run-call
partitions do not alter device deadlines. Existing device/bus tests retain
before/at/after deadline and invalid-handshake/width negative controls.

Three original pending-command ROMs issue SETSMEM (12/34/56/78), SETTIME
(2024-10-31 12:34:56), or status-only INTBACK in a valid window, then publish
SF=1, the actual OREG31 command and a mailbox. Native tests verify pending state
and a positive remaining deadline after that mailbox and after 0/1/7 additional
scheduler turns. The same instructions and offsets execute in production Wasm.
The host then cold-resets into an original [observer ROM](../tests/fixtures/smpc-reset-observer.s),
which executes normally for 100,000 turns. It waits through a full raster
transition (over 390,000 clocks) **before** inspecting cancelled state, exceeding
all former remaining deadlines (at most 8600 clocks). It records SF=0,
OREG31=0, SR=0 and SCU bit7=0, then issues a timing-valid fresh status request
and checks the unchanged default calendar/RTC-set flag and zero SMEM. All twenty
records, zero exceptions, clocks and CPU/instruction progress agree across
commands, offsets and chunks 1/3/127/10000, with Debug observation. No fault or
host register injection prevents time from passing or supplies guest results.
Each case starts with a fresh machine so retained battery time from previous
cases cannot contaminate the independently specified default-calendar oracle.
The case itself resets the same machine, preserving the battery reset contract.

These tests establish valid-window execution and cancellation beyond former
deadlines in the functional model. They do not establish electrical timing,
invalid-window behavior, cache-enabled MMIO,
or private BIOS/menu compatibility. #103 stays open and #104 stays draft under
the partial publication rule; #17 and #81 scope is unchanged. Required post-merge
verification: none.


### Executed review-follow-up validation

Same pinned toolchain as the prior checkpoint. No runtime source changes, ROM
imports, Issue/PR updates, push or merge were performed. Reproduce all seventeen
original ROMs with `python3 tests/fixtures/build_smpc_transactions.py --check`.
The new/current expanded images are:

| Expanded 524288-byte original ROM | SHA-256 |
| --- | --- |
| smpc-byte-audit | `77939c738d2411666943c01c91871b9e96ef7590283d45527882c2ff24fb7774` |
| smpc-timed-peripherals | `59139be608b572036485728deaff66a344da63f0fb6024448f7fac887b4dd35e` |
| smpc-reset-pending-17 | `2620ea11fc6e30dfe61c8f9bc9881471f0bd13ffd60e2c5935535304cd0a46eb` |
| smpc-reset-pending-16 | `f1a4386dd47739001ac3f857815c5045db08bc50c682f9360afdd1cd15e4438d` |
| smpc-reset-pending-10 | `1bdafbf0338a2136c4e04cadbe866bf7e28db39315e43464977825f1e22bea3a` |
| smpc-reset-observer | `575c97f8a8bb6dac27ed2a7de463129cb3ed388998d4e7ccacdcdd958eedafaf` |

- `cargo fmt --check`, `cargo build --locked`: pass.
- `python3 scripts/ci-time.py issue-103-review-native cargo test --locked`:
  **1,179 pass**, zero failures/ignored, 105 harnesses; 514.42 s summed harness
  time, 575.595 s wall time including compilation and local contention.
  This includes all five `smpc_transactions` tests (5.87 s) and all five revised
  `smpc_byte_contract` tests (16.29 s).
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`: pass.
- `sh scripts/check-fixtures.sh`: all **104** registered commands pass. The
  transaction builder additionally passes its final focused reproduction check;
  registry size and prior generated fixtures are unchanged.
- `sh scripts/build-web.sh`: production Wasm/bindings build passes (Cargo release
  compilation 6.94 s). No changes were made to the tested web artifact afterward.
- In an isolated archive of inherited `2a323e7`, the exact revised byte-audit
  test/ROM/TSV pass **5/5** (38.29 s). The timing-valid INTBACK test uses only
  inherited public APIs after removing the new diagnostic query: **1/1** pass
  (10.42 s), including all 79 records and six actual IRQs for every partition.
  This remains a master-context no-rejection result; it does not cover the
  prohibited slave acceptance corrected below.
- `npm run test:browser -- --group=hardwareBoundaries`: pass, **408 assertions**,
  41.963 s group / 42.396 s total, using pinned Chromium 145.0.7632.6. UI uploads
  exercise the expanded byte/timed guests; cancellation cases instantiate fresh
  production Wasm machines and call their real `reset_bios`/`run_steps` exports.
- Complete `npm run test:browser`: **74/74 groups pass**, 42 scenario groups
  (397.083 s) and 32 shared groups (413.140 s), 413.415 s total wall time.
  The two-worker aggregator verifies complete coverage and the production web
  artifact digest. Existing BIOS-only UI, HLE, dual-CPU, backup, media and
  audio/video regressions all pass. The expanded hardware-boundary group also
  passes inside this full run (42.105 s).
- `node --test scripts/ci-runner.test.mjs`: **13/13 pass**, 14.212 s.
- JavaScript syntax, relative Markdown link targets and Git whitespace: pass.

Initial checks found an unsupported assembler spelling (`cmp/hi`, replaced by
`cmp/hs`) and a zero-turn native `run_steps` call (`StepLimit(0)`, replaced by an
empty step loop). The first browser invocation lacked the pinned shell; its
official installer downloaded but stalled during extraction. The installer was
stopped and the same official 1208 archive was CRC-checked/extracted with original
permissions. Initial cancellation runs also demonstrated correct retained battery
state/time from earlier cases; fresh per-case production instances now match
native setup while each cancellation resets that same instance. These development
failures are excluded from passing validation results.


## Review follow-up: shared native/Chromium progress evidence

Related to #103. Review correctly identified that comparing each backend only
with its own first run did not establish agreement between native and Wasm.
Both native test harnesses and `scripts/hardware-boundaries.mjs` now read the
same checked-in [progress record](../tests/fixtures/smpc-progress.tsv). Every
case asserts scheduler turns, main clocks, retired master instructions and
master exceptions against that shared record, in addition to the existing
independent guest-result TSVs. A backend with consistently different progress
now fails even if all its own partitions agree. The record was captured from
native `Machine` execution and checked through the production Wasm exports in
pinned Chromium; it is a functional-model regression baseline, not a claim of
measured hardware timing. No test regenerates it from its first run.

| Original guest | Scheduler turns | Native = Chromium main clocks | Native = Chromium master instructions | Exceptions | Guest result words |
| --- | ---: | ---: | ---: | ---: | ---: |
| byte-audit | 2,000,000 | 24,027,263 | 675,272 | 0 | 201 |
| timed-peripherals | 500,000 | 5,559,242 | 130,511 | 6 | 79 |
| reset-observer | 100,000 | 1,040,642 | 20,375 | 0 | 20 |

Each backend checks all three guests under chunks **1/3/127/10000**, with and
without passive Debug register/mailbox/fault observation between run calls.
Byte-audit includes two cold boots per combination (16 executions per backend).
Timed-peripherals has eight executions per backend. Reset-observer covers
pending commands 17/16/10, offsets 0/1/7, all four chunks and both observation
settings (72 executions per backend), with a fresh machine for each case and
cold reset of that same machine into the observer. Thus **96 executions per
backend** assert the shared progress values and all guest records. Existing
complete native CPU-state comparisons remain in place. No runtime source,
firmware bytes or guest expectations were changed.

The reported private one-byte fault remains unclassified and the other limits
in [Finding and scope](#finding-and-scope) remain. Keep #103 open, PR #104 draft,
and its Related Issues section `Related to #103`. This parity correction does
not authorize a closing reference or assert general BIOS/menu compatibility.


### Executed parity-follow-up validation

Local macOS arm64, rustup stable Rust 1.99.0, Node 26.12.1,
wasm-bindgen 0.2.129, Playwright 1.58.2 and pinned Chromium 145.0.7632.6.
Native and browser regression runs overlapped; these are local wall times,
not hosted-CI timings. No emulator runtime, guest ROM or web source changed.

- `cargo fmt --check`, `cargo build --locked`: pass (0.398 s / 3.983 s).
- `python3 scripts/ci-time.py issue-103-parity-native cargo test --locked
  --test smpc_transactions --test smpc_byte_contract`: **10/10 pass**, 56.034 s,
  including all 96 shared-record cases and existing device/negative controls.
- `python3 scripts/ci-time.py issue-103-parity-all-native cargo test --locked`:
  **1,179 pass**, zero failures/ignored, 105 harnesses, **531.450 s** including
  compilation. The only later Rust edit clarifies the byte-test module comment;
  final all-target Clippy also compiles that edit.
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`:
  pass, 5.457 s / 5.557 s.
- `sh scripts/check-fixtures.sh`: all **104** registered commands pass, 3.946 s.
  The progress TSV is an independent regression record consumed by tests,
  rather than a result generated by an emulator run during fixture checking.
- `python3 scripts/ci-time.py issue-103-parity-web sh scripts/build-web.sh`:
  production Wasm/bindings build passes, 6.562 s. Its bytes were unchanged
  during browser validation.
- `python3 scripts/ci-time.py issue-103-parity-all-browser npm run test:browser`:
  **74/74 groups pass**, **457.566 s** including the wrapper. The shared shard
  passes 32 groups in 428.489 s; scenarios passes 42 in 457.073 s. The aggregator
  verifies complete coverage and the exact tested production artifact digest
  (457.337 s suite time). `hardwareBoundaries` passes in **93.010 s**, including
  all 96 shared-record cases in Chromium and existing fault/width controls.
- `node --test scripts/ci-runner.test.mjs`: **13/13 pass**, zero skipped,
  13.574 s wall time (13.479 s reported by Node).
- Negative control: temporarily increase the main-clock column of the shared
  record by **one**, leaving emulator behavior and guest results untouched.
  `cargo test --locked --test smpc_transactions --test smpc_byte_contract`
  fails at the byte-audit shared-progress assertion (exit 101; first harness
  4 pass / 1 expected failure). `npm run test:browser --
  --group=hardwareBoundaries` fails at the reset-observer shared-progress
  assertion (exit 1). Both identify the common-oracle mismatch rather than
  accepting their own first result. Restore the record before the passing
  complete runs above; no mutation is committed.
- `node --check scripts/hardware-boundaries.mjs`, new relative documentation
  links/anchors and Git whitespace checks: pass.

The pinned browser download completed, but Playwright's Node 26 archive
extraction stalled. The installer was terminated; its exact downloaded archive
passed the system unzip CRC check and was extracted with original executable
modes. The executable reports 145.0.7632.6 and the passing suites above use it.
This setup interruption and the deliberate negative controls are excluded from
passing results. No hosted CI, personal firmware, live Pages inspection,
Issue/PR mutation, push or merge was performed.


## Review follow-up: command issuer

Related to #103. Re-read ST-169 §2.2, printed pp.19–20/table2.5 from the
original PDF. Master may issue all sixteen listed commands. Slave may issue
00/06/07/08/09/0D/18/19/1A/17/16, but not **02/03/0E/0F/10**. The table does
not specify electrical responses to prohibited issuance. The bounded policy is
`ProhibitedIssuer` at COMREG, before testing busy/SF and without changing the
prior command, OREG31, SF reservation, IREGs or deadline. Machine execution
retains this as a sticky host fault.
No convenient command result, ignored store or architectural SH-2 exception is
invented. This priority makes the prohibited CPU/command combination explicit
even if the guest also violates handshake requirements.

`Bus::write_from` retains the processor from `CpuBus` for direct transfers and
from the transaction owner at scheduled CPU/write-buffer/DMAC/reset-drain
completion. `Smpc::write_from` validates it at the device boundary. Existing
host `Bus`/device `write` helpers retain master semantics. No mutable global
"current CPU" is used; interleaved or deferred stores cannot inherit a different
processor's identity. Existing machine diagnostics identify the failing slave
and the effective address, write operand and command snapshot without extra reads.

The public bus test checks **128** command/issuer/alias combinations, all sixteen
commands from both processors over four aliases. It asserts all eleven permitted
slave commands still schedule, all five prohibited ones leave the full snapshot
unchanged immediately and after 8,601 clocks, and master can reuse the handshake
for CDOFF afterward. This supplements the historical master-context audit.

The [fixture builder](../tests/fixtures/build_smpc_transactions.py) now reproduces
six original MIT 512 KiB dual-SH-2 ROMs; no new fixture command or browser group
is added. Master sets a shared flag before SSHON releases slave's real reset
vector. Every command waits for a real VBlank + FRT delay over 300 µs using
the existing independently checked window routine. Slave executes permitted
SETSMEM in every guest. The allowed guest also executes SETTIME; master reads
the actual memory/calendar through its permitted INTBACK. The five other guests
attempt each prohibited COMREG byte store with SF=1 and valid 01/00/F0 IREGs.
They must stop as **Slave SH-2 / ProhibitedIssuer**, retain command/report 17,
SF=1 and no pending deadline, and never publish their success mailbox. Repeated
run calls retain the same fault and clocks. The CPUs have zero guest exceptions.

Native and production Wasm/Chromium run all six ROMs with chunks **1/3/127/10000**,
with and without passive Debug observation: **48 executions per backend**. They
assert identical clocks and both CPUs' retired instruction counts against the
same [native-captured progress baseline](../tests/fixtures/smpc-issuer-progress.tsv),
as well as independent result words. This is functional-model parity, not
physical timing evidence.

| Guest | Main clocks | Master instructions | Slave instructions |
| --- | ---: | ---: | ---: |
| allowed, after 250,000 scheduler turns | 2,167,640 | 66,856 | 24,811 |
| each of 02/03/0E/0F/10, at fault | 1,292,657 | 43,870 | 24,702 |

Negative control uses an isolated archive of inherited review commit
`75cf904b6486171711584484838519ebf9033178`, leaving the assigned worktree
unchanged. A standalone public-API slave probe with SF=1 and IREGs=01/00/F0
confirms that **all five** commands were accepted: pending 02/03 at 807 clocks,
0E/0F at 2,687,410 clocks, and INTBACK at 8,600 clocks. The independent rejection
assertion fails on that accepted list. Building that inherited source to
production Wasm and running the new `hardwareBoundaries` controls also fails:
the allowed case passes, but slave 02 continues executing instead of stopping,
and the shared fault-progress assertion detects the mismatch. No runtime,
private firmware, fixture expectations or committed progress values are modified
for either control.

The private one-byte fault remains unclassified and the other limits in
[Finding and scope](#finding-and-scope) remain. Keep #103 open, PR #104 draft,
and its Related Issues section `Related to #103`. No push, PR update, merge,
personal firmware, human result, hardware or live Pages check is required or
claimed by this correction.


### Executed issuer-follow-up validation

Local macOS arm64, final Rust checks on rustup stable **1.99.0**, Node **26.12.1**,
wasm-bindgen **0.2.129**, Playwright **1.58.2**, pinned Chromium **145.0.7632.6**.
Commands use the existing `scripts/ci-time.py` wrapper for wall times. Native
and browser runs overlapped; these are local measurements, not hosted-CI times.
The final cargo/web commands select the installed rustup stable binaries and
matching wasm-bindgen CLI through PATH.

- `cargo fmt --check`, `git diff --check`, `node --check
  scripts/hardware-boundaries.mjs`, relative documentation paths/anchors: pass.
- `cargo build --locked`: pass, **16.061 s**.
- `cargo test --locked --test smpc_transactions --test smpc_byte_contract`:
  **12/12 pass**, **66.072 s**, including all new issuer controls and previous
  byte/width/handshake/IRQ/reset/shared-progress checks.
- `cargo test --locked`: **1,181 pass**, zero failures/ignored, **105 harnesses**,
  **598.381 s** including compilation.
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`:
  pass, **8.285 s / 13.446 s**.
- `sh scripts/check-fixtures.sh`: all **104 registered commands pass**,
  **10.074 s**, including the six new ROMs under the existing SMPC builder.
  The shared progress TSV is an independent captured regression baseline,
  never generated by the fixture checker or seeded by each backend's first run.
- `sh scripts/build-web.sh`: production Wasm/bindings build passes,
  **12.614 s**; the production assets remain unchanged throughout browser runs.
- `npm run test:browser -- --group=hardwareBoundaries`: pass,
  **103.113 s** wrapper / **101.824 s** group time, including all 48 new
  native/Wasm issuer-oracle combinations and existing SMPC controls.
- `PLAYWRIGHT_BROWSERS_PATH="$PWD/target/issue103-browser" npm run test:browser`:
  final complete rerun **74/74 groups pass**, **451.180 s** wrapper /
  **450.944 s** suite time. Shared passes **32** groups in **408.260 s**;
  scenarios passes **42** in **450.666 s**, including `hardwareBoundaries`
  (**100.725 s**) and `largeMedia`. The aggregator verifies complete coverage
  and the exact tested production artifact digest. No runtime, browser test or
  production asset changed between the interrupted run and this passing rerun.
- `node --test scripts/ci-runner.test.mjs`: **13/13 pass**, zero skipped,
  **13.867 s** wrapper / **13.774 s** Node time.
- Inherited negative controls: `cargo test --locked --test issuer_negative
  -- --nocapture` fails as expected with all five prohibited commands accepted
  (**0.491 s**, exit 101). `npm run test:browser -- --group=hardwareBoundaries`
  on inherited production Wasm fails as expected on slave 02 progress
  (**5.233 s**, exit 1). The temporary probe/archive is outside the checkpoint.

Initial web attempts found wasm-bindgen off PATH and Homebrew Rust without the
Wasm standard library; selecting the already installed matching tools fixed both.
The first full browser attempt passed 73 groups, then Chromium closed while
opening `largeMedia` before its assertions. A fresh targeted launch showed the
pinned executable was missing from the shared Playwright cache. Restoring the
same official 145.0.7632.6 headless shell to a private ignored test directory
(with archive CRC verification and original executable modes) makes
`npm run test:browser -- --group=largeMedia` pass: **6.050 s** wrapper,
**5.234 s** group. These setup interruptions and deliberate negative controls
are excluded from passing suite results. No runtime/test policy was relaxed.


## Review follow-up: unaligned exception operands

Related to #103. The review found that `Memory::exception_write32` /
`exception_read32` and scheduler `ReplayMemory::exception_write32` split
unaligned longwords into byte operations before the existing SMPC width guard.
Thus `exception_write32(20100001,12003400)` succeeded and changed IREGs,
while the equivalent regular longword operand was rejected. This is another
width-policy bypass; it does not identify the private one-byte COMREG fault.

The complete exception operand now passes a side-effect-free validation hook
through Bus, both CPU views and replay before any byte transfer. The existing
SMPC guard checks every covered byte, including operands beginning before the
chip-select boundary and physical/cache-through mirrors. Rejections retain the
original address, four-byte width, direction, full write value and SMPC snapshot.
The hook defaults to no restriction for other Memory implementations. It leaves
SH7604 §§4.8.2–3's existing exact-address RAM data policy and delayed
architectural address-error handling intact; it does not assert physical Saturn
fault behavior for prohibited device widths.

Two original MIT 512 KiB ROMs are reproducible with the existing registered
`python3 tests/fixtures/build_smpc_transactions.py --check` command:

- `smpc-exception-stack`: reset SP=20100009, then TRAPA #32. The first SR
  push at 20100005 fails before IREG mutation or vector dispatch.
- `smpc-exception-vector`: reset into original code that sets VBR=200FFFE1,
  then TRAPA #32. RAM SR/PC pushes complete (F0/210), then the longword vector
  fetch at 20100061 fails. Its SR/even/SF/even bytes are individually readable,
  so a mistaken byte decomposition cannot hide behind a write-only rejection.

The native public-API regression also checks Bus and both processor views,
aligned and all three unaligned offsets, mirrors, select-boundary overlap, full
operand diagnostics, no IREG changes and unchanged RAM exception transfers.
Native and production Chromium tests use chunks 1/3/127/10000 with and without
Debug observation, unchanged success sentinels and sticky, stationary faults.
Both assert the checked-in shared stopping-point record: stack 66 clocks / 0
retired instructions; vector 133 clocks / 4 retired instructions, neither with
a completed exception. These counts describe this functional model only.

Before modifying runtime code, `cargo test --locked --test smpc_transactions
exception_` fails both new tests (exit 101, 8.111 s including compilation): the
public exception store succeeds and the stack guest produces no SMPC fault.
After correction, both focused tests pass (1.059 s). An initial test expectation
for the vector guest's saved PC was corrected to 210 from the assembled ROM;
no runtime behavior or fixture instruction was changed to fit that assertion.

The original symptom and remaining scope in [Finding and scope](#finding-and-scope)
are unchanged. Keep #103 open and PR #104 draft with `Related to #103`. No
personal firmware, human confirmation, live Pages, push or merge is required or
claimed.


### Executed exception-follow-up validation

Local macOS arm64, rustup stable 1.99.0, Node 26.12.1, wasm-bindgen 0.2.129,
Playwright 1.58.2 and pinned Chromium 145.0.7632.6. The existing
`scripts/ci-time.py` wrapper records wall time; native and browser execution
overlap. Rust commands select the installed rustup stable toolchain and matching
wasm-bindgen CLI with PATH. Browser commands use
`PLAYWRIGHT_BROWSERS_PATH="$PWD/target/issue103-browser"`.

- `cargo fmt --check`, `git diff --check`, `node --check
  scripts/hardware-boundaries.mjs`, changed documentation relative targets and
  new anchor: pass.
- `cargo build --locked`: pass, **4.263 s**.
- `cargo test --locked`: **1,183 pass**, zero failures/ignored across
  **105 harnesses**, **512.240 s** including compilation. This includes all
  **8** SMPC transaction tests and existing unaligned-RAM exception stacking,
  vector repair and dual-SH-2 regressions.
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`:
  pass, **5.696 s / 6.253 s**.
- `sh scripts/check-fixtures.sh`: all **104 registered commands pass**,
  **5.548 s**, including both new ROMs in the existing SMPC builder. Shared
  stopping-point expectations are checked-in native captures independently
  asserted in Chromium; the fixture checker does not generate them.
- `sh scripts/build-web.sh`: production Wasm/bindings build passes,
  **6.571 s**; production assets remain unchanged during browser validation.
- `npm run test:browser -- --group=hardwareBoundaries`: pass,
  **106.815 s** wrapper / **104.873 s** group, including all sixteen new
  guest/chunk/observation combinations and existing hardware controls.
- `node --test scripts/ci-runner.test.mjs`: **13/13 pass**, zero skipped,
  **14.271 s** wrapper / **14.178 s** Node time.
- `npm run test:browser`: **74/74 groups pass**, **461.625 s** wrapper /
  **461.366 s** suite time. Shared passes **32** groups in **412.789 s**;
  scenarios passes **42** in **461.055 s**. The aggregator verifies complete
  coverage and the exact tested production artifact digest.

Initial browser and runner attempts stopped because the shared Playwright cache
lacked its pinned executable. The ordinary private-directory installer stalled
after downloading the shell; it was stopped. Downloading the same official
145.0.7632.6 archive directly, verifying all ZIP CRCs and extracting with original
file modes restored the executable in the ignored private test directory. The
passing runs above use that version without runtime/test-policy changes; setup
interruptions and the deliberate inherited-code negative control are excluded
from passing results.


## Review follow-up: complete browser scheduling

Related to #103. The review's [hosted scenarios job](https://github.com/takahirox/saturn-rust/actions/runs/42002816210/job/127162428973)
reached the existing **1,200-second worker limit** in `scspMvol`, reporting
41/42 groups and never reaching `largeMedia`. Its expanded `hardwareBoundaries`
group passed in **276.543 s**, while the separate shared job passed all 32 groups
in **682.283 s**. These are actual hosted log measurements, not local estimates.

Move `hardwareBoundaries` after `cdRuntime` and `externalDiscovery` in the shared
worker, after closing the dependent UI trace's context. The existing isolated
module runner creates a fresh context for this group. The same two workers now
run **33 shared + 41 scenarios = 74 groups**, exactly once each. The independent
module selection and serial order remain available. No hardware assertion,
guest/chunk/Debug combination, expected value, guest clock budget, firmware byte,
20-minute worker limit, retry behavior or artifact/publication gate changes.
The inventory regression checks the new counts; the existing aggregate negative
controls additionally reject reports using the previous 32/42 group assignment.

This addresses the complete-suite scheduling finding only. The private one-byte
COMREG symptom remains unclassified, and the other limits in
[Finding and scope](#finding-and-scope) remain. Keep #103 open and PR #104 draft
with `Related to #103` in Related Issues. No Issue/PR change, push or merge is
performed by this follow-up.

### Executed scheduling-follow-up validation

Local macOS arm64, rustup stable Rust **1.99.0**, Node **26.12.1**,
wasm-bindgen **0.2.129**, Playwright **1.58.2** and pinned Chromium
**145.0.7632.6**. Commands below use `scripts/ci-time.py` for wall time.
Browser commands set `PLAYWRIGHT_BROWSERS_PATH="$PWD/target/issue103-browser"`.
The build selects the installed rustup stable and matching bindgen CLI via PATH.
Runner tests finish before the complete browser run; no native guest suite runs
alongside it. These are local results, not a new hosted-CI result or a controlled
before/after performance comparison.

- `npm ci --ignore-scripts`: pass, locked Playwright installed.
- `python3 scripts/ci-time.py issue103-browser-fix-web sh scripts/build-web.sh`:
  production Wasm/bindings build passes, **7.371 s**.
- `python3 scripts/ci-time.py issue103-browser-fix-runner node --test
  scripts/ci-runner.test.mjs`: **13/13 pass**, zero failures/skipped,
  **13.711 s** wrapper / **13.618 s** Node time. This includes real Chromium
  assertion failures, stale-success removal, crash/rejection/timeout/cancellation
  propagation, inventory checks, rejection of the old shard assignment, missing
  groups/reports, stale revisions, changed assets and publication gates.
- `python3 scripts/ci-time.py issue103-browser-fix-complete npm run test:browser`:
  **74/74 groups pass**, **510.659 s** wrapper / **510.420 s** suite time.
  Shared passes **33** groups in **510.151 s**; scenarios passes **41** groups
  in **356.693 s**, including `scspMvol` (**63.445 s**) and `largeMedia`
  (**5.093 s**). The moved `hardwareBoundaries` passes in **99.018 s**, with
  every existing guest/chunk/observation case and shared native/Wasm oracle
  assertion intact. Both workers complete within the unchanged 1,200-second
  budget. The aggregator verifies all 74 groups, revision and the exact tested
  production artifact digest. The site digest remains unchanged before/after:
  `1d5184b05916144ec70f1bcad69db09c7fc1dc389170aafbdf5d02971e79c089`.
- `cargo fmt --check`, `node --check scripts/browser-selection.mjs`,
  `node --check scripts/ci-runner.test.mjs`, changed documentation relative links
  and anchors, `git diff --check` and `git diff --cached --check`: pass.

The ordinary private-directory browser installer downloaded the pinned archive
but stalled during extraction. It was stopped; system unzip verified every ZIP
CRC and extracted the same archive with its executable modes. The executable
reports 145.0.7632.6. This setup interruption is excluded from passing results.
No Rust runtime, fixture, hardware test or production web source changed; the
full native, fixture and Clippy suites were not repeated for this scheduling-only
follow-up. Their previous results above are historical evidence. No new hosted
run, personal firmware, human result or live Pages inspection is claimed.


## Review follow-up: DMA and cache-refill read context

Related to #103. The review identified two calls to `external_read_group` that
bypassed complete-unit width validation. DMA longwords were split by the BSC
before checking the SMPC byte-only restriction, reporting a two-byte operand.
With CCR.CE enabled, a permitted byte read generated a cache-refill longword;
its fault instead described a decomposed two-byte transfer at the refill
address, losing the issuing operand's address and width.

`external_read_group` now validates the complete read unit before scheduling or
BSC decomposition. DMA faults retain their four-byte operand. The cache callback
retains the issuing operand in `AccessFault.address/width` and separately records
the generated longword in `AccessFault.refill`. Both the formatted host fault
and opt-in Debug snapshot distinguish these contexts without extra bus reads.
Unsupported refill units still fail explicitly; permitted byte accesses through
cache-through, disabled data refill or disabled cache retain their behavior.
This follows the existing ST-169 §1.2/fig1.3 byte-width boundary and SH7604
§8.4.1/fig8.4 longword-refill order; it adds no electrical/open-bus claim.

The focused [public-API tests](../tests/smpc_read_context.rs) exercise both
processors, physical/mirrored/cache-through aliases, all DMA first/last flag
combinations and successful RAM DMA. Cached byte reads cover every byte of an
OREG line plus SR/SF and disconnected lanes, the first refill address including
line wrap, explicit four-byte refill context, unchanged command state/SF,
formatted operands and retries without line population. The byte controls
independently compare cache-through, refill-disabled and cache-disabled reads
with the direct bus result.

Before changing runtime code, the two tests using only inherited diagnostic
fields both fail on the inherited checkpoint: DMA reports width **2**, and
the cached read at **00100020** reports **00100024 / width 2** instead of the
issuing byte. `cargo test --locked --test smpc_read_context` exits **101** in
**7.440 s** including compilation. After correction and adding assertions for
the separate refill field, both tests pass in **2.429 s** including compilation.
The negative control changed no emulator behavior or fixture expectations.

The private one-byte COMREG failure remains unclassified, and the other limits
in [Finding and scope](#finding-and-scope) remain. Keep #103 open, PR #104 draft
and its Related Issues section `Related to #103`. No human result, proprietary
firmware, hosted CI, live Pages verification, push or merge is claimed.

### Executed read-context validation

Local macOS arm64, rustup stable Rust **1.99.0**, Node **26.12.1**,
wasm-bindgen **0.2.129**, Playwright **1.58.2** and pinned Chromium
**145.0.7632.6**. The existing `scripts/ci-time.py` wrapper records wall time.
Rust/web commands select the installed stable toolchain and matching bindgen CLI
via PATH. Browser/runner commands set
`PLAYWRIGHT_BROWSERS_PATH="$PWD/target/issue103-browser"`. Native and browser
execution overlap; these are local results, not hosted-CI measurements.

- `cargo fmt --check`, changed documentation links/anchor and
  `git diff --check`: pass.
- `cargo build --locked`: pass, **4.020 s**.
- `cargo test --locked`: **1,185 pass**, zero failures/ignored across
  **106 harnesses**, **510.919 s** including compilation. This includes both
  new read-context regressions and the existing CPU/cache/DMA, SMPC guest,
  exception, issuer, command timing, IRQ, reset and media coverage.
- `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`:
  pass, **4.576 s / 5.294 s**.
- `sh scripts/check-fixtures.sh`: all **104 registered commands pass**,
  **4.082 s**. No fixture bytes or shared progress expectations change.
- `sh scripts/build-web.sh`: production Wasm/bindings build passes,
  **6.600 s**; production assets stay unchanged during browser validation.
- `node --test scripts/ci-runner.test.mjs`: **13/13 pass**, zero failures/skipped,
  **13.036 s** wrapper / **12.950 s** Node time.
- `npm run test:browser`: **74/74 groups pass**, **507.789 s** wrapper /
  **507.567 s** suite time. Shared passes **33** groups in **507.312 s**;
  scenarios passes **41** in **350.246 s**. The existing `hardwareBoundaries`
  group passes in **99.980 s**. The aggregator verifies complete coverage,
  revision and the exact tested production artifact digest.

The ordinary browser installer stalled during extraction after completing its
download. It was stopped; system unzip verified every ZIP CRC and extracted the
same pinned official archive with original executable modes into the ignored
private test directory. The executable reports **145.0.7632.6**. This setup
interruption and the deliberate inherited-code negative control are excluded
from passing results. No runtime or test policy was relaxed.

## Review follow-up: balance both hosted browser workers

Related to #103. The [reviewed shared job](https://github.com/takahirox/saturn-rust/actions/runs/42011452118/job/127191666435)
spent **724.096 s** across the dependent UI groups, **218.265 s** on `cdRuntime`
and **186.887 s** on `externalDiscovery`. It began `hardwareBoundaries` with
only about 70 seconds left and hit the unchanged **1,200-second** timeout.
The [scenarios job](https://github.com/takahirox/saturn-rust/actions/runs/42011452118/job/127191666609)
passed all **41 groups** in **599.105 s** (599.558 s including its wrapper).
Moving only `hardwareBoundaries` to shared had therefore left that worker
overloaded; the previous local pass did not establish hosted success.

Move `cdRuntime` and `externalDiscovery` back to scenarios. Shared retains the
30 dependent UI groups followed by isolated `hardwareBoundaries`, for a
**31/43 split covering all 74 groups exactly once**. Using the observed timings
above and the earlier complete hosted `hardwareBoundaries` time of **276.543 s**
gives estimates of **1,000.639 s shared / 1,004.257 s scenarios**, about 195
seconds below each worker's limit. These estimates combine separate runs;
they are scheduling evidence, not a measured successful hosted run.

Both moved modules already use fresh browser contexts. Preserve every assertion,
guest, original serial order, opt-in module selection, two-worker cap, timeout,
failure propagation and artifact/publication gate. Update the existing inventory
regression and its stale-assignment negative control to reject the old 33/41
reports, even when their combined coverage still contains all 74 names.
No emulator, fixture or production web source changes.

### Rebalanced-worker validation

Local macOS arm64, stable Rust **1.99.0**, Node **26.12.1**, matching
wasm-bindgen **0.2.129**, Playwright **1.58.2** and pinned Chromium
**145.0.7632.6**. The existing `scripts/ci-time.py` wrapper records wall time.
Rust/web commands select the installed stable toolchain and matching bindgen
CLI through PATH. Runner/browser commands set
`PLAYWRIGHT_BROWSERS_PATH="$PWD/target/issue103-browser"`. Runner tests finish
before the full browser run, with no concurrent native guest suite.

- `npm ci --ignore-scripts`: pass, locked dependencies installed.
- `python3 scripts/ci-time.py issue103-rebalance-web sh scripts/build-web.sh`:
  production Wasm/bindings build passes, **6.906 s**.
- `python3 scripts/ci-time.py issue103-rebalance-runner node --test
  scripts/ci-runner.test.mjs`: **13/13 pass**, zero failures/skipped,
  **12.641 s** wrapper / **12.552 s** Node time. Includes inventory, stale
  assignment, missing coverage, revision/digest mismatch, real Chromium
  failures, timeout/cancellation and publication-gate regressions.
- `python3 scripts/ci-time.py issue103-rebalance-complete npm run test:browser`:
  **74/74 groups pass**, **494.007 s** wrapper / **493.798 s** suite time.
  Shared passes **31** groups in **356.174 s**, including `hardwareBoundaries`
  (**100.800 s**). Scenarios passes **43** in **493.562 s**, including the
  moved `cdRuntime` (**80.196 s**) and `externalDiscovery` modules. Both workers
  finish within the unchanged 1,200-second limit. The aggregator verifies all
  groups, revision and exact tested production artifact digest. The site digest
  remains unchanged before/after:
  `02e74246dc5922e3ab9a689b3a97b30b94411f2f5dd039dc70b1d0a6460da673`.
  These are local results, not hosted-CI timings.
- `cargo fmt --check`, `node --check scripts/browser-selection.mjs`,
  `node --check scripts/ci-runner.test.mjs`, documentation anchor and
  `git diff --check`: pass.

The installer completed the pinned headless-shell download but stalled during
extraction. It was stopped; system unzip verified every ZIP CRC and extracted
that same official archive with executable modes into the ignored private test
directory. The executable reports **145.0.7632.6**. This setup interruption is
excluded from passing results. Native, fixture and Clippy suites were not
repeated for this scheduling-only change; their earlier results are historical.

Hosted verification of this revision remains pending: this node must commit
locally without pushing, so it cannot trigger the PR workflow with its changes.
After publication, require both hosted browser jobs and the aggregate `build`
gate to succeed; do not treat the timing estimates or local suite as that result.
Keep #103 open and PR #104 draft with `Related to #103` in Related Issues while
the limits in [Finding and scope](#finding-and-scope) remain unresolved.
