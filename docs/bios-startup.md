# Local BIOS startup

Related to #17. This is a partial increment; no firmware, screenshots,
private instruction dumps or extracted ROM data belong in public assets or test
fixtures. The supplied authorized 512 KiB Japanese BIOS reaches its own Japanese
date/time setup screen in native Machine execution and production browser Wasm.
Date confirmation proceeds to the actual Saturn startup logo and then the BIOS
CD player in production Chromium. Arrow input changes its selected control and
the firmware-rendered label; no host UI substitutes for the firmware.
The earlier unavailable firmware reports in #103/#109/#111 remain historical;
this observation does not retrospectively identify their transactions.

## Findings and contract

The initial run stopped at frame 38 on an actual SCU IST longword store. After
correcting that endpoint, initialization exposed the missing VDP2 VRSIZE word,
reserved words and stores to read-only VDP2 status/counter words. These changes
are shared hardware behavior, independent of firmware identity and PC.

- [System Program reference, SYS_SETSCUIM and SYS_CHGSCUIM](https://www.infochunk.com/saturn/segahtml_en/prgg/sysg/prog/hon/p02_11.htm)
  specifies IST writes as part of interrupt-mask changes. The compatibility
  model clears bits written as zero and preserves bits written as one.
  [ST-210 item 7, printed p.4](https://antime.kapsi.fi/sega/files/ST-210-110194.pdf)
  warns of lost interrupts on IST writes; it does not specify an SH-2 bus
  exception. Same-edge physical interrupt-set/write-clear priority remains
  unverified. No DMA cancellation, mask change or fabricated interrupt occurs.
- [VDP2 §3.1](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p03_10.htm)
  defines VRSIZE as a word with a writable capacity bit and a read-only version
  nibble. The base 4 Mbit mode and version zero are supported. Selecting expanded
  8 Mbit addressing retains an explicit implementation-boundary diagnostic.
- [VDP2 §16 register list](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p16_10.htm)
  identifies +0C/+FE as reserved, and TVSTAT/HCNT/VCNT as read-only. Stores to
  those read-only words have no effect, including on external-latch read-clear
  state. The two reserved words read zero and ignore stores as a scoped
  compatibility policy, not an electrical open-bus measurement.
- During an SF-claimed INTBACK status operation, one parameterless control can
  wait behind the current status deadline. Its command report and unchanged
  execution duration start after status completion, canceling the unused
  peripheral continuation. Further overlap, parameter writes, no-SF overlap and
  prohibited slave issuers remain diagnostic. This is an explicit compatibility
  scheduling policy; ST-169 §2.2 documents software avoidance of overlap and does
  not establish this mailbox depth or electrical behavior. Native tests check
  retained deadlines, real reset/sound/CD effects, full-slot rejection and reset
  cancellation. The original guest also performs SETTIME and reads back STE/date.
- [SCU ST-097 §3.2, fig.3.13, printed pp.48–50](https://antime.kapsi.fi/sega/files/ST-097-R5-072694.pdf)
  describes DSTA. ST-210 item 13 removes it. The legacy read compatibility view
  now derives bits 4/8/12/0 from actual CPU-level/DSP DMA work, including pending
  arbitration. They clear only after completion; reads never advance or cancel
  DMA. This conservative activity view does not reproduce the removed wait,
  break or access flags or assert their production-silicon readback. Writes and
  adjacent removed ports remain unsupported.
- SCU DSP operation fields with X-bus mode 001/101 now preserve the disabled
  P-input selector while executing the remaining arithmetic, RX and D1 fields.
  [ST-097 §4.5 X-bus formats](https://docs.exodusemulator.com/Archives/SSDDV25/segahtml/hard/scu_/hon/p04_52.htm)
  defines the normal enable/source fields, not these unused combinations. This
  scoped compatibility decode leaves P unchanged when its write enable is clear;
  it is not a whole-instruction NOP. Independent original parallel-field programs
  require ADD results, RX multiplication, D1 stores and postincrements. Other
  illegal DSP instruction classes and DMA-bank conflicts retain diagnostics.
- [SCSP ST-077 §4.1, figs.4.2/4.5](https://antime.kapsi.fi/sega/files/ST-077-R2-052594.pdf)
  leaves the last four words of each slot and the +7C0–7FE gap between MADRS
  and MPRO unused. Initialization writes to those ranges now have no effect;
  reads deterministically return zero. This avoids converting initialization
  sweeps into repeated 68000 bus exceptions. Alignment checks, live slot/DSP
  registers and other unsupported endpoints keep their handling. No firmware
  handler, sound acknowledgement or audio is supplied by host code.

Widths, CPU alignment exceptions, unknown neighboring registers, unsupported
cycle-pattern commands and other device errors retain their own handling.

## Reproduce privately

From the repository root, with the documented Rust toolchain:

```sh
python3 scripts/probe-bios.py BIOS_JP_v1_01.zip --frames 200
```

The helper accepts a raw BIOS or a ZIP containing exactly one 524288-byte entry.
It validates and reads the entry without extracting archive paths, runs the
normal `Boot::ExternalBios` path and removes its temporary raw copy. The native
example reports real progress/fault diagnostics and writes the rendered frame
to ignored `target/private-bios/frame.ppm`; successful bounded execution alone
does not assert a menu or game boot. No HLE boot is substituted.

Optional `--buttons controls.tsv` drives only the normal controller input. Each
line is a zero-based frame number and hexadecimal pressed-button mask; zero
releases buttons. Frames must increase. This supports repeatable private input
experiments without writing guest RAM or changing firmware.

For browser use, extract an authorized raw copy outside `web/`, build with
`sh scripts/build-web.sh`, serve only `web/` as in [setup](setup.md), select
**User-provided BIOS**, select the raw file and press **Start**. Focus the display
for arrows and Z (Saturn A). New battery state presents the firmware's date/time
setup. No BIOS is uploaded or fetched by the page.

## Local mixed-mode disc startup

The same probe can mount local CUE/BIN media before resetting the actual BIOS:

```sh
python3 scripts/probe-bios.py BIOS.bin --cue game.cue --frames 1800 --buttons controls.tsv
```

The default track path is the CUE's same-basename `.bin`. For multiple files or
different names, pass each explicitly with repeatable `--track FILE` arguments.
The probe supplies these basenames and bytes to the ordinary checked media
loader; it does not follow arbitrary paths embedded in a CUE. Inputs and frames
remain local and outside `web/`. Mounting and bounded execution alone do not
establish a game's title screen or playability.

Disc startup exposed the following independent functional corrections:

- Bootstrap PVD volume size can describe a whole mixed-mode volume. Its envelope
  is distinct from the first data track's readable sectors: audio and gaps still
  fail as bootstrap file data. Root directories share the existing runtime
  allocation bound of 1 MiB instead of the former bootstrap-only 128 KiB bound.
  These are checked resource policies, not claims of an ISO directory limit.
  [ST-040-R4](https://antime.kapsi.fi/sega/files/ST-040-R4-051795.pdf) §§2–4 and
  [ECMA-119 second edition](https://ecma-international.org/wp-content/uploads/ECMA-119_2nd_edition_december_1987.pdf)
  §§8.4.8/9.1 provide the layout/volume/record contracts.
- Halfword DATATRNS at physical `05898000` shares the existing FIFO with
  `05818000`. P0/P1 aliases and passive debugger reads retain their usual
  behavior. Only the observed halfword endpoint is added; neighboring addresses,
  byte/long accesses and general A-bus conversion remain outside this change.
  [Firsthand A-bus measurements](https://segaxtreme.net/threads/a-bus-cs2.24940/)
  distinguish the word window used for TOC/file information from the longword
  sector window. [CD ST-162](https://antime.kapsi.fi/sega/files/ST-162-062094.pdf)
  §3.1 specifies the FIFO and 16-bit communication interface.
- TOC, Q/R–W and file-information size responses place their 24-bit word count
  in CR1's low byte and CR2. They must replace the normal report's flag/repeat
  byte. Retaining the CD-ROM flag made a 204-word TOC look like `008000CC`
  words; a real firmware copy then overwrote its own code with exhausted FIFO
  reads and entered an illegal-instruction handler. ST-162 §§7.2.3/8.2.1 specify
  the 408-byte/204-word TOC; the packed count reconstruction was independently
  observed in the authorized local firmware. Earlier recorded `4180 00CC`
  TOC responses in historical validation records describe the superseded model;
  the current paused-disc response is `4100 00CC`.
- Session location reports the first track's offset before the 150-frame FAD
  bias. Returning 150 for the first session made firmware add the bias twice
  and read sector 150 instead of the IP. The original
  [CD firmware analysis](https://github.com/user-none/erings/blob/main/docs/cdblock/saturn_cdblock_firmware.md#disc-operation-commands-92a2-945c)
  describes the first-track FAD minus 150; ST-162 §7.2.3 distinguishes session
  location from TOC position. This implementation remains single-session.
- Disc-only `E0` starts a separate check; `E1` reports its result. The
  [author's hardware experiments](https://segaxtreme.net/threads/sega-cdc-library-recreated.11053/)
  report Saturn result 4 and separate CSCT/EFLS completion. Ordinary image
  formats omit the physical security ring. The functional image policy returns
  4 for a first data sector with the Saturn system identifier, 2 for other data,
  and 1 for audio. This is not physical-disc authentication. A two-frame 75 Hz
  model delay separates CMOK from completion; it is not a measured seek time.
  Replacement, tray open and CD reset discard stale results. MPEG authentication
  and MPEG boot-ROM commands remain unsupported.
- The entered disc initialization code writes a longword to `FFFFFFB8`, an
  unassigned SH-2 internal address. This exact write is discarded as a scoped
  compatibility policy. It does not alias DMAOR, add readback, accept narrower
  accesses or change neighboring undefined addresses. The
  [SH7604 manual](https://antime.kapsi.fi/sega/files/sh7604.pdf) §9.1.4/table9.2
  identifies the implemented DMA registers; it does not establish silicon
  behavior for this unused address. Both CPU views and the original cd-drive
  guest check the policy without copying the private initialization code.
- BGON's unused bit6 no longer rejects the complete VDP2 frame. It does not
  enable another screen or affect defined layer/transparency bits. This scoped
  rendering policy preserves the existing captured debugger value and other
  unsupported configuration diagnostics. The
  [VDP2 §4.1 register map](https://www.infochunk.com/saturn/segahtml_en/hard/vdp2/hon/p04_10.htm)
  assigns no screen to bit6. A private disc run reproduced the rejection at
  field1043 in both native and production Chromium; the original cd-drive guest
  checks an independently authored green back screen with bit6 set in both
  backends, and literal pixel vectors cover an enabled bitmap as well.

The original external-startup guest now decodes all 24 TOC count bits and uses
both FIFO windows without changing its sector DMA endpoint. The original
cd-drive guest checks pending/completed authentication and distinguishes an
original Saturn-header fixture from zero-filled data. Both guests run through
the native Machine and production browser Wasm; private BIOS/game bytes are
never incorporated into these fixtures. These additions do not close #17 or
establish general retail-game compatibility.

## Independent regression evidence

The original MIT [initialization guest](../tests/fixtures/bios-startup.asm) is
authored independently of the private BIOS. The existing
`python3 tests/fixtures/build_scu_dstp.py --check` command reproduces its source,
512 KiB ROM representation and literal [records](../tests/fixtures/bios-startup.tsv).
It executes reset vectors, polls real HBlank status, preserves/clears/reobserves
interrupts, initializes VDP2 words, overlaps a control/status request, performs
SETTIME/readback, waits for real DMA completion and checks data/IRQ, computes
parallel DSP results and starts an original 68000 initialization loop. The 68000
clears the DSP-address gap and all slot registers before acknowledging via sound
RAM; only guest code writes that acknowledgement and the SH-2 completion record.

[Native tests](../tests/bios_startup.rs) and the required
[`scuFringe` production-Wasm browser group](../scripts/scu-dstp.mjs) execute the
same bytes with advancement chunks 1/127/10000 and repeated cold boots. Native
unit controls cover individual IST bits, new interrupts after clear, active DMA
completion, version read-only state, reserved-word isolation, retained external
latches/counters and neighboring width/configuration errors. Existing DSTP guest
oracles and negative controls are retained.

## Validation and limits

Private verification used the real UI file picker, BIOS mode and Start without a
disc, then the normal keyboard/SMPC path. Native execution reached the startup
logo at field 650. Production Chromium reached the CD player by field 1008 and
remained responsive through field 1204, including right/up selection changes and
changed firmware labels. Native and browser runs reported no sticky machine
faults on that path. These are bounded local observations of the supplied BIOS.

The CD player still displays its checking message without a mounted disc, and
sprite/overlay fidelity has not been checked against a reference. A completed
no-disc status transition, pixel-perfect startup, BIOS audio fidelity and broad
BIOS revision coverage are not established. The later mixed-mode observations
below are limited to the supplied inputs. Broad hardware completion and physical
same-edge behavior remain in #17; that Issue stays open. Validation is local;
it does not imply hosted CI or publication. Screenshots and private diagnostic
outputs remain under ignored `target/private-bios/` and `target/private-game/`.

The original fixture representation `bios-startup.hex` has SHA-256
`73840b9ebe7379fd0f16c53cd05ae80d9e3b8881ca15967c67676c153a7ff6b1`.
This is the original redistributable fixture, not a hash or dump of private
firmware. Final local check results follow below.

Local validation on 2026-11-07 used macOS arm64, rustup stable Rust 1.99.0,
wasm-bindgen 0.2.129, Node 26.12.1, Playwright 1.58.2 and installed Chrome
157.0.8103.38. Native stable commands used
`CARGO_TARGET_DIR=target/stable-native`; production Wasm used the stable PATH
selection documented in [setup](setup.md).

| Check | Result |
| --- | --- |
| `rustup run stable cargo fmt --check` | Passed. |
| `rustup run stable cargo build --locked` | Passed. |
| `rustup run stable cargo test --locked` | Passed: 1,211 tests, zero failed or ignored; empty doctests. |
| `rustup run stable cargo clippy --locked --all-targets -- -D warnings` | Passed. |
| `sh scripts/check-fixtures.sh` | Complete reproduction registry passed, including the new original guest. |
| `sh scripts/build-web.sh` | Passed; production release Wasm and matching bindgen CLI. |
| Wasm `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed. |
| Runner regression tests | 13/13 passed using installed Chrome; see environment note below. |
| `BROWSER_CHANNEL=chrome npm run test:browser` | All 74 groups passed, with revision/coverage/tested-artifact aggregation. Parallel wall time 411.872 s; shared 410.216 s, scenarios 370.336 s. |
| Private ZIP helper | Passed against the supplied archive; raw temporary copy removed after execution. |
| Markdown links, new-file whitespace and `git diff --check` | Passed. |

The complete 1,211-test native suite also passed with the preinstalled Homebrew
Rust 1.94.0. The initial runner invocation could not find its pinned downloaded
Chromium binary. A download stalled during extraction and was stopped. The
unchanged runner then passed with a temporary Node launch adapter selecting the
installed Chrome, including its two direct launch calls that ignore
`BROWSER_CHANNEL`. That adapter is local under ignored `target/`, not part of
the emulator, test expectations or public assets.

An intermediate browser run exposed diagnostic text compatibility: inserting the
new queued-control field between existing fields broke strict diagnostic regexes.
The field is now appended, retaining the original field order and all inherited
assertions; the complete final browser suite passed. Obsolete negative controls
at newly modeled endpoints were replaced with state-preservation checks. The
reset-prefetch bus-fault control moved from now-benign slot padding to the still
unsupported common-register hole +430, retaining halt/no-stack-write/recovery
assertions. No original execution deadline or required check was waived.

The fixed longword DATATRNS source also retains four-byte FIFO reads for short
SCU DMA tails. Previously a transfer length not divisible by four reached a
byte read at the CD port, failed the device-width check and never signaled DMA
completion. The destination still receives exactly the requested count,
including unaligned byte fringes; guards outside that count remain untouched.
Unused bytes from the last source longword are consumed and discarded, and
DataEnd includes those FIFO words plus the CD-side prefetched word as usual.
This is an explicit functional compatibility policy for the fixed `DATA`
source, not a claim that ST-097's general byte-fringe diagram establishes this
CD endpoint's electrical timing. Incrementing sources, ordinary RAM fringes,
the single-word window and direct unsupported byte MMIO retain their behavior.
ST-162 §3.1 defines a word transfer interface; the firsthand A-bus CS2 captures
linked above distinguish its longword window. The short-tail rule is a model
choice, not a new independent physical measurement.

Original native vectors exercise all three DMA levels, counts 1–9 and 258,
all four destination alignments, two scheduler chunk sizes, exact payloads,
guards, FIFO consumption and DMA-end versus DMA-illegal status. The original
CD drive guest performs a six-byte transfer into an unaligned destination and
checks both guards and DataEnd through native and production Wasm execution.
No private disc bytes or firmware instructions form these regressions.


## Mixed-mode startup observations

The supplied Sound Novel Machi Disc 1 CUE/BIN and authorized external BIOS now
execute the actual disc initialization and game engine without HLE substitution
or guest RAM/PC patches. Native execution passes the CHUNSOFT animation and
TrueMotion notice, displays the game logo by field 2400 and reaches the book
selection screen by field 2824 with ordinary date-confirmation and Start input.
Pressing A for eight fields and then releasing it advances to the game's
Japanese operation-guide text by field 3252. The ordinary production browser UI,
with local BIOS and CUE/BIN selection, reproduces the logo and the same book
selection screen and A-triggered operation guide at those field boundaries;
its error display remains empty.
The former uniform gray screen was a pending CD read, not a compositor priority
problem: after the DMA tail correction, the game itself removes its covering
background. No forced BGON, priority, completion flag or callback is used.

The current working tree passes all 1288 native tests (109 result groups), the
standard two-worker browser suite (74 groups plus artifact digest/aggregation),
the full fixture reproduction registry, formatting, and native/Wasm Clippy.
Production Wasm is built by the ordinary Binaryen 127 build, not a diagnostic
core. An additional legacy serial-order browser run timed out at `sh2Store`'s
reset-status wait; that group passes in the standard independent-context suite.
The legacy serial runner was not changed as part of this disc correction.

These bounded local observations do not establish a full playthrough, reference
image/audio fidelity, other retail discs or 60fps. Original tests remain runnable
without any personal firmware or commercial game data.
