# SCSP master volume numerical contract

Related to #29 and #17. This increment covers the shared 16-bit output stage;
it does not establish analog or bit-exact chip/DAC conformance.

The [Sega sound FAQ, Overview, volume-settings question](https://www.infochunk.com/saturn/segahtml_en/faq/sound/p01_10.htm)
specifies nominal 3 dB MVOL steps over slots, DSP and CD-DA.
The [SCSP User's Manual §1.3, MVOL and stereo routing](https://www.infochunk.com/saturn/segahtml_en/hard/scsp/hon/p01_30.htm)
defines zero as mute, F as unity, and MVOL after source mixing.
[ST-077-R2](https://antime.kapsi.fi/sega/files/ST-077-R2-052594.pdf),
printed pp.80–88 (§4.2 MIXER, especially tables 4.27–4.31 and p.88 MVOL),
places MVOL after direct/effect/external mixing and explicitly says lowering
MVOL cannot remove existing overflow distortion. Printed p.35 specifies zero
reads for write-only fields. Printed p.ii describes half amplitude as -6 dB.

We model those nominal steps using the manual's binary amplitude convention:
for setting `v=1..15`, `a(v)=2^((v-15)/2)`, and `a(0)=0`. This choice makes
13 exactly half amplitude and 1 exactly 1/128. It differs from interpreting each
step as exactly 3.000 dB (`10^(-3*(15-v)/20)`): the binary step is about
3.010300 dB, at most 0.1442 dB different over fourteen steps. Neither interpretation
is a measurement of SCSP coefficients. This is the explicit numerical model used
here, consistent with the existing binary send levels and pan coefficients.

Each positive coefficient is independently rounded to the nearest integer in Q15,
`c=round(32768*a)` (nearest, ties upward). There are no ties in the sixteen
coefficients. The implementation stores this constant table and uses integer
arithmetic only. Each channel's result is
`trunc_toward_zero(clamp(mix,-32768,32767)*c/32768)`.
Negative results therefore round symmetrically with positive results; this is
not an arithmetic right shift. All intermediate products fit in signed 64 bits.
Zero mutes exactly, F is an exact identity after clipping. Coefficient rounding
contributes at most 0.5 output LSB at full scale and final truncation less than
1 LSB: absolute error is less than 1.5 LSB relative to the ideal binary model.
Tests of this quantized model require exact integer equality (zero tolerance).

| MVOL hex | Nominal attenuation dB | Binary amplitude | Q15 coefficient | Output for +16384 / -16384 |
| --- | ---: | --- | ---: | --- |
| 0 | mute | 0 | 0 | 0 / 0 |
| 1 | -42 | 1/128 | 256 | 128 / -128 |
| 2 | -39 | sqrt(2)/128 | 362 | 181 / -181 |
| 3 | -36 | 1/64 | 512 | 256 / -256 |
| 4 | -33 | sqrt(2)/64 | 724 | 362 / -362 |
| 5 | -30 | 1/32 | 1024 | 512 / -512 |
| 6 | -27 | sqrt(2)/32 | 1448 | 724 / -724 |
| 7 | -24 | 1/16 | 2048 | 1024 / -1024 |
| 8 | -21 | sqrt(2)/16 | 2896 | 1448 / -1448 |
| 9 | -18 | 1/8 | 4096 | 2048 / -2048 |
| A | -15 | sqrt(2)/8 | 5793 | 2896 / -2896 |
| B | -12 | 1/4 | 8192 | 4096 / -4096 |
| C | -9 | sqrt(2)/4 | 11585 | 5792 / -5792 |
| D | -6 | 1/2 | 16384 | 8192 / -8192 |
| E | -3 | sqrt(2)/2 | 23170 | 11585 / -11585 |
| F | 0 | 1 | 32768 | 16384 / -16384 |

The literal bipolar vectors and independently authored source sums in
[`src/sound/tests.rs`](../src/sound/tests.rs) were established and run against the
inherited linear expression before changing `Scsp::push_output`. They exercise
real PCM reads, an executing DSP program, external stereo and combinations,
including silence, asymmetric signs, clipping, signed truncation, queued output,
SCSP reset and reference-clock advancement chunks 1/3/17/10000.
The production change is confined to that common final output calculation.

The output-stage latch convention is deliberately separate from measured hardware:
completed writes visible at `end_sample` affect that frame's output; a write after
that deadline affects subsequent frames. Already queued PCM retains its original
gain. The sound scheduler orders an SCSP deadline before a sound-CPU write at the
same reference-clock deadline. These rules are deterministic emulator conventions;
no sub-sample MVOL latch measurement is claimed. SCSP reset restores mute and
MEM4MB default while retaining previously queued audio and sample counters;
cold Machine reset clears the audio queue/counters and requires guest setup again.

## Original executable fixture

[`build_scsp_mvol.py`](../tests/fixtures/build_scsp_mvol.py), the generated
[`scsp-mvol.s`](../tests/fixtures/scsp-mvol.s), hex ROM, operand table and mixed-track
CUE/data/audio files are original content under the adjacent
[MIT license](../tests/fixtures/LICENSE). No personal firmware or commercial assets
are used. Reproduce with `python3 tests/fixtures/build_scsp_mvol.py --check`.

The SH-2 uploads a real 68k program, writes the common word and low byte, starts
runtime CD playback at track 2/FAD151 and performs timed mute/restore low-byte
writes. The 68k computes `[4096,-4096,-8192,8192]`, configures a looping PCM voice,
uploads a DSP program computing half of MIXS0 into EFREG0, writes MVOL's low byte
and continues incrementing a mailbox. Both CPU views check a high sound-RAM
sentinel to detect lost MEM4MB. The generated CD right channel is `-1000-i` for
frame `i`, with left 1024; its nonrepeating progression detects replay after mute.
The SH-2 and 68k counters advance in the muted interval. Source routing variants
isolate direct PCM, executing DSP, external CD and their sum.

[`tests/scsp_mvol.rs`](../tests/scsp_mvol.rs) and
[`scripts/scsp-mvol.mjs`](../scripts/scsp-mvol.mjs) author their expected stereo
arithmetic independently of production code and this encoder. They choose only an
initial waveform phase/CD frame index, then require the entire checked buffer to
advance continuously from it. They cover gains 0/1/D/E/F, changed waveform,
coefficient and audio operands, an omitted-volume-write control and repeated cold
boots. Full unity/mute traces must have one contiguous muted interval and agree
exactly after restoration, with equal CPU/sample progress. Native fixed-horizon
Machine traces additionally agree across instruction chunks 1/3/17/10000.

The browser runs this same guest/media in rebuilt production Wasm, then delivers
its verified PCM through production `AudioOutput`/`AudioWorklet` and checks peak,
received/rendered frames, underflow and zero drops at every distinguishing gain,
including guest mute. Host muted and blocked states remain separately observable;
they never supply the MVOL amplitude evidence. The existing browser privacy and
missing-file checks continue to run. Test media remains outside the public web
asset boundary.

Artifact hashes, actual commands/results and mutation evidence are recorded in
[Issue #29 validation](validation-issue-29.md). Envelope rates, 18-bit output,
other sound timing gaps and all remaining umbrella hardware requirements remain
mandatory; commercial-game compatibility is unverified.
