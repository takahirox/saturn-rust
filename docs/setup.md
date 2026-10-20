# Build, run and test

Use a Rust stable toolchain managed by rustup, its `wasm32-unknown-unknown`
target, and Python 3.10+ for building the homebrew disc and static serving. Node.js 18.17+ and npm are needed only for
the automated browser check. There is no application backend or JavaScript bundle
step. Run commands from the repository root.

## Install build tools

```sh
rustup toolchain install stable --profile minimal --component rustfmt --component clippy
rustup target add --toolchain stable wasm32-unknown-unknown
export PATH="$(rustup run stable rustc --print sysroot)/bin:$HOME/.cargo/bin:$PATH"
cargo install wasm-bindgen-cli --version 0.2.129 --locked
```

The PATH line selects rustup's compiler and Cargo together, including on systems
that also have Homebrew Rust. The CLI must match the crate version pinned in
`Cargo.toml`. Cargo and npm lockfiles are committed; no emulator-core dependency
is used. Initial tool/dependency installation needs network access.

## Native checks

```sh
cargo fmt --check
cargo build --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

The Rust library is browser-independent. Tests cover instructions/flags/delay
slots, synthetic BIOS vectors, RAM/VRAM/MMIO, digital-pad selection, VDP2 pixels,
ISO parsing/errors and actual guest gameplay/completion/restart. No proprietary
files are required. The committed sample can be rebuilt and verified without an
external cross compiler or SDK:

```sh
python3 homebrew/star-collector/build.py
python3 homebrew/star-collector/build.py --check
```

## Build and serve the browser client

```sh
sh scripts/build-web.sh
cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings
python3 -m http.server 8000 --bind 127.0.0.1 --directory web
```

Open <http://127.0.0.1:8000>. The build regenerates the original licensed ISO and
its public license, then creates
`target/wasm32-unknown-unknown/release/saturn_rust.wasm` and the generated
`web/pkg/saturn_rust.js` / `saturn_rust_bg.wasm`. Generated Wasm bindings are ignored
by Git; the reproducible ISO and its license are committed. Rebuild after changing
Rust or guest source. Opening `index.html` as a `file:` URL will not work. The client needs browser support for ES modules, Wasm and the
`wasm-unsafe-eval` CSP directive; the tested browser is recorded in
[validation results](validation-issue-9.md).

1. The default **Play** view centers the native 320×224 display, scaled at 10:7
   with pixelated rendering. Click **Play Star Collector** to fetch the packaged
   MIT-licensed ISO and boot it through BIOS-less HLE. Initial guest bitmap
   clearing takes a few frames. Click the screen for keyboard focus; arrows move
   the cyan player, yellow stars are collected one at a time. Five gold blocks and a green banner
   mean success. Press **Enter / Start** to restart inside the guest, including
   after completion. This is a silent build.
2. Use the adjacent **Pause/Resume**, **Restart** and **Fullscreen** controls.
   Restart cold-boots the selected disc and starts playback when settings are
   closed. Fullscreen targets the player shell; exit with its button or the
   browser's Escape action. Unavailable/denied requests show an explanation and
   leave the normal player usable. Focus returns to the Fullscreen button on exit.
   Losing display/window focus releases held buttons. Hiding the browser tab pauses.
3. Choose **Debug** for CPU R0–R15 and special registers, retired instructions,
   pending delay target, machine status, **Step**, **Run**, **Reset** and **Advance
   frame**. Frame advancement requires a ready, paused disc. Switching Play/Debug
   pauses and releases input without resetting, changing boot mode, discarding
   selected files or clearing errors. Returning to Play requires **Resume**.
   Invisible register tables are not refreshed during Play.
4. Open **Load ISO / BIOS & boot settings** with click or Enter/Space on its
   disclosure. Opening pauses and releases input; closing it does not reset or
   resume. Even if a pending sample download finishes after you dismiss settings
   or return from Debug, playback stays paused. Choose **BIOS-less HLE disc** and
   a local authorized ISO to use the same loader without fetching the sample.
   Local images load paused; close settings and use Resume to play.
   Only the documented single-area Saturn IP + ISO9660 cooked 2048-byte-sector
   format and device subset are supported, up to 16 MiB. BIN/CUE, compressed images
   and generic game compatibility are future work. Malformed or unsupported media
   produce errors and stop the old machine.
5. To inspect the preserved debugger fixture, select **HLE bootstrap prototype**
   in settings and choose **Debug**. Click Step to see `R0=5`, then
   Reset and Run the default 6 instructions. Expect `R0=4`, `R1=7`,
   `PC=0x0600000E`, and 6 retired instructions.
6. Step again to see a pending branch, and again to execute its NOP delay slot.
   Further bounded runs stay in that loop. This is a test/homebrew startup path,
   not BIOS service emulation.
7. In settings, choose **User-provided BIOS**. Missing input shows a size error
   and disables stepping. Select a legally obtained **raw 524288-byte** BIOS.
   Incorrect sizes are rejected; correct size validates the layout, not authenticity or region.
   Native Sega BIOS boot is incomplete; BIOS mode never switches to HLE.
   PC and R15 are loaded from the first two big-endian vector words. Step or Run
   may stop immediately with the unsupported opcode and its PC.
8. Debug Reset restarts the selected mode. Changing mode or file performs a cold
   reset; switching modes discards the previous file selection. Run accepts a whole
   count from 1 to 10000. Faults remain stopped until reset.

Only static files, including the original MIT-licensed sample ISO, are fetched.
Local BIOS/disc bytes are read via `File.arrayBuffer()` and
copied into this tab's Wasm memory, with no upload, browser storage, analytics,
CDN or external runtime assets. They are discarded from application state on mode
change or tab close (this is not a secure memory-erasure guarantee). The fixture
works with network access disabled after the static assets have loaded; offline
reload is not supported because there is no service worker.

## Automated browser smoke test

After building `web/pkg`:

```sh
npm ci --ignore-scripts
npx playwright install chromium
npm run test:browser
```

On a machine with Google Chrome already installed, skip the browser download and
use `BROWSER_CHANNEL=chrome npm run test:browser`. Linux hosts may need Playwright's
documented browser system dependencies. The script starts and stops its own
loopback static server on a free port and runs the real UI/Wasm in a headless
browser mounted at `/saturn-rust/`, matching the GitHub project page. It rejects
unexpected files or links in `web/` before upload and checks nonempty runtime
assets, HTML/CSS/JS/Wasm responses and MIME types, stylesheet loading and Wasm
initialization. It checks fixture results, controls, invalid run counts,
missing/invalid BIOS, synthetic reset vectors, visible execution faults,
reset/mode transitions, offline execution and the absence of requests carrying
BIOS/disc data. It boots the actual ISO through Wasm, drives two distinct
keyboard sequences, verifies guest-written position/score/completion/update
counters and Canvas pixel differences, completes and restarts, rejects malformed
local discs, checks pending-file/mode races and loads a local ISO offline. It uses
only original licensed guest content and no Sega firmware. It also checks the
Play-first state, keyboard view navigation and native settings disclosure,
CPU/media/diagnostic preservation, pause and held-key release, hidden register
updates, loading/view races, Play Restart and Debug frame advancement. At
1440×900, 768×1024 and 390×844 it asserts native canvas dimensions, rendered 10:7
ratio, pixelated scaling, no document horizontal overflow and reachable controls
in Play, Debug and expanded settings (including a selected BIOS).

The test exercises actual Fullscreen API entry/exit and focus restoration when
the headless browser allows it, and reports the path taken. It always checks
unavailable feedback and, when the API is enabled, a deterministic rejected
request. Headless Chromium cannot establish OS fullscreen/Escape behavior or
support in Safari, Firefox or mobile browsers; responsive tests resize a desktop
browser and do not emulate touch hardware. Keyboard, not virtual touch controls,
is required. See [Issue #9 validation](validation-issue-9.md) for actual browsers
and results. Nonzero exit means failure.

## GitHub Pages

The [Browser Pages workflow](../.github/workflows/pages.yml) uses stable Rust,
the committed Cargo/npm lockfiles, matching `wasm-bindgen-cli 0.2.129`, and
`scripts/build-web.sh`. It verifies the committed ISO against its source, then runs
all native checks above, Wasm build/Clippy and
the Chromium smoke test before uploading only `web/` through GitHub's Pages
artifact action. Only successful `main` pushes (including merges) deploy through
the `github-pages` environment; PRs targeting `main` validate without deployment
permissions. Main runs are serialized so deployments cannot overtake each other,
and a current-`main` check prevents old reruns from publishing a stale revision.
Superseded PR validation can be canceled. No `gh-pages` branch is used.

One-time administrator setup may be required: **Settings → Pages → Build and
deployment → Source: GitHub Actions**, with Actions and deployments permitted.
The workflow does not enable Pages itself. The expected URL is
<https://takahirox.github.io/saturn-rust/>; it is not live until Pages is enabled
and the first successful deployment completes. On failure, inspect **Actions →
Browser Pages** and the `github-pages` environment, correct the failing build/test
or repository setting, then push a fix or rerun the latest `main` workflow.
The existing relative asset URLs and `wasm-unsafe-eval` CSP work at the project
prefix; no custom domain or backend is required.

Required post-merge verification (pending until actually performed):

1. After this change merges and **Browser Pages** deploys `main`, record the
   deployed commit and successful Actions/environment deployment at
   <https://takahirox.github.io/saturn-rust/>. Confirm HTML/CSS/ES modules/Wasm and
   the sample ISO load at the project prefix.
2. On the live page, confirm the default **Play** view and click **Play Star
   Collector**, move using arrows, collect stars,
   observe score blocks, reach the green completion banner and restart with
   Enter. Verify Pause/Resume, Restart, Debug registers/stepping, boot settings
   and view switching without resets. Record production-only defects and fix
   them in follow-up work.

These checks require the merged deployment and cannot be claimed from a green
local smoke test. This implementation run does not push, publish or verify a new
live revision.

The hosted build supports only the original silent Star Collector hardware
subset. External BIOS mode retains local/private reset-vector stepping but does
not promise a Sega BIOS splash screen or game boot. No proprietary BIOS/security
code or commercial disc is distributed. See [architecture](architecture.md) and
[game license/provenance](homebrew-provenance.md).
