# Local disc input

Select media in **Load disc / BIOS & boot settings**. Files remain in tab memory;
the Play app never uploads them, invokes a host converter, writes an expanded
disc, or fetches a codec, firmware or per-title database.

| Selection | Files needed to mount |
| --- | --- |
| CHD | One self-contained supported `.chd` CD-ROM |
| Cooked ISO | One `.iso` with whole 2048-byte sectors |
| CUE/BIN | One `.cue` and its actual referenced BINARY track files, selected together with the picker’s multiple-file selection |

A CUE and its referenced tracks constitute the medium. A lone raw BIN lacks the
track layout and needs its normal CUE. Ordinary images need no private manifest,
extracted `GAME.BIN`, patched/rebuilt disc, title-specific JSON, `.srw`, `.sub`,
or extra BIOS-like assets. The built-in original MIT Star Collector, Prism
Courier and Hardware Lab demos remain available.

**BIOS-less HLE disc** needs no firmware. **User-provided BIOS** needs a locally
provided raw 524288-byte BIOS and mounts the same media for the firmware to read.
Firmware is separate from disc content, never bundled or downloaded. BIOS mode
never falls back to HLE. Mounting/decoding a disc image does not establish retail
game, BIOS or complete hardware compatibility. Physical accessories and broader
retail compatibility are separate work, not prerequisites for mounting media.

## Supported CHDs

The native `Media::chd` / `chd_owned` reader is also compiled into production
Wasm. Single-session MAME CHD **v5 CD-ROM** containers must have a 124-byte
`MComprHD` header, 2448-byte CD units and consecutive tracks starting at 1.
`CHTR` and `CHT2` track metadata are supported. Supported compression is
**cdlz (CD LZMA), cdzl (CD Deflate), cdfl (CD FLAC), and uncompressed**; these
cover the usual `chdman createcd` CD codecs. Hunk maps can mix these codecs,
uncompressed hunks and self references. CRCs and any nonzero raw SHA-1 are checked
before a medium is returned. CRC-less/unhashed uncompressed containers cannot
provide a container integrity guarantee; existing sector integrity rules still
apply to their data.

Supported track types are `MODE1` (2048), `MODE1_RAW` (2352), `MODE2_RAW` (2352)
and `AUDIO`. The adapter removes MAME's four-frame track padding, translates
stored INDEX 00 versus unstored PREGAP and POSTGAP into the existing FAD model,
and swaps CHD big-endian audio samples into the existing little-endian BIN/CD-DA
representation. Raw Mode2 uses the same bounded XA/plain detection as CUE.
First-track INDEX 01 remains FAD150, including stored first-track pregaps.
The same sector reader supplies HLE boot, external BIOS, FIFO, CD commands,
SCU DMA/IRQs and SCSP audio. No game-name detection or boot interception is added.

CHD track metadata does not preserve all CUE information: additional INDEX 02–99
and CUE FLAGS such as PRE/DCP/4CH are not present in CHTR/CHT2. They cannot be
reconstructed. Keep the original CUE/BIN when those details matter. Unsupported
versions, parent/differencing CHDs, GD-ROM/DVD/hard disk or multi-session metadata,
other codecs (including cdzs/Zstandard), other sector types and pregaps with a
different stored sector/subcode type fail with an explicit error. No private
companion is requested to compensate for an unsupported variant.

CHD `RW_RAW` and `RW` payloads are decoded and checked as part of the complete
2448-byte units. They do **not** feed the existing pack decoder: that decoder's
input is already optically deinterleaved/permuted with the seven-pack delay
compensated, while standard CHD subcode does not identify this reconstructed
boundary. Optical R–W deinterleaving is not implemented here. These images mount
with the existing no-subchannel behavior; no companion is requested. `NONE`
also supplies no R–W packs. P/Q position reporting continues to use track
metadata. Audio and missing subcode bytes are never manufactured.

For focused diagnostics only, a CUE containing `REM SATURN_RW "file.srw"`
explicitly requests the original optional [SRW1 companion](cd-rw.md), whose
representation is different from raw optical subcode. Ordinary CHD/ISO/CUE/BIN
never needs SRW1.

## Resource and failure behavior

ISO: 18–450000 sectors in the picker, at most 921600000 bytes. CUE: at most
1058400000 aggregate BINARY bytes, 1 MiB of text, 99 files/tracks. CHD input:
at most 1058400000 compressed bytes, 450297 stored CD units (450000 frames plus
at most three padding units per track), 99 track metadata entries, at most
1024 bytes per entry, and whole-unit hunks of 2448–1048576 bytes. Logical
leadout must remain within FAD450150. These are application resource limits.

The reader validates header sizes before decoder allocation, bounds map lengths
and Huffman runs, checks hunk ranges and CD codec block lengths, rejects cyclic
metadata and self references, and resolves self references iteratively. Every
logical hunk is validated on admission using a reusable buffer. This costs
a sequential decompression/hash pass at load; CD FLAC additionally decodes
its frames to validate channel/bit-depth/count constraints before upstream
reassembly. This avoids discovering corrupt
payload after changing the machine. It does not retain an expanded image.

Retained memory is the compressed input, the bounded hunk map (at most about
5.4 MB), resolved hunk targets (at most about 1.8 MB), one decoded hunk (at most
1 MiB), a compressed scratch buffer and codec scratch/dictionary buffers bounded
by the admitted hunk sizes and FLAC's fixed maximum block size. All three
allowed codecs together use several MiB
of scratch at the maximum hunk size. Decoder allocations are bounded but are
not all fallible reservations; as with ISO/CUE, a near-limit file still requires
sufficient browser/Wasm memory for input marshaling. Reset/Restart shares the
retained medium without re-uploading compressed or expanded bytes.

A replacement selection immediately stops and clears the previous medium.
Cancellation, pending-read cancellation, mode switching and failed HLE loads
keep the existing UI reset semantics. The native/Wasm external mount operation
validates before attaching and preserves the previous machine on failure.
Errors do not silently select HLE or continue an old disc.

## Decoder and reproducible evidence

The pinned [`chd` 0.4.0](https://github.com/SnowflakePowered/chd-rs) decoder is
BSD-3-Clause. It supplies the CHD map and pure Rust LZMA, Deflate and FLAC CD
codecs, including raw-sector reconstruction, subcode and block CRC verification.
Defaults, C acceleration and Zstandard acceleration are disabled. CD FLAC
frames are also checked with `claxon` 0.4.3 (Apache-2.0) for stereo 16-bit
format and exact frame counts before calling the upstream CD wrapper. The
`nonstandard_channel_count` feature avoids the decoder's stereo-iterator
assertion on malformed FLAC channel layouts; the adapter rejects those
layouts explicitly, with output size/CRC/hash checks still in force. `sha1` 0.10.6 (MIT OR Apache-2.0) validates logical content. The
[Cargo lockfile](../Cargo.lock) pins all transitive dependencies. This adds
container/compression dependencies, not an emulator core. Native MAME/libchdr
FFI would add C toolchains/ABI and Wasm portability work; an external conversion
program or server would violate the runtime contract. The Rust decoder trades
additional Wasm code and admission CPU for a single native/browser path.

Format references: [MAME chdman](https://docs.mamedev.org/tools/chdman.html),
[MAME CD metadata/layout](https://github.com/mamedev/mame/blob/mame0289/src/lib/util/cdrom.cpp),
[CHD header/map](https://github.com/mamedev/mame/blob/mame0289/src/lib/util/chd.cpp).

The original MIT fixture builder is
[build_chd.py](../tests/fixtures/build_chd.py). Regenerate with
`python3 tests/fixtures/build_chd.py --chdman /path/to/chdman`; MAME 0.289 with
one encoder thread produced the committed files. `chdman` is used **only for
fixture authoring**. `python3 tests/fixtures/build_chd.py --check` needs just
Python: it independently reconstructs the complete logical CD units, checks
raw SHA-1 and header sizes, and compares the frozen compressed SHA-256 inventory.
Compression-library/platform revisions may change compressed bytes; the
logical byte oracle is independent of those differences. CI also decodes the
committed containers against the original source media.

Data-only and mixed data/audio fixtures test each of cdlz/cdzl/cdfl/none; other
fixtures mutate data, exercise stored/unstored gaps, raw Mode2 and padding, and
mount opaque standard subcode without SRW1. Native tests compare every sector/TOC/index and
run original guests. Production Chromium at `/saturn-rust/` transfers every
first-track byte and all 408 TOC bytes through guest CD FIFO commands, compares
stereo samples, exercises HLE SCU DMA/IRQ and picker/reset/errors. Existing
ISO/CUE and diagnostic SRW1 coverage remains mandatory. See
[Issue #95 validation](validation-issue-95.md) for executed results and limits.
