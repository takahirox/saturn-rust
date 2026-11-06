# Idle SMPC COMREG with SF=0 (#109)

## Outcome and boundary

An otherwise valid, permitted byte COMREG write is now accepted while the SMPC
is internally idle with either SF=0 or SF=1. COMREG never implicitly sets SF.
The existing dispatcher validates parameters, latches command/OREG31 and schedules
the original deadline; completion clears SF and applies the same effects.
`busy()` still describes pending work and INTBACK waits, independently of SF.
The obsolete `MissingSf` diagnostic and its two fault fixtures are retired.

This is a deliberately bounded emulator compatibility interpretation, supported
by third-party emulator precedents, **not verified physical Saturn electrical
behavior**. No proprietary firmware or trace was supplied. The unavailable BIOS's
preceding SF writes, transaction history and stock BIOS/menu compatibility remain
unknown. No ROM/PC-specific bypass, fake rendering, guest-memory injection or CPU
exception change is introduced. #17 remains open; #81's endpoint finding is
unaffected. Personal BIOS testing is optional feedback, with no post-merge gate.

## Evidence and policy

- [Sega ST-169](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-169-R1-072694.pdf),
  §§1.2, 2.2–2.3: software checks SF=0, writes SF=1, then issues COMREG;
  completion clears SF. OREG31 reports the command. This documented sequence
  remains supported. The manual does not conclusively establish electrical
  rejection or acceptance when the pre-write is omitted. The detailed primary
  contract reconstruction remains in [#105](validation-issue-105.md#primary-contract).
- [Mednafen SMPC](https://github.com/OpenEmu/Mednafen-Core/blob/master/mednafen/ss/smpc.cpp),
  `SMPC_Write`: normalized COMREG offset 0x0F queues without testing SF; SF has
  its own write path. These normalized offsets differ from Saturn byte offsets.
- [5thPlanet SMPC](https://github.com/hiroshiyui/5thPlanet/blob/main/crates/saturn/src/smpc.rs):
  COMREG queues independently of SF. Its [debugging case](https://github.com/hiroshiyui/5thPlanet/blob/main/doc/debugging-playbook.md)
  reports a SNDOFF single-read/spin hang caused by an automatic SF set. That is
  compatibility evidence, not proof of the unavailable user's BIOS sequence or
  an independent hardware measurement. No implementation text was copied.

Overlapping COMREG still returns deterministic `Busy`: accepting another command
would replace a pending deadline or corrupt an INTBACK phase. SF=0 cannot make
that work disappear, and setting SF during work cannot reschedule it. This
bounded overlap diagnostic is emulator policy, not a claimed SH-2 architectural
exception or measured prohibited-use electrical response. Invalid commands and
parameters, master-only issuer restrictions, invalid SF values, byte-only widths,
disconnected even lanes, write-only reads and read-only writes retain their
separate behavior. INTBACK's existing SF status/continue transitions are retained;
SF=0 acceptance does not rewrite its multi-phase handshake.

## Acceptance matrix

| Issue criterion | Evidence |
| --- | --- |
| Minimal source change | `src/smpc.rs` removes only the SF guard and uses the existing validated dispatcher after issuer/busy checks. No change to command clocks, completion, reset or `busy()`. `src/smpc/diagnostics.rs` removes the unreachable rejection. |
| RESDISA effects | Original `smpc-idle-zero` and `smpc-idle-prewrite` firmware issue 1A via P1 2010001F, record SF/report, and expose a pending-command mailbox. Native observes real pending/deadline and cold reset cancellation. RESENAB followed by RESDISA changes INTBACK.RESd from 00 to 40. Unit tests assert NMI state before/at the unchanged 807-clock deadline. |
| SNDOFF and other commands | Same guests install an original 68k STOP program, issue SNDON/SNDOFF and CDOFF/CDON, read SF once immediately after each issue, and record real INTBACK sound/CD status. The zero-SF SNDOFF has an explicit read-once/spin control that would remain stuck if COMREG set SF. Zero variant never writes SF. Records require SF=0 during execution and continued progress through all commands. SF=1 control observes 1 before and 0 after completion. |
| Negative controls | New guest faults cover busy with SF=0 and SF set independently during work, plus unsupported/invalid-parameter bytes with SF=0. Existing SF=1, invalid SF, widths, COMREG read, CPU exception-entry, DMA/cache and lane controls remain. Both CPU views check every command and table2.5 issuer restrictions with both SF values. |
| Reset/INTBACK invariants | Unit tests cover all 16 valid command deadlines at chunks1/3/127/10000, unchanged effects until deadline, no double completion/IRQ and independent SF claims. New no-prewrite INTBACK checks status, continue, BREAK, VBlank waits, active collection and reset cancellation. Existing native/Wasm peripheral, timed reset and SCU IRQ guests remain. |
| Native and production Wasm/browser | Existing transaction builder produces original MIT 512 KiB firmware, assembly and authored TSV oracles. Native and Chromium execute identical bytes and compare all 114 records, seven actual IRQs, chunk invariance and reset. BIOS-only offline Play Start/Restart executes both variants without a disc. Existing audio/media/UI tests remain in the complete suite. |
| Documentation | This record, conformance matrix and historical #105 notice distinguish manual requirements from the compatibility policy and preserve wider limitations. |
| Required validation | Commands/results below. Exact-head hosted PR checks and normal review remain mandatory before merge; this worktree node does not push or create a PR. |

## Fail-before evidence

Before editing the shared model, the new native firmware regression failed on
its first SF=0 RESDISA at guest PC 000002A8 (not the private BIOS's PC):
`2010001F`, width1, value1A, `MissingSf`, SF=false, busy=false, command/report=0,
pending=None, main clock392856. The completed version of the same regression
passes. Guest code is authored here and redistributable under the adjacent
[MIT license](../tests/fixtures/LICENSE).

The status records use a separate RAM region from the IRQ counter and pending
mailbox. Completion waiting uses real raster/FRT progress and the next serviced
SCU IRQ, so already-clear SF is never misread as completion. No host writes set
flags or forge results. The inherited direct/activity RESDISA command-literal
mutations still prove that substituting RESENAB changes the actual RESd result.

## Executed validation

Local environment: macOS arm64, stable Rust 1.99.0, Node 26.12.1,
wasm-bindgen 0.2.129, Playwright 1.58.2 and its pinned Chromium
145.0.7632.6. All required local checks passed:

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Passed. |
| `cargo build --locked` | Passed. |
| `cargo test --locked` | Passed: 1,195 tests, zero failures or ignored tests, plus empty doctests. |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed. |
| `sh scripts/check-fixtures.sh` | Complete registry passed. The final one-shot SNDOFF fixture also passed `python3 tests/fixtures/build_smpc_transactions.py --check`. |
| `sh scripts/build-web.sh` | Passed, matching bindgen CLI and production release Wasm. |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed. |
| `node --test scripts/ci-runner.test.mjs` | Passed: all 13 runner regressions, no skips. |
| `npm run test:browser` | Passed: all 74 groups, 33 shared and 41 scenarios, with coverage/revision/artifact aggregation. Parallel elapsed 446.522 s; shared 446.177 s, scenarios 407.268 s. Existing audio/media/BIOS-only UI assertions passed. |

The final native idle-firmware regression also passed separately after adding
the explicit single-read/spin control.
`npm run test:browser -- --group=hardwareBoundaries` also passed after the final
diagnostic assertions: one group in 62.247 s, covering SF=0/SF=1 without repeating
unrelated modules. New native coverage uses one canceled boot and one complete
boot per variant/chunk, avoiding redundant full replays.

Browser commands used `PLAYWRIGHT_BROWSERS_PATH=target/playwright-browsers`.
Initial runner launch failed because its pinned Chromium was absent. The standard
installer stalled during extraction under this Node installation; the official
pinned headless archive was extracted locally into ignored `target/`, then the
unchanged runner and complete suite passed. No repository tooling, timeouts or
browser assertions were altered for that environment repair.

These are local worktree results. **Exact-head hosted PR checks and normal review
remain required before merge**; no push, PR, comment, merge or Pages inspection
was performed by this implementation node.

No tests are dropped, timeout raised or artifact/publication checks weakened.
The historical missing-SF rejection checks are replaced by the required positive
and overlap controls. No post-merge verification is required.
