# Issue #7 implementation validation

Local pre-merge validation on 2026-10-20, macOS aarch64. Rust/Cargo 1.99.0 stable,
wasm-bindgen CLI 0.2.129, Python 3.13.13, Node 24.12.0, Playwright 1.58.2,
Chromium 145.0.7632.6. The existing matching CLI and Rust target were available;
`npm ci --ignore-scripts` and `npx playwright install chromium` completed.

## Result and evidence

Star Collector is an original MIT-licensed Saturn SH-2 program, built without a
Sega SDK, linked libraries or firmware. Its generated 131072-byte cooked ISO boots
through the Rust IP/ISO loader and an original guest AIP. The complete source,
license and layout are in [homebrew/star-collector](../homebrew/star-collector/README.md).
The [candidate/provenance audit](homebrew-provenance.md) explains why third-party
release media were not bundled. No third-party emulator source or core was reused.

Native and actual Wasm/browser tests run the image and verify:

- initial player `(32,112)`, score 0, completion 0;
- eight Right updates reach `(64,112)`, score 1 and a new yellow progress block;
- 48 more Right updates reach `(256,112)`, score 5, completion 1 and a green banner;
- Start/Enter restarts through guest code, restores the initial framebuffer and
  score, without a host reset or writes to game memory;
- another route moves Up, then Right to `(64,96)` with score 0, then Down to
  `(64,112)` with score 1, distinguishing functional collision/input from elapsed
  frame count;
- CPU retirement/update counters advance, complete framebuffer hashes change,
  selected progress/banner pixels have expected colors, and completed movement
  freezes until restart (native test).

Host tests only set emulated pad levels, advance frames and read state/pixels.
The guest writes diagnostic longs at 06010000; neither UI nor Rust emulator writes
those diagnostics. A no-op controller, static screenshot or non-executing CPU
fails these checks. Boot tests also change the file name/load destination to
verify that the loader follows metadata rather than a game-specific hook.

## Checks performed

All final checks passed:

```sh
python3 homebrew/star-collector/build.py
python3 homebrew/star-collector/build.py --check
cargo fmt --check
cargo build --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
sh scripts/build-web.sh
cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings
npm run test:browser
git diff --check
git diff --cached --check
```

The native suite has 30 passing tests, including the original baseline. It covers
CPU flags, arithmetic, signed branches and load extension, delay slots, data/fetch
faults, exact BIOS vectors/private mode separation, memory access errors/aliases,
SMPC pad selection, VDP2 pixel/pitch/mode handling, malformed ISO extents/records,
size/endian/alignment errors, reset isolation and deterministic gameplay.
Parser robustness also exercises deterministic mutations across the original ISO.

The Chromium smoke runs the real static app beneath **/saturn-rust/**, with no
repository fallback. It verifies static asset allowlisting, MIME types, ES
modules/Wasm, original debugger traces, invalid counts, BIOS input/vectors/faults,
mode changes, disc game controls/state/pixels/score/completion/restart, release on
blur, offline local ISO loading, malformed media and delayed-file/mode races.
All observed network requests are GETs for explicit public static paths, without
local media bodies or query parameters. No browser errors were observed. A local
paused-game UI screenshot was inspected for the visible playfield, controls and
instructions; no screenshot is required at runtime or used as a render source.

During development, an initial Rust parse error and Clippy's constant-chunk
iteration warnings were corrected. Final checks have no failures or skips.
The ISO is rebuilt and checked in CI before native/Wasm/browser validation; the
public MIT license is copied beside it by the web build. Pages upload continues
to include only allowlisted public files. No workflow/deployment was triggered
by this local execution.

## Compatibility limits and checks not performed

Only this small silent homebrew hardware subset is supported. The scheduler uses
instruction budgets, not accurate Saturn cycles. No runtime BIOS/CD services,
CD-block device, SCU DMA/interrupts, VDP1, additional VDP2 modes, slave SH-2 or
audio are implemented. External user BIOS mode still uses actual vectors and
never falls back into HLE; successful native Sega BIOS boot is not claimed.
Optional reference-emulator/physical-hardware comparison, Gamepad API, audio,
additional titles and baseline FPS measurement were not performed. The ISO omits
Sega security code and is not claimed to boot on an authenticated stock console.

## Required post-merge verification — pending

After merge and successful **Browser Pages** deployment, record the published
`main` revision and deployment, then verify the live page at
<https://takahirox.github.io/saturn-rust/> loads the ISO, displays graphics, accepts
arrows, updates score, reaches completion and restarts with Enter. Record and fix
production-only defects. Local pre-merge tests do not establish production
playability. This implementation did not push, open a PR, merge, comment or
publish anything.
