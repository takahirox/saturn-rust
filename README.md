# saturn-rust

An independently implemented Rust SH-2 interpreter and minimal Saturn devices,
compiled to WebAssembly. The browser opens as a **Saturn Disc Player** with an
empty display and a visible four-step setup:

1. Select your authorized disc: a supported self-contained CHD v5 CD-ROM, a
   cooked ISO, or a CUE together with all referenced BINARY tracks. Use the file
   picker or drop the files onto **Select disc**. A lone raw BIN needs its CUE.
2. Optionally add a local raw 512 KiB BIOS. HLE does not use this file.
3. Choose **BIOS-less HLE disc** or **User-provided BIOS**. BIOS mode requires the
   firmware and never falls back to HLE. Switching between these modes retains
   loaded files, cancels pending reads and pauses playback.
4. Press **Start** once the required inputs have loaded and validated.

Open **https://takahirox.github.io/saturn-rust/** for the player. No bundled game
is selected or downloaded automatically. This UI does not expand emulator
compatibility: general homebrew, retail games and native BIOS boot remain
incomplete. Original MIT-licensed Star Collector, Prism Courier and Hardware Lab
remain buildable fixtures with native/Wasm/browser coverage; they can be loaded
explicitly as local discs.

The desktop layout places a large player beside setup; smaller screens place
setup first. The standard 320×224 framebuffer is displayed at 4:3 with pixelated
rendering; other core dimensions update the display ratio. Focus the display for
keyboard input: arrows for directions, Z for A, Enter for Saturn Start. Standard
gamepads are supported; virtual touchscreen controls are not included.
Pause/Resume, Restart, audio and Fullscreen are beneath the display. Fullscreen
reports unavailable or denied requests without interrupting the normal player.

**Advanced → Debug** reveals CPU registers, Step, bounded Run, Reset and frame
advance. **Advanced / backup RAM & format details** contains import/export and
media limits. Opening advanced settings or switching views pauses and releases
held input; closing settings or returning to Play never resumes automatically.
Losing display/window focus releases input; hiding the tab pauses. View changes
preserve CPU state, files and errors. Restart cold-boots retained media without
reading the file again; it resumes an existing Play session when settings are
closed. Debug Reset cold-boots and stays paused.

The display is rasterized in Rust from guest-written video state; input goes
through emulated SMPC hardware. **Enable audio** requires a user gesture and
reports blocked, muted, paused and underflow states. Actual guest sound travels
through the bounded WebAudio consumer; no soundtrack is supplied by the UI.

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

Use **Select disc** and **BIOS-less HLE disc** to load one supported
self-contained CHD v5 CD-ROM, one cooked ISO, or one CUE
with its referenced BINARY tracks selected together. Ordinary discs need no
emulator-specific sidecar, extracted executable or per-title metadata. Firmware
is optional in HLE mode and supplied locally in BIOS mode. See the
[media formats, codec/size limits and diagnostic SRW1 exception](docs/media.md)
and [sector-addressed boot contract](docs/architecture.md#disc-and-bios-less-hle-boot).
Image mounting alone does not establish commercial-game compatibility.
Full hardware and BIOS/CD-library compatibility remain pending. The packaged
sample is MIT licensed, including its original visuals
and generated disc image. No Sega security objects, SDK libraries or firmware are
bundled; this is not a stock-console authenticated disc.

The original **HLE bootstrap prototype** debugger fixture remains available in
**Advanced → Debug → Load HLE bootstrap fixture**. Loading this diagnostic
clears both image selections. Reset, then Run 6 yields `R0=4`, `R1=7`,
`PC=0x0600000E`. **User-provided BIOS** accepts a local raw 512 KiB file and uses
its actual reset vectors. Native Sega
BIOS boot/game support is incomplete and often faults on unimplemented devices
or bus behavior. BIOS mode never silently changes into HLE.

Local BIOS/disc bytes stay in the tab's memory, with no upload, browser storage,
analytics, CDN or backend. Only public static runtime assets are fetched by
the normal Play flow. Use files you are authorized to use; do not contribute
proprietary media.

**Advanced / backup RAM & format details** offers **Backup RAM** import/export.
The packed 32768-byte raw
backup survives resets and boot changes in the current tab. Export before closing
or reloading; import explicitly replaces the bytes. This is not a save state or
BIOS backup-library emulation. A second direct
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
- [Test and CI principles](docs/ci-performance.md#test-and-ci-principles)
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
