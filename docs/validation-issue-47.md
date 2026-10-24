# Issue #47 validation: live slave raster interrupts

Related to #47 and #17. Starting checkpoint:
`20e37f807ee799cbddb422491b4dc4226eac6783`. Validation date: 2026-10-24.
No push, PR, merge, Issue mutation or post-merge verification is part of this node.

## Primary contract and implementation

[STN-28 attachment, Dual CPU Usage Ver.1.1 §2.3](https://www.infochunk.com/saturn/segahtml_en/info/hon/stn28_1.htm)
establishes slave HBlank IRL2/vector `0x41`, VBlank IRL6/vector `0x43`, level re-entry
throughout a blank, independence from the master's SCU and masking by slave SR.I.
[SH7604 Hardware Manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf)
§§4.4–4.7 (printed pp.72–77), §5.2.3/table 5.3 (pp.83–85), table 5.4
(pp.87–88), §5.3.8 (pp.95–96) and §§5.4–5.6 (pp.97–102) establish acceptance,
deferral, exception stack/vector accesses, arbitration and autovectors. The
actual manual PDF and Sega's STN-28/VDP2 pages were inspected.
[VDP2 ST-058-R2 §§2.3–2.5](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p02_50.htm)
distinguishes blanking state from the forced VBLANK status readback when DISP=0.

`Bus::interrupt(Slave)` selects VBlank level 6 before HBlank level 2 from the live
blanking flags. It uses normal SH-2 autovectors and the existing external/internal
priority comparison (external wins equal priorities; UBC precedes IRL15).
Slave acceptance does not clear a raster level or touch SCU state. Master
mask/status/acknowledgement and DMA triggers retain their existing paths.
The current CPU-boundary journal already freezes a sampled request across pending
bus phases. No new queue, interrupt acknowledgement or timing state is needed.
Existing reset cancellation and raster deadlines therefore cover the new input.

The DCC level changes at the inherited raster events. CPU issue sees that level
at its next boundary; once issued, the journal retains it even if the blank ends
before stack/vector completion. This is the emulator's sampling convention,
not measured DCC electrical phase. The SH7604 noise canceler is not newly modeled.
Slave VECMD=1 external board-vector acquisition remains unsupported: no D7–D0
value is invented, and this mode does not deliver raster vectors. The diagnostic
explicitly writes ICR.VECMD=0. Half-line interlace and HLE interrupt initialization
remain separate outstanding work.

## Independent vectors and guest evidence

[Nine native tests](../src/bus/slave_raster_tests.rs) author expected levels,
vectors and deadlines directly from the contract and explicit inherited raster
conventions, without calling production helpers to generate expected results.

- Before/at/after HBlank IN/OUT and VBlank IN/OUT, including HBlank during VBlank;
  NTSC/PAL, both PLLs, normal/high resolution, 224/240/256-line modes, single/double
  interlace across both fields and exclusive 480-line modes. Main-clock line/H
  pairs are 1708/1280, 1820/1408, 852/640 and 848/704. Reference deadlines use
  independently written rational PLL frequencies and ceiling division.
- SR.I 0/1/2/5/6/7/15, SLEEP wake, stacked PC/SR, unchanged retirement at entry,
  RTE/slot re-entry, unmask during and after entire masked HBlank/VBlank periods,
  simultaneous H/V selection and internal FRT above/equal/below external priority.
- SCU mask/status/acknowledgement independence, unchanged master pending state on
  slave acceptance, and DISP=0 throughout the board edge vectors.
- A pending exception sampled on the final HBlank clock completes exactly once
  after deassertion, with one eight-byte frame and no duplicated retirement.
  A unit-test probe changes the completed SR store while vector entry is still
  pending; replay retains that changed byte, proving it does not repeat the
  completed memory effect. BRA slots and LDC successors defer acceptance. Pending entry is canceled by
  cold reset, SSHOFF, SYSRES and both clock commands. Timed SSHON then accepts
  current VBlank after software unmasks, including following system/PLL restarts.
  Shared RAM/backup checks preserve each reset domain. Reset tests that arrange
  an imminent long SMPC command shorten only its unit-test countdown, not the
  production command latency.

The original [MIT](../tests/fixtures/LICENSE)
[builder](../tests/fixtures/build_slave_raster.py) reproduces
[assembly](../tests/fixtures/slave-raster.s) and [ROM](../tests/fixtures/slave-raster.hex).
Expanded 524288-byte ROM SHA-256:
`a33a1b1623df8430aee0616516266e21b302c694b26b4263801c7fd16cf450af`.
The master starts the slave through SMPC SSHON; the slave selects its stack,
installs a VBR-relative RAM table and explicitly selects autovectors. Actual
unified-cache reads prime the H handler so two entries fit in one 428-clock
HBlank; this is guest code, not host priming. SLEEP, hardware exception stacking,
RAM vector reads, computed handlers and RTE all execute normally. The second
handler of each kind edits saved SR.I to suppress further requests and returns
to ordinary guest code that publishes the final sum and completion marker.

The independently authored records are H count 2, H seed+14, V count 2, V seed+22,
final H seed+V seed+36 and completion `0x600D`. Both H samples must have identical
line counters and HBLANK set; both V samples must share a line in 224–262 with
VBLANK set. Seeds 3/5 produce17/27/44, and seeds 23/41 produce37/63/100. Masking
with SR.I=15 or omitting only the master's COMREG store leaves both IRQ counts
and completion zero. Two cold boots per variant repeat the assertions.
[Native Machine tests](../tests/slave_raster.rs) and the registered
[production Wasm check](../scripts/slave-raster.mjs) use identical ROM bytes and
controls. Hosts only load the ROM/operands, advance execution and inspect state.
Native records are identical across chunks 1/3/17/10000 with and without passive
inspection. Rational board-edge traces also agree across those chunk sizes.

## Inherited fixture corrections

Older guests assumed no slave raster input. Their unused inputs now need explicit
masks; no IRQ, arithmetic, DMA, reset or peripheral assertions were removed.

| Original fixture | Correction | Current hash |
| --- | --- | --- |
| DUAL | Slave FRT priority6→7, SR.I=6; original bidirectional capture handlers, two starts/stops and NMI records retained | expanded ROM `fdd1302400b98b911ac32f257f1ff7aa4170d096f4044af1e2026446100f2896` |
| BSC | SR.I=2 during level 5 refresh, then15 after completion | expanded ROM `8ef46a1e059ff3618f1ca2faf0b329bbcb4b24fa85177639906c3f7d50137511` |
| DIVU | SR.I=6 during level 10 DIVU and UBC tests | hex text `0c68ce19d942105c9c5b604cd2aeb0d0b8afb610e14a21a395ed4f493a4c8494` |
| DMAC bus | Internal DMA priority6→7 and SR.I=6; all eight per-CPU transfers/IRQ records retained | hex text `4bbe6c438f8c16c3e1c3147d2a01b9cf686be50e90f6227c035a2e52293bd3ad` |
| UBC IF overrun | SR.I=6 while testing compatible/native fetch breaks; vector12 and original opcode mutations retained | expanded ROM `7a8cac79361a3c9ae7bf867a713acf690972fd4b94a192784defdc40109f47a5` |

These changes follow STN-28 §2.3's application masking requirement and SH7604
table 5.4's external-before-equal-internal rule. BSC, DMAC and the UBC IF guest originally failed
the full suite after adding live inputs; their corrected focused suites pass.
DUAL/DIVU now also explicitly mask inputs they do not handle. Historical validation
hashes describe their original bytes; current conformance entries are updated.

## Validation results

Temporarily replacing the slave external request with the inherited unconditional
`None` made the independent boundary test fail at clock 1280 (expected IRL2/0x41,
observed none). Both executing native guest tests failed with all-zero computed
records and missing completion `0x600D`. The exact implementation was restored.
This mutation changes neither fixtures nor expected assertions. The same guest
bytes and independent assertions are registered for browser Wasm.

Environment: macOS arm64; Rust/Cargo 1.99.0, wasm-bindgen 0.2.129,
Node 24.12.0, npm 11.6.2 and locked Playwright 1.58.2.

| Check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 940 tests across 87 test/doc-test binaries, zero failures and zero ignored tests |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed, including the final replay-store and bounded-restart probes |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| All 86 workflow-registered guest/media reproduction commands | Passed; final DMAC/UBC fixture changes also reverified after regeneration |
| `sh scripts/build-web.sh` | Passed; original discs and production Wasm/bindings built |
| Complete `npm run test:browser` | Passed in Chromium 145.0.7632.6 under `/saturn-rust/`, no required skips; new raster and all existing integration assertions passed |
| Markdown local links and `git diff --check` | Passed |

The nine final native raster tests were also rerun after strengthening the replay
probe and bounding waits. The diagnostic remains identical in native/Wasm;
Star Collector, Prism Courier and Hardware Lab stay in the unabridged suites.

## Remaining scope

This is a partial #17 increment. It implements #47's normal slave raster path;
remaining CPU/peripheral semantics, board arbitration/reset/timing, SCU/DSP,
video/audio, CD/media/HLE, SMPC and comprehensive integration requirements stay
mandatory. No proprietary firmware, commercial media, physical console, human
approval or live Pages inspection is required. Commercial-game compatibility is
unverified; green tests do not establish umbrella hardware completion. #17 stays
open. Required post-merge verification: none.
