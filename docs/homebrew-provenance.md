# Homebrew selection and redistribution audit (Issue #7)

## Candidate review

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
