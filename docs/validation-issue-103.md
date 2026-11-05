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

There is no evidence here that a documented, correctly issued **byte command**
is rejected. The original failure must not be labeled a COMREG read or attributed
to a specific command. A synthetic test of each candidate now distinguishes these
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

The scope is therefore independently reconstructed SMPC access semantics,
rejection diagnostics and a proven width-policy correction. The original byte
failure is unverified. Keep #103 open if tracking resolution of that symptom;
do not generate a closing reference from this checkpoint. Broad hardware
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
| SF +63 | R/W; SH-2 may only set with 01; SMPC clears on completion; upper read bits undefined (§1.2 p.6) | Read returns bit0 with upper bits deterministically zero. Writes other than 01 fail `InvalidSf`; this boundary is policy, not a claim that hardware raises an exception. Setting SF alone does not start a command. |
| IREG0–6 +01…0D | Byte, W parameters; INTBACK IREG0 also continue/break (§3.1/table3.2) | Read fails `WriteOnly`. Input changes while busy fail except implemented INTBACK continue/break. Rejected writes do not change the pending command or deadline. SETSMEM copies actual guest operands. |
| OREG0–31 +21…5F | Byte, R results; OREG31 identifies command at processing start (§2.3 p.21/§2.4 p.35) | Writes fail `ReadOnly`. SF distinguishes active/completed command reports. Status publication preserves INTBACK code. |
| SR +61 | Byte, R, readable independent of command (§1.2); RESB sampled at VBlank-in | Writes fail `ReadOnly`; existing status/peripheral/reset-button behavior retained. |
| Word/longword | Only byte register accesses specified (fig1.3) | `ByteOnly` before any lane side effect, including CPU aliases and physical DMA bus paths. A CPU operand alignment error still follows SH7604 table4.6 through the existing operand checker. No fabricated guest exception for unsupported device behavior. |
| Disconnected even byte lane | No register on this lane; electrical value not specified by fig1.3 | Existing deterministic FF read/ignored byte write policy retained, confined to disconnected lanes. It does not apply to odd COMREG reads. |
| Type B/C commands | No SMPC result IRQ; SF/OREG31 report completion (table2.4) | Existing reset/power/RTC/SMEM effects and deadlines retained. CDOFF/CDON use 1075 reference clocks (40 µs rounded up), SETSMEM 1075, SETTIME 1882; CPU/NMI/reset-enable commands use 807 (30 µs). These are deterministic functional deadlines, not measured oscillator/internal-task latency. |
| INTBACK type D | IREG0=00/01, IREG2=F0; reserved port mode prohibited; status/peripheral readiness interrupts (§2.4.2 pp.37–44, §3) | Existing 8600-clock status bound, SF/OREG/SR publication, SCU bit7, VBlank collection, continue/break retained. SF completion is not SCU acknowledgement. Native/Wasm guest observes pending IRQ, then services it through the real SH-2 vector. |
| Reset/power | Power-on domains table1.1; SYSRES/clock commands §§2.3.8–10 | Cold reset cancels pending commands, snapshots and results. Existing battery/backup retention, SYSRES, PLL, sound/CD and dual-SH-2 tests remain the wider evidence; this change does not reclassify their timing approximations as full conformance. |

ST-169 does not define the electrical return value of a write-only register read,
invalid SF write, dual command issue, or a prohibited wide access. Explicit,
sticky host failure remains the policy. It is not an SH-2 architectural memory
exception, and no open-bus assertion is inferred from the board address map.
The model still does not enforce every documented command-issuance window or
CPU-specific command limitation in table2.5. This checkpoint does not claim full
SMPC or stock-firmware conformance.

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
reproduces the source `.s` and sparse big-endian `.hex` representation of eleven
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
controls. The new reset test interrupts guest-issued SETSMEM at three pending
points and verifies cold boot cancels it. Existing SMPC, clock-domain, dual-CPU,
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

## Executed validation

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
