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
no-disc status transition, pixel-perfect startup, BIOS audio fidelity, retail boot and other
BIOS revisions are not established. Broad hardware completion and physical
same-edge behavior remain in #17; that Issue stays open. No hosted CI,
publication, commit or push is performed. Screenshots and private diagnostic
outputs remain only under ignored `target/private-bios/`.

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
