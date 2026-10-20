# Issue #9 implementation validation

Local pre-merge validation on 2026-10-20, macOS aarch64. Rust/Cargo 1.99.0 stable,
wasm-bindgen CLI 0.2.129, Python 3.13.13, Node 24.12.0 and Playwright 1.58.2.
The installed Rust target/CLI and browser binaries were reused; dependency setup
was `npm ci --ignore-scripts` (no new dependencies).

## Result

The default Play view centers Star Collector and its pixelated 320×224 display
with a prominent Play Star Collector action. Pause/Resume, disc Restart and
Fullscreen sit beside the display. A native keyboard-operable disclosure below
the player contains the existing ISO/BIOS loaders and all three boot modes.
Debug provides a desktop grid and mobile column for machine state, instruction
and frame controls, the fixture reference and register tables. Register tables
are refreshed only in Debug. Status/error regions remain beside the player,
including in fullscreen; unchanged live status text is not rewritten per frame.

Switching views and opening settings pause and release held buttons without
changing the CPU, delay target, boot mode, loaded bytes or errors. Closing
settings/returning to Play requires explicit Resume. Opening settings or changing
views during a sample download cancels automatic playback, while the image still
loads. Local image selection cold-resets and loads paused. Debug Reset stays
paused; Play Restart cold-boots a loaded disc and starts it with settings closed.

The emulator core, original guest source, ISO, Wasm interface, CSP, static asset
allowlist and dependency manifests are unchanged. No third-party art, fonts,
framework, sound controls, storage, upload or analytics were introduced.

## Checks performed

All final checks passed:

```sh
python3 homebrew/star-collector/build.py --check
cargo fmt --check
cargo build --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
sh scripts/build-web.sh
cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings
node --check web/app.js
node --check scripts/browser-smoke.mjs
npm run test:browser
BROWSER_CHANNEL=chrome npm run test:browser
git diff --check
git diff --cached --check
```

The native suite passes all 30 tests. Browser smoke passes with Playwright
Chromium **145.0.7632.6** and installed Chrome **155.0.8059.85**. Both exercise
actual Fullscreen API entry/exit, canvas focus on entry, button focus on exit,
and deterministic denied/unavailable feedback.

Browser assertions cover:

- Play-first visibility and primary CTA; initially unbuilt/hidden register tables.
- At **1440×900**, **768×1024**, **390×844**: native canvas dimensions, rendered
  10:7 ratio, pixelated rendering, useful canvas size, canvas/CTA in the initial
  viewport, no horizontal document overflow, no overlapping controls, reachable
  Play/Debug/settings controls and keyboard view switching without reset.
- The six-instruction HLE reference, bounded/invalid instruction counts, pending
  branch preservation across view changes and its subsequent delay slot.
- BIOS size errors, synthetic reset vectors, unsupported opcode/PC diagnostics,
  and CPU/error/file-selection preservation across navigation/settings dismissal.
- Real packaged ISO loading; playback/pause, Play Restart and Debug frame advance;
  no hidden register mutations during playback; delayed sample loading across
  settings/view changes; key release on display/window blur, pause, view switching
  and opening settings; no game input while navigating settings or typing counts.
- Actual guest SH-2 movement, five-star score/completion, Enter restart, two routes,
  guest-written counters/state and framebuffer hashes/progress/banner pixels.
- Offline local ISO loading and retained bytes after view changes, malformed disc
  errors, replacement-file/mode race cancellation, missing Wasm recovery, Pages
  `/saturn-rust/` assets/MIME/Wasm, public-file allowlisting and static-only GET
  requests without local file transmission.

Desktop and mobile screenshots of the actual paused game and Debug were inspected
locally. During development, the fullscreen focus assertion was adjusted to wait
for its asynchronous event. Async frame predicates were replaced with synchronous
read-only predicates because this pinned Playwright version treats the returned
Promise as truthy before its boolean result. Final browser runs have no errors.

## Limits and unperformed checks

Responsive tests resize desktop browsers; they do not test physical touch devices.
No virtual touch controls are provided. Browser fullscreen permission and OS
Escape behavior vary; headless entry/exit does not establish those behaviors on
physical devices or in Safari/Firefox. Those browsers were not tested. No native
Sega BIOS boot, commercial compatibility or sound support is claimed. Existing
hardware limits are described in [architecture](architecture.md).

## Required post-merge verification — pending

After merge and the merge-triggered Browser Pages deployment, record the deployed
`main` revision and successful deployment at
<https://takahirox.github.io/saturn-rust/>. Confirm Play-first UI, assets resolving
at `/saturn-rust/`, Star Collector movement/score/completion/Enter restart,
Pause/Resume/Restart, Debug stepping/registers, boot settings and view switching
without resetting selected media. Record any production-only defects. This run
commits local changes only; local checks do not establish publication of the new
revision. Production verification is not a pre-merge gate.
