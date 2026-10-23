# Issue #41 validation: CPU sound-memory allocation

Related to #41 and #17. This is a partial hardware-completion increment, not
hardware completion or commercial compatibility. No push, PR, merge, Issue
comment/closure, personal firmware, commercial media, physical-console test,
human approval/listening or live Pages inspection is part of this work.

## Contract and implementation

Primary sources are Sega [ST-077-R2, §3.1 printed pp.18–19 and §3.2 pp.20–21,
figs.3.1–3.2](https://antime.kapsi.fi/sega/files/ST-077-R2-052594.pdf) and the
[later Sega chapter 3](https://www.infochunk.com/saturn/segahtml_en/hard/scsp/hon/p03_10.htm).
They establish buffered main-interface waits through internal completion,
128 DRAM cycles/sample, two refresh demands and PCM/DSP > refresh > internal
DMA > main CPU > sound CPU priority. Allocation precedes actual access. The
[transaction table](scsp-cpu-memory.md) was authored before the new allocation
tests, with independently enumerated rational deadlines and explicit phase
conventions. No measured console oscillator/interface phase is claimed.

The [controller](../src/sound/memory.rs) keeps queued requests, admitted
per-word address/payload/geometry and retained completed reads separately from
external transactions. A main 32-bit transfer consumes two internal words;
the existing SH-2 read bridge retains its separately sampled halfwords. The
[68k journal](../src/sound/transactions.rs) queues at the start of each byte/word
cycle and acknowledges after both its four-clock duration and memory completion.
Reset vector reads and instruction fetches use the same allocator. No aligned
scheduled 68k RAM access can complete through the direct/debugger path.

[Sound scheduling](../src/sound.rs) completes an admitted CPU word before the
next allocation and considers CPU requests when waking an idle memory clock.
[Board transactions](../src/bus/transactions.rs) consume retained results,
including SCU/DSP DMA and both SH-2 DMACs, without sampling again or repeating a
write. Bus admission occurs before selecting the next board deadline, preventing
advancement chunks from backdating a new sound request. Register-only accesses
and debugger inspection remain independent of DRAM acquisition.

SNDOFF cancels sound-CPU work; SYSRES/cold boot clear volatile journals. Manual
SH-2 reset retains its accepted operand drain. SCSP RESET drops an unfinished
main DRAM grant while the external master may still hold its buffered request:
retain completed halves, wait through RESET and re-admit only the unfinished
word after release. Canceling that external request discards unfinished work.
Completed writes remain visible and no completed halfword replays. These RESET
crossing phases are explicit functional conventions, not specified electrical
behavior. An attempted main-interface reset fault was rejected during validation
because ordinary existing guests poll sound RAM through the reset pulse; the
final implementation preserves that operation without inventing a bus fault.

The existing DSP instruction-boundary memory sampling/IWT pipeline is preserved.
Internal DSP trace end events denote ownership release; they do not claim an
additional memory sample at the interval's end. PCM and DMA retain their existing
separate acquisition/completion behavior. The main interface's documented
word-only restriction does not establish physical byte support: the existing
bridge byte-lane compatibility behavior is preserved and labeled as such.

## Independent native and original-guest evidence

- [Allocation vectors](../src/sound/memory/tests.rs) enumerate before/at/after
  arrivals, immediate idle wake, all internal priorities, main-versus-sound
  eligibility, later higher priority after admission, completion before
  reassignment, finite progress, retained reads, byte lanes, geometry/absent chip,
  queued/granted cancellation and partial main longwords across RESET. A real
  68k performs six reset reads, seventeen further fetches and six data cycles:
  exactly 29 CPU memory grants/completions, with no extra access on replay.
- [Board vectors](../src/bus/sound_tests.rs) cover both SH-2s, both on-chip
  DMACs, all three SCU DMA channels and executing SCU-DSP DMA, reads/writes,
  independently retained operands, SCU publication/IRQ ordering, both main PLLs,
  chunks 1/3/17/10000, debugger invariance and manual/power-reset drains. Existing
  [bridge](../src/bus/bridge_tests.rs), DSP, PCM, sound/reset and integration
  assertions remain in place. The old board reference50 is replaced by an
  enumerated SCPU grant48/completion53, following main grant43/completion48.
- The original [MIT](../tests/fixtures/LICENSE)
  [fixture builder](../tests/fixtures/build_scsp_cpu_memory.py) emits real SH-2
  and 68k instructions. DMAC copies 256 longwords from a stable guest-computed
  DIVU quotient into sound RAM with a fast bridge. Even before refresh and
  source/buffer overhead, 512 DRAM cycles require 2437.56009 reference clocks,
  hence at least 304 FRT/8 counts. The independently authored interval is 304–519.
  A separately executing 68k polls shared RAM, reads the uploaded longword,
  programs PCM and a half-gain DSP, then toggles waveform words 1024 times.
  Records include the retained longword `20002000` and guest completion `600D`.
  Stereo includes `(a,a/2)` and `(b,b/2)`; the left interpolation may produce
  `(a+b)/2`. The test host never supplies running guest results or samples.
- [Native guest tests](../tests/scsp_cpu_memory.rs) and the
  [registered production-Wasm guest](../scripts/scsp-cpu-memory.mjs) use identical
  bytes, three operand pairs, omitted initiating DMA, repeated cold boots and
  the same independent cycle/audio assertions. Native execution chunks 1/3/17/10000
  also compare exact records, stereo buffers and elapsed clocks. The browser
  uses the production AudioWorklet and checks actual rendered audio, expected
  peak, zero drops and observable underflow/blocked states.

Two existing guest schedules were corrected without relaxing their assertions:

1. The original PCM read fixture adds one guest NOP between its two waveform
   writes, keeping a write between independently retained interpolation operands
   under the new CPU allocation. All three arithmetic midpoint, endpoint,
   stereo, omitted-KEYON and repeated-boot expectations are unchanged.
2. The basic sound fixture starts timer A at FD instead of FE. Two sample edges
   ensure its post-acknowledgement overflow cannot merge with the existing
   power-on request during the first handler. The two real IRQ/RTEs, STOP wake,
   saved PC 182, 32 retired instructions, mailbox and exact stereo assertions
   remain unchanged. Only that basic diagnostic opts into FD; Prism Courier,
   Hardware Lab and other generated sound guests retain their original bytes.

Expanded 512-KiB ROM SHA-256 hashes:

| Fixture | SHA-256 |
| --- | --- |
| New CPU memory guest | `188649564bbf678b1fd97cdd1bec829e5234a7d28b4bf88661e5cbfa71b27b24` |
| PCM read guest with phase NOP | `d604f2162e8611354e160ee0bbc5ac767045b5055030add104b8b30b8d161ade` |
| Basic sound guest with FD timer | `0cea305717c24d7a62118bc9291c1609b4724c503b16d70c52e74df3be6d0994` |

## Inherited-admission negative controls

Production sources were saved, then these five files were temporarily replaced
with their inherited versions from
`af661f51db9604b70109d67b596489f68354a801`: `src/sound.rs`,
`src/sound/dma.rs`, `src/sound/transactions.rs`, `src/bus.rs` and
`src/bus/transactions.rs`. This restores the CPU busy-gate admission path while
retaining the original guest and its independent assertions. Both controls
failed at the intended budget assertion, after producing the expected data and
completion records:

- `cargo test --locked --test scsp_cpu_memory guest_cpu_memory_budget -- --nocapture`
  exited 101: `DRAM cycle budget [73, 536879104, 24589]`.
- `cargo build --locked --release --target wasm32-unknown-unknown` and matching
  `wasm-bindgen --target web --no-typescript` rebuilt the production module into
  an isolated static-site copy. `node scripts/scsp-cpu-memory-probe.mjs target/scsp-negative-site`
  imported the same browser guest under `/saturn-rust/` and exited 1 at
  `DRAM cycle budget 73,536879104,24589`. The inherited Wasm SHA-256 was
  `faf76c6e88df985d70bcaddb303d0851157314a16c2e38dd3ee5de203672a940`.

All five production source files were restored byte-for-byte in `finally`, then
production Wasm was rebuilt. The focused positive runner passed all records,
operand controls and AudioWorklet assertions. To reproduce that focused probe,
run `node scripts/scsp-cpu-memory-probe.mjs [static-site-directory]`; it serves
only that directory at the project prefix and cleans up the browser/server.
The full mandatory browser command remains `npm run test:browser`.

## Final checks

Checks used Rust 1.99.0 (`b940084d7`, stable toolchain) on 2026-10-24.

| Command/check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 897 tests, 576 unit and 321 integration; 84 result groups including doc tests; zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| All registered fixture/media `--check` commands from Browser Pages workflow | All 83 passed, including the new guest and both corrected existing schedules |
| `sh scripts/build-web.sh` | Passed with matching wasm-bindgen 0.2.129 |
| `npm ci --ignore-scripts` | Passed; no reported vulnerabilities |
| `node scripts/scsp-cpu-memory-probe.mjs` | Passed: same production module, all guest controls and AudioWorklet assertions |
| Complete extended `npm run test:browser` | Passed under `/saturn-rust/` in Chromium 145.0.7632.6, with no required skips |
| JavaScript syntax, local documentation file links and Git whitespace | Passed; 819 local documentation file links checked |

Production Wasm SHA-256:
`7fbbeae0984fe2db6edf488bf5890e4af49a1ccb9eea36accc6b93514d90b7e7`.
The three demonstration discs keep their original bytes. Prism Courier and
Hardware Lab, the existing PCM/DSP/refresh guests, sound/MMIO/reset vectors and
all other registered fixtures remain in the mandatory suites.

## Review correction: cancellation before replacement admission

Related to #41 and #17. The [bus allocator](../src/bus/transactions.rs)
now synchronizes retained sound-memory state before allocating a replacement
bus transaction, as well as afterward. This cancels an aborted request before
a waiting peer can submit different operands; an active manual-reset drain
retains its existing DRAM operation.

The [new regression](../src/bus/sound_tests.rs),
`sound_memory_reset_releases_canceled_request_before_admitting_waiting_peer`,
reproduced `request changed before acknowledgement` at `src/sound/memory.rs:77`
before the allocator fix. Afterward, all 64 combinations passed: either SH-2
with a waiting peer, power/manual watchdog reset, assertion ticks 4/7/10/15,
and advancement chunks 1/3/17/10000. Tick 7 includes the reported peer request
at tick 5. Literal expected words and DRAM completion counts check canceled
publication, completed-lane retention, manual drains and independent peer
completion. Later host mutations and another 1000 clocks verify that neither
write is repeated and no further main-interface DRAM operation appears.

Review-fix checks on 2026-10-24 used Rust 1.99.0 and wasm-bindgen 0.2.129:

- `cargo fmt --check`, `cargo build --locked` and `cargo test --locked` passed:
  898 tests (577 unit, 321 integration), zero failed/ignored.
- Native `cargo clippy --locked --all-targets -- -D warnings` and Wasm
  `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` passed.
- All 83 registered Browser Pages fixture/media reproduction checks passed.
- `sh scripts/build-web.sh` and `npm ci --ignore-scripts` passed.
- Complete extended `npm run test:browser` passed in Chromium 145.0.7632.6
  under `/saturn-rust/`, with no required skips, including the CPU sound-memory
  guest, production AudioWorklet and all three demonstration discs.
- Git whitespace checks and all 14 local file links in this record passed.

Rebuilt production Wasm SHA-256:
`65a50b440d78f61bf555390f8a282015c2564e6e7ce7a0c6dba3575ccc6069d2`.

## Remaining limits

This increment implements CPU sound-memory participation and its autonomous
validation. Oscillator origin, interface eligibility and equal-edge RESET/bus
crossings remain explicit model conventions. General 68k microsequencing,
SCSP register-bus arbitration, calibrated envelopes, DAC modes, full DSP
pipeline/conversion coverage and physical interface timing remain outside this
Issue's scope. Remaining CPU/peripheral, board/reset, SCU/DSP, video/audio,
CD/media/HLE, SMPC and comprehensive integration criteria of #17 remain mandatory.
Neither this checkpoint nor green CI closes #17 or proves commercial compatibility.
Required post-merge verification: none.
