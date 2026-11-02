# Guest runtime file access and DataEnd

Related to #91 and #17. The original MIT [SH-2 reader builder](../tests/fixtures/build_cd_runtime.py)
produces [assembly](../tests/fixtures/cd-runtime.s), one application binary and
two Saturn-IP/ISO9660 discs. The unchanged [#85 firmware](external-startup.md)
loads file identifier 2 through hardware and enters the original AIP, which
jumps to this application. ExternalBios installs no HLE services. Rust and JS
supply the selected firmware and physical medium, never file identifiers,
handles, file extents, bytes or a library result.

CDC and GFS are application software libraries. This reader implements original
library-style semantics over real CD commands; its private register ABI is not
Sega SDK binary compatibility or a BIOS file syscall. Existing system/backup
HLE services remain independent. No proprietary software is required.

## Primary references and exercised subset

[CD Communication I/F ST-162-062094](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-162-062094.pdf)
PDF pages below are one based; printed pages are twelve higher. The downloaded
primary PDF was read directly. Register command numbers correspond to the
existing documented CD command implementation, not invented GFS opcodes.
[ST-040-R4](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-040-R4-051795.pdf)
§3.2 and ECMA-119 §9.1 describe the actual ISO records; #85's reference table
records their revision boundaries and the boot-file contract.

| Guest operation / software concept | Hardware and expected state | Primary boundary |
| --- | --- | --- |
| `init`: CDC-like drive setup | Status `00`, hardware info `01` (version word 2); `04`, flags00, standbyFFFF, ECC02, retry01; flags80/ECCFF/retryFF retain settings. ECC06 rejects (`CR1=FF00`) without mutation | CD §8.2.1 function1.7, PDF66–67/printed78–79: 00 means one ECC pass; 01–05 mean up to 2–6; 80 disables; FF retains. Retry 00–0F/40–4F/80/FF have distinct effects |
| Firmware and guest directory acquisition | Play `10`, absolute FAD=LSN+150, finite count1; connection `30`, reset filter `4480`, wait PEND; GetDelete `63` plus fixed-source SCU DMA to directory RAM; DataEnd `06` returns 1024 words | CD §§3.1–3.4, §5, §8.2.2 and §8.2.7; SCU ST-097 §3.2/3.5 and ST-210 DMA corrections, as reconciled in the startup contract |
| `find` and `open`: GFS-like name/ID/handle work | Guest rereads the root extent/length discovered by firmware, resolves DATA by actual ISO name and record order, issues ChangeDir `70` root then child and checks EFLS/GetFileScope `72`. It rereads the child records and resolves ALPHA/BETA names itself. No name is interpreted by production CD code | CD §6.2/6.8 and §8.2.8 functions8.1–8.4, PDF87–88. IDs 0/1=self/parent, subsequent IDs follow record order. CMOK acceptance precedes EFLS acquisition |
| `seek`, `read`: independent byte cursor, EOF and short read | ReadFile `74`, logical sector offset=cursor/2048, selector 1 or 2, discovered FID. Wait EFLS before publishing any file bytes. GetDelete `63` selects one acquired sector, guest FIFO MOV.W skips the prefix and copies the exact requested byte range. DataEnd ends every transfer; Delete `62` releases an unused suffix | CD §8.2.8 function8.5/PDF88; §8.2.7. Device reads sector units; byte slicing and cursor ownership are guest software |
| Interrupted GET fence | For host word count H below normal N: `H < cdwnum <= N`. Fully drained and excess GET: `cdwnum=N`. Partial PUT: `cdwnum=H`. No active transfer: error countFFFFFF | CD §8.2.1 function1.10, PDF69/printed81, transfer-count table |
| `close` and reopen | Original 24-byte handle: FID, extent, length, byte cursor, open flag, selector. Close invalidates only that handle. Read on closed handle and seek past EOF returnFFFFFFFF; EOF returns0, without CD commands or cursor movement | Original test ABI; these return values are not claimed to be Sega library return codes |
| Abort and recovery | AbortFile `75` pauses access and raises EFLS, preserving selector/buffer contents; subsequent ReadFile reinitializes its selected work partition. Cold reset or SYSRES reruns firmware and application on retained media | CD §8.2.8 function8.6/PDF89; startup reset contract |
| Command/IRQ fencing | CR1..4 at25890018/1C/20/24, HIRQ25890008, DATATRNS25818000; clear CMOK before command, wait CMOK before next request, read CR4; DRDY=2 before FIFO and EHST=80 after sector DataEnd; masks/acknowledgment remain separate | CD §§3.2–3.4 and §8.2.9; SCU A-bus and DMA-end vectors use the unchanged firmware handlers |

The reader uses a two-sector maximum directory workspace and contiguous Mode1
runtime files, with exact ISO identifiers including `;1`. The firmware's 64 KiB
first-read bound remains independent of runtime lengths. Runtime file-library
requests in this fixture are at most 9999 bytes and fit the tested sector pool.
There is no added production file-size or filename special case.

## Hardware mismatch and minimal correction

Inherited `main` (`db96e2d`, also the selected checkpoint's production code) can
boot this application, initialize, resolve both assets and read three file bytes.
The first partial FIFO transfer consumes two words (the fourth byte is outside
the requested byte range). DataEnd reports **2**, violating the primary manual's
strict interrupted-GET inequality. The guest stops at E091 before committing its
file cursor. With the same application and oracle, corrected DataEnd reports 3
and the application continues to success 600D.

`CdBlock::execute(06)` already modeled one CD-side offered word for R–W GET.
The fix applies that accounting to every GET class (sector, TOC, Q and file
information), using the accepted transfer's position, direction and length.
It caps the result at N and adds no prefetch for PUT. It neither advances the
host position nor consumes a byte from a subsequent transfer. Buffer identity,
GetDelete's removal fence, EHST class ownership, command scheduling and DMA
remain in the existing hardware path. Full/excess counts remain N. Existing
partial-count expectations in CD copy/drive/Q tests are corrected accordingly;
their transferred bytes, requests, timing, resets and negative controls remain.

The one-word amount is an explicit deterministic FIFO policy. The manual
specifies the inequalities, not the physical optical-controller prefetch depth.
It does not justify claiming cycle-exact controller internals. All CD commands
retain the existing 1075-reference-clock response delay; drive/seek/sector and
command completions remain distinct. Words/registers use big-endian fixed widths,
FAD and command offsets use24-bit fields, byte positions are guest u32 values,
and capacity arithmetic remains bounded by existing checked CD ranges.

## Layouts and independent oracles

| Disc | Root / DATA LSN | First read | ALPHA LSN / bytes / FID | BETA LSN / bytes / FID |
| --- | --- | --- | --- | --- |
| cd-runtime | 20 / 48, one sector each | LSN32, same application at06020000 | 64 / 5003 / 2 | 72 / 2051 / 3 |
| cd-runtime-moved | 27 / 60, DATA spans two sectors | Same binary and destination | 80 / 4096 / 3 | 96 / 6145 / 2 |

Root DATA identifiers also change (3 to4). Both discs carry an accessible MIT
license, coherent path tables, an original IP/AIP and multiple non-boot files.
The application never embeds these layout LBAs, lengths or FIDs.

[Native tests](../tests/cd_runtime.rs) and [production Wasm/Canvas tests](../scripts/cd-runtime.mjs)
independently author the payload formulas, lengths, requests and results. They
compare the complete loaded application, all 65,536 destination/canary bytes,
partial/cross-sector reads, exact EOF/tails, both final handle records, complete
command logs, SCU source/destination and interrupt counters. The guest sums all
bytes of both files and derives its actual VDP2 back-screen color. Changing a
real asset byte changes those bytes, arithmetic and Canvas. Both layouts use the
same application; native/browser comparisons use scheduler chunks 1/3/17/10000
and passive FIFO/CR4 inspection, with a fixed 2,600,000-step snapshot after measured
completion at 2,260,000/2,530,000 steps. Guest polls remain bounded at 200,000 iterations;
errors stop without publishing buffered data or committing cursors.

Controls include finite pending commands, partial/full/excess GET and PUT,
competing FIFO WAIT, independent query/selector work, empty DataEnd, bad init
parameters, invalid FID, absent name/records, malformed/out-of-medium child
records, a genuinely uncorrectable raw runtime sector, truncated physical-sector
mount rejection, abort, reset with an active runtime FIFO, cold repeat, SYSRES,
no media and recovery on a valid medium. Existing deterministic CD tests retain
ECC/retry/no-change/reset state, software reset during transfer and optical
command/sector deadline coverage. No wall-clock sleep is used to complete a CD
operation. The browser checks actual Canvas bytes and local-file privacy; the
web asset allowlist and publication gates remain unchanged.

## Remaining compatibility limits

No Sega SDK library binary ABI, genuine Sega BIOS execution, authenticated disc,
commercial title or complete CD-block behavior is verified. This increment does
not add a host GFS service, general path traversal, a full file-handle API for
applications, asynchronous GFS callbacks, fragmented/multi-extent reader support,
optical-controller firmware or command-specific physical timing. Existing XA,
interleave, CD-DA and R–W functionality has its own bounded evidence. The reader
is synchronous and single threaded; simultaneous outstanding library calls are
unsupported, while two handles retain independent cursors and owned output RAM.
The wider #17 hardware scope and #81/#82 SCU endpoint finding remain unresolved.
See [executed validation](validation-issue-91.md) for results and CI limits.


The separate [Issue #93 streaming guest](cd-streaming.md) exercises drive
acquisition overlapping host FIFO/DMA, cached computation and actual VDP2 output,
with fast and pool-limited consumers. It corrects the full-buffer notification
at the last-slot allocation. This original synchronous reader and its complete
byte/cursor/IRQ/DMA/screen oracles remain unchanged; the new scenario does not
add asynchronous calls to its private library API.
