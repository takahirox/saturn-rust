# Issue #99 validation: CUE filename case

Related to #99. Shared Rust CUE resolution now chooses a unique exact filename
first, then a unique ASCII case-insensitive match. Missing and ambiguous
references have distinct diagnostics. The optional explicit SRW1 companion uses
the same rule and still cannot alias a track. Full paths, Unicode spelling,
track interpretation, bytes and resource limits retain their existing semantics.
JavaScript passes selected filenames unchanged; no dependency or runtime asset is
added. This parser fix establishes no additional retail-game/BIOS compatibility.

## Regression evidence

Before changing `src/media.rs`, the new
`cue_ascii_case_single_raw_bin_preserves_complete_sectors` test failed with
`DiscError("Missing or ambiguous CUE file reference")` for `GAME.BIN` versus
`game.bin`. After the fix all 14 media tests pass. Five new tests exercise both
borrowed and owned APIs, including nonzero owned offsets, raw/cooked bytes,
multi-file table order, complete TOC, track modes, flags, indices and gaps.
They also cover exact precedence, duplicate exact entries, case collisions in
both orders, missing references, full paths, special characters, non-ASCII
case/normalization boundaries, optional SRW1 matching and track-alias rejection.

The existing `ui-bios-and-exceptions` group now selects the original MIT Hardware
Lab CUE and tracks with differing ASCII filename case through both the real picker
and multi-file drop. Each path checks guest arithmetic/completion mailboxes,
Canvas pixels, stereo, retained-media Restart, missing/ambiguous failures and
exact precedence over an invalid case-fold candidate. The CUE and track bytes
remain unchanged; no companion is selected and zero network requests occur
during these selections. The existing `cdRw` group additionally selects its
explicit SRW1 companion as `CD-RW.SRW`, retaining guest pack and privacy assertions.
No browser group, fixture builder or CI stage is added.

## Executed checks

Local macOS arm64, rustup stable Rust 1.99.0, Python 3.13.13, Node 26.12.1,
Playwright 1.58.2 / Chromium 145.0.7632.6, wasm-bindgen 0.2.129:

- `cargo fmt --check`, `cargo build --locked`: passed.
- `cargo test --locked`: 1,166 passed, zero failed/ignored (103 harnesses including
  doc tests).
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `sh scripts/check-fixtures.sh`: all 103 registered commands passed.
- `sh scripts/build-web.sh`: passed using the pinned CLI and release Wasm target.
- `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings`: passed.
- Focused production `playUi` Chromium run: passed.
- `node --test scripts/ci-runner.test.mjs`: all 13 passed, zero failures/skips.
- Complete `npm run test:browser`: all 74 groups passed (32 shared / 42
  scenarios), with coverage and tested-site identity verified. Aggregate wall time
  was 398.223 seconds; the UI and R–W groups took 4.293 and 3.953 seconds.
  These concurrent local timings are not hosted-CI measurements.
- `git diff --check`: passed.

The initial focused browser attempt read PCM after the UI frame helper had
drained it. The test now follows the existing hardware scenario's order: advance
the emulator, inspect PCM, then render through the UI helper. Final assertions
retain the independent stereo and Canvas oracles. Native diagnostic assertions
were also corrected to inspect `DiscError.0` rather than its prefixed Display
representation.

Node 26's Playwright ZIP extraction stalled after download. The official pinned
Chrome and headless-shell archives were CRC-checked and extracted locally with
`ditto`; the launched browser reported 145.0.7632.6. No browser version, production
code or timeout was changed to accommodate the environment.

Hosted GitHub Actions was not invoked: this node must not push or open a PR.
All applicable native/Wasm/browser commands ran locally; no hosted-CI timing or
live deployment result is claimed. Optional raw-transport/serial repeats were
not needed for this parser change. Required post-merge verification: none.
