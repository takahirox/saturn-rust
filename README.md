# saturn-rust

An independently implemented Rust SH-2 interpreter and minimal Saturn devices,
compiled to WebAssembly. Open the [saturn-rust player](https://takahirox.github.io/saturn-rust/),
insert an authorized local disc or BIOS, then select **Play**.

- **Disc** accepts a supported self-contained CHD v5 CD-ROM, a cooked ISO, or
  a CUE and all referenced BINARY tracks. Use **Insert disc** or drop the files
  onto the Disc slot.
- **BIOS** accepts a raw 512 KiB file. An accepted BIOS takes precedence and can
  boot with or without a disc. With a disc alone, the player uses BIOS-less HLE.
  Pending or rejected BIOS selections block Play. Replace or clear the BIOS to
  recover. Changing the BIOS stops playback.
- During a BIOS session, choose **Open cover**, insert or drop a disc, or
  **Remove** the current one, then choose **Close cover**. The running guest
  detects the change and decides what to do next. A paused session stays
  paused until **Resume**. BIOS-less HLE uses cold disc selection.

The display is beside the media slots on desktop and above them on narrow
screens. The 320×224 framebuffer is shown at 4:3 with pixelated rendering.
Other core dimensions update the display ratio. On touch screens, the Player 1
pad includes the D-pad, A/B/C, X/Y/Z, L/R and Start. Slide across the D-pad for
diagonals and hold multiple controls at once. In wide landscape layouts the
controls flank the screen, including in fullscreen.

Focus the display for keyboard input. Player 1 uses the arrows, Z/X/C for
A/B/C, A/S/D for X/Y/Z, Q/W for L/R and Enter for Start. Player 2 uses I/K/J/L
for directions, U/O/P for A/B/C and Right Shift for Start. Standard gamepads
provide full digital controls on both ports. See **Help** for the physical
gamepad mapping and media details.

The toolbar has a single **Play**, **Pause** or **Resume** action, plus
**Reset**, **Mute**, **Volume**, **Display filter** and **Fullscreen**.
Play also starts audio and shows **Enable audio** if the browser blocks it.
Mute preserves the volume setting. Fullscreen fits the picture to the window
and includes **Exit fullscreen**.

**Display filter** defaults to **Clean**, which shows the original pixels.
**CRT Composite** softens horizontal edges and blends dithering most.
**CRT S-Video** keeps more color detail, and **CRT RGB** preserves the sharpest
edges with a subtle phosphor mask. The presets approximate analog video and CRT
display characteristics. Switch during playback or pause. If WebGL 2 is
unavailable, the player shows Clean.

**Debug** reveals CPU registers, Step, bounded Run, Reset machine and frame
advance. **Advanced** contains the virtual cartridge selector, Save management
and audio diagnostics.
Help, Advanced and Debug pause playback and release held input. Losing focus
releases input, and hiding the tab pauses. Select **Resume** to continue.
Toolbar Reset cold-boots accepted media and restarts an existing Play session
when Advanced and Debug are closed. Debug's Reset machine stays paused.
Both retain internal memory, backup cartridge saves and clock settings. A
cartridge change cold-boots the selected BIOS/disc and leaves playback paused.

The display is rasterized in Rust from guest-written video state, and input
goes through emulated SMPC hardware. Play uses
[relaxed CPU/device interleaving](docs/runtime-performance.md#relaxed-timing-playback-2026-11-25)
for real-time playback. Debug instruction steps and tests use the exact
scheduler. Volume applies after the bounded Web Audio consumer, leaving guest
PCM, emulation timing and pitch handling intact.

- [Build, run, test and Pages setup](docs/setup.md)
- [Player and touch controller validation](docs/validation-issue-257.md)
- [Cartridges and persistent saves](docs/validation-issue-260.md)
- [Supported CPU, disc boot and hardware; limitations and primary manuals](docs/architecture.md)
- [Local BIOS startup, private ZIP probe and compatibility limits](docs/bios-startup.md)
- [Live CD cover behavior and validation](docs/cd-cover.md)
- [Hardware conformance matrix and outstanding Issue #17 scope](docs/hardware-conformance.md)
- [Hardware implementation and automated validation](docs/validation-issue-17.md)
- [Historical address-error implementation and automated validation](docs/validation-issue-15.md)
- [Historical backup RAM and second-pad validation](docs/validation-issue-13.md)
- [SH-2 expansion validation](docs/validation-issue-11.md)
- [Game source, reproducible ISO build and MIT license](homebrew/star-collector/README.md)
- [Redistribution/provenance audit](docs/homebrew-provenance.md)
- [Play/Debug validation and pending deployment verification](docs/validation-issue-9.md)

Use **Insert disc** to load one supported
self-contained CHD v5 CD-ROM, one cooked ISO, or one CUE
with its referenced BINARY tracks selected together. Ordinary discs need no
emulator-specific sidecar, extracted executable or per-title metadata. Adding
local firmware automatically switches to BIOS boot. See the
[media formats, codec/size limits and diagnostic SRW1 exception](docs/media.md)
and [sector-addressed boot contract](docs/architecture.md#disc-and-bios-less-hle-boot).
Image mounting alone does not establish commercial-game compatibility.
Full hardware and BIOS/CD-library compatibility remain pending. The packaged
sample is MIT licensed, including its original visuals
and generated disc image. No Sega security objects, SDK libraries or firmware are
bundled; this is not a stock-console authenticated disc.

The original **HLE bootstrap prototype** debugger fixture remains available in
**Debug → Load HLE bootstrap fixture**. Loading this diagnostic
clears both image selections. Reset machine, then Run 6 yields `R0=4`, `R1=7`,
`PC=0x0600000E`. **User-provided BIOS** accepts a local raw 512 KiB file and uses
its actual reset vectors, with or without a disc. Play advances the real guest;
BIOS-only eligibility does not guarantee an authentic firmware menu. Native Sega
BIOS boot/game support is incomplete and often faults on unimplemented devices
or bus behavior. BIOS mode never silently changes into HLE.

The browser stores internal backup memory (32 KiB), Power Memory saves (512 KiB),
the SMPC clock and settings in one IndexedDB database per origin. Games and
boot modes share the same console. The cartridge preference is stored
separately. BIOS, disc images, ROMs, traces, save states and expansion RAM stay
in session memory. The player fetches its public runtime assets and has no
uploads, network sync or analytics. Use media you are authorized to use.

**Advanced → Save management** exports and imports complete packed raw images.
Export a copy before replacing or clearing either device. The cartridge's saves
are available even when the slot is empty or contains expansion RAM. HLE uses
its own [SRBUP001 format](src/hle/README.md#independent-backup-library). Use the
game's save menus or your supplied firmware for individual files.

Keep exported copies if you change browsers or origins, or if your browser
restricts or clears site data. The player reports a save as stored when
IndexedDB commits it. An abrupt browser or process exit can lose outstanding
changes.

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
- [Runtime profiling, benchmarks and measured limits](docs/runtime-performance.md)
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
