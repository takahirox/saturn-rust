# Homebrew selection and redistribution audit (Issues #7 and #13)

## Issue #13 candidate recheck — redistribution remains uncleared

Rechecked upstream default-branch commits, root listings, license/build files and
release metadata on 2026-10-20. The recipes below are upstream build instructions,
**not builds performed or compatibility results**. Only text metadata was read;
no third-party media, assets, SDK objects or emulator implementations were added.
The absence of a complete asset/dependency permission chain means the three
third-party candidates remain **link-only; no Pages redistribution**. These
decisions record the evidence reviewed, not a conclusion that upstream use is
unlawful.

| Source / inspected revision | Project software license / attribution | Upstream build recipe / image availability | Assets and linked dependencies needing separate evidence | Requested hardware / reuse decision |
| --- | --- | --- | --- | --- |
| [Disasteroids](https://github.com/slinga-homebrew/Disasteroids/tree/867aef851d10286cc0e27e90e19d5ee649865677) | [MIT](https://github.com/slinga-homebrew/Disasteroids/blob/867aef851d10286cc0e27e90e19d5ee649865677/LICENSE), slinga-homebrew (2022); retain notice for any permitted reuse | README: place in Jo Engine Projects, run `./compile.sh`; [makefile](https://github.com/slinga-homebrew/Disasteroids/blob/867aef851d10286cc0e27e90e19d5ee649865677/makefile) uses Jo Engine/compiler, audio/3D/SGL. [Release 1.0.0](https://github.com/slinga-homebrew/Disasteroids/releases/tag/1.0.0) lists Disasteroids.zip; README advertises ISO. Archive not downloaded/hashed/built. | Four WAV files, fonts/images/logo and prebuilt objects; README credits Jeremy Blake and Jimmy Fontanez for two recordings but does not establish redistribution grants for the complete release. Jo Engine/SGL and IP/security/build objects still need a component audit. Independently owned complete assets not established. | Video, SMPC, sound/CD-DA; multitaps optional. **Link only**, no binary/source reuse. Music credit is not clearance. |
| [saturn-examples / wordle](https://github.com/buhman/saturn-examples/tree/6b85fb8303423d464eac021b5cd433a81b36ecd9) | [MIT](https://github.com/buhman/saturn-examples/blob/6b85fb8303423d464eac021b5cd433a81b36ecd9/LICENSE), saturn-examples contributors (2023); retain notice for permitted reuse | [Makefile](https://github.com/buhman/saturn-examples/blob/6b85fb8303423d464eac021b5cd433a81b36ecd9/Makefile) target `make wordle/wordle.cue`, includes `saturn/common.mk`, generated word-list, fonts and SH runtime objects. No GitHub release returned by the releases API; complete standalone image not built/verified. | Word-list provenance, converted fonts, linked Saturn library/runtime and all selected resources need review. Root examples include resources from other games; repository MIT is not evidence of ownership of each asset. Independently owned complete image not established. | VDP1/VDP2, input, SCSP and SCU DSP examples are useful future exercises. **Link only**, no source/media reused. |
| [Noiz2sa Saturn port](https://github.com/willll/noiz2sa-0.51a-saturn/tree/3864f5da341fd20401b5f19e1fa3fb64aa2b7f9d) | [BSD-2-Clause](https://github.com/willll/noiz2sa-0.51a-saturn/blob/3864f5da341fd20401b5f19e1fa3fb64aa2b7f9d/LICENSE), Michal Nowikowski (2016); retain notice, conditions and disclaimer for permitted reuse | [README](https://github.com/willll/noiz2sa-0.51a-saturn/blob/3864f5da341fd20401b5f19e1fa3fb64aa2b7f9d/README.md): `cmake -B build`, `cmake --build build`; Saturn toolchain, IPMaker, xorrisofs and iso2raw; BuildDrop ISO/BIN/CUE outputs. No GitHub release returned; no built image/hash verified. | Kenta Cho game/data and original title images, BulletML, SDL/SDL_mixer, SaturnRingLib and IP.BIN generation chain each need permission/provenance verification. Independently owned complete assets not established. | VDP/input/audio/CD and backup integration candidate; current prototype cannot execute it. **Link only**, no artifacts or source reused. |
| [Star Collector](../homebrew/star-collector/README.md), source at upstream main `80cb1509c96aea110f559ed41b200516c6d007c1` | Original MIT game/build/visuals; [license](../homebrew/star-collector/LICENSE), saturn-rust contributors (2026) | `python3 homebrew/star-collector/build.py --check`; committed cooked ISO, no guest compiler/SDK required | Original guest-generated rectangles/colors and original IP/AIP/container; no linked SDK or externally sourced assets. Python is a host build tool only. | Supported NBG0 bitmap, direct-pad input and guest scoring/restart. **Included** as regression demo; no audio/VDP1/full-hardware claim. |

Reproduce the metadata check by retrieving `repos/OWNER/REPO/contents?ref=SHA`
and `repos/OWNER/REPO/releases` through the public GitHub API, then the pinned
license, README and build files linked above. Release availability can change;
the statements above describe only the dated review. Before including any
third-party build, pin every input, audit assets/SDK/runtime/boot objects and
attributions, build from those inputs, and record the output's cryptographic hash
and explicit web-public redistribution basis. An upstream download link is not
such evidence.

Star Collector ISO SHA-256 (byte-for-byte reproducible on this checkpoint):
`a1d85374c7d37b6a547c8504c7c6917d0e1d04b8be23c846f14340f890326150`.

Because no complete third-party redistribution chain was established, Issue #13's
mandatory fallback still applies: **two additional original, explicitly licensed,
reproducible demonstrations** (multi-device game and hardware showcase). Neither
has been completed. The new original MIT [backup/pad diagnostic](../tests/fixtures/backup-pad.hex)
is covered by [fixture licensing](../tests/fixtures/LICENSE). Native and browser
tests also place this small program into the existing original ISO container in
memory; this temporary diagnostic image is not served or committed as a new game
and does not satisfy either demonstration requirement. No assets or guest SDKs
were imported to create it.

## Earlier Issue #7 selection

Reviewed the following public repository listings, build metadata and LICENSE
files on 2026-10-20 before packaging any media. No third-party game binaries,
source, recordings, images or SDK objects were downloaded into this repository.
This is a selection audit, not a legal clearance of the rejected games.

| Candidate | Evidence checked | Decision |
| --- | --- | --- |
| [Disasteroids](https://github.com/slinga-homebrew/Disasteroids) | MIT LICENSE (slinga-homebrew, 2022); root contains four numbered WAV recordings, assets, prebuilt objects and Jo Engine build references | Individual recordings/assets and complete linked release provenance were not established. No ISO/assets included. Optional future compatibility target. |
| [saturn-examples/wordle](https://github.com/buhman/saturn-examples) | Repository MIT LICENSE (contributors, 2023); root Makefile includes Saturn libraries and Wordle disc target; wordle contains word_list.csv and generated-list tool | Complete word-list provenance, linked build inputs and redistributable built disc were not established. No source/media reused. |
| [Jo Engine](https://github.com/johannes-fetz/joengine) | MIT LICENSE (Johannes Fetz, 2024); root includes Compiler, Emulators, Samples and engine | An engine, not a game license or clearance for every SDK/asset. No components included. |

The issue expressly permits an original game when complete third-party provenance
cannot be cleared. Star Collector was selected before device development: one
SH-2, a self-contained first-read executable, NBG0 direct RGB bitmap, direct-port
digital pad and VBLANK polling. Its hardware/instruction requirements, build
command, disc layout and HLE boot contract are in the
[guest README](../homebrew/star-collector/README.md).

## Included components

| Distributed component | Origin and permission |
| --- | --- |
| game.s, build.py, README and license in homebrew/star-collector | Original work for this repository; explicitly MIT licensed in that directory only |
| Geometric playfield, player, stars, score blocks and completion banner | Original RGB values and rectangles drawn by guest instructions; same MIT license; no fonts/textures/audio |
| web/star-collector.iso | Deterministic output of build.py and game.s; includes LICENSE.TXT; same MIT license |
| web/star-collector-LICENSE.txt | Exact copy of the game's MIT license, available beside the ISO on Pages |
| homebrew/prism-courier source, generated geometry/texture and original 68k sound computation | Original work under its adjacent MIT license; reuses the repository's MIT-licensed assembler/ISO builder and sound guest |
| web/prism-courier.iso and prism-courier-LICENSE.txt | Deterministic original guest disc including LICENSE.TXT, plus an exact public license copy; no SDK/firmware/recorded audio |
| src/hle/system.s, system.bin and build.py | Original SH-2 system-service routines and build output under src/hle/LICENSE; manual-defined ABI, no SDK routine or instruction hook |
| tests/fixtures/build_hle_system.py, hle-system.iso and hle-system.tsv | Original SH-2 diagnostic, ISO/AIP and authored expectations under tests/fixtures/LICENSE; disc includes LICENSE.TXT; no external assets |
| Python 3 | External build tool under the PSF license, standard library only; no Python code/runtime embedded in disc |
| wasm-bindgen and Playwright | Existing Wasm/browser infrastructure dependencies; retain their package licenses and lockfiles; no guest linkage |

No project-wide license has been added. The permission above applies only to the
new game and its build/distribution files, not Sega documentation, hardware
trademarks, firmware or the rest of the emulator. The Saturn hardware-identifier
bytes are format metadata, not a copied Sega boot/security executable. No Sega
security program, SDK, BIOS, commercial game or third-party emulator core is
bundled. Manuals are consulted externally and are not redistributed.

Run `python3 homebrew/star-collector/build.py --check` to verify the committed
ISO byte for byte. `scripts/build-web.sh` rebuilds it and copies the license on
every Pages build. The browser test permits only the explicit public runtime
asset list in `web/`, including this ISO and license.

Prism Courier supplies the required original playable VDP1+VDP2+SCSP guest
demonstration for Issue #17. Its [source/build/provenance](../homebrew/prism-courier/README.md)
records the reproducible hash, original geometric assets, guest computation,
memory contract and automated native/browser evidence. Verify with
`python3 homebrew/prism-courier/build.py --check`. Its sound is generated by
68k arithmetic and SCSP MMIO; no recording or external asset is used. The separate
hardware exerciser disc and remaining hardware acceptance are still required.


The Issue #17 advanced-video and direct-controller diagnostic sources
`tests/fixtures/effects.s` and `controller.s`, their original Python builders and
hex ROMs are covered by [tests/fixtures/LICENSE](../tests/fixtures/LICENSE).
The programs generate all pixels/tables or read emulated controller levels by
executing SH-2 instructions; no art, audio, firmware, SDK object or external core
is included. Their [reproducible hashes and native/browser evidence](hardware-conformance.md#original-advanced-video-and-controller-fixtures)
are recorded separately from the demonstration discs. They do not replace the
still-required comprehensive hardware exerciser disc.


Hardware Lab adds the original MIT [mixed-track hardware diagnostic](../homebrew/hardware-lab/README.md).
Its Python builder uses the repository's original assembler/ISO writer, original
SH-2 diagnostic, original SCU DSP program, original 68k waveform generator and
original rotation scene. The CD-DA PCM is new integer-generated stereo data.
The generated ISO, CUE, audio track and source are licensed together; all hashes
are printed by `python3 homebrew/hardware-lab/build.py --check` and recorded in
[Issue #17 validation](validation-issue-17.md). The native and browser suites
execute the actual guest and assert changed-media results. No third-party code,
recordings, firmware or SDK objects are introduced. The independent HLE reset
trampoline also has [original source and MIT license](../src/hle/boot.s).
Hardware Lab now includes an original 68k-uploaded SCSP effects program in
`tests/fixtures/build_sound.py`, covered by that directory's MIT license. Its
eight literal instructions, original coefficient/data initialization and
reproducible media have native and browser numeric/mutation evidence. Remaining
hardware acceptance is still required.


The original [slot-synthesis fixture](../tests/fixtures/build_synthesis.py) extends
only the original dual-architecture diagnostic builder. Its SH-2 starts the
independent 68k, which writes guard waveforms, configures FM and amplitude LFO,
and handles a timer interrupt. No sound recording, firmware, SDK object, assembler
or external synthesis core is incorporated. The source and generated
[synthesis.hex](../tests/fixtures/synthesis.hex) use the adjacent MIT license.
The [conformance record](hardware-conformance.md#executing-scsp-slot-modulation-and-independent-guest-evidence)
records the reproducible ROM hash and native/browser mutation evidence.


The original [RBG1 diagnostic](../tests/fixtures/build_rbg1.py) generates
[guest SH-2 source](../tests/fixtures/rbg1.s), matrices, cell patterns, names and
bitmap colors independently. Its source and ROM use the adjacent MIT license.
Native and browser checks mutate only an original matrix literal and compare
emulated output with authored pixel expectations. The reproducible hash and
programming contract are in the [RBG1 conformance evidence](hardware-conformance.md#original-dual-rotation-guest-and-rbg1-evidence).

The original [line-color/CRAM-coefficient builder](../tests/fixtures/build_line_color.py)
and [special priority/color builder](../tests/fixtures/build_special_video.py)
generate readable SH-2 sources and synthetic ROMs under the adjacent
[MIT fixture license](../tests/fixtures/LICENSE). Guest instructions populate all
images, palettes, matrices, coefficients, code masks and controls; no external
assets, firmware, SDK routines or device cores are included. Their
[conformance evidence and hashes](hardware-conformance.md#line-color-cram-coefficients-and-specialextended-composition)
include native and browser guest-literal mutations, repeatable resets and
production Canvas comparisons. CI requires both `--check` builders. Existing
demonstration discs remain unchanged; the full Issue #17 scope remains required.


The original Issue #17 interrupt/latch and sound-fault diagnostics are covered by
[the fixture MIT license](../tests/fixtures/LICENSE). This includes
[build_intc.py](../tests/fixtures/build_intc.py), its generated ROM and signed
quotient/remainder TSV, [m68k_faults.py](../tests/fixtures/m68k_faults.py), and the
resulting family ROM/fault-frame TSV. Fixtures use only the project's original
assembler and Python standard library. Register, frame and arithmetic expectations
are source-authored; no outputs are sampled from an emulator, Sega firmware or SDK.
These are diagnostic programs; the three existing demo media retain their hashes.


The original HLE backup-library diagnostic is built by
[build_hle_backup.py](../tests/fixtures/build_hle_backup.py) using only the existing
original SH-2 assembler and Python standard library. Its
[ISO](../tests/fixtures/hle-backup.iso) embeds the [fixture MIT license](../tests/fixtures/LICENSE).
All guest logic/data and the separately built original
[backup-library source](../src/hle/backup.s) are authored for this project.
Sega manual/interface declarations provide behavioral facts only; no SDK routine,
object, BIOS or third-party filesystem/core code was reused. The independent
[HLE storage contract](../src/hle/README.md#independent-backup-library) documents
SRBUP001, and the [conformance entry](hardware-conformance.md#guest-executed-hle-backup-library-bupg)
records hashes and native/Wasm evidence. This is diagnostic media, not either of
the required new demonstrations, and it does not change their bytes or licenses.


The original MIT [BSC turnaround](../tests/fixtures/build_bsc_idle.py) and
[SCU DSP DMA](../tests/fixtures/build_dsp_dma.py) builders use only Python's
standard library and this project's original SH-2 assembler. Their generated
BIOS/TSV files are covered by the adjacent [fixture license](../tests/fixtures/LICENSE).
They contain authored SH-2/DSP instructions and arithmetic/register expectations;
no Sega BIOS, SDK, third-party core, captured emulator output or external assets
are included. The [conformance inventory](hardware-conformance.md#bsc-intercycle-turnaround-and-scu-dsp-dma-execution)
records the hashes and native/browser evidence. They are diagnostics and do not
replace either required demo. All three existing demonstration media are unchanged.
DSPD's expanded original fixture also includes nonzero-address bootstraps,
SH-2 initialization of a full 256-word DSP program, DSP MVI-PC entry and FF→00
execution. Its 13-word golden record and operand/count/bank-conflict mutations
are authored from the primary programming contracts linked in the
[program-DMA entry](hardware-conformance.md#scu-dsp-program-dma-origin-and-data-bank-restrictions).

The original MIT [sprite rotation](../tests/fixtures/build_sprite_rotation.py) and
[double-interlace](../tests/fixtures/build_sprite_interlace.py) builders generate
readable SH-2 source, synthetic BIOS images and authored TSV records using only
the existing original assembler and Python standard library. Guest commands draw
all polygons/textures; no captured emulator images, external assets, firmware or
SDK objects are included. Source/generated files use the adjacent fixture license.
The [readout conformance entry](hardware-conformance.md#vdp1-rotatedinterlaced-readout-and-rotation-start-reloads-rscg)
links implementation, native vectors, browser/Canvas tests and mutations. HSS's
existing guest now waits for its documented framebuffer-change latch without
changing its original expected samples. All three demonstration media are unchanged.

The additional BSC-sharing, CD-filesystem and boot-layout diagnostics are original
MIT works under [tests/fixtures/LICENSE](../tests/fixtures/LICENSE). Their
[sharing builder](../tests/fixtures/build_bsc_share.py),
[XA/filesystem builder](../tests/fixtures/build_cd_filesystem.py) and
[boot-layout builder](../tests/fixtures/build_boot_layout.py) contain their complete
source. Standard-library Python and the existing original SH-2 assembler produce
the ROM, ISO/CUE, XA EDC/P/Q and golden records; there are no SDK objects, firmware,
commercial data or third-party assets. Both new disc images embed the license as
a file. CI verifies their reproducible bytes. Native and browser Wasm execute the
same original guests; the [conformance inventory](hardware-conformance.md#bsc-sharing-and-isoxa-bootstrap-and-runtime-files)
records behavior, checks, hashes and limits. These diagnostics supplement the
preserved Star Collector, Prism Courier and Hardware Lab discs.

The original [DSP B-bus diagnostic](../tests/fixtures/build_dsp_bus.py), its
synthetic BIOS and 30 authored word vectors are MIT-licensed under the adjacent
[fixture license](../tests/fixtures/LICENSE). The builder uses only Python's
standard library and the project's original SH-2 assembler; it authors DSP
instructions and input data directly from SCU manual interface facts. Native
and browser Wasm execute identical SH-2/DSP programs, including changed data
and stride operands. No third-party firmware, SDK, emulator output or assets
are included. [Conformance evidence](hardware-conformance.md#scu-dsp-b-bus-cycles-and-program-prefetch)
records hashes and limits; all three existing demo media remain unchanged.


The original [HLE CD player](../src/hle/player.s), its glyph assets in the
[builder](../src/hle/build.py) and generated ROM fragment are covered by the
[HLE MIT license](../src/hle/LICENSE). The original
[caller/mixed-disc builder](../tests/fixtures/build_hle_player.py) uses only Python
standard-library code and the existing original SH-2 assembler. Its ISO embeds
[the fixture MIT license](../tests/fixtures/LICENSE); the CUE and two separately
identifiable stereo signals use that same license. The fonts, control graphics
and audio are authored here. No Sega screen, SDK routine, proprietary firmware,
commercial track or third-party core is included. `python3 src/hle/build.py --check`
and `python3 tests/fixtures/build_hle_player.py --check` reproduce the media.
[Native](../tests/hle_player.rs) and [Wasm/Canvas/WebAudio](../scripts/hle-player.mjs)
tests execute the guest, including both analog pad report IDs, changed audio and
reset. These diagnostics supplement the existing three demonstrations.

The existing original MC68EC000 family fixture now includes ten additional
[source-authored operand fault programs](../tests/fixtures/m68k_faults.py),
and four MOVEM terminal-read cases, for twenty-four total access-frame records.
Its adjacent fixture MIT license remains
in force. The original assembler/standard-library builder reproduces the expanded
ROM, and native/browser guest mutations distinguish required destination reads
from ordinary writes. Changing the terminal-read guest's actual longword load
to a word load moves the terminal read into connected sound RAM and removes only
that exception record; both native and browser Wasm verify the other records and
repeat after reset. [The conformance entry](hardware-conformance.md#mc68ec000-movem-load-bus-sequence)
records exact implementation/tests, primary manual references, hashes and limits.


The original [dual-SH-2 DMAC bus diagnostic](../tests/fixtures/build_dmac_bus.py)
reuses only this repository's original SH-2 assembler and Python's standard
library. Its [hex ROM](../tests/fixtures/dmac-bus.hex) and
[source-authored result words](../tests/fixtures/dmac-bus.tsv) use the adjacent
[MIT license](../tests/fixtures/LICENSE). Both guest CPUs configure their own
DMACs, handle five interrupts, execute all four transfer sizes and test overlapping
short bursts and byte/word decrement. Tests change source operands and repeat
cold boot in native and production browser Wasm. There is no bundled firmware,
SDK routine, third-party core, external asset, host result writer or game-detection
path. The three demonstration discs are preserved. Reproduce with
`python3 tests/fixtures/build_dmac_bus.py --check`; hashes and primary references
are recorded in [the conformance entry](hardware-conformance.md#on-chip-dmac-bus-transactions-dmabg).

The original [refresh/self-refresh BIOS builder](../tests/fixtures/build_refresh.py)
and its [generated ROM](../tests/fixtures/refresh.hex) use the adjacent
[MIT fixture license](../tests/fixtures/LICENSE). It reuses only the original
SH-2 assembler and Python standard library. Both guest CPUs compute their own
RAM-read sums and FRT intervals. The master exits self-refresh using cached
guest instructions and local MCR writes. Native and production browser Wasm
checks change timer/data operands, omit the guest exit, and repeat cold resets.
There is no external firmware, asset, third-party core, host-computed result
or game-specific path. The [RFSHG inventory](hardware-conformance.md#granted-refresh-indivisible-cpu-groups-and-self-refresh-rfshg)
records primary references, exact bus vectors, fixture hash and remaining scope.

The same builder generates [monitor.hex](../tests/fixtures/monitor.hex), an
original guest under that same license. The master polls real shared RAM while
the slave's cached instructions read SCI/FRT through its internal bus. Guest
BRCR and SCI operand mutations demonstrate the UBC monitor's timing effect and
computed sums in native and browser Wasm, with repeated cold resets. No
third-party code or content is included.
