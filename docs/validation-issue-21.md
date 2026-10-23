# Issue #21: scheduled CD directory acquisition

Related to #21. Related to #17. This increment does not establish hardware
completion, full BIOS CD-library behavior or commercial-game compatibility.

## Contract and implementation

The primary reference is [ST-162-062094](https://antime.kapsi.fi/sega/files/ST-162-062094.pdf),
CD Communication Interface §§3.1–3.2/table 3.2, §§6.2.1–6.2.3/table 6.1 and
§8.2.8 functions 8.1–8.6. In particular, §6.2.3 requires an empty workspace sector,
permits drive commands during filesystem work and invalidates an interrupted
acquisition's table. EFLS reports file-access completion/stop independently of
CMOK command acceptance.

[CdBlock](../src/cd.rs) implements ChgDir/ReadDir as a resumable directory job.
A root request acquires each volume descriptor separately, retaining only the
parsed primary root record and volume bound. It then acquires and parses each
directory sector at its own drive deadline. No complete directory is extracted
at acceptance or after an artificial completion delay. The existing record,
volume, track, extended-attribute, interleave and 256-entry window checks remain;
the parser retains at most 256 file records and one parsed primary record.

Every acquired sector uses `Media::decode_sector` in `drive_frame`, with the same
ECC repetitions, header confirmation, finite/infinite retry and received-error
policy as runtime ReadFile/play. Failed attempts consume individual sector
periods. Terminal decoding/record errors produce drive ERROR, unavailable scope
and EFLS. They never fabricate a REJECT by overwriting an accepted response.
A later status query reports the current error. A retained command response is
unchanged by asynchronous completion/error.
Acquisition at leadout or on an audio track also ends with ERROR and EFLS at the
scheduled read deadline, releasing workspace instead of leaving the job pending.

Invalid selectors, unavailable current tables and unknown navigation identifiers
reject before mutation. A busy directory engine or unavailable workspace returns
WAIT without changing selector/table state. An out-of-range ReadDir window or
malformed record discovered during acquisition ends asynchronously and leaves
no table. Successful acquisition publishes the whole bounded table atomically.

The job reserves exactly one sector in `used()`, shared with stored partition
sectors and all host PUT reservations. This workspace is reused across reads;
received directory sectors do not enter host-visible partitions. The designated
partition is cleared and its filter programmed when access starts. Once the root
extent is known, the filter range becomes that directory's range. Removing the
job releases workspace ownership. AbortFile pauses access, cancels seek/scan,
invalidates an in-progress table and retains stored partition data and filter
settings. Tray open, initialization/reset, CD disable and media replacement also
discard unfinished work. Replacement cancels an unexecuted old command so that
it cannot start against the new medium. No delayed publication callback survives.

Drive commands continue to execute during directory work. Pause obstructs reads;
play/seek can move away from the next required sector, with ordinary stream
routing and pool-full behavior. A job consumes only its required FAD. Independent
host FIFO and selector copy/move engines continue through their existing paths.
ReadFile, strict bootstrap parsing and supported media formats are unchanged.

## Explicit timing policy

These are deterministic functional timings, not measured console head latency:
command acceptance remains 1075 reference clocks; initial acquisition restarts
the drive's sector phase and uses the existing 26875-clock seek. Moving from the
descriptor sequence to the root uses the same seek cost without restarting sector
phase. Each acquired/retried sector consumes one 75/150 Hz drive deadline at
reference frequency 26874100 Hz. The seek completes before the next sector tick.
Completion is published on the final acquired sector, before a command whose
response deadline coincides with that tick, matching existing event ordering.

For the new two-descriptor, three-sector root fixture, deadlines relative to
acceptance are 179161, 358322, 537482, 716643, 895804 clocks at 2×, and
358322, 716643, 1074964, 1433286, 1791607 at 1×. The literal FAD trace after those
reads is 167, 170, 171, 172, 173, with SEEK at the transition to 170 and PAUSE only
after completion. Filesystem completion does not raise sector-buffer CSCT or
host-I/O EHST for its private workspace.

## Independent evidence and original guest

[Native vectors](../tests/cd_directory_schedule.rs) check the literal schedule
and table availability at each edge using advancement chunks 1, 3, 17, 10000 at
both speeds, child/parent navigation, clean/correctable/uncorrectable raw Mode1
sectors, disabled ECC, two finite retries, retained responses and cancellation
of infinite retries. Full-capacity and 199-sector host reservations demonstrate
WAIT, the last workspace slot, no over-allocation, unchanged unrelated payload
and release. AbortFile, tray open, software reset, disable and replacement are
covered before the first read, between directory sectors and one clock before
completion. They check resource release and no later stale EFLS/table. Concurrent
status/pause/resume, host FIFO and selector copies retain payload canaries.
Unreadable-root regressions use a 16-sector ISO whose first descriptor is at
leadout and a 32-sector audio-only disc. At both speed settings and all four
advancement chunks they distinguish acceptance from the first read deadline,
check retained responses, ERROR, unavailable scope, released workspace, no stale
EFLS and acceptance of a subsequent ChgDir. Audio retains the drive model's 1×
deadline even when 2× is selected.

The original [MIT-licensed builder](../tests/fixtures/build_cd_directory_schedule.py)
reproduces the [SH-2 ROM](../tests/fixtures/cd-directory-schedule.hex) and
[media](../tests/fixtures/cd-directory-schedule.iso); the adjacent
[license](../tests/fixtures/LICENSE) applies. The guest issues all commands,
measures acquisition with FRT/128, checks EFLS at CMOK, polls completion, observes
masked SCU source 16, unmasks it, executes its vector 50 handler and acknowledges
HIRQ. It then navigates, reads actual directory-derived FIFO metadata and records
results in RAM. Host code supplies ROM/media and inspects records only.

Both native Machine and [production Wasm browser tests](../scripts/browser-smoke.mjs)
run identical bytes, changed navigation FID 2→0, an omitted initial directory
command, an FRT-timed mid-directory abort and repeated cold boots. Root/child
scope counts 3/1 and FIFO FAD 174/178 are independently authored expectations.
Root acquisition takes 6900–7199 FRT ticks, rather than completing at CMOK.
The omitted command cannot reach completion; AbortFile leaves scope invalid.

The identical guest/media and native test were also run against inherited
`281c404d1725ebae04f5020944a8d5c1c43975b6` in an isolated worktree. The negative
control **failed** the substantive acceptance assertion: guest-recorded EFLS at
CMOK was 512, expected 0. This is expected negative-control evidence, not a passing
baseline run. No inherited production code was modified for that control.

Existing [directory/malformed/window vectors](../tests/cd.rs),
[directory guest](../tests/cd_drive.rs) and
[XA/interleaved guest](../tests/cd_filesystem.rs) now wait for EFLS separately
from CMOK. The old command-response REJECT expectation for malformed acquired
media becomes accepted-command plus invalid scope after EFLS, as required by
§§3.2/6.2. The old preserved-window expectation after an interrupted accepted
acquisition becomes invalid scope under §6.2.3. Invalid command parameters still
preserve a valid table. Existing payload, metadata, malformed/bounds, audio,
backup, input and privacy assertions remain substantive; demo guests are unchanged.

## Validation results

Checks executed in this worktree on 2026-10-23:

- `cargo fmt --check`, `cargo build --locked` and native all-target
  `cargo clippy --locked --all-targets -- -D warnings`: passed with Homebrew
  Rust 1.94.0.
- Complete `cargo test --locked`: **774 passed** (483 unit, 291 integration),
  zero failures, ignored or filtered tests, plus passing doc tests. Subsequent
  focused directory checks also pass the exact finite-retry edge and literal
  concurrent-transfer trace assertions.
- The unreadable-root regression passed all short-ISO/audio-only, speed and
  chunk combinations. Against the pre-fix `src/cd.rs`, it failed as expected at
  the first read deadline: EFLS was 0, expected 512. The fixed implementation
  was restored before the complete native and production Wasm checks.
- All **73** CI fixture/media reproduction commands: passed. The final updated
  scheduled guest/media additionally passed its builder's `--check` again.
- `sh scripts/build-web.sh` and Wasm Clippy with `-D warnings`: passed using
  installed rustup stable Rust 1.99.0 and wasm-bindgen 0.2.129. Production Wasm
  was rebuilt from this implementation.
- `npm ci --ignore-scripts`, JavaScript syntax, documentation links and Git
  whitespace checks: passed.
- Complete `npm run test:browser`: passed against rebuilt production Wasm on
  Chromium **145.0.7632.6**, Node **24.12.0**, mounted under `/saturn-rust/`,
  with no required skips. The final run includes CD/SCU masking and guest EFLS
  acknowledgement readback, directory FRT/navigation/omission/abort controls,
  all original diagnostics, Star Collector completion/restart, Prism Courier,
  Hardware Lab, audio/AudioWorklet, input, backup and privacy checks. An earlier
  complete run also passed before the final acknowledgement assertions; it is
  not substituted for this final run.

Current artifact SHA-256 values (ROM hashes cover the expanded 512 KiB bytes):

| Artifact | SHA-256 |
| --- | --- |
| Scheduled-directory ROM | `dcbf2ecc819d2c9ff858b062b3f2f3c94c49cc67c67cefdf3039df279299dc4d` |
| Scheduled-directory ISO | `5110e92f220726bf24f4470a92d3d000baeca3f63d8b106ccb78a45f61e74fa9` |
| Adapted directory ROM | `0608232bfe44c079830915615f57d8fbd794f33431bfd9635c6b9f6fda4cdba4` |
| Adapted XA filesystem ROM | `a9b887ea682cb209c22ee6f681f7094a22b7a5e09adc1ce19fa49bfe63428c4a` |

The intentionally failing inherited-revision control is described above. Early
focused runs exposed legacy synchronous expectations and mistakes in new trace
expectations: the selector copy period starts at command acceptance, and a raw
sector's user-data offset is 16 bytes after its header. Those were corrected;
none of those failing runs is reported as a pass.

## Remaining scope

Physical seek timing is the explicit deterministic policy above. Remaining CD
command/drive/subcode/media/HLE contracts, complete BIOS CD-library ABI and new
media formats are outside this increment. Issue #17's CPU/peripheral,
multiprocessor/bus/reset, SCU/DSP, video/audio, CD/media/HLE, SMPC and comprehensive
integration requirements remain outstanding. No tracker closure, personal
firmware, commercial disc, physical hardware, live Pages inspection or human
approval is used as acceptance. Required post-merge verification: none.
