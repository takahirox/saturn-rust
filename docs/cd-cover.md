# Live CD cover and media changes

Related to #263 and #17.

After starting a user-provided BIOS, select **Open cover** in the Disc slot.
Insert or drop a local ISO, a self-contained CHD v5, or a CUE with all referenced
files. **Change** replaces the inserted disc and **Remove** leaves the drive
empty. **Close cover** lets the guest detect the disc and act on it. A rejected
replacement or a cancelled picker leaves the previously accepted disc in the
slot. **Resume** is required if playback was already paused. Explicit Reset
still cold-boots the selected media with the cover closed. BIOS-less HLE retains
cold disc selection.

## CD block behavior

The guest-visible behavior follows §§3.1–3.2 for HIRQ, §4.1 for the manual
tray and TOC sequence, §5.5 for retention, §7.2.4 for report validity, and
§8.2.1 for TOC, session and opening commands. [Sega ST-162-062094](https://antime.kapsi.fi/sega/files/ST-162-062094.pdf)

Mednafen also invalidates
TOC/authentication/file metadata and raises DCHG on eject, then transitions
through BUSY during disc startup. Its Get TOC path waits during startup.
See [Mednafen's CD block implementation](https://sources.debian.org/src/mednafen/1.32.1%2Bdfsg-3/src/ss/cdb.cpp).

| Physical action | Guest-visible behavior |
| --- | --- |
| Start closed and empty | NODISC after initialization, with invalid position fields. TOC and session requests WAIT. |
| Open | OPEN with invalid position fields. HIRQ latches DCHG and EFLS at the opening boundary and normal write-zero acknowledgement clears them. The pickup, scan, queued read work, and CD-DA source stop immediately. |
| Insert, replace, or remove while open | The physical medium changes. The guest receives OPEN, or BUSY if a drive command is waiting for closure. TOC and session requests WAIT with no DATATRNS payload. |
| Close empty | NODISC, with no new DCHG edge. Any deferred drive command is discarded. |
| Close with a disc | BUSY with invalid position fields and TOC/session WAIT, followed by PAUSE at the first track or execution of the latest deferred drive command. TOC and session information becomes usable. DCHG reports the earlier opening and remains latched until acknowledged. |

Recognition lasts 1,343,705 reference clocks (50 ms). This provides three
stopped-report intervals at 60 Hz for polling and makes no claim about
measured spin-up time. A drive command in flight at the recognition deadline
can extend BUSY by up to one command-response interval (1,075 clocks).
Ordinary Initialize, Play, Seek (including Pause and Stop), and Scan issued
while open or recognizing return BUSY. The latest command supersedes any
earlier deferred command and takes effect after recognition. Soft reset takes
effect at command completion, clears the deferred command, and preserves the
physical cover position. Pending commands and unread CR4 responses retain their
normal handshake.
[Sega CD Communication Interface §4.1](https://docs.exodusemulator.com/Archives/SSDDV25/segahtml/prgg/sysg/cdif/hon/p04_10.htm)

The existing software-only automatic tray behavior of command 05 remains
available separately. A physically open cover requires a host close.

The block keeps initialization settings, play range, repeat state, selectors,
sector lengths and host-written sectors. It invalidates the disc-derived
sector cache (including copies), FIFO, file information, authentication,
and pending medium work on opening. Sega's generic tray contract retains
buffer data. The host exchange path explicitly flushes old disc-derived
sectors so they cannot cross the media boundary. The game can still use data
it placed in work RAM or other board devices before opening.

The Worker waits for the current field and presentation receipts. Sound is
canonical at that boundary. The Machine clears the SCSP's latched CD input
and drops accumulated mixed PCM on opening. The page clears host-queued PCM.
CPU, renderer, Sound devices, backup, and RTC continue on the same Machine
and playback epoch. The guest still controls system resets and game launches.

## Validation

- `cargo test --locked --all-features --test cd_cover` checks opening, interrupt
  and command acknowledgement, CD-DA and FIFO cancellation, retention,
  recognition timing, deferred drive commands, removal and replacement. The
  command regression verifies that Seek Home supersedes Play and leaves the
  drive in STANDBY with no sector delivery after recognition. The original
  reset-vector guest polls periodic CD reports and reads TOC, session lead-out
  and distinct sector data via MMIO in both exact and relaxed modes. The native
  harness checks PC, frame and clock continuity, WRAM records, and backup RAM.
- `cargo test --locked --all-features --test cdda` checks that the first audio
  field after opening emits silence with the original CD-DA routing guest in
  both exact and relaxed modes.
- `python3 tests/fixtures/build_cd_cover.py --check` verifies the original guest
  bytes. The adjacent [MIT license](../tests/fixtures/LICENSE) covers the fixture.
- `npm run test:browser -- --group=externalStartup` includes the same guest in
  production Wasm and Chromium. It exercises BIOS-only Play, live ISO picker
  and CUE drop, decoder rejection, cancellation, paused removal and Resume,
  including the guest records and local-only loading. The existing CHD browser
  group checks the shared CHD loader.
- The guest is an original diagnostic. Stock BIOS multiplayer compatibility,
  game-specific multi-disc behavior and measured mechanical timing remain
  outside this evidence.
