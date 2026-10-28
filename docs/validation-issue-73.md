# Issue #73 validation: VDP1 HDTV / 31 kHz

Related to #73 and #17. The scoped mode implementation leaves #17 open and
preserves all other mandatory hardware-completion criteria. No personal firmware,
physical monitor, human approval, live Pages inspection or post-merge verification
is required. This node performs no push, PR, merge, comment or Issue mutation.

## Implementation and primary reconciliation

The [primary-derived mode table](vdp1-hdtv.md) records geometry, all TVMR
encodings, the four output coordinate mappings, programming windows, erase
behavior and the distinction between hardware requirements and emulator phases.
VDP1 §4.1 table 4.2 permits TVM=4, 16-bit/noninterlaced, with DIE clear, and
prohibits 5–7. §1.2 specifies a 320/352×240 source whose physical output repeats
colors over 2×2. VDP2 §2.1 exposes 320/352 or 640/704 horizontal dots, yielding
`(x,y/2)` or `(x/2,y/2)`. This is a source-coordinate conversion, with existing
storage and no rotation parameters. §4.4 requires automatic blank erase and no
display erase, including when VBE is clear. The existing integer raster and
retained erase engine already supply the literal capacities 29340 and 53136.

The chapter 4 update list and ST-210 item 3 were reconciled before implementation;
they agree with the existing delayed VBE and word-register contract. No technical
assumption in #73 needed correction, so the instruction against GitHub writes
requires no exception. Unspecified phase choices retain their documented emulator
status. [Production changes](../src/vdp1.rs) accept the additional encoding and
exclude it from display erase; [sprite readout](../src/vdp2/rotation.rs) performs
the independent exclusive coordinate mapping. Other mode paths remain separate.

## Native and original guest evidence

[Bus vectors](../src/bus/hdtv_tests.rs) use both scheduled SH-2 MMIO paths,
checking MODR before and after data completion. They preserve buffered diagnostics
for prohibited 5–7 encodings and existing host byte/alignment/atomic-pair behavior.
Literal storage corners include x511/y255, adjacent rows, and out-of-range
coordinates. Erase vectors use `(852−200)×45=29340` and
`(848−200)×82=53136`, assert the last word just before/at blank end and the first
untouched word afterward, retain pending B separately from captured A, and check
both physical destinations. They verify no display erase and no post-OUT suffix
writes with chunks 1/3/17/10000 and passive reads/previews. PTMR1/2, automatic/manual
changes, END, COPR/EDSR/draw-end IRQ, TVM transitions and SYSRES cancellation are
covered. Separate reset vectors cancel a pending sample, a partially published
erase job and an outstanding mode word on both SH-2 paths. The
[device capture vectors](../src/vdp1/erase_tests.rs) additionally cover TVM=4
with VBE clear/set and pending bounds on both physical buffers. Internal fixture
initialization is not a claim about power-on values or prohibited write windows.

The [MIT builder](../tests/fixtures/build_vdp1_hdtv.py) creates two independent
original synthetic BIOS guests. Their source, sparse bytes and expanded-ROM
hashes are reproduced by `--check` in the required fixture registry. The
[Machine tests](../tests/video_transactions.rs) and
[production browser scenario](../scripts/vdp1-hdtv.mjs) execute the same bytes.
The command guest draws through VDP1 commands, checks 16 framebuffer words plus
an unsigned checksum, four right-edge words and the final storage word, reads
MODR/EDSR/COPR/SCU IRQ, and publishes a stable framebuffer. The expected source
texture and literal RGBA oracles are independently authored in the tests, not
emitted by a renderer or copied from observed output. Palette priority 2 loses to
background 3, RGB/palette priority 4 wins, and zero reveals the opaque background.
Both horizontal densities and vertical repetitions, adjacent rows, source row 239
and rightmost columns are checked. Each mode/color/X/omitted-draw variant repeats
two cold boots in native and browser. Browser assertions additionally read actual
Canvas pixels after production `advanceFrame` under `/saturn-rust/`.

The separate erase guest keeps VBE clear and requests automatic changes. It
initializes distinct physical buffers, captures a full-screen 1234 erase, writes
pending 5678 and smaller bounds, and observes actual capacity boundaries and the
held suffix. No host writes framebuffer, status, IRQ or completion results.
The existing sampled-VBE guest, backgrounds-only exclusive guest, rotation,
standard-TV, interlace, memory ownership and all original demos remain required.

## Negative controls

Before changing production code, `cargo test --locked --lib hdtv_` failed all
three new bus vectors. The direct TVM write returned `UnsupportedDevice`; the
scheduled word left MODR 1000 instead of 1004. The remaining new tests were then
completed and the buffered-diagnostic expectation corrected to the established
asynchronous error path; production transaction semantics were not changed.

With TVMR=4 acceptance retained, a temporary mutation restored the inherited
`0xA` display-erase exclusion and unscaled sprite `(x,y)` readout:

```
cargo test --locked --test video_transactions guest_hdtv -- --nocapture
```

Both independently authored guest oracles failed, exit 101. At mode 8004, source
x0/output y1 was yellow `[255,255,0,255]` instead of repeated red
`[255,0,0,255]`. The erase guest observed 1234 at word 29340 instead of untouched
FFFF. Guest framebuffer records still came from real execution. The mutation was
restored immediately; no acceptance oracle or budget was weakened. This proves
that accepting TVMR 4 alone cannot satisfy either readout or erase acceptance.

## Executed checks

Local validation on 2026-10-28. Hosted CI, deployment and physical-device results
are not claimed. The complete browser suite passed its assertion, artifact-digest
and inventory aggregation barriers; no required scenario was skipped.

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed 1,068 tests, zero failed/ignored |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `sh scripts/check-fixtures.sh` | Passed all 96 registered commands |
| `sh scripts/build-web.sh` | Passed, release Wasm rebuilt with pinned bindgen 0.2.129 |
| `npm run test:browser -- --group=videoTransactions` | Passed 1 required group, including all new and existing assertions |
| `node --test scripts/ci-runner.test.mjs` | Passed 12 tests, zero failed/skipped |
| `npm run test:browser` | Passed all 66 groups (30 shared + 36 scenarios), no required skips; aggregation passed, exit 0 |
| `git diff --check` / staged equivalent | Passed; new documentation relative links resolve |

Native checks use installed Homebrew Rust 1.94.0; Wasm build/Clippy use the
installed rustup stable Rust 1.99.0 compiler and target, selected together on PATH.
Browser checks use Playwright 1.58.2's installed bundled Chromium 145.0.7632.6. No usage reset,
additional allowance purchase, or model/provider switch was used. Required CI
coverage and publication dependency gates are unchanged; the runner's exact
fixture count increases from 95 to 96 and explicitly requires the new builder.

The complete browser run finished in 201,822ms. Assertion worker totals were
181,945ms for shared and 201,481ms for scenarios, with successful cleanup and
aggregation. The tested web-directory path/size/content SHA-256 was
`c0cf963c8adbb80bcc5863c7152e23eea8f11c08e004b3ff982d97061d4d04b2`.

## Artifact hashes

All originals are covered by [the adjacent MIT license](../tests/fixtures/LICENSE).

| Artifact | SHA-256 |
| --- | --- |
| `vdp1-hdtv.s` | `8f711e8e3ea9dd12af3504f7cc9f1d8b7be28e51bc64298f47d1a6590f7c9ee0` |
| `vdp1-hdtv.hex` | `fa87e732abe3ae28f5becadb93ac1eecba98738600b9e01ad2f520908d2f1dc1` |
| Expanded command ROM | `757c5581da80da21deb3580bb73c7d9dc56b3ccf8cf7a46d40d6940397488bbc` |
| `vdp1-hdtv-erase.s` | `3bfc474a928a489758cb4127a0827984fab886cda1285d57de786d72bb0cfc5c` |
| `vdp1-hdtv-erase.hex` | `86e4639b723df891ea2efc8f96fad8be187f9da0ddae5a3603ac17d2ed0fc7af` |
| Expanded erase ROM | `3201055a7ae01009210e3e71135d4689c78be692030e1f5239e6703722827735` |

## Remaining deviations

This establishes the scoped HDTV video programming path. Physical erase bus
cadence, sub-line monitor synchronization, unrelated half-line work and general
rasterizer refinements are not established. CPU/peripheral semantics,
multiprocessor synchronization, bus/cache/reset/arbitration, SCU/DSP, remaining
video/audio/CD behavior and timing, media/HLE/SMPC services and comprehensive
integration remain mandatory under #17. Genuine BIOS operation and broad
commercial-game compatibility remain unverified. No umbrella completion criterion
is met by this increment, and no post-merge verification is required.
