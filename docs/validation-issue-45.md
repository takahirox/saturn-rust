# Issue #45 validation: CD report validity and retained position

Related to #45 and #17. Baseline main `2220cf841c6d9138e692f09f60da6d0f6031f668`
was supplied through GitWeave checkpoint `77519eb3b9730cefe4f06032c7183587994e6ab3`.
Validation date: 2026-10-24. No push, PR, merge, comment or Issue mutation is
part of this node. Required post-merge verification: none.

## Contract and implementation

The [contract](cd-drive-reports.md) was written from the primary manuals before
new test execution. Later Sega §7.2.5 reports and §7.2.4 Q correspond to
ST-162-062094 §7.2.4 pp.59–61 and §7.2.5 p.62. Both have the same invalid-report,
home/error recovery and ordinary standby rules. §4.2 pp.37–38 supplies stop,
repeat retention and lead-out behavior; §4.3 p.39 permits Q in every drive state.
The task's abbreviated PDF citation is clarified here: printed p.61 contains
retention notes; Q begins on p.62. No behavioral or acceptance correction to the
source Issue was necessary, and the execution instruction prohibits Issue mutation.

`CdBlock` retains pickup/range/repeat data separately from public FF bytes and
records the distinct Seek Home origin. A current-position PAUSE from home seeks
to 150, while ordinary standby and ERROR recover their retained positions. Home
recovery also restarts the audio sample offset and de-emphasis history. Explicit
movement clears the home marker, including file/directory drive paths; mounting
new media resets its repeat/range origin. An unchanged maximum repeat preserves
its finite limit and notification count. Changing range or maximum clears the
notification count. Q returns a five-word transfer in OPEN/NODISC and other
supported states, shares validity/track metadata with reports, and identifies
track-AA lead-out at disc-end completion. Command payloads and startup signature
remain separate from generic report serialization.

Existing contradictory assertions were corrected with primary references in
[CD registers](../tests/cd.rs), [directory schedule](../tests/cd_directory_schedule.rs),
[drive guest](../tests/cd_drive.rs), [retry guest](../tests/cd_decode.rs), its
[builder](../tests/fixtures/build_cd_decode.py) and browser assertions. ERROR now
exposes `09FF FFFF FFFF FFFF`, and OPEN exposes `06FF FFFF FFFF FFFF`.
All retry counts/deadlines, no-corrupt-sector publication, workspace/capacity,
received/corrected data and recovery checks are retained. The finite retry test
additionally requires PAUSE recovery to the previously failed FAD.

## Independent native vectors

Nine [register/Machine tests](../tests/cd_reports.rs) use literal expected words
and Q data, never production report/position helpers to generate expected values:

- Startup signature and every generated drive state: PAUSE, STANDBY, PLAY, SEEK,
  SCAN, OPEN, NODISC, RETRY and ERROR. Both standby origins start away from 150;
  ordinary standby retains 181, home recovery targets 150. Q is transferred in
  each state. Existing CUE vectors retain audio, track/index and CONTROL coverage.
- Nonzero repeat notifications 1/2 survive timeout, home, open/close and seek.
  Unchanged maximum/range retain the finite limit: playback stops after the
  second repeat rather than looping indefinitely. ERROR masks a noninitial 182
  and a nonzero count 1, then PAUSE restores both on the same damaged medium.
- Literal before/equal/after seek, command and periodic publication boundaries;
  command completion wins a coincident periodic deadline, pending queries
  suppress replacement, CR4 release affects only subsequent publication, SCDQ
  continues through a held response, masking and write-zero acknowledgement are
  independent. Generic reports never replace hardware-information payloads.
- Software reset drops Q transfers and home state, power cancellation before/at/
  after query completion prevents stale data, and cold defaults retain the
  signature. Existing executing SMPC CDOFF/CDON tests remain in the full suite.
- Chunks 1/3/17/10000 agree with passive CR4/FIFO peeks; dedicated recovery vectors
  compare both with/without passive reads. `CdBlock::peek16` neither consumes Q
  words nor releases CR4; architectural reads retain both protocol side effects.
- Disc-end PAUSE and Q identify `41AA 0100 00FA`; home recovery on audio media
  starts at sample 0/FAD 150 instead of retaining an old within-sector offset.

## Original executing guest and media

The original [MIT](../tests/fixtures/LICENSE)
[builder](../tests/fixtures/build_cd_reports.py) reproduces the
[ROM](../tests/fixtures/cd-reports.hex), CUE and two 100-sector XA images.
Expanded 512-KiB ROM SHA-256:
`3d2bb8c8dd3a4f92120ff71030bcce1d7257b055954aa9b14a361bfa7230f9b1`.
Original/changed media SHA-256:
`f1d0d0927f59d65e24e4801053f5a6b9554a41b489271bd27a8268d6ab5ec304` /
`8f40fffd5a354f01666aaef83a74fafc416cdb9b593b1662f851bd39e06e93cf`.
The registered builder verifies each byte, including original generated EDC/ECC
and the deliberate error at 173; both variants use the same damaged position.

Native Machine and [production Wasm](../scripts/cd-reports.mjs) execute identical
bytes. Instructions issue commands, poll HIRQ/drive status, transfer all five
words of six Q records and compute their sums. They recover from home, open,
exhausted retries at 173 and read a good sector at 174. Hosts load ROM/media,
advance execution and read records only; they never supply running records or
substitute command results. FAD operands 181/187, media words 6969/A6A6 and an
omitted-home opcode produce independently predicted changed reports/Q/data.
Each variant runs twice on cold resets, with identical complete records.
The omitted-home control retains 181 rather than 150, distinguishing actual guest
command execution from a canned result. Native and browser checks assert full
home/OPEN/ERROR reports, recovered positions, every Q word, guest sums and actual
sector data/counts. The browser runs rebuilt release Wasm at `/saturn-rust/`.

## Executed negative controls

Final production source bytes were saved and restored in `finally` blocks. Negative
modules were generated into separate static-site copies; production `web/pkg`
was never overwritten by a negative module. Both tests retain the positive guest
bytes and literal expected assertions.

| Temporary inherited behavior | Independent native result | Executing guest result |
| --- | --- | --- |
| Disable `invalid_report` and resolve home as old `self.fad` | `home_seek_recovery_reports_target_before_at_and_after_completion`: exit 101, actual `0280 4101 0100 00B5`, required `02FF FFFF FFFF FFFF` | Native Machine exit 101 and rebuilt browser Wasm exit 1 at the same full home report assertion |
| Resolve home as old `self.fad` with validity correction retained | Native recovery test exits 101: SEEK target 181 instead of 150 | Native Machine exits 101; rebuilt browser Wasm exits 1 with recovered PAUSE FAD 181 instead of 150 |

Final-source report/home negative Wasm SHA-256:
`2fdd2fac9f09915a90ab0c5cbdecb021698672b340313a7e568a7dcb4634e361`.
Final-source position-only negative Wasm SHA-256:
`8c675a63b06368a7870649ca1de228085f54f271d4b6bf58ff860a6c9fa60041`.

Reproduction: in a temporary source copy replace the body of `invalid_report`
with `false` and/or the body of `pause_position` with `self.fad`. Run the two
named native tests (the guest test is
`executing_guest_transfers_q_computes_sums_and_recovers_from_home_and_error`).
Build release Wasm and bind it into a copied site's `pkg`, then run
`node scripts/cd-reports-probe.mjs target/cd-reports-final-inherited-negative-site` (or the path
to that copy). Restore source bytes and rebuild production before positive
validation. Restoring the final source reproduced the identical positive Wasm
hash below, and all nine positive CD tests passed again. The positive focused probe is `node scripts/cd-reports-probe.mjs`;
the required complete browser suite remains `npm run test:browser`.

## Required checks

Early focused
native checks used Homebrew Rust 1.94; final checks put rustup stable Rust 1.99.0
on PATH for both Cargo and its compiler subprocess. An initial Wasm invocation
used Homebrew's compiler despite rustup Cargo and failed to locate its target;
selecting the installed stable compiler/target fixed this without downloading
or switching providers/models. Initial native Clippy found a fixed-size
`chunks_exact_mut` test loop; it was changed to the repository's `as_chunks_mut`
convention. Neither failure was waived.

Node 24.12.0, Playwright 1.58.2, Chromium 145.0.7632.6 and matching
wasm-bindgen 0.2.129 were used. No allowance reset, purchase or provider/model
switch occurred.

| Check | Actual result |
| --- | --- |
| `cargo fmt --check`, `cargo build --locked` | Passed |
| `cargo test --locked --no-fail-fast` | Passed: 921 tests (589 library, 332 integration), zero failed/ignored |
| `cargo test --locked --test cd_reports` (included above) | Passed: nine independent register/Machine tests |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| All registered fixture/media reproduction checks from `.github/workflows/pages.yml` | All 85 passed, including new ROM and both media variants |
| `sh scripts/build-web.sh` | Passed, production release module rebuilt after restoring source |
| `npm ci --ignore-scripts` | Passed, zero reported vulnerabilities |
| `node scripts/cd-reports-probe.mjs` | Passed against rebuilt production Wasm |
| Complete extended `npm run test:browser` | Passed in Chromium 145.0.7632.6 under `/saturn-rust/`, final rebuilt production module, no required skips |
| JavaScript syntax, local documentation links and Git whitespace | Passed |

All mandatory checks passed. No required check remains pending or skipped.

Production Wasm SHA-256:
`2b40b47154e2f8b3ce3fabbaf81fa2c09d29c69aac865a40c1799ab516704feb`.

The complete native suite preserves Star Collector, Prism Courier, Hardware Lab,
scheduled directory acquisition, selectors/transfers, ECC/retries, CD-DA, audio,
backup, controllers, CPU/bus/reset, SCU/DSP and video regression assertions.
The complete browser suite includes those demonstrations, actual AudioWorklet,
large-media handling, offline/input/backup and local-file privacy assertions.

## Remaining hardware scope

This is partial #17 work. Full mechanical timing, R–W decoding, additional media
formats and remaining CD/HLE completion are separate mandatory umbrella work.
BUSY phases and FATAL values are not invented. CPU/peripheral semantics,
multiprocessor/bus/reset, SCU/DSP, video/audio, SMPC and comprehensive integration
requirements remain mandatory. Green checks establish this increment's evidence,
not hardware completion or demonstrated commercial compatibility. #17 stays open.
No personal firmware, commercial media, physical console, human testing/approval,
live Pages inspection or investigation of administrative Issue closure is a gate.
