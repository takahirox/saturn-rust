# Cartridge and console-save validation

[Issue #260](https://github.com/takahirox/saturn-rust/issues/260) covers the
four exclusive slot configurations, browser battery storage, and the
existing Advanced controls. The evidence here uses original MIT-licensed SH-2
guests and a synthetic reset-vector ROM. [The wider hardware tracker](hardware-conformance.md)
covers other Saturn devices and accuracy limits.

## Cartridge evidence

Sega's hardware catalog identifies the 4 Mbit Power Memory HSS-0111/HSS-0138
and the 1 MiB HSS-0150 and 4 MiB HSS-0167 work RAM cartridges. ST-097 §1.2
figs. 1.3/1.5 defines the A-bus chip selects. STN-47 §§3–5 gives the expansion
bank addresses and IDs, the shared Power Memory ID address and the write-only
initialization strobe. Table 3 of the Japanese bulletin and ST-097
§3.2 give the A-bus register addresses. [Sega hardware catalog](https://www.sega.jp/history/hard/segasaturn/devices.html),
[SCU ST-097](https://antime.kapsi.fi/sega/files/ST-097-R5-072694.pdf),
[STN-47](https://docs.exodusemulator.com/Archives/SSDDV25/segahtml/info/hon/stn47_j.htm).

| Slot | ID byte at 04FFFFFF | Data decode |
| --- | --- | --- |
| None | FF | Pulled-up reads and ignored writes on CS0/CS1 |
| Power Memory | 21 | CS1 04000000–04FFFFFF. One 512 KiB packed store on the odd lanes, repeated each 1 MiB of bus address space. The final odd address exposes the ID instead of the mirrored data byte |
| 1 MiB expansion | 5A | CS0. Two 512 KiB banks at 02400000–0247FFFF and 02600000–0267FFFF. Each repeats once, through 024FFFFF and 026FFFFF respectively. The intervening 025 and 027 ranges are holes |
| 4 MiB expansion | 5C | CS0 02400000–027FFFFF. Two independent 2 MiB banks start at 02400000 and 02600000 |

CS0 occupies physical 02000000–03FFFFFF and CS1 occupies
04000000–04FFFFFF. The reserved select (05000000–057FFFFF) and CD CS2
(05800000–058FFFFF) stay separate. P0 and its P1 cache-through alias reach
the same physical selects. Both SH-2 caches follow the existing fill,
write-through and alias rules. The ID low lane is read-only. The adjacent
upper lane reads FF, so a word read at 04FFFFFE produces FFxx. SRAM reads
and writes use the odd lane of each 16-bit beat, including ordered byte/word/
long CPU transfers. Expansion DRAM accepts both lanes. Absent selections and
holes use pulled-up data rather than a remembered bus value.

The mirrors and holes of the 1 MiB model follow Ymir and Yabause. Mednafen
extends the 1 MiB mirror across both full 2 MiB bank selects. STN-47 defines
the usable ranges and leaves the unused ranges unspecified. Mednafen/Ymir
agree on the Power Memory ID and width of ordinary word data. Yabause and
Mednafen corroborate CS1's 1 MiB address wrap. Sources differ on unused
ID lanes, even-lane byte behavior and ID-write effects. The model holds
unused lanes high and ignores ID writes. The Power Memory chip-level
topology and electrical mirroring across board revisions remain unverified.
[Ymir expansion decode](https://github.com/StrikerX3/Ymir/blob/main/libs/ymir-core/include/ymir/hw/cart/cart_impl_dram.hpp),
[Ymir Power Memory ID](https://github.com/StrikerX3/Ymir/blob/main/libs/ymir-core/src/ymir/hw/cart/cart_impl_bup.cpp),
[Yabause CS0/CS1](https://github.com/Yabause/yabause/blob/master/yabause/src/cs0.c),
[Mednafen expansion](https://github.com/libretro/beetle-saturn-libretro/blob/master/mednafen/ss/cart/extram.c),
[Mednafen Power Memory](https://github.com/libretro/beetle-saturn-libretro/blob/master/mednafen/ss/cart/backup.c).

STN-47 §§7–9 permits byte/word/long CPU transfers, normal writes, burst
reads, and SH-2 DMA reads/writes. SCU DMA can read cartridge RAM. SCU
destination validation already rejects the A-bus. The guest issues the
documented word value 1 at 257EFFFE and programs ASR0 23301FF0 at 25FE00B0
and refresh 13 at 25FE00B8. The initialization strobe completes on the
ordinary bus and reads pulled up. A separate DRAM enable latch and the
effect of other strobe values are unverified. Existing ASR waits, precharge,
refresh contention and arbitration apply in the exact scheduler. Relaxed
playback shares data/order and the existing approximate interleaving.
Pin-level external WAIT, physical DRAM row/refresh behavior and the
32-bit internal read path described by STN-47 remain bounded by the
scheduler's existing A-bus bridge. [STN-47](https://docs.exodusemulator.com/Archives/SSDDV25/segahtml/info/hon/stn47_j.htm),
[SCU ST-097](https://antime.kapsi.fi/sega/files/ST-097-R5-072694.pdf),
[SCU final precautions](https://antime.kapsi.fi/sega/files/ST-210-110194.pdf).

ST-169 §2.3.8 describes SYSRES for the standard console and retains the
256 Kbit internal backup. STN-47's access procedure calls for switching
off power before reinserting an expansion cartridge. The powered slot
retains DRAM across SYSRES and watchdog manual reset, then cold power-on
and slot replacement clear it. This is a compatibility choice for
external RAM. Mednafen's reset callback clears only at power-up and
marks the behavior untested. Ymir clears when its cartridge reset method
is called. Both backup images and the SMPC battery retain data across
these resets. [SMPC ST-169](https://antime.kapsi.fi/sega/files/ST-169-R1-072694.pdf),
[STN-47](https://docs.exodusemulator.com/Archives/SSDDV25/segahtml/info/hon/stn47_j.htm),
[Mednafen expansion reset](https://github.com/libretro/beetle-saturn-libretro/blob/master/mednafen/ss/cart/extram.c),
[Ymir expansion reset](https://github.com/StrikerX3/Ymir/blob/main/libs/ymir-core/include/ymir/hw/cart/cart_impl_dram.hpp).

## Format compatibility

ST-162-R1 backup-library data specifications 1–4 and functions 1–11 specify
the ABI, one partition for internal memory and one for the cartridge,
and documented error results. Sega does not specify the physical filesystem
in that manual. Independently authored reverse-engineering notes for
SAROO and Save Game Copier, and Ymir's filesystem code, describe the
BackUpRam Format signature, status/header bytes, the block table and
payload continuation. Ymir also identifies the 64-byte internal and
512-byte cartridge block sizes. The evidence explains why a raw BIOS image
differs from SRBUP001, but does not establish safe writes to the range of
legacy, partly damaged, or variant BIOS images accepted as opaque raw data.
[ST-162-R1](https://docs.exodusemulator.com/Archives/SegaDTSMirror/antime/docs/ST-162-R1-092994.pdf),
[SAROO reverse-engineering notes](https://github.com/tpunix/SAROO/blob/master/doc/SAROO%E6%8A%80%E6%9C%AF%E7%82%B9%E6%BB%B4.txt),
[Save Game Copier allocation table](https://github.com/slinga-homebrew/Save-Game-Copier/blob/master/backends/sat.h),
[Ymir backup memory](https://github.com/StrikerX3/Ymir/blob/main/libs/ymir-core/src/ymir/sys/backup_ram.cpp).

Both HLE devices use the project's original SRBUP001 records. The internal
layout remains byte-compatible with older SRBUP001 images. The cartridge
uses 1024 blocks of 512 bytes, with 1022 available and the same 40-byte record
overhead. An unknown signature returns the unformatted status without
modifying bytes. A valid signature allows bounds-checked reads and writes.
Explicit guest Format initializes SRBUP001. The browser's confirmed Clear
operation erases the entire chosen image and leaves formatting to the
guest or firmware. There is no migration path. Genuine BIOS mode runs the
selected ROM over the unchanged raw storage bytes, with the supplied
firmware responsible for interpreting them.
[HLE backup library contract](../src/hle/README.md#independent-backup-library).

## Browser storage contract

The playback Worker opens origin-scoped IndexedDB before accepting commands.
Version 1 has one object store with a head, optional internal and cartridge
images, and a clock snapshot. The head contains a version, random commit token,
per-image length-checked integrity hashes and a host-time anchor. Each write
checks the last accepted token and updates the head and dirty records in one
read/write transaction. The completion event acknowledges the write. An
unobserved revision or site-data deletion blocks stale writes, including
destructive imports. Retry handles transient open/read/write failures.
After a conflict, export current RAM and clock settings and reload before
explicitly replacing data in the newer console. Opening an unreadable record
leaves it intact. If the session then changes backup bytes or clock settings,
retry preserves those changes instead of installing the recovered record over
them.

The battery snapshot contains the SETTIME calendar and time-set bit, the
four SETSMEM bytes and the subsecond phase. ST-169 §§2.4.2–4 distinguishes these
from live reset-button state and region straps. Restore advances this calendar
by the nonnegative milliseconds between the acknowledged host anchor and the
newly opened session. A backward clock gives zero elapsed time and the anchor
does not move backwards on commits. Native and Wasm clocks advance deterministically
with emulated time while a session is active. Paused tabs retain that time until
emulation proceeds. Gregorian month/year/leap rules and 9999-to-0000 wrap apply.
[SMPC ST-169](https://antime.kapsi.fi/sega/files/ST-169-R1-072694.pdf).

The separate 36-byte clock recovery file contains the ASCII signature
`SRSMPC01`, an unsigned 64-bit big-endian host time in Unix milliseconds, the
16-byte SMPC snapshot and a 32-bit big-endian FNV-1a checksum of the preceding
32 bytes. Import validates the file and the SMPC snapshot before replacing the
clock. A durable import uses the same revision check as RAM imports and
installs the recovered clock after IndexedDB acknowledges the commit.

Small revision checks run once per second. Clock-only periodic checkpoints are
at least 30 seconds apart and write 16 bytes plus the head. Pause, reset,
media/slot changes and tab hiding attempt an immediate flush. Page shutdown is
best effort. Dirty images alone cross the Wasm boundary and are hashed. One
coalesced transaction can carry 557072 bytes of image and clock data. Expansion
RAM is allocated only for the inserted expansion, uses 1 or 4 MiB and is
never part of persistence. The detached Power Memory lazily occupies 512 KiB
when initialized or restored. No storage reads, copies or scheduling work take
place on the frame hot path.

## Reproduction and acceptance

| Criterion | Checks |
| --- | --- |
| Evidence and device decoding | The original cartridge ROM checks ID byte/word/long and lanes, P0/P1, endpoints, mirrors/holes, independent banks, both SH-2s, SH-2 DMAC in both directions, and SCU DMA reads in exact and relaxed native execution. Existing A-bus timing, arbitration and reset oracles stay enabled |
| Real save services and format | Original HLE media tests internal ABI and external discovery, format, capacity, partitions, read/write/replace/delete/directory/verify, absent devices, corrupt lengths/checksums and unknown images. An independent reset-vector guest writes and reads raw storage |
| Reset and slot lifecycle | Native SYSRES and watchdog reset guests observe expansion retention. Cold resets/slot replacement discard expansion, preserve independent backup bytes and retain accepted media. Browser slot change leaves execution paused |
| Durable browser saves | Production Chromium commits and reloads actual HLE and reset-vector guest saves. Whole-image digests, read-only guest records, another disc, BIOS switches, detached Power Memory, fresh storage and site-data clear exercise the shared console |
| RTC and settings | Native guest SETTIME/SETSMEM and post-restore INTBACK, rollover vectors and rejected snapshots. Chromium probes guest SMEM after reload, injected elapsed time and rollback |
| Failure safety | Chromium uses real IndexedDB with permission/read failure, quota-aborted write, damaged/unsupported records, dirty snapshot acknowledgement and retry. Production tabs demonstrate stale-writer rejection, export recovery and confirmed clear. Existing browser coverage checks failed file reads and cancelled selection |
| UI and privacy | Chromium checks capacities, keyboard focus, Escape cancellation, responsive width, raw file bytes, and network/IndexedDB inventory. Existing player, audio, input, Debug and CRT groups remain active |
| Performance and regression | See local run results below. The normal PR CI gate still applies before merge |

Run scripts/check-fixtures.sh, the locked native suite, and the default
production Wasm build, then npm run test:browser. The focused Chromium
group is available as npm run test:browser -- --group=consoleSaves.
[Cartridge ROM source](../tests/fixtures/build_cartridge.py),
[native cartridge tests](../tests/cartridge.rs),
[HLE save guest source](../tests/fixtures/build_hle_backup.py),
[native BUP tests](../tests/hle_backup.rs),
[Chromium scenario](../scripts/console-saves.mjs).

## Local run results

- The locked native build and all 1,529 tests in the all-feature native suite passed.
  Focused final
  reruns passed the cartridge/SMPC reset-vector guests, the HLE backup guests,
  and the A-bus DMA publication oracle at the same deadlines for an empty slot,
  expansion DRAM and Power Memory. Formatting and native/Wasm Clippy passed.
- All 107 fixture commands passed. The original internal BUP ISO and its
  expected records remained byte-identical.
- The compatible and accelerated production Wasm builds passed with
  Rust 1.100.0, wasm-bindgen 0.2.129 and Binaryen 127. Both the baseline and
  current builds emitted LLVM warnings about discarded counts from the inherited
  PGO profile. The uncompressed compatible binary grew by 12,629 bytes and the
  accelerated binary by 21,386 bytes.
- Complete Chromium 145.0.7632.6 validation passed all 76 registered groups.
  The 32 shared groups took 300.626 seconds and the 44 scenarios took
  364.363 seconds. The aggregate verified coverage and the tested site digest
  c85e6b250d3abca4af901b2377e43e8485d4169ae8198007d01c0a91b307565a.
  Hosted PR CI remains pending.
- All 26 runner, pacing and Worklet tests passed. The direct production
  playback-audio verifier delivered 299 fields and 299 audio packets in
  5.00 seconds with a two-field queue peak and zero underruns or drops.
  The optional experimental playback and Worker facade differentials
  reproduced their respective field-time and register failures on the
  unchanged baseline as well as on this revision.

A bounded Star Collector comparison used the production player with clean
display, relaxed timing and real Worklet delivery in Chromium 145.0.7632.6 on
an Apple M4 Max. Each side warmed up for 60 fields, then presented three
180-field windows. Play was unpaced at the producer, and normal audio
admission continued to constrain delivery.

| Revision | Combined fields/s | p95 field gap | Peak pending fields | Audio underflow/drop |
| --- | ---: | ---: | ---: | ---: |
| Upstream 232d077 | 59.805 | 21.3–21.4 ms | 2 | 0 / 0 |
| Issue #260 | 59.805 | 21.4 ms | 2 | 0 / 0 |

Each measured field reached the Canvas. The comparison describes bounded
playback and makes no host-specific throughput assertion. Reproduce both
sides with scripts/benchmark-player.mjs, a production package for the desired
revision, and options --disc=web/star-collector.iso --unpaced --warmup=60
--fields=180 --runs=3. The --web option selects a frozen baseline package.
