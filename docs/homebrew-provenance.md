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
