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

This implementation does not provide SYS_EXECDMP,
backup-library or CD-library services. Their required implementations and full
system/boot conformance remain tracked by [Issue #17](https://github.com/takahirox/saturn-rust/issues/17).
