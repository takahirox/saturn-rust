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
movement clears the home marker, including file/directory drive paths. Replacement
media preserves both play-range bounds, maximum repeat and notification count;
only cold initialization or software reset initializes this host information.
An unchanged maximum repeat preserves its finite limit and notification count. Changing range or maximum clears the
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

Twelve [register/Machine tests](../tests/cd_reports.rs) use literal expected words
and Q data, never production report/position helpers to generate expected values:

- Startup signature and every generated drive state: PAUSE, STANDBY, PLAY, SEEK,
  SCAN, OPEN, NODISC, RETRY and ERROR. Both standby origins start away from 150;
  ordinary standby retains 181, home recovery targets 150. Q is transferred in
  each state. Existing CUE vectors retain audio, track/index and CONTROL coverage.
- Nonzero repeat notifications 1/2 survive timeout, home, open/close and seek.
  Unchanged maximum/range retain the finite limit: playback stops after the
  second repeat rather than looping indefinitely. Replacement media with a
  different lead-out retains both bounds and the repeat maximum/count. Canceling
  PAUSE after home at 150 stays paused outside the retained [181,183) range;
  independent vectors cover 150/180/181/182/183/184 and PEND. ERROR masks a
  noninitial 182
  and a nonzero count 1, then PAUSE restores both on the same damaged medium.
- Literal before/equal/after seek, command and periodic publication boundaries;
  command completion wins a coincident periodic deadline, pending queries
  suppress replacement, CR4 release affects only subsequent publication, SCDQ
  continues through a held response, masking and write-zero acknowledgement are
  independent. Generic reports never replace hardware-information payloads.
- Software reset drops Q transfers and home state, power cancellation before/at/
  after query completion prevents stale data, and cold defaults retain the
  signature. Existing executing SMPC CDOFF/CDON tests remain in the full suite.
- Chunks 1/3/17/10000 agree with passive CR4/FIFO previews through the Bus
  debugger API used by Machine and production Wasm. Dedicated FIFO vectors
  preview two successive words (including the final word padded with FFFF),
  exercise physical/P1 aliases and verify CR4 protection across periodic updates.
  Architectural reads still consume the five Q words and release CR4. Other
  devices retain their existing debugger behavior; this evidence concerns CD.
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
Each variant runs twice on cold resets without previews and twice with actual
Machine/Wasm debugger previews between 17-step guest chunks. Previews read CR4
and FIFO longwords, must observe pending guest Q data, and leave all complete
records identical to execution without previews.
The omitted-home control retains 181 rather than 150, distinguishing actual guest
command execution from a canned result. Native and browser checks assert full
home/OPEN/ERROR reports, recovered positions, every Q word, guest sums and actual
sector data/counts. The browser runs rebuilt release Wasm at `/saturn-rust/`.

## Initial implementation negative controls

The initial implementation source bytes were saved and restored in `finally` blocks. Negative
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

## Initial implementation checks

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

All mandatory checks passed for the initial implementation. Review-revision
results are recorded below.

Initial implementation production Wasm SHA-256:
`2b40b47154e2f8b3ce3fabbaf81fa2c09d29c69aac865a40c1799ab516704feb`.

The complete native suite preserves Star Collector, Prism Courier, Hardware Lab,
scheduled directory acquisition, selectors/transfers, ECC/retries, CD-DA, audio,
backup, controllers, CPU/bus/reset, SCU/DSP and video regression assertions.
The complete browser suite includes those demonstrations, actual AudioWorklet,
large-media handling, offline/input/backup and local-file privacy assertions.

## Review revision: pause bounds, replacement media and debugger access

The three blocking PR #46 findings are addressed in this worktree. PAUSE
cancellation checks both retained bounds and reports PAUSE/PEND outside the range,
including the direct Home → current PAUSE → unchanged Play sequence. Mounting
replacement media retains host play information. Bus/Machine debugger APIs now
provide passive CD word/longword inspection; Wasm `read_long` uses that route and
`read_word` previews CR4 without releasing it. CPU/DMA architectural reads retain
their consuming protocol behavior.

The error-recovery vector now explicitly performs software reset and then applies
its retry configuration before starting its separate damaged-media scenario;
mounting media no longer implicitly clears its previous notification count 2.
The audio sample-offset test retains a play range containing 150, seeks to 181,
then uses unchanged-position Play after home recovery, preserving its original
sample-offset assertion while respecting the range rule.

Three temporary negative controls were executed independently and restored:

| Restored defect | Actual regression failure (exit 101) |
| --- | --- |
| Unconditionally enter PLAY when canceling PAUSE | Range vector reports `0381 4101 0100 0096`, requires `0181 4101 0100 0096` |
| Clear range/repeats on replacement-media mount | Replacement vector reports `0180 4101 0100 0096`, requires notification `0181` |
| Route debugger FIFO longwords through consuming `Bus::read32` | Executing Machine guest preview changes `41010100` to `001F0000` on the second inspection |

All twelve positive CD tests passed after restoration. Register vectors now use
the Bus debugger API, and the executing guest uses the actual Machine API.
The same browser guest runs two cold boots without previews and two with Wasm
previews for each original/changed/omitted-operation variant; hosts continue to
load fixtures, advance execution and inspect results only.

During revision checks, Wasm compilation caught a word-read error-adapter type
mismatch; it was corrected before successful Wasm Clippy/build. The first web
build could not find the already installed matching wasm-bindgen CLI on PATH;
adding the documented Cargo bin directory resolved that without installation.
No failure or required check was waived.

| Revision check | Actual result |
| --- | --- |
| `cargo fmt --check`, `cargo build --locked` | Passed |
| `cargo test --locked --no-fail-fast` | Passed: 924 tests (589 library, 335 integration), zero failed/ignored |
| Final `cargo test --locked --test cd_reports` | Passed: twelve tests, including the direct home/pause/cancel regression |
| Native and Wasm Clippy with `-D warnings` | Passed |
| All registered fixture/media reproduction checks | All 85 passed |
| `npm ci --ignore-scripts` | Passed, zero reported vulnerabilities |
| `sh scripts/build-web.sh` | Passed with matching wasm-bindgen 0.2.129 |
| `node scripts/cd-reports-probe.mjs` | Passed against rebuilt production Wasm |
| Complete extended `npm run test:browser` | Passed in Chromium 145.0.7632.6 under `/saturn-rust/`, no required skips; fullscreen entered/exited and denied/unavailable feedback passed |
| JavaScript syntax, updated local documentation links and Git whitespace | Passed |

All revision checks passed. No required check remains pending or skipped. The
complete suites retain Star Collector, Prism Courier, Hardware Lab, directory
acquisition, selector transfers, ECC/retries, CD-DA/audio, backup, controllers,
CPU/bus/reset, SCU/DSP, video, UI and privacy regressions.

Revision production Wasm SHA-256:
`c365c6f86de83d7e499c08e91225121405fc111a1f3f497ef5807fd7f92625db`.

## Follow-up review: reset defaults and shorter replacement media

Both additional PR #46 findings were reproduced before changing production
code. The noninitial-reset register vector returned PAUSE
`0180 4101 0100 0096` instead of PLAY `0380 4101 0100 0096`; the shorter-disc
vector returned REJECT `FF00` instead of PLAY. Both tests failed with exit 101.

Software reset now initializes the play start to FAD 150 independently of the
pickup position. Retained play settings survive replacement media, while the
effective playback/repeat end is bounded by the current lead-out. Bounding does
not overwrite the retained end or clear repeat information. This follows
ST-162 §5.5 p.49 and §7.2.6 pp.65–69, including the default whole-disc range and
disc-end bounds; it does not change the specification reconciliation.

Two new literal register vectors run with advancement chunks 1/3/17/10000 and
passive debugger reads. They cover reset at 181 followed by seek/resume at 150,
completion at 250, replacement of a 100-sector disc by a 50-sector disc, PAUSE
and Q at lead-out 200, and cancellation at that boundary. A nondefault range
`[198,250)` repeats at the shorter disc's end; a later 120-sector disc restores
use of the retained end 250 and lower bound 198, preserving notification count
1 and the finite maximum 2 through the final stop.

| Follow-up check | Actual result |
| --- | --- |
| `cargo fmt --check`, `cargo build --locked` | Passed |
| `cargo test --locked --no-fail-fast` | Passed: 926 tests, zero failed/ignored |
| Final `cargo test --locked --test cd_reports` | Passed: all fourteen tests after the final lead-out assertion |
| Native and Wasm Clippy with `-D warnings` | Passed |
| All registered fixture/media reproduction checks | All 85 passed |
| `npm ci --ignore-scripts` | Passed, zero reported vulnerabilities |
| `sh scripts/build-web.sh` | Passed with matching wasm-bindgen 0.2.129 |

The complete extended `npm run test:browser` also passed in Chromium
145.0.7632.6 against the rebuilt production module under `/saturn-rust/`, with
no required skips. It retained the CD report guest, Star Collector, Prism
Courier, Hardware Lab, audio, backup, controllers, offline execution and privacy
checks. Git whitespace checks passed. No required follow-up check remains pending.

Follow-up production Wasm SHA-256:
`e4d7ad974e6d902949ef725c905cf273bb73199ed80709368c5aeb69d53f5e32`.
No guest or media fixture bytes changed.

## Follow-up review: power-reset range and replacement-media scans

Both PR #46 findings were reproduced with new literal register tests before
changing production code (exit 101): power reset left an empty retained range,
so Play at 150 with unchanged end returned `FF00 4101 0100 0096` instead of SEEK
`0400 4101 0100 0096`; scanning a 50-sector replacement after a 100-sector disc
returned `0500 FFFF FF00 00D2` instead of PAUSE `0180 4101 0100 00C7`.

CD power reset now initializes the retained medium's whole-disc end alongside
the other defaults. A later mount preserves this usable range, just as it
preserves genuinely configured tray-change settings. Scan movement and
termination use the effective end bounded by current lead-out; the effective
lower bound also stays within that end when the saved start is beyond the
replacement disc. Neither bound overwrites the retained range or repeat state.

The new register vectors run with advancement chunks 1/3/17/10000 and passive
debugger reads. They cover power cycling with default and configured repeat
settings, Play before and after a larger replacement mount, forward/reverse
scans and complete Q words on shorter media, a saved lower bound beyond
lead-out, and a later larger disc restoring use of both saved bounds and the
finite repeat count. No guest or media fixture bytes changed.

| Power/scan follow-up check | Actual result |
| --- | --- |
| `cargo fmt --check`, `cargo build --locked` | Passed |
| `cargo test --locked --no-fail-fast` | Passed: 928 tests, zero failed/ignored |
| `cargo test --locked --test cd_reports` | Passed: all sixteen tests |
| Native and Wasm Clippy with `-D warnings` | Passed |
| All registered fixture/media reproduction checks | All 85 passed |
| `npm ci --ignore-scripts` | Passed, zero reported vulnerabilities |
| `sh scripts/build-web.sh` | Passed with matching wasm-bindgen 0.2.129 |
| Complete extended `npm run test:browser` | Passed in Chromium 145.0.7632.6 under `/saturn-rust/`, rebuilt production Wasm, no skips |
| Local documentation link targets and Git whitespace | Passed |

No required check remains pending. The complete suites preserve the CD report
guest, Star Collector, Prism Courier, Hardware Lab, directory acquisition,
selectors, ECC/retries, CD-DA/audio, backup, controllers and privacy regressions.

Power/scan follow-up production Wasm SHA-256:
`5523bc9b88bd79d70c0ed9eb2722000f1f46c6eaffaa9ef36992ec99edba4131`.

## Remaining hardware scope

This is partial #17 work. Full mechanical timing, R–W decoding, additional media
formats and remaining CD/HLE completion are separate mandatory umbrella work.
BUSY phases and FATAL values are not invented. CPU/peripheral semantics,
multiprocessor/bus/reset, SCU/DSP, video/audio, SMPC and comprehensive integration
requirements remain mandatory. Green checks establish this increment's evidence,
not hardware completion or demonstrated commercial compatibility. #17 stays open.
No personal firmware, commercial media, physical console, human testing/approval,
live Pages inspection or investigation of administrative Issue closure is a gate.
