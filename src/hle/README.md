# Independent system services

The HLE disc boot installs original SH-2 code from [system.s](system.s), built
with the repository's original assembler by [build.py](build.py). The code and
generated ROM fragments are covered by [LICENSE](LICENSE). No SDK routine,
firmware, emulator core or instruction-interception hook is used. External BIOS
mode loads only its selected ROM and never installs these services.

The behavioral reference is Sega's [ST-162-R1 System Program manual §2.2](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-162-R1-092994.pdf)
and [its later function specifications](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/prgg/sysg/prog/hon/p02_11.htm).
Arguments use R4–R7, results R0, and callbacks obey the SHC callee-save convention.

| Work-RAM interface | Implemented service | Observable behavior |
| --- | --- | --- |
| `06000310`, `06000314` | SYS_SETSINT / SYS_GETSINT | Use the calling CPU's VBR; zero restores the appropriate dummy or SCU wrapper |
| `06000300`, `06000304` | SYS_SETUINT / SYS_GETUINT | Register/retrieve C callbacks independently of architectural vector overrides |
| `06000340`, `06000344`, `06000348` | SYS_SETSCUIM / SYS_CHGSCUIM / SYS_GETSCUIM | Write-only hardware mask plus RAM shadow; read/modify/publish excludes maskable interrupts; last address contains data |
| `06000330`, `06000334` | SYS_TASSEM / SYS_CLRSEM | All 256 shared bytes, real SH-2 TAS, uncached accesses on either CPU |
| `06000320`, `06000324` | SYS_CHGSYSCK / SYS_GETSYSCK | Guest-issued timed CKCHG320/352; return with caller SR and work RAM retained; public clock value is data |
| `06000280` | SYS_CHGUIPR | Copy all 32 priority words; later caller writes cannot alter the active table |
| `06000210` | SYS_PCLRMEM | Pointer to eight bytes; zero on cold boot, retained across NMI and SYSRES |
| `06000358`, `06000354` | BUP_Init, backup function table | Copy 16 KiB of original SH-2 library into caller RAM, relocate ten public functions, publish the table and three device configurations |

SCU callbacks execute through per-vector guest stubs and a shared guest wrapper.
It preserves R0–R7, MACH, MACL and PR, saves/restores the previous mask for nested
callbacks, applies the copied priority word, calls the registered routine and
returns using RTE. R8–R14 remain the callback's callee-save responsibility.
Initial priority masks permit strictly higher hardware levels; SYS_CHGUIPR can
replace every word. Reserved SCU vectors 4E/4F retain ordinary dummy handlers.

Private master-only mask/callback/priority state uses P0 accesses, which update
the caller's resident write-through cache lines. Shared semaphores use P1; the
dual-CPU guest likewise polls its inter-CPU mailboxes through P1. The service
diagnostic enables data caching and primes the mask shadow before changing it,
so an uncached private write leaving a stale public value fails its ABI records.

ST-162-R1 predates the [SCU final specification ST-210 #7](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-210-110194.pdf),
which prohibits writes to IST. These services follow that production restriction:
they update IMS and its shadow and arm AIACK when A-bus interrupts are enabled;
they do not write IST. Pending edges clear through ordinary SCU acknowledgement.
This explicit reconciliation does not add an ignored-MMIO path to the SCU.

Private HLE storage uses reserved work RAM below the loaded IP: master vectors
at `06000000`, slave vectors at `06000400`, semaphores at `06000A00`, callback
and copied priority tables at `06000B00`/`06000B80`, and power-clear bytes at
`06000C00`. Reset discards callback, priority and semaphore state. HLE startup
executes the original reset trampoline before entering the original disc AIP;
these 23 additional retired instructions are visible in diagnostics.

Run `python3 src/hle/build.py --check` and
`python3 tests/fixtures/build_hle_system.py --check` to reproduce both ROM
fragments and the original MIT diagnostic disc. [Native guest assertions](../../tests/hle_system.rs)
and [browser Wasm assertions](../../scripts/browser-smoke.mjs) compare authored
register records, nested callbacks, both CPU views, changed data/priority
operands and reset traces. The fixture is diagnostic media, not another game.

Clock calls mask interrupts during SMPC completion and update the public clock
value and reset SCU mask shadow before restoring the caller SR. The original
[clock disc and synthetic BIOS](../../tests/fixtures/build_clock.py) exercise
clock and reset domains in [native execution](../../tests/clock.rs) and browser
Wasm, including a changed caller argument. The clock disc carries the fixture
MIT license; the synthetic BIOS has no HLE installation.

This implementation does not provide SYS_EXECDMP or CD-library services.
Their required implementations and full
system/boot conformance remain tracked by [Issue #17](https://github.com/takahirox/saturn-rust/issues/17).

## Independent backup library

[backup.s](backup.s) implements the built-in device's documented backup-library
functions 1–11 from ST-162-R1 (backup chapter, data specifications 1–4 and
function specifications 1–11). The public addresses, table slots and numeric
status definitions were checked against Sega's original
[SEGA_BUP.H declarations](https://github.com/johannes-fetz/joengine/blob/master/Compiler/COMMON/SGL_302j/INC/SEGA_BUP.H).
Only interface facts were used; no SDK routine, object or firmware was copied.

BUP_Init executes SH-2 loads/stores to expand the original library to the
caller's 16 KiB buffer. It publishes that buffer at `06000354`; function slots
at offsets 4–40 implement SelPart, Format, Stat, Write, Read, Delete, Dir,
Verify, GetDate and SetDate. The first private word retains the caller's work
buffer pointer. Built-in device 0 reports unit 1/one partition; optional devices
1/2 are absent. R8–R14 and PR obey the SHC callee-save convention. Internal
helpers are ordinary SH-2 subroutines in this independent boot ROM.

Status values are success=0, absent=1, unformatted=2, write-protect=3,
space=4, not-found=5, found=6, mismatch=7 and broken=8. Invalid names/partition
arguments return other-failure=9. Internal RAM is writable, so there is no
write-protect switch for this device. Write checks capacity and overwrite policy
before altering stored bytes. Dir matches prefixes, writes at most the supplied
number of 36-byte SHC directory structures and returns the negative hit count
when the output table is short. GetDate/SetDate convert Gregorian dates for
the representable BupDate years 1980–2235 to/from minutes since 1980-01-01;
weekday is computed, independent of the SetDate input weekday.

The HLE has its own explicitly identified storage format, **SRBUP001**.
ST-162-R1 leaves the library's physical format opaque. No interoperability with
Sega firmware's private filesystem representation is claimed. External BIOS
mode continues to execute only the selected ROM against the unchanged raw RAM
bus. Unknown imported formats report unformatted and remain byte-for-byte
intact until the guest explicitly formats them. Installing HLE or calling Init
does not format, migrate or clear backup RAM. The existing packed raw
import/export, atomic length rejection, local-file privacy and reset retention
contracts remain intact.

Packed bytes are stored only through odd-byte bus addresses at `20180001`.
There are 512 physical 64-byte blocks: blocks 0–1 hold the eight-byte signature
and reserved zeros, leaving 510 blocks for dense file records. Each record has
the following original layout (offsets in packed physical bytes):

| Offset | Stored field |
| --- | --- |
| 0 | Active marker A5, published after data/metadata; zero ends the directory |
| 1–2 | Big-endian allocated block count |
| 3 | Reserved zero |
| 4–35 | First 32 bytes of the public BupDir (name/comment/language/date/size) |
| 36–37 | Big-endian directory block count |
| 38–39 | Sum of payload bytes modulo 65536 |
| 40 onward | Payload and zero allocation padding |

Record blocks are `ceil((datasize+40)/64)`. Delete/overwrite compact subsequent
records using guest byte copies and clear the released tail. Allocation scans
bound every next record and payload length to the physical window, check name
and comment terminators, language and redundant block counts, and reject broken
metadata. Read/Verify additionally check payload sums; this checksum detects
ordinary damage, not every possible same-sum change. Interrupted writes are not
transactional across reset, as the manual's reset-disable warning allows.
Stat reports physical total capacity/blocks, free block capacity and the number
of records of the requested size that fit. Raw bytes after a zero directory
terminator are unused. Dates outside BupDate's documented field ranges are not
validated by this void date-conversion ABI.

The original MIT [guest builder](../../tests/fixtures/build_hle_backup.py) creates
a redistributable diagnostic ISO carrying its license. It runs all ten copied
entry points via the published table. [Native tests](../../tests/hle_backup.rs)
and the identical [browser Wasm checks](../../scripts/browser-smoke.mjs) compare
158 independent ABI records, including all 510 allocated blocks, failed-write
retention, empty files, compaction, prefix-table limits, eight calendar vectors,
callee saves, changed guest computation, relocated code and persistent import/
export. Corrupt metadata/payload probes are read-only and bounded. Native vectors
add malformed count/size boundaries. Neither suite writes running guest results.
This evidence does not complete Issue #17's other hardware or service criteria.
