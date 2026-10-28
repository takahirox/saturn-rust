# Issue #67: retained SCSP synthesis publication

Related to #67. Related to #17. This is partial hardware-completion progress;
#17 remains open, with the unmet scope in the [conformance matrix](hardware-conformance.md).
No push, PR, merge, Issue update/comment or post-merge verification is part of
this node. No human testing/approval, personal firmware or live Pages inspection
is required.

## Contract and independent evidence

The [source-derived stage/consumer table](scsp-synthesis-pipeline.md) identifies
ST-077-R2 printed pp.20–21/47/55–57/83–86/106, Figures
3.2/4.18/4.30–4.31/4.56/5.1 and the later manual Figures 4.24–4.25.
Figure 4.18 connects the waveform level result to DIRECT and DSP; IMXL/ISEL
route that result to MIXS, and Figure 5.1 routes MIXS to DSP INPUTS. This
establishes the post-level consumer ordering independently of stack latency.
The table distinguishes documented relative stages from unspecified absolute
interface phase, register capture and equal-edge ordering conventions.

Before changing production behavior, the independently authored native
`primary_pipeline_deadlines_keep_direct_and_executing_dsp_private_until_level_completion`
vector failed on the inherited path (exit101): DIRECT already contained
`[-4096,-4096]` before the required post-level publication, expected `[0,0]`.
It now passes for slots0/7/31, both PCM widths and chunks1/3/17/10000. Retained
signed operands `-32768,+24576` interpolate to `-4096`, full IMXL gives
20-bit MIXS `-65536`, and executing half-gain DSP yields `-2048` only after
publication. DSP reads immediately before and on the publication edge are
separately observed through the following instruction's shifter.

[Production pipeline](../src/sound/pcm.rs) keeps seven bounded stages. OP1
advances phase/LFO, OP2 reads FM and retains waveform addresses, OP3 separately
grants/completes operands, OP4 interpolates/commits the retained envelope,
and completed level results publish at OP7. The existing five-interval
[stack queue](../src/sound/synthesis.rs) commits that same result without a
second artificial delay. Generation tags prevent an old EG operation from
undoing a later KEYOFF/retrigger. Idle slots run at their own deadlines;
SCSP RESET cancels all stages while SNDOFF keeps synthesis running.

[Native vectors](../src/sound/pcm/tests.rs) also cover simultaneous DIRECT/MIXS
sends, once-only contributions, mutation after reads, sample wrap, sparse/dense
idle LFO/noise/stack parity, activation at multiple phases, draining after idle,
reset at each stage, independent CPU/SCSP domains and passive inspection.
The unchanged standalone stack test still covers all selectors/generations;
new scheduled tests obtain actual DRAM results through OP7 and check all32
carriers × all64 selectors, distances on either side of five, wrap and STWINH.
[Shared-memory vectors](../src/sound/memory/tests.rs) and both
[SH-2/SCU adapter vectors](../src/bus/sound_tests.rs) retain substantive
PCM/DSP/refresh/DMA/main/CPU ownership and per-word mutations.

Inherited PCM timing assertions moved grants from OP1 to OP3 (+8 memory steps)
and publication from read completion to OP7 (+24 from OP1). A cold first sample
contains slots0..25, with slots26..31 completing in the next sample. CA is
observed at OP2 rather than before its address calculation; slot31 crosses the
sample edge. These changes follow the primary stage table and preserve signed
arithmetic, memory geometry, waveform, pan, envelope, mutation and ownership
assertions. For example, the all32-voice constant32 vector produces 832 in its
cold sample and 1024 thereafter; the separate PCM8 waveform with only its first
byte nonzero produces 26*256 then 6*256, rather than fabricating later samples.

Hardware Lab's old guest DSP read at instruction4 relied on the collapsed
producer path. Complete native/browser validation caught the missing wet
component. Its original [68k uploader](../tests/fixtures/build_sound.py) now
places the same four multiply/store pairs at instructions24..31, after OP7.
The original coefficient mutations, diagnostic arithmetic, wet/dry stereo,
CD-DA, gameplay, backup, dual CPUs, video and restart assertions are unchanged.
The regenerated tracked disc SHA-256 is
`a0ad3c3e357e656052e808eb793b1dafa3827d71096013ba112a81b1d4ee23b9`.
This supersedes the instruction4 observation in the historical #17 validation
record; it does not weaken its arithmetic oracle. The original MVOL diagnostic
had the same instruction4 assumption. Its [builder](../tests/fixtures/build_scsp_mvol.py)
now reads MIXS0 at24 and stores EFREG0 at25. All original gain, wet/dry,
clipping, CD-DA, byte-write, omitted-write, mute/resume and chunked progress
expectations remain unchanged. Its expanded ROM SHA-256 is
`a76f32429a220b50c89aca7ab13dda35cd5e402cad45a97fd21d34a7ca08472e`;
hex `51fbaf609c0ab1e5cc9cb611381d3f62da4c43fbd2f9265cd1ece1331f89591d`,
mutation offsets `f4bc52935d59ef1c90d83dd2b698b3d91b21409b0f56db41cab492b427981d46`.
Its SH-2 source and mixed-track media bytes are unchanged.

## Original guest and negative control

The [builder](../tests/fixtures/build_scsp_pipeline.py),
[source](../tests/fixtures/scsp-pipeline.s),
[hex ROM](../tests/fixtures/scsp-pipeline.hex),
[mutation offsets](../tests/fixtures/scsp-pipeline.tsv) and adjacent
[MIT license](../tests/fixtures/LICENSE) contain original authored content.
An actual SH-2 copies the sound program and starts the 68k via SMPC. The 68k
writes voices0/31 and waveform operands, uploads an executing DSP program,
waits, then copies DSP-computed sound-RAM words into its mailbox. The SH-2
reads that mailbox into its own RAM records. Host code supplies no results.

The DSP multiplies MIXS0 by .5 at steps22 and24 (before/at slot0 OP7), and
MIXS1 at steps18 and20 (before/at slot31 OP7 in the following sample).
Subsequent odd TABLE/NOFL stores write four RAM words and EFREG0..3.
A separately computed marker is `600D/2 = 3006`. For default operands
A=`2000`, B=`4000`, the records are `[00001000,00002000,30060000,0000600D]`:
early words zero, late words A/2 and B/2. EFREG1/3 route to right/left,
respectively; settled stereo is exactly `[8192,4096]`. Two signed operand
mutations and separate omitted-slot31-send/KEYON controls have authored
half-gain expectations, with two cold boots per case.

The unchanged [Machine oracle](../tests/scsp_pipeline.rs), using identical final
guest bytes, was copied to an isolated checkout of inherited main
`03b6a0580b19cabcf68b92994d06c1d0b6865311` and run with
`cargo test --locked --test scsp_pipeline -- --nocapture`. It failed (exit101):
records `[10001000,00000000,30060000,0000600D]`, expected
`[00001000,00002000,30060000,0000600D]`. This independently rejects both premature
slot0 publication and missing late-slot carry; it is not a successful check.

The rebuilt production Wasm runs the same bytes/expectations under
`/saturn-rust/` through [scsp-pipeline.mjs](../scripts/scsp-pipeline.mjs), called
by the existing required `scspPcm` group. Exact guest records, deterministic
settled stereo and cold-boot parity are asserted. The production AudioWorklet
renders each case with an authored peak; underflow, no normal drops, an exact
8193-frame oversize drop and suspended-context blocked state remain observable.
The new builder is registered in `scripts/check-fixtures.sh`; the runner's
fixture inventory increases from92 to93 without removing any original command.

SHA-256:

| Artifact | Hash |
| --- | --- |
| Expanded 512-KiB ROM | `d6598408369e4d3d32fc53c1624aab6b327b46b6bdbf39397e2a2bf5ca00ebf9` |
| Builder | `3b799fbdd26beabccd8b57e9546701abe09a82fcaf830ea08066b1db450b20ec` |
| Source listing | `514005b59af738d825b8ac5d9baec89026b1185aa18d15ccb5c2e7de6721ca15` |
| Hex fixture | `1f634523537f4ec5dcc9f401ab8c9fc2865ea3c009bf82a6548bd8bfd055d724` |
| Mutation offsets | `f802853e0690cb83bf8facfedd2234aad46b0ba53e433dc4262d47b0e214bf1b` |
| Tested production Wasm | `c3c99285d3a999e2e91154408b644933b8852dde1b1be39d075869f5417f05c5` |

## Validation

Local checks on 2026-10-28 (macOS ARM64; native Rust1.94.0, rustup stable
Rust1.99.0 for Wasm and additional native Clippy; Node24.12.0, Playwright1.58.2, Chromium145.0.7632.6):

| Command | Actual result |
| --- | --- |
| `cargo fmt --check` | Pass |
| `cargo build --locked` | Pass |
| `cargo test --locked` | Pass: 1,022 tests across94 harnesses, zero failures/ignored; all original integration suites and demonstrations included |
| `cargo test --locked --lib` | Pass: all660 library tests after the final memory-resumption and simultaneous-send oracle refinements |
| `cargo clippy --locked --all-targets -- -D warnings` | Pass |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Pass with rustup stable |
| `sh scripts/check-fixtures.sh` | Pass: all93 registered builders, including both corrected existing guests and the new pipeline guest |
| `sh scripts/build-web.sh` | Pass with installed rustup stable and pinned wasm-bindgen0.2.129; original three demonstration assets rebuilt |
| `npm run test:browser` | Pass: all65 groups, shared30/30 and scenarios35/35; complete aggregate and artifact-digest verification passed, 180.685s wall time, two workers |
| `node --test scripts/ci-runner.test.mjs` | Pass: 12 tests, zero failures/skips |
| `git diff --check` and staged whitespace check | Pass, including new files |

The full browser suite exercises original gameplay/results/restart, both CPUs,
DMA/IRQs, CD-DA, backup and both input ports, plus production Canvas and
AudioWorklet paths. There are no required skips. Final memory and
simultaneous-publication checks retain exact grant/completion and literal
DIRECT/MIXS arithmetic assertions; follow-up library coverage passes in full.

Initial complete checks exposed the two old guest programs' instruction4
MIXS assumptions; moving their reads to24 preserved all substantive arithmetic
oracles. Those failures were corrected, not accepted as skips. The first
browser-runner check detected the fixture inventory change (93 versus92);
its explicit inventory now requires the additional builder. The first web-build
attempt used Homebrew Rust without its Wasm standard library; selecting the
already installed rustup stable toolchain succeeds. No requirement was weakened
for either setup correction. The independent inherited negative controls above
are deliberate exit101 results and are separate from these passing suites.

Remaining deviations are numeric envelope rates, exact noise polynomial/seed,
unrelated DSP instructions and physical interface timing, plus all other unmet
#17 umbrella scope. Completion of this increment does not establish hardware
completion, deployed Pages state or commercial compatibility.
