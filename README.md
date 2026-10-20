# saturn-rust

An independently implemented Rust SH-2 interpreter and minimal Saturn devices,
compiled to WebAssembly. **Star Collector** is the first supported browser-playable
homebrew: an original, silent Saturn-targeted disc game with graphics, keyboard
input, scoring, completion and in-game restart. It needs no proprietary BIOS.
General homebrew and commercial-game compatibility are not supported.

Open **https://takahirox.github.io/saturn-rust/** and choose **Play Homebrew** after
the updated Pages build deploys. Move the cyan square with arrows to collect five
yellow stars. Gold blocks show progress; a green banner means completion.
**Enter (Saturn Start)** restarts. Click the screen for keyboard focus. Pause,
Resume, Advance frame and Reset are available; Reset cold-boots the selected mode.
The display is rasterized in Rust from guest-written VDP2 VRAM, and controls go
through emulated SMPC hardware. Game rules and drawing execute as guest SH-2 code.

- [Build, run, test and Pages setup](docs/setup.md)
- [Supported CPU, disc boot and hardware; limitations and primary manuals](docs/architecture.md)
- [Game source, reproducible ISO build and MIT license](homebrew/star-collector/README.md)
- [Redistribution/provenance audit](docs/homebrew-provenance.md)
- [Issue #7 validation and pending publication check](docs/validation-issue-7.md)

Choose **BIOS-less HLE disc** to select your own authorized compatible cooked ISO
(2048-byte sectors, up to 16 MiB). The narrow single-area IP/ISO9660 boot contract
is documented above; other formats, hardware and runtime BIOS/CD services are
unsupported. The packaged sample is MIT licensed, including its original visuals
and generated disc image. No Sega security objects, SDK libraries or firmware are
bundled; this is not a stock-console authenticated disc.

The original **HLE bootstrap prototype** debugger fixture remains available:
Reset, then Run 6 yields `R0=4`, `R1=7`, `PC=0x0600000E`. **User-provided BIOS**
accepts a local raw 512 KiB file and uses its actual reset vectors. Native Sega
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
