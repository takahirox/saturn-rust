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
Keyboard and standard Gamepad inputs are supported; virtual touchscreen controls are not included.

The display is rasterized in Rust from guest-written VDP2 VRAM, and controls go
through emulated SMPC hardware. Game rules and drawing execute as guest SH-2 code.
The original hardware diagnostics also execute a sound CPU and direct SCSP PCM
through a bounded WebAudio path. **Enable audio** requires a user gesture and
reports playback/blocked/mute/underflow states; Star Collector has no soundtrack.

- [Build, run, test and Pages setup](docs/setup.md)
- [Supported CPU, disc boot and hardware; limitations and primary manuals](docs/architecture.md)
- [Hardware conformance matrix and outstanding Issue #17 scope](docs/hardware-conformance.md)
- [Hardware implementation and automated validation](docs/validation-issue-17.md)
- [Historical address-error implementation and automated validation](docs/validation-issue-15.md)
- [Historical backup RAM and second-pad validation](docs/validation-issue-13.md)
- [SH-2 expansion validation](docs/validation-issue-11.md)
- [Game source, reproducible ISO build and MIT license](homebrew/star-collector/README.md)
- [Redistribution/provenance audit](docs/homebrew-provenance.md)
- [Play/Debug validation and pending deployment verification](docs/validation-issue-9.md)

Open **Load ISO / BIOS & boot settings** and choose **BIOS-less HLE disc** to
select your own authorized compatible cooked ISO (2048-byte sectors, up to 450,000 sectors / 921,600,000 bytes).
The sector-addressed IP/ISO9660 boot contract and local BIN/CUE support are
documented in [architecture](docs/architecture.md#disc-and-bios-less-hle-boot).
Full hardware and BIOS/CD-library compatibility remain pending. The packaged sample is MIT licensed, including its original visuals
and generated disc image. No Sega security objects, SDK libraries or firmware are
bundled; this is not a stock-console authenticated disc.

The original **HLE bootstrap prototype** debugger fixture remains available in
boot settings. In **Debug**, Reset, then Run 6 yields `R0=4`, `R1=7`,
`PC=0x0600000E`. **User-provided BIOS** accepts a local raw 512 KiB file and uses
its actual reset vectors. Native Sega
BIOS boot/game support is incomplete and often faults on unimplemented devices
or bus behavior. BIOS mode never silently changes into HLE.

Local BIOS/disc bytes stay in the tab's memory, with no upload, browser storage,
analytics, CDN or backend. Only public static assets and the licensed sample are
fetched. Use files you are authorized to use; do not contribute proprietary media.

Boot settings also offer **Backup RAM** import/export. The packed 32768-byte raw
backup survives resets and boot changes in the current tab. Export before closing
or reloading; import explicitly replaces the bytes. This is not a save state or
BIOS backup-library emulation. Star Collector does not use it. A second direct
digital pad is available to guest programs: I/K/J/L directions, U/O/P for A/B/C,
Right Shift for Start. Both pads release on focus loss and pause.

The **Browser Pages** workflow validates native Rust, Wasm and Chromium on pushes
to `main` and PRs targeting `main`; successful `main` revisions deploy only `web/`.
Generated `web/pkg/` bindings are not committed. The build regenerates the sample
from source, and smoke tests allow only known public assets in the upload.
A repository administrator may need to select **Settings → Pages → Build and
deployment → Source: GitHub Actions** and permit Actions/deployments. Check
**Actions → Browser Pages** and the `github-pages` environment for failures.
Local tests do not prove live publication. [Optional automated deployment checks](docs/setup.md#github-pages)
are separate from Issue #17's native/Wasm/browser acceptance requirements.

Contributor and AI agent guidance:

- [Development flow and English collaboration convention](docs/development-flow.md)
- [Review and merge guidelines](docs/review-guidelines.md)
- [Issue template](.github/ISSUE_TEMPLATE/issue.md)
- [Pull request template](.github/pull_request_template.md)

## UI/UX

Apply these principles to the browser Play/Debug views, boot settings and controls,
diagnostic messages, and documented commands and examples.

1. **Keep it simple.** Show only necessary information and actions; avoid
   unnecessary UI elements.
2. **Show, don't tell.** Prefer layout, spacing and visual hierarchy over
   explanatory labels or prose to communicate structure.
3. **Intuitive and predictable.** Keep appearance and behavior consistent with
   user expectations; don't interrupt the user's work.

Preserve necessary control labels, accessible names, error messages, and
information needed to use the interface. Simplification must preserve meaningful
consent, destructive-action safeguards, necessary warnings, and necessary user
decisions.
