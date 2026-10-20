# Issue #15 address-error checkpoint validation

**Result: partial implementation; the umbrella Issue #15 is incomplete.** Related
to #15. No hardware completion checkbox is satisfied by this checkpoint alone.
No push, PR, merge, comment, Issue-state change or deployment was performed.

## Inputs and implemented scope

Read live [Issue #15](https://github.com/takahirox/saturn-rust/issues/15) through
`gh issue view`, the repository instructions, development/review guidance,
architecture and conformance matrix. `git ls-remote origin refs/heads/main`
confirmed upstream `1327d81dfd1a011b69990fd26abf4cefd04a9a52` on 2026-10-21.
The assigned checkpoint was `6f219a42da4a43703214dc8fab3dc23d4c798ccc`; the
readiness input supplied no technical diagnosis. All deliverables are in the
assigned worktree.

Implemented CPU address-error vector 9, a pending latch, operand-cycle checks,
instruction completion and delayed-branch deferral. Stack entry preserves SR and
saves the next PC; illegal-instruction dispatch remains separate. Ordinary
missing hardware, ROM writes and failed exception entry retain bounded sticky
diagnostics. Native/Wasm expose the pending latch for deterministic assertions.

Primary behavioral source: original
[Hitachi SH7604 Hardware Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf),
§4.3.1 Table 4.6 (invalid cycles), §4.3.2 (instruction completion and entry),
§4.6 Table 4.10 (deferred acceptance), §4.7 Table 4.11 (stacked PC/SR),
§4.8.3 (remaining nested/misaligned stacking limitation), and §8.3 Table 8.2
(address-space partitions). The PDF was inspected using temporary extraction
tools outside the repository. No reference-emulator source/core, Sega firmware,
SDK code or manual content is bundled.

## Executed checks (2026-10-21)

Used Rust 1.99.0 stable through the consistent compiler/Cargo PATH documented in
[setup](setup.md), installed Wasm target and wasm-bindgen CLI 0.2.129. `npm ci`
installed the locked Playwright dependency; existing headless Chromium was used.

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 70 native tests, none ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `sh scripts/build-web.sh` | Passed: original ISO, release Wasm and bindings |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `python3 homebrew/star-collector/build.py --check` | Passed: 131072-byte ISO unchanged |
| `npm run test:browser` | Passed: Chromium 145.0.7632.6, real Wasm under `/saturn-rust/` |
| `node --check web/app.js` / `node --check scripts/browser-smoke.mjs` | Passed |
| Relative Markdown links/anchors | Passed: 59 links across six updated documents |

The initial implementation correctly invalidated the previous tests' assumption
that unaligned CPU accesses are host diagnostics. Those diagnostic fixtures now
use aligned unmapped accesses or ROM writes, with new vector-9 tests covering
the replaced behavior. The first new BIOS test expected 57 retirements; manual
recount of its executed branches gives 55 (11 main + 14 + 14 + 16 in handlers).
Clippy caught a single-element loop left by the test update; it was removed.
All reported successful checks ran after these corrections. Hosted CI was not
executed by this node; the existing workflow invokes the expanded suites.

## Independent guest and semantic evidence

- [Nine CPU address-error tests](../src/cpu/address_tests.rs) cover word/long
  misalignments in 26 instruction/addressing forms, unchanged neighboring bytes,
  deterministic invalid-read data, predecrement/postincrement and alias effects,
  on-chip widths, PC-relative/TAS restrictions, odd/peripheral fetches, IRQ/NMI
  exclusion while pending, branch deferral, RTE/SLEEP and sticky entry failure.
- The MIT-licensed [original synthetic BIOS](../tests/fixtures/sh2-address-errors.hex)
  boots via the actual external-ROM reset path. Its guest handler writes PC/SR
  pairs `[0x108, 0xF0, 0x112, 0xF0, 0x11B, 0xF0]` to work RAM, repairs an odd
  stacked PC and executes RTE. Native and browser tests assert this same golden
  trace, 55 retired instructions, three exceptions, restored SP/SR, ordinary
  register/auto-update results, bounded sleep and reset reproducibility.
- [Native integration](../tests/sh2_address_errors.rs) and the
  [browser suite](../scripts/browser-smoke.mjs) also reset before accepting a
  pending error and verify its latch is discarded. JS only loads, steps and
  inspects; the guest computes and writes all handler results. There is no
  fixture-name, PC or game-specific implementation path.
- A second original browser vector executes an erroneous RTE followed by SLEEP.
  Debug Step/Run remain enabled while the synchronous error is pending; stepping
  then reports bounded failure from the still-misaligned exception stack.
  Sleeping without a pending error continues to disable the controls.
- Existing Star Collector movement/scoring/completion/restart, backup atomicity,
  reset retention, two digital ports, focus release, local-file privacy and
  project-subpath packaging checks still pass.

The hex source is the reproducible guest build input: expand its address/word
records into a zero-filled 524288-byte buffer in big-endian order, as both tests
do. It requires no external assembler, assets or firmware. License:
[tests/fixtures/LICENSE](../tests/fixtures/LICENSE), covering source and generated
fixture. SHA-256:

| Artifact | SHA-256 |
| --- | --- |
| `sh2-address-errors.hex` source | `2a40af262e5e4ab366edf1e444c00be94091ffd7cb75a91081fd246f77e35aa9` |
| Expanded synthetic BIOS | `bb914d20caa0d5c9a7d52d0b396435183e0cf41f1111df7fcfa58f7f7ce7de63` |

## Limitations and remaining work

Invalid reads return zero and invalid writes/device transactions are suppressed
while instruction effects complete. This tested deterministic policy is not a
physical invalid-cycle bus-data claim. Pipeline fetch contents, misaligned
exception stacking/undefined writes and nested address-error inhibition remain
unsupported; exception-entry failures remain sticky diagnostics. See the
[conformance matrix](hardware-conformance.md#address-error-scope-and-deterministic-deviations).

All umbrella categories remain incomplete: full SH-2/SH7604 and slave CPU;
MC68EC000; full bus/cache/reset/backup services and hardware-time scheduler;
SCU/DMA/DSP; VDP1 and advanced VDP2 composition; SCSP/DSP/CD-DA/WebAudio;
runtime CD, BIN/CUE, BIOS/HLE services; full SMPC/analog input; both required new
licensed guest demonstrations; and complete inventory-linked native/Wasm/guest
coverage. The synthetic BIOS is a diagnostic, not either new demonstration or a
complete boot-service fixture. Genuine BIOS/retail compatibility is **not
independently confirmed** and is optional.

Current guidance now follows #15 instead of the superseded #13 tracker and removes
old workflow-owner, personal-BIOS and manual deployed-site gates. Continue pending
technical implementation without waiting for those inputs. No human-dependent
post-merge verification is required. External administrative closure is not
hardware evidence; do not reopen #11/#13 or claim #15 complete on this checkpoint.

## PR review correction (2026-10-21)

Related to #15. Updated [PR #16](https://github.com/takahirox/saturn-rust/pull/16)
to remove the Issue-closing reference and its associated draft publication
warning. Retained the plain related-Issue reference, partial outcome and full
remaining-scope description. Keep that corrected description during subsequent
publication; this checkpoint does not complete the umbrella Issue.

Read back the live PR metadata and verified the exact corrected body, unchanged
head commit, draft status and unmerged state. Rechecked upstream `main` at
`1327d81dfd1a011b69990fd26abf4cefd04a9a52`. This revision changes only PR metadata
and this validation record; the runtime suites above were not rerun. Markdown
inspection and `git diff --check` passed. No push or merge was performed.
