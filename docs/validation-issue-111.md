# Legacy SCU DSTP compatibility (#111)

## Result and scope

Aligned longword DSTP accesses no longer stop the emulator solely because the
old force-stop function is unimplemented. `Scu::read32` returns zero only at
`+0x60`; `write32` ignores only that removed port. This is a final-specification
compatibility interpretation with explicit uncertainty, not verified physical
readback or proof of an ignored electrical write. No force-stop implementation,
DMA scheduling change, guest exception change or global error suppression is
introduced. The reusable [MMIO policy](mmio-compatibility.md) distinguishes device
behavior, deterministic fallbacks, invalid operations and architectural exceptions.

The user's private BIOS and transaction are unavailable. The reported PC does
not classify its instruction, direction, value or access origin. Stock BIOS/menu
compatibility remains unverified. Personal BIOS feedback is optional; there is
no required post-merge verification. Related to #17; this does not complete the
hardware tracker or waive #81's endpoint finding.

## Source findings

Reviewed 2026-11-06. References are pinned to the retrieved repository heads;
no external implementation code was copied. Original PDF SHA-256 values:
ST-097 `d56a86087f10c61d0bafaf5dfa0de134f92fe413e6e40f69a656df5192ccc8d3`,
ST-210 `7842694c3fb747a9db921ac73e2454741d960fe40c2289fc515a2cb2feb7abdb`.
The PDFs were read locally using text extraction, including the direction labels
and item 5's generated-read order.

| Source and location | Finding and limit |
| --- | --- |
| [Sega ST-097-R5-072694](https://github.com/luckyseoul/saturn-docs/blob/6245110605642ae6048df507ad8737115d865be6/files/ST-097-R5-072694.pdf), §1.3 pp.7–8 fig.1.8; §3.1 p.40; §3.2 p.47 fig.3.11 (PDF p.63) | Maps a 32-bit DSTP at +60, initial value zero. DSTOP bit 0 is labeled **W** and requests stopping operating DMA when set. Page 8 restricts force-stop to debugging and warns that DMA cannot subsequently be used. Initial zero is not readable-storage evidence. Exact stop timing, trigger, interrupt and bus-grant consequences are not fully established. |
| [Sega ST-210-110194](https://github.com/luckyseoul/saturn-docs/blob/6245110605642ae6048df507ad8737115d865be6/files/ST-210-110194.pdf), introduction p.1; item 5 p.4; items 13–14 p.5 (PDF pp.5/8/9) | The October 1994 final-specification notice supersedes earlier manual details. Item 14 removes forced quit and forbids writing DSTOP; it prescribes neither an SH-2 exception nor a read value. No numeric production/revision applicability threshold or measured prohibited-write response is supplied. Item 5 explains indirect register reads from cache fills; item 13 removes old DMA status functionality. |
| [Mednafen `SCU_RegRW_DB`](https://github.com/OpenEmu/Mednafen-Core/blob/a02bb7b1ad8a79ca0abf3a888ef51c0e3c45a4bd/mednafen/ss/scu.inc), write `case 0x60`, read default | Bit 0 clears active/request state for all three DMA levels and recalculates DMA halt. The case explicitly carries an untested TODO. Unknown reads produce zero. This is an implementation precedent, not independent hardware measurement. |
| [Yabause `ScuReadLong` / `ScuWriteLong`](https://github.com/Yabause/yabause/blob/7e38821dbac265490f115e163c523a939acda759/yabause/src/scu.c) | Longword +60 writes retain DSTP; +60 reads fall through to zero. The write case supplies no demonstrated force-stop effects. Storage on write does not establish storage readback or actual stopping. |
| [5thPlanet `Scu::read32` / `write32`, `dstp_and_dsta_round_trip`](https://github.com/hiroshiyui/5thPlanet/blob/7d87093b835c5744505b065c4db089c4f253d9cc/crates/saturn/src/scu.rs) | Reads and writes use storage; the test establishes a modeled round trip only, without proof of DMA stopping or physical readback. |
| [MAME `saturn_scu_device::regs_map`](https://github.com/mamedev/mame/blob/ce12c0474a7f5786748327d0320179344bddffba/src/mame/sega/saturn_scu.cpp), +0060/+007C | The force-stop write mapping is commented out, while DMA status reads are active. This is not evidence of either a CPU trap, zero readback or an independent electrical observation. |

There is no demonstrated consensus on electrical reads or stop behavior. The
model already reports `VER=4` and applies the final-spec corrections. Function
removal is high-confidence documentary evidence; ignored-write behavior is a
compatibility choice of lower confidence, and zero readback is a low-confidence,
deterministic fallback. It is chosen over readable storage or an invented stop
mechanism because neither is established by the final specification. It is not
generalized to arbitrary unknown registers. An actual stop interpretation would
need its real transfer/deadline/IRQ consequences and revision evidence first.

## Acceptance matrix

| Criterion | Implementation / executable evidence |
| --- | --- |
| Exact failure and origin | Original 512 KiB read and write firmware has separate known operands. Access firmware covers both P0/P1 aliases, values 0/1/FFFFFFFF/A55A1234, CE=0 and CE=1 with data replacement disabled. A cacheable +6C operand generates +60 then +64. The opt-in completed-cycle trace retains direction/value, original operand, PC, physical cycle and source, without rereading firmware. The private BIOS instruction remains unknown. |
| Grounded decision | The source comparison above distinguishes initial write-only stop semantics, final removal/programming restrictions, emulator precedents and unmeasured electrical behavior. |
| Nonfatal bounded correction | Two exact +60 cases in the shared SCU endpoint serve CPU aliases and device cycles. Existing Bus/CPU error propagation remains intact. Developer capture is off by default, bounded to 64 events, local and passive. |
| DMA/status | Unit tests cover each level alone in idle/armed/requested/active states and direct/indirect mode, retained descriptors/data and one retrigger. Every subsequent prepared cycle/result, pointers, pending/wake state, interrupt and final data matches an untouched control. Scheduler tests compare retained granted cycles, data, IRQ and physical deadlines at fixed boundaries/chunks 1/3/127, another level entering during transfer, and reset cancellation. Armed channel disable remains distinct. Removed DSTA still diagnoses; IST and real DMA-end IRQ are preserved. Both SH-2 DMACs read/write DSTP through scheduled physical cycles without a CPU PC or spurious DMA error. |
| Safety controls | Bus tests retain byte/word rejection, host alignment errors, unknown +64, removed DSTA, unused +AC, write-only DMA count/IMS, read-only VER and prohibited IST write. The cache negative control still faults honestly at +64. Inherited CPU vector-9, DMA-invalid-parameter, bus ownership, SMPC, SCU/DSP and other negative controls remain. No blanket successful MMIO return is added. |
| Original end-to-end tests | `build_scu_dstp.py` reproduces four adjacent-MIT-licensed original reset-vector firmware images as sparse hex and assembly, with source-authored record oracles. Native runs read/write/access guests twice at chunks 1/3/127/10000, without/with original ISO, compares identical clocks/CPU/IRQ/results with tracing disabled/enabled. Wasm uses identical bytes, chunks 1/127/10000, media/no-media, debug inspection, cold resets, 64-event bound and sticky +64 negative control. Existing `scuFringe` group runs the added module; Play UI executes access firmware through offline BIOS-only Start/Restart. |
| Preserve regressions | All existing guest expectations and deadlines are retained. The obsolete unit expectation that DSTP is fatal is replaced with the explicit scoped policy; other rejected ports remain checked. Full validation results below. |
| Validation and publication | Native/fixtures/release-Wasm/Clippy/runner/full browser checks below. The 74-group inventory and aggregate revision/artifact/publication barrier remain intact. The fixture registry becomes 105 builders; runner assertions retain all old commands and add the new builder. Exact-head hosted PR CI and ordinary review are still required before merge. This node does not push, open PRs, comment or merge. |

## Changed files and validation cost

- `src/scu.rs` changes the two exact longword cases; `src/scu/tests.rs` retains
  neighboring negative controls and adds direct/indirect channel-state checks.
- `src/bus.rs`, `src/bus/transactions.rs`, `src/lib.rs` and `src/wasm.rs` add the
  opt-in completed-cycle trace. `src/bus/dstp_tests.rs` checks scheduled DMA
  retention/deadlines, arbitration, cancellation and both on-chip DMAC sources.
- `tests/scu_dstp.rs`, the fixture builder and its `.asm`/`.hex`/`.tsv` outputs
  supply the original native reset-vector guests and authored record expectations.
- `scripts/scu-dstp.mjs` runs those bytes through production Wasm inside the
  existing `scuFringe` group; `scripts/play-ui.mjs` adds one BIOS-only Start/Restart
  case to its existing offline loop. No browser group or worker is added.
- The fixture registry/runner inventory, setup/current performance inventory and
  conformance documentation reflect the added builder and scoped policy.

Broad state combinations stay at unit/bus level. The browser adds only short
10,000-turn boots to exercise bindings, media retention, trace observability and
UI launch. The first focused combined DSTP/fringe group passed in 1.609 s on
installed Chrome (including inherited fringe assertions); this is a local sample,
not a hosted performance guarantee. No existing guest clock budget, result,
fixture, browser group, timeout or publication dependency is relaxed.

## Before and after

Before changing the model, the original read firmware failed its first DSTP
load at guest `PC=0x20000218`, physical `0x05FE0060`, width 4, with
`UnsupportedDevice`. That failure is **not** a trace of the private firmware's
PC. The same read guest now records zero and sleeps normally; the store guest
executes 100 writes of `A55A1234`, records completion and sleeps. The access guest
records sixteen zero reads, three actual DMA results `12345678`, three real IRQ
handler counts of one and the final `600D` marker. Source-authored values and
clock budgets are unchanged by the model fix. Trace capture stops at 64 events
while the write guest still completes all 100 writes.

The new cache guest intentionally remains a negative control: cached operand
`05FE006C` generates a successful read at `05FE0060`, then faults at `05FE0064`.
It proves why neither the reported PC nor a physical address identifies a direct
register instruction. ST-210 item 5's cache-through programming rule remains
relevant; no adjacent port is silently admitted to make that guest pass.

## Executed validation

Local environment: macOS arm64, stable Rust 1.99.0, Node 26.12.1,
wasm-bindgen 0.2.129, Playwright 1.58.2. Complete browser validation uses its
pinned Chromium headless shell 145.0.7632.6. The first focused run also passed on
installed Chrome 157.0.8103.38.

| Command | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed. |
| `cargo build --locked` | Passed. |
| `cargo test --locked` | Passed: 1,201 tests, zero failures or ignored tests, plus empty doctests. |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed. |
| `PYTHONDONTWRITEBYTECODE=1 sh scripts/check-fixtures.sh` | All 105 registered builders passed. |
| `sh scripts/build-web.sh` | Passed with matching bindgen CLI and production release Wasm. |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed. |
| `node --test scripts/ci-runner.test.mjs` | All 13 runner regressions passed, no skips. |
| `BROWSER_CHANNEL=chrome npm run test:browser -- --group=scuFringe` | Passed including new DSTP and inherited fringe guests; group 1.609 s, command 2.407 s. |
| `npm run test:browser` | Passed: all 74 groups (33 shared / 41 scenarios), with coverage/revision/artifact aggregation. Parallel elapsed 398.050 s; shared 397.798 s, scenarios 357.757 s. Pinned Chromium executed the new guests and offline Play Start/Restart. |
| `git diff --check`, new-document relative links and changed JavaScript syntax | Passed, including staged whitespace before commit. |

Commands selected the rustup stable compiler and Cargo together as documented in
setup. Browser and runner commands use
`PLAYWRIGHT_BROWSERS_PATH=target/playwright-browsers` for the pinned local install.
The initial runner attempt failed because Chromium was absent, and its exact
fixture-count assertion caught the new 105th builder; that assertion now includes
the new builder while retaining the previous entries. The standard browser
installer downloaded the pinned archive but stalled extracting it under this Node
installation. Extracting the same official archive into ignored `target/` supplied
the pinned executable; the unchanged runner then passed. No browser assertion,
timeout or application tooling was altered to accommodate that environment issue.

These are local worktree results, not hosted exact-head PR CI. Successful
**exact-head PR checks, the aggregate artifact barrier and normal review remain
mandatory before merge**. This implementation node does not publish anything.
No post-merge developer verification is required; optional private-BIOS feedback
is separate and cannot establish full BIOS/menu compatibility.
