# Standard-size media and sector-addressed bootstrap validation

Related to #33 and #17. This change covers media admission and HLE bootstrap.
Full hardware completion, full BIOS initialization, HLE CD-library services,
remaining drive timing/error behavior and commercial-game compatibility remain
outstanding. No personal firmware, commercial media, physical console or live
Pages inspection is required or used. No required post-merge checks.

## Primary contract

- [ST-040-R4](https://antime.kapsi.fi/sega/files/ST-040-R4-051795.pdf)
  §§1.2, 1.5–1.6, 2.2–2.4 specify recording/layout separately from
  §§3.2, 4.1–4.3 and 4.7 bootstrap metadata and first-read selection.
- [Technical Bulletin #7](https://antime.kapsi.fi/sega/files/Sattechs.pdf#page=9)
  corrects recording range/track layout: last data sector FAD 283650,
  independently of executable memory size. Our 60-minute volume plus two
  150-frame gaps and 300-frame original audio ends at FAD 270750.
- [Technical Bulletin #35](https://antime.kapsi.fi/sega/files/Sattechs.pdf#page=113)
  reserves `060FF000..060FFFFF` during first-read transfer. The accepted interval
  begins after `06002000 + IP size` and ends exclusively at `060FF000`;
  stack-overlap checks also apply. Runtime application memory remains separate.
- [ECMA-119 second edition](https://ecma-international.org/wp-content/uploads/ECMA-119_2nd_edition_december_1987.pdf)
  §§6.1–6.4, 8.4.8, 8.4.12 and 9.1 govern logical volume/block bounds,
  root directory records, attributes and interleaved extents. Directory contents
  can span sectors; individual records cannot cross a sector boundary.
- [ST-162 CD Communication Interface](https://antime.kapsi.fi/sega/files/ST-162-062094.pdf)
  §§2.2, 3.4, 7.2.3 and 8.2 cover addressing, interrupts and transfer commands.

No source-Issue correction or external update was needed. The broader existing
runtime envelope remains supported; the published 63-minute range is not used
as a new admission rejection. [Architecture](architecture.md#disc-and-bios-less-hle-boot)
documents the separate application resource policy and strict boot decode policy.

## Original reproducible inputs

[Builder](../tests/fixtures/build_large_media.py),
[SH-2 source](../tests/fixtures/large-media.s),
[small seed](../tests/fixtures/large-media.seed),
[Mode1 vectors](../tests/fixtures/large-media-mode1.raw),
[XA vectors](../tests/fixtures/large-media-xa.raw) and
[license](../tests/fixtures/LICENSE) are wholly original MIT content. The assembler
is the repository's original assembler. No firmware, security object, SDK,
commercial payload or captured emulator output is included.

```sh
python3 tests/fixtures/build_large_media.py --check --output target/large-media
cargo test --locked --test large_media
npm run test:browser
```

Only compact artifacts are committed. The builder creates a sparse but complete
270,000-sector ISO (552,960,000 bytes) and hashes every byte. Both native and
browser validation consume actual full-length media. The browser builds its own
temporary copies outside `web/`, selects paths through the production file input,
and removes the copies afterward; these inputs are not deployed assets.

The [manifest](../tests/fixtures/large-media.json) records complete hashes:

| Artifact | SHA-256 |
| --- | --- |
| ISO | `47949ba9ce5b90fa0dd24578cb264d3bbd6fdcd9e1c6ef56fac6753d4087d41c` |
| Original stereo audio | `853d1e997e0f5bcb12f7a5430e38990ac434727e809913f4ccb66c23b2a29a90` |
| CUE | `ec3ca1c604d095ae2e76aeb00c8d4b92af4eafcdd3a00174ca153fa3b4d445be` |

The root spans LBAs 20–21. File ID2 has extent LBA 9000, with a two-sector
extended-attribute unit at 9000–9001 and a one-sector gap at 9002. With
unit=2/gap=1, its 0x1400 file bytes occupy LBAs 9003/9004/9006. This follows
ECMA-119 §§6.4.3.4–6.4.3.5: attribute length equals unit size, and file data
begins in the second unit. A nonzero unequal attribute length is rejected.
The separately authored runtime sector is LBA 260000 (FAD 260150). Its 512
big-endian words are `3*(i+1)`; the SH-2 DMA/FIFO sum plus literal 7 is 393991.
The mixed CUE's data postgap is FAD 270150–270299, audio pregap
270300–270449, audio INDEX 01 is 270450 and leadout is 270750. Original
CD-DA source is constant stereo `(4096,-2048)` for 300 sectors. These arithmetic,
address and stereo expectations do not import emulator implementation helpers.

## Execution evidence

[Native integration tests](../tests/large_media.rs) execute cold ISO/CUE boot,
CD/FIFO/SCU DMA, relevant interrupt/RTE and acknowledgement, guest-requested
SYSRES, retained-media cold reset, HLE mode and backup bytes. Input mutations
change literal 7 to 11 (393995), first runtime word 3 to 9 (393997), or omit
the late Play command (no completion/result).

The observed bootstrap trace is exactly
`0,0,1,16,17,20,21,9003,9004,9006`: header/IP, descriptors, root and selected
first-read units. Scratch does not grow with the full medium. Both original raw
Mode1 and XA Form1 vectors bootstrap with deliberately invalid unread sectors;
a required damaged executable sector fails before execution, with Reset state.
Parser vectors also check virtual oversized counts without allocation, declared
volume escape, missing storage, arithmetic overflow, interleave/attributes,
sector-spanning metadata, record boundaries and Bulletin #35 load-area bounds.
Existing disc tests preserve descriptor scanning, multi-area and zero-first-read.

[Browser extension](../scripts/large-media.mjs), registered in the
[complete smoke suite](../scripts/browser-smoke.mjs), uses actual generated ISO
and CUE files in headless Chromium under `/saturn-rust/`. It executes the same
late-sector checks, guest SYSRES, cold restart and backup retention, checks actual
SCSP stereo and AudioWorklet delivery, cancels pending local reads with Reset,
rejects malformed selections and executes three independently mutated ISO
inputs. Offline request observations require GET-only static asset access.
Successful large-media evidence does not use metadata-only mocks. Oversized
rejection probes use metadata so they need no >1 GiB allocation.

## Validation results

Local validation on 2026-10-24:

| Check | Observed result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed, no ignored tests; final focused media/parser regressions also passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed |
| All 79 registered reproduction commands in `pages.yml` | Passed, including full-medium hash reproduction |
| `sh scripts/build-web.sh` | Passed with matching wasm-bindgen 0.2.129 |
| Focused production Chromium standard-size test | Passed ISO, CUE, SYSRES/restart/backup, audio, cancellation/rejection and mutations |
| Complete extended `npm run test:browser` | Passed in Chromium 145.0.7632.6, including standard-size ISO/CUE execution; no required skips |

The build used rustup stable Rust 1.99.0 and its installed Wasm target. An
initial attempt using the system Homebrew compiler could not find the Wasm target;
selecting rustup's compiler and Cargo together, as documented in setup, resolved
that toolchain mismatch. Initial focused browser test assertions were corrected
for the existing mode label and for loading the AudioWorklet before going offline;
no emulator behavior was bypassed. No required checks are intentionally skipped.


## Combined attribute/interleave review correction

The review found that the bootstrap skipped attribute blocks but omitted the
initial interleave gap. The large-media and older boot-layout builders also used
an invalid one-block attribute length with unit size two. The checked mapping
now includes that initial gap and rejects unequal nonzero attribute/unit lengths
under ECMA-119 §§6.4.3.4–6.4.3.5. Contiguous attributes and interleaving without
attributes retain their respective mappings.

The [independent regression](../tests/disc.rs) authors distinct sector bytes and
record fields without either layout builder. It checks file bytes at
9003/9004/9006, both preserved mappings, mismatched attribute lengths, the last
physical sector's declared-volume bound, and overflow caused by the initial gap.
It failed against the original parser and passes with the correction.

Both affected builders and compact media were regenerated. The older
boot-layout now places its two-block attribute unit at 29–30 and gap at 31,
preserving data at 32/33/35/36 and its native/browser guest expectations.
Large-media native read traces, raw damage probes and browser mutation offsets
now use 9003/9004/9006. Complete ISO/audio/CUE hashes are recorded above and in
the manifest; corrected compact SHA-256 values are:

| Artifact | SHA-256 |
| --- | --- |
| `large-media.seed` | `55b1a36e9632f54672716ad4bd4d21f85837532412ae45b1a4a414511283c7fc` |
| `large-media-mode1.raw` | `63b56416a43e8edb12c49bd4f7c81d35978633b073e959bc93be09919f2fe8aa` |
| `large-media-xa.raw` | `2ef158139c0ebb5e52d2e04bce7fae1b696adc23442c49d77695f30fa5e8c8ba` |
| `boot-layout.iso` | `16ccece8c58c3eb8e4620318f6763cc057a84f619eb2d2478b894b20c31c3aed` |

Revalidation of this correction on 2026-10-24 passed `cargo fmt --check`,
`cargo build --locked`, all **853 native tests** (none failed or ignored), native
and Wasm Clippy with warnings denied, all **79 registered fixture reproduction
commands**, and `sh scripts/build-web.sh`. The complete extended
`npm run test:browser` passed in Chromium **145.0.7632.6**, including both corrected
layout guests, actual standard-size ISO/CUE selections, late-sector DMA/IRQ/sum,
SYSRES/reset/backup retention, CD-DA/AudioWorklet, cancellation/rejection and all
three input mutations. No required checks were skipped. Relative documentation
link targets, recorded compact hashes and Git whitespace checks also pass.
