# saturn-rust

An independently implemented Rust SH-2 interpreter and minimal Saturn devices,
compiled to WebAssembly. **Star Collector** is the first supported browser-playable
homebrew: an original, silent Saturn-targeted disc game with graphics, keyboard
input, scoring, completion and in-game restart. It needs no proprietary BIOS.
General homebrew and commercial-game compatibility are not supported.

Open **https://takahirox.github.io/saturn-rust/** and choose **Play Star Collector**
in the default **Play** view after the updated Pages build deploys. Move the cyan
square with arrows to collect five yellow stars. Gold blocks show progress; a
green banner means completion.
**Enter (Saturn Start)** restarts. Click the pixel-crisp, responsive display for
keyboard focus. Pause/Resume, Restart and Fullscreen sit beside the player.
Fullscreen uses the browser API and reports when unavailable or denied.

Choose **Debug** for CPU registers, Step, bounded Run, Reset and Advance frame.
Switching views or opening settings pauses and releases held input; returning or
closing settings never resumes automatically. View changes preserve the CPU,
selected local media and diagnostics. Reset cold-boots the selected mode; Restart
in Play cold-boots the loaded disc and starts playback when settings are closed.
Keyboard required; virtual touchscreen controls are not included.

The display is rasterized in Rust from guest-written VDP2 VRAM, and controls go
through emulated SMPC hardware. Game rules and drawing execute as guest SH-2 code.

- [Build, run, test and Pages setup](docs/setup.md)
- [Supported CPU, disc boot and hardware; limitations and primary manuals](docs/architecture.md)
- [Game source, reproducible ISO build and MIT license](homebrew/star-collector/README.md)
- [Redistribution/provenance audit](docs/homebrew-provenance.md)
- [Play/Debug validation and pending deployment verification](docs/validation-issue-9.md)

Open **Load ISO / BIOS & boot settings** and choose **BIOS-less HLE disc** to
select your own authorized compatible cooked ISO (2048-byte sectors, up to 16 MiB).
The narrow single-area IP/ISO9660 boot contract
is documented above; other formats, hardware and runtime BIOS/CD services are
unsupported. The packaged sample is MIT licensed, including its original visuals
and generated disc image. No Sega security objects, SDK libraries or firmware are
bundled; this is not a stock-console authenticated disc.

The original **HLE bootstrap prototype** debugger fixture remains available in
boot settings. In **Debug**, Reset, then Run 6 yields `R0=4`, `R1=7`,
`PC=0x0600000E`. **User-provided BIOS** accepts a local raw 512 KiB file and uses
its actual reset vectors. Native Sega
BIOS boot/game support is incomplete and often faults on unsupported instructions
or devices. BIOS mode never silently changes into HLE.

Local BIOS/disc bytes stay in the tab's memory, with no upload, browser storage,
analytics, CDN or backend. Only public static assets and the licensed sample are
fetched. Use files you are authorized to use; do not contribute proprietary media.

The **Browser Pages** workflow validates native Rust, Wasm and Chromium on pushes
to `main` and PRs targeting `main`; successful `main` revisions deploy only `web/`.
Generated `web/pkg/` bindings are not committed. The build regenerates the sample
from source, and smoke tests allow only known public assets in the upload.
A repository administrator may need to select **Settings → Pages → Build and
deployment → Source: GitHub Actions** and permit Actions/deployments. Check
**Actions → Browser Pages** and the `github-pages` environment for failures.
Local tests do not prove live publication; [post-merge verification](docs/setup.md#github-pages)
remains necessary for the deployed revision.

Contributor and AI agent guidance:

- [Development flow and English collaboration convention](docs/development-flow.md)
- [Review and merge guidelines](docs/review-guidelines.md)
- [Issue template](.github/ISSUE_TEMPLATE/issue.md)
- [Pull request template](.github/pull_request_template.md)
