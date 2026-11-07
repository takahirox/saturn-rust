# BIOS initialization and no-media software startup (#113)

Related to #113 and #17. This is a local implementation checkpoint, not a claim
of stock BIOS/menu compatibility or hardware completion. No changes are pushed,
no PR is opened, and neither Issue is closed by this node.

## Result and scope

Shared device paths now support the scoped initialization accesses in the
[source contract](bios-startup.md#source-contract-and-compatibility-decisions):
IST stores, legacy DSTA activity, VRSIZE/read-only/reserved VDP2 initialization,
INTBACK status/control sequencing, inactive DSP P selectors and SCSP unused-word
sweeps. Real DMA data/completion, sound CPU execution, deadlines, reset domains,
issuer/width restrictions and unrelated faults remain meaningful.

The established CD defect is **TOC/session REJECT instead of WAIT in
OPEN/NODISC**. Independent reconstruction from ST-162 proves the required
handshake; no private firmware transaction is inferred. The original guest
executes software reset, ESEL/CMOK acknowledgement, no-media queries, SCU A-bus
routing, CR4 consumption and two periodic NODISC reports. The bounded
[device contract](bios-startup.md#reconstructed-no-media-sequence-and-completion-bound)
is independent of any visual checking label or arbitrary drive timeout.

## Acceptance matrix

| #113 criterion | Local evidence | Assessment / remaining gate |
| --- | --- | --- |
| 1. Independent primary-source contract and per-finding matrix | [Source matrix](bios-startup.md#source-contract-and-compatibility-decisions), ST-097/ST-210 reconciliation, ST-169 software restriction versus mailbox hypothesis, ST-077 map, ST-162 query/report contract | Implemented; unmeasured behavior is explicitly classified. |
| 2. Correct narrow shared hardware behavior | `src/scu.rs`, `src/vdp2.rs`, `src/smpc.rs`, `src/scu/dsp.rs`, `src/sound.rs`, endpoint and inherited regression tests | Implemented. DSTP and the separate HLE path are preserved. No blanket MMIO success, error swallowing, firmware identity, fake menu/disc or host acknowledgement. |
| 3. Diagnose/reconstruct bounded no-disc flow with failing-before regression | `tests/cd_no_media.rs`, original guest query/IRQ/report records, literal deadlines and independent ST-162 reconstruction | Established query-handshake defect corrected; stable software-initialized NODISC demonstrated. The private BIOS's checking message has not been observed here. |
| 4. Original licensed firmware, native + production Wasm | Reproducible 512 KiB reset-vector ROM, 62 literal records, independently authored DSP/68000 bytes; `tests/bios_initialization.rs` and required `externalStartup` browser group | Implemented. Guest writes progress/completion, DMA data/IRQ, sound acknowledgement and DSP-derived graphics. |
| 5. Reset/chunks/observation/negative controls | Cold/repeated boots, chunks 1/127/10000, reset during status/mailbox, omitted SNDON, changed DSP operand, CR4 preview/release, power cancellation, full mailbox/issuer/width/reserved-neighbor controls | Implemented, with inherited controls retained as described below. |
| 6. BIOS-only UI, input, media and privacy | Existing `ui-bios-and-exceptions` / `play-ui.mjs`, local-file orders/races and Canvas in `externalStartup`, CD runtime/media groups; new same-byte Wasm guest | Local complete browser run required below. Private firmware, hardware, human testing and live Pages are optional. |
| 7. Complete locked validation, artifact barrier, exact-head CI/review | Commands/results below; unchanged browser inventory and tested-web artifact barrier; fixture registry guard updated to106 | Local checks recorded below. Exact-head hosted PR CI and normal review/merge checks remain for downstream publication; this node is prohibited from publishing. |
| 8. Architecture/conformance/validation and honest limits | This record, `bios-startup.md`, architecture/conformance, CD reports, MMIO compatibility and setup documentation | Implemented locally. Related to #17; the umbrella stays open. No post-merge verification is required. |

## Before / after evidence

Baseline source is the selected upstream `4919dcb` (its workflow-only commits
inherit the investigated main implementation). A temporary detached worktree
ran the new endpoint/guest tests against that source, separately from final
validation. The native guest stopped on its real IST longword store at physical
`05FE00A4`, guest PC `00000274`, width4. The VDP2 and SCSP endpoint tests failed
at the first newly supported initialization words. These are original guest
faults, not a trace of any proprietary BIOS.

Both new CD tests fail on the baseline: a no-media TOC query returns
`FF00 FFFF FFFF FFFF`, while the independently authored oracle requires
`87FF FFFF FFFF FFFF`. The corrected path retains WAIT's valid drive status and
has no transfer data. The final same-byte native/Wasm guest reaches completion,
with 62 literal records, real sound count 544/acknowledgement 600D and two stable
periodic `27FF FFFF FFFF FFFF` reports.

Changing the ROM data operand 7 to9 changes DSP 84 to 108 and the complete guest
color from RGBA `[165,16,0,255]` to `[99,24,0,255]` in browser Canvas. Replacing
SNDON with SNDOFF leaves both sound acknowledgement and whole-guest completion
absent. Reset during the active status request discards the retained control;
the subsequent cold execution recomputes the results.

The new guest starts ordinary SMPC work after VBlank-in + more than 300 µs;
only its deliberate status/control overlap violates the software double-issue
rule. The old startup guest's no-media detector now checks NODISC WAIT rather
than REJECT. Its no-application/no-HLE sentinel, omitted-read/selector/decode
controls, disc layouts, real IRQ handlers and success oracles are retained.
Only the relevant immediate changes in its three generated ROM variants.
Current expanded active startup ROM SHA-256:
`0c253d107aa0478b0f3f1d4b65ccbe7c351100b72d018d98b89ee453081faf96`.
Older validation records retain their historical hashes.

The sound reset-prefetch negative uses still-unsupported `100430` instead of
newly supported unused slot word `100018`. It continues to require a halt before
any instruction/exception/stack mutation. Busy-command negatives now use a
parameterized command where necessary, retaining their no-state-change purpose.
DSTP's phase/trigger/IRQ/data controls still run; DSTA observation is compared
through those same real transfer phases. New DSP conflict vectors include active
RX with unused P selection and a separate dormant-selector control.

## Executed validation

Validated locally on 2026-11-07. Final results are recorded below. Rust tools use the
rustup stable toolchain (`rustc 1.99.0`, Cargo 1.99.0), with its matching Wasm
target; bindings use `wasm-bindgen 0.2.129`. Browser checks use Playwright 1.58.2
and local Chrome 157.0.8103.38. The runner's two direct Chromium-launch tests now honor the same
`BROWSER_CHANNEL` override as the suite; CI retains default Chromium. An initial
runner attempt lacked the default browser executable; an optional browser
download was cancelled after choosing the existing Chrome override. The final run selects
the installed Chrome consistently for all thirteen checks. No prototype validation results are reused.

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked --no-fail-fast` | Passed; 1210 tests across 110 harnesses, no failures |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| `PYTHONDONTWRITEBYTECODE=1 sh scripts/check-fixtures.sh` | Passed; 106 registered commands, including the new original guest |
| `PYTHONDONTWRITEBYTECODE=1 sh scripts/build-web.sh` | Passed; production Wasm + matching bindings |
| `BROWSER_CHANNEL=chrome node --test scripts/ci-runner.test.mjs` | Passed; all 13 regressions |
| `BROWSER_CHANNEL=chrome npm run test:browser` | Passed; all 74 groups (33 shared + 41 scenarios), aggregate/artifact checks; 400149 ms |
| `git diff --check` and staged equivalent | Passed |

Initial native runs also identified inherited negative assertions that treated
newly supported IST/DSTA endpoints as faults. These now check the supported
semantics and still-invalid neighbors/widths; the final complete native run
passed all 1210 tests. The complete browser inventory remains 74 groups. The first full attempt stopped
on inherited diagnostic-text assertions missing the new empty mailbox field;
those assertions now explicitly require `deferred_control: None`. The fresh full
run passed every group. The new guest is part of the
existing required `externalStartup` group; no group or failure assertion is
removed. `scripts/ci-runner.test.mjs` retains the exact fixture-registry check,
now 106, and explicitly requires the new builder. CI still validates the same
immutable web artifact and fails on missing/duplicate groups, failed/skipped
shards, stale revisions or changed bytes. Local execution does not establish
hosted exact-head CI or review approval.

## Remaining limits

No private BIOS/archive, firmware dump, private path or screenshot was consumed
or added. This does **not** claim the specific private BIOS's CD-check message
is fixed, identify the unavailable transactions from #103/#109/#111, establish
stock BIOS/menu fidelity, or satisfy the remaining integrated hardware scope of
#17. None of these optional private observations is a pre-merge gate.

SMPC mailbox behavior, legacy DSTA readback, unused X-field interpretation and
reserved readback/inert-store policies remain explicitly labeled compatibility
choices. Tests establish their bounded functional implementation, not electrical
measurements. Same-edge IST behavior, complete old DSTA flags, mechanical drive
waveforms, full BIOS services/security and retail-game fidelity are unmeasured.
The existing normal drive/mechanical approximations are not used to claim a
physical tray cycle or authenticated disc. Required hosted CI/reviews are still
necessary before any eventual merge. No publication or post-merge action is
performed here.
