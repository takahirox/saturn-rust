# Issue #29 validation: shared SCSP master volume

Related to #29 and #17. The change implements the bounded MVOL increment;
#17 remains open and no prior administratively closed tracker is reopened.
No post-merge verification is required. No external Issue, PR, deployment or
live Pages state is used as hardware-completion evidence.

## Specification, implementation and independent expectations

The [MVOL numerical contract](scsp-mvol.md) records precise primary references,
all sixteen settings, nearest-Q15 coefficients, symmetric signed truncation,
exact mute/unity, the less-than-1.5-LSB binary-model error bound and the separate
unmeasured sub-sample latch convention. Nominal 3 dB is modeled using binary
half-power steps, not claimed as measured chip coefficients or an analog/bit-exact
DAC result. No acceptance criterion was removed or weakened.

`src/sound.rs::Scsp::push_output` is the only changed production calculation.
It clamps the common stereo sum first, then applies MVOL once, after direct PCM,
executing DSP effects and EXTS/CD-DA routing. Register and scheduler behavior is
retained. The tests cover both positive and negative clipping, silence, fractional
signed truncation, opposite-sign/asymmetric channels and every MVOL setting.

Independent literal bipolar vectors in `src/sound/tests.rs` and arithmetic in
`tests/scsp_mvol.rs` / `scripts/scsp-mvol.mjs` were authored before changing the
production calculation. The initial inherited-expression runs failed at MVOL1.
After implementation, restoring the inherited `clamp(mix)*MVOL/15` expression
again failed both required assertions (exit 101, assertion failures rather than
build failures):

- `cargo test --locked --lib mvol_all_settings_render_independent_bipolar_pcm_vectors -- --nocapture`:
  actual `[1092,1092,-1092,-1092]`, expected `[128,128,-128,-128]`.
- `cargo test --locked --test scsp_mvol guest_mvol_amplitudes_distinguish_pcm_dsp_runtime_cdda_and_combined_sources -- --nocapture`:
  direct PCM at gain1 produced `[-17,-273,-34,-546,34,546,17,273]`,
  rejected by the independent guest amplitude oracle (correct values have
  magnitude 2/32 or 4/64 according to waveform phase).

The mutation was restored before final positive validation and is not committed.

## Execution and reset evidence

Seven focused native unit regressions cover all sixteen gains, independently
summed PCM/DSP/EXTS combinations, both clipping polarities, signed fractions,
continued muted PCM phase/DSP execution/sample counts, output-boundary writes,
high/low byte preservation, write-only zero reads, queued PCM and SCSP reset.
Reference-clock boundary checks and real 68k byte-store traces run with clock
advancement chunks 1/3/17/10000. Already queued unity/half/low/muted frames retain
their original amplitudes after later writes and SCSP reset.

The original MIT guest/media described in [the fixture contract](scsp-mvol.md#original-executable-fixture)
runs natively and in rebuilt production Wasm. It uses actual SH-2 and 68k writes,
a computed bipolar PCM waveform, an executing MIXS0 effects program, and guest
CD play commands against generated mixed-track media. Four routing variants
isolate PCM, DSP, runtime CD-DA and their combination; each is checked at gains
0/1/D/E/F across repeated cold boots. Both CPU views read the high sound-RAM
sentinel after byte MVOL writes, proving MEM4MB preservation. Cold resets clear
audio counters/queues and replay guest setup.

Waveform 16→8/24, coefficient .5→.25/0 and CD left 1024→2048 mutations change
independent expected stereo values. Omitting the volume writes preserves mute.
The SH-2 loop reaches 45000 iterations; its snapshots prove the 68k counter
continues through mute. A fixed-horizon unity reference and mute/restore trace
have equal CPU/sample progress and one contiguous zeroed interval, followed by
identical audio. The nonrepeating CD right channel prevents a replayed track from
passing. Native Machine traces also agree across instruction chunks 1/3/17/10000.

The browser executes this same fixture/media under `/saturn-rust/` and checks
exact sample amplitudes before delivering that PCM to the production
AudioOutput/AudioWorklet. At gains 0/1/D/E/F, received/rendered counts, peak,
underflow and zero drops are checked, with muted and blocked host states separately
observable. No host volume adjustment or manufactured PCM supplies guest gain
evidence. Original full-volume fixtures and the complete existing browser suite
remain required, including Star Collector, Prism Courier, Hardware Lab,
sound/DSP/CD-DA, input, backup, reset, local-file errors and privacy.

## Reproducible artifacts

All new fixture source/media is original [MIT content](../tests/fixtures/LICENSE).
Reproduce with `python3 tests/fixtures/build_scsp_mvol.py --check` (also registered
in the existing CI reproduction block and setup instructions).

| Artifact | SHA-256 |
| --- | --- |
| Expanded 524288-byte BIOS | `ac4e59bf7c4ba071df129b7daeec2edaf6a551e7c4a1ef61c9fbd4c72fd51bb1` |
| `scsp-mvol.hex` | `af8ffc961cb818e9d085f8003866182c84cb7eee36b78d4a6e50d57af5d62807` |
| `scsp-mvol.s` | `a9ee3fc876be6d3051051390238199465229086fcd348b3503726cd2acb02ad2` |
| `scsp-mvol.tsv` | `810fc7575a092626e2358e3322bd5525cef32b94c85526e42696d6bc2d3018ed` |
| `mvol-data.bin` | `e5a00aa9991ac8a5ee3109844d84a55583bd20572ad3ffcd42792f3c36b183ad` |
| `mvol-audio.bin` | `dda5161cf4b892c2b22f87f4a34dd2cab9f061bd680a8aa95d2011a46d5feeb4` |
| `mvol.cue` | `bc6490bb8c934400b56bee5b0576d32a222c90a61011786555bef86fbbbdc2f0` |

## Validation results

Executed locally on 2026-10-23 with rustup stable `rustc 1.99.0`, matching
`wasm-bindgen 0.2.129`, Node `24.12.0`, and headless Chromium `145.0.7632.6`.
Final positive validation used the restored staircase implementation.

| Required command/check | Actual result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 818 tests, 0 failed, 0 ignored; 78 result groups including doc tests |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| All 77 fixture/media `python3 ... --check` commands in `.github/workflows/pages.yml` | Passed, including the new MVOL fixture and all original demo discs |
| `sh scripts/build-web.sh` | Passed; rebuilt production release Wasm and original demo media |
| `npm ci --ignore-scripts` | Passed, 0 reported vulnerabilities |
| Complete extended `npm run test:browser` | Passed under `/saturn-rust/`, no required skips; final `Browser smoke passed (145.0.7632.6)` |
| Final isolated `scspMvol` run using the same Playwright/headless/project-prefix workflow | Passed, including added explicit cold-reset queue/counter assertions |
| Restored-linear source-path and guest negative controls | Both failed at the intended MVOL1 assertions (required negative evidence) |
| New documentation relative links and `git diff --check` | Passed |

The production `web/pkg/saturn_rust_bg.wasm` used by these browser runs had
SHA-256 `4d5e42a8ad39d3310de907791b89faa7de72f4c6df3aff5126a5087d9b1ecd21`.
The ignored build output is reproducible through the existing build script and
is not committed. Test-only media is retained in `tests/fixtures/`; no additional
public web assets or dependencies were introduced.

The complete native/browser runs preserve existing exact full-volume vectors
and the required Star Collector, Prism Courier, Hardware Lab, sound/DSP/CD-DA,
input, backup, reset and privacy checks. Earlier development-time Clippy findings
required converting constant-sized chunk iterators to `as_chunks`; those were
fixed and both final warnings-denied Clippy runs passed. There are no remaining
failed, blocked or unperformed required checks.

## Remaining deviations

No measured sub-sample MVOL latch rule, analog DAC response or bit-exact hardware
coefficient claim is made. The numerical quantization/latch conventions are
explicit above. Envelope rates, 18-bit DAC operation and other sound timing gaps
remain separate required umbrella work. Remaining CPU/peripheral,
multiprocessor/bus/reset, SCU/DSP, video/audio, CD/media/HLE, SMPC and comprehensive
integration requirements remain mandatory under #17. Commercial-game compatibility
remains unverified. No personal firmware, commercial game, physical console,
human listening/approval or live Pages inspection was needed.
