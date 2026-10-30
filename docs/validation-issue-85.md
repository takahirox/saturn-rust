# Issue #85: firmware-executed first-read discovery

Related to #85 and #17. This checkpoint replaces the active original synthetic
ROM's fixed-disc assumptions with ordinary SH-2 discovery and loading. It uses the
existing CD/MMIO/SCU scheduler without Rust, Wasm API or player changes. The
[boot contract](external-startup.md) records exact primary revisions/sections,
address/entry reconciliation, supported metadata, transaction fences and bounds.
No original Sega BIOS, security/area object, proprietary media, console testing,
human approval or live Pages inspection was used or is required.

## Inherited-main control, before firmware changes

The selected repository worktree starts from application main
`260f1a3fd543faf9d8fca74d221ec69257bbcbd5`, followed by GitWeave bookkeeping
commits through `85423359c9e8582037411deec2da95d6f32feb8a`. Issue #85 was read
through the connected GitHub issue tool and was open. No upstream diagnosis was
provided beyond the issue snapshot.

Before editing the active firmware generator, a native control moved the original
first-read file from LBA21 to35 and changed its length from4096 to4093, retaining
all selected executable bytes and its original destination/stack. The directory's
both-endian extent/length fields were updated and old sectors cleared. The
independent existing HLE parser accepted the resulting disc and returned exactly
the moved bytes. The unchanged ROM nevertheless requested FAD171, copied zeros
from its old location and reported E002 before entry. The focused inherited-main
control passed (1 test), demonstrating the actual fixed-read failure rather than
an invalid whole-disc image.

The [frozen builder](../tests/fixtures/build_external_fixed.py) reproduces that
inherited ROM/ISO unchanged as `external-startup-fixed.hex/.iso`. Native and
production browser Wasm first prove that this ROM succeeds on its own original
disc, then apply the same moved-file control and check FAD171, zero loaded bytes,
E002 and no program entry. A different genuine original 512 KiB ROM selected after
a successful boot also cannot reuse prior code/success; short invalid ROM
selection explicitly errors, retains selected medium/backup and never falls back
to HLE.

## Implemented behavior and independent observations

The active ROM has no selected-file extent, length, destination, stack or
build-time executable checksum. Its SH-2 code fetches full IP and descriptor/root
bytes through scheduled CD requests and SCU DMA, validates fields in guest RAM,
selects root record sequence identifier 2 and obtains its extent/length from that
record. IP F4 is ignored for CD; every valid disc supplies contradictory
`DEADBEEC`. Destination and both stacks come from IP. Master enters the loaded
AIP; the original AIP owns the jump to the loaded program. Zero destination
skips first read, with or without a child file and an original standalone
AIP; it still rejects a root missing mandatory self/parent records. Renaming GAME.BIN does not affect selection. Pointing record 2 at the original license text
loads that text and reaches the ROM exception trap, never the guest success oracle.

| Disc | Directory; record 2 offset | File | Destination / master SP | AIP entry / DMA IRQ count |
| --- | --- | --- | --- | --- |
| baseline | LBA20,2048 bytes;68 | LBA21,4096 bytes | 06004000 / 060FE000 | 06002E20 /7 |
| moved | LBA27,4096 bytes;92 | LBA35,4093 bytes | 06030000 / 060FC000 | 06002E40 /9 |
| long | LBA31,4096 bytes;116 | LBA48,6145 bytes | 06050000 / 060FA000 | 06002E80 /10 |

The moved disc has a boot descriptor at16, PVD17 and terminator18; the others
have PVD16/terminator17. Self-record system-use lengths vary record positions.
The two larger roots contain 48 additional original files and cross a real
record/padding sector boundary. No firmware operand changes between these discs.

[Native](../tests/external_startup.rs) and
[production Wasm](../scripts/external-discovery.mjs) author their literal layouts
and expected CD responses separately from the Python generator and Rust parser.
They compare every loaded executable and IP byte, final padding, derived metadata,
AIP BSR/PR entry, program PC/SP/VBR, guest arithmetic48, slave SP, actual SMPC,
DMA/CD IRQ counts, DMA register readback, exhausted FIFO and DRDY/EHST. Each read
uses a one-sector play/count/GetDelete/DMA/DataEnd sequence; literal request and
response tuples include positions, statuses, counts and1024-word completion.
No emulator helper generates expected results. The existing UI scenario retains
both sequential/overlapping file selection/completion orders and reset/restart,
network-method/body and browser-storage privacy checks, HLE isolation and the
entire 320×224 Canvas RGBA comparison against literal green.

Each disc is relocated again to LBA40 by an independently authored test. Mutating
a real fetched `MOV #4,R3` to `MOV #3,R3` changes the guest-computed result from 48
to 31 and produces its BAD diagnostic. All relocated/mutated instruction bytes
are compared, proving that neither stale data nor a canned result supplied the
handoff. The HLE parser is used only as a native cross-check of that independently
constructed valid image, outside ExternalBios execution.

For every layout, native and Wasm compare fixed1,000,000-step runs in chunks
1/3/17/10000, with and without passive FIFO/response inspection. They preserve
complete clock/CPU/log state and once-only effects. Cold restart reloads fresh
sectors; guest SYSRES retains the selected ROM/medium/backup. Pending command,
metadata and partial-file reset controls clear old code and transaction state
before repeating discovery. Eject/restart reports no medium instead of executing
old RAM.

The 47 metadata controls cover hardware ID/reserved fields, empty/duplicate/unknown/
mis-padded/excessive area symbols, IP size/alignment, unsafe stacks/destination/
wraparound/workspace overlap, descriptor ID/version, medium/volume/block bounds,
endian disagreement, PVD root shape/name/extent/size, self/parent mismatch,
record/name lengths/flags/sequence, missing/directory first child, zero/oversized/
out-of-range file, XAR, interleave unit/gap, associated and multi-extent formats,
and a physically shortened medium. All stop explicitly before AIP and executable
copy. Additional controls omit connection/read/deletion, issue actual abort or
software-reset commands after a play response reports a still-pending seek, and supply an actual uncorrectable
raw Mode1 sector at the second executable sector. That decode failure occurs
after 2048 bytes have been copied; it parks in ROM with E003 and never enters AIP
or the partially loaded program. No blanket retries, flaky skips or host writes
to CPU/volatile RAM are used to make a valid boot pass.

## Automation and corrections

Validation host: macOS arm64; native Cargo/Rust1.94.0 (Homebrew), Wasm Rust stable
1.99.0 with installed wasm32 target, wasm-bindgen0.2.129, Node26.12.1,
npm11.20.0 and Playwright1.58.2/Chromium145.0.7632.6. The initial web-build attempt
selected Homebrew Rust, whose own target installation lacked Wasm core; selecting
the installed rustup stable toolchain as prescribed by setup.md fixed the build.
No dependency or tool version was changed in the repository.

During implementation, earlier expectations still described the fixed two-sector
transfer and were replaced by independent actual per-sector traces. An assertion
against write-only SCU count/status ports was removed in favor of documented
source/destination readback, actual IRQ/fence and full-data checks. A root outside
the declared volume initially surfaced as an I/O error; validation now rejects its
extent before starting a root read. Growth past the inherited early error branch range was corrected with an absolute
conditional jump. The stricter EHST fence exposed an initial
blanket HIRQ clear; the ROM now preserves reset-provided EHST and acknowledges
ESEL before selector connection. The first complete browser attempt failed at
that fence; it was corrected without relaxing the oracle or retrying unchanged
code. The complete suite was then rerun.

| Check | Final result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo build --locked` | Passed |
| `cargo test --locked` | Passed: 1135 tests = 739 library + 396 integration; zero failed/ignored; 376.42s |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed, no warnings |
| `cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings` | Passed, no warnings |
| `sh scripts/check-fixtures.sh` | Passed: all 100 registered builders, including all discovery/probe/frozen fixtures |
| `sh scripts/build-web.sh` | Passed with matching CLI and rustup Wasm target |
| `node --test scripts/ci-runner.test.mjs` | Passed: 13 tests, zero failed/skipped, including real discovery failure propagation |
| Complete `npm run test:browser` | Passed: 71/71 groups; shared 30 (259081ms), scenarios 41 (370666ms), total371235ms |
| Required PR GitHub Actions | Not run by this node: publication prohibited; downstream pre-merge requirement |

During final review, the cancellation controls were strengthened
from command substitution to separate original guest ROM probes that issue75/0401
after a genuine1080 play response reports SEEK0400. They never participate in a
valid-disc success oracle. The final 13-test native startup harness and final
production Wasm `externalDiscovery` group passed those stronger probes, including
native re-selection/recovery on the retained medium. A subsequent source review also made zero first-read destination bypass file
selection while retaining mandatory self/parent validation. Native and Wasm
standalone-AIP controls cover present/absent child files and an empty invalid
root. Both complete commands subsequently passed (native 376.42s; browser 371.48s).
The final focused native startup harness passed 13/13 tests (25.62s), and final
Wasm discovery passed (73.347s), covering the strengthened zero-destination and
pending-command controls.
The final fixture registry, native Clippy and13-test CI-runner command also passed
again. The 13-test native startup harness includes7 tests beyond #83's6.

The fixture registry still contains all 100 builders; the registered external
builder also checks every new disc/source and frozen control. Browser inventory
is71 groups (30 shared/41 scenarios), retaining the complete 64→69→70 lineage.
The inventory, missing/duplicate/stale/corrupt-artifact and failure gates remain
active. A new real discovery-oracle failure injection must fail its supervised
worker and remove a stale success report. Firmware/fixtures are not deployed web
assets; the exact public-asset allowlist and full pixel/PCM/security tests remain.
No GitHub push, PR, review, merge or comment was made in this node.

## Reproducibility hashes

The expanded 512 KiB active ROM SHA-256 is
`4682382f9c517db8c69f196283546521bf48f560288259305399b292f555e107`. The exact same bytes are selected for all three valid layouts.

| Artifact | SHA-256 |
| --- | --- |
| `external-startup.hex` | `ddacd88bfe52625514c49d35ea103f34df563fb113c8dd2747f8f367f7eecf6f` |
| `external-startup.s` | `3bb6708dd5dcc9451a2f6d89c1df89993f222ccdabef08da329fcb2a5adcf1b3` |
| `external-startup.iso` | `efc5289c5a5f72e701415eaa0fa3228f46e2b5d879cba6aa7353754b739dec40` |
| `external-startup-moved.iso` | `fc5a763fb9715d18cae264214f471a825ef6dcdd485c1a45c04017bbf2c66edd` |
| `external-startup-long.iso` | `5d9615a47642f9b993f8a6aa7f419d88f016f1b0d19f4bdada3fae80ecd5aba4` |
| `external-startup-abort.hex` | `995613fcd7b63a1a76aff3faff696ebba495113712744ecc0602d8a3a4587ed8` |
| `external-startup-read-reset.hex` | `4a1624d70ce830db77fb36929f19a19608bb9068eb599c1e5e8942bfd972a2a6` |
| `external-startup-fixed.hex` | `767128f277d6e355f357f09b2b1fe5600ac25bd08184695387877f350c652d27` |
| `external-startup-fixed.iso` | `c8c1228f1b1d29e507a3e6d61be9921c18fbfcae0c0e91d0251b8da041774092` |


## Remaining acceptance limits

The firmware accepts contiguous single-extent2048-byte-sector first reads only,
with no XAR/interleave/multi-extent addressing, at most64 KiB selected data,
one-to-eight root sectors and the bounded descriptor/IP/stack/RAM policy in the
contract. Unsupported layouts fail explicitly rather than being truncated or
silently routed to the broader independent HLE parser. This does not implement
all ISO validation, BIOS libraries, system menu, proprietary security/area code,
authentication, real regional/PLL behavior or exhaustive CD/reset/bus corners.
Scheduled deadlines and SCU endpoint conventions are existing emulation choices,
not new electrical/optical measurements. #81/#82's endpoint limitation is unchanged.

Required hosted PR checks remain pre-merge work for the downstream workflow;
local checks cannot be reported as hosted results. Issue #17 remains open and no
hardware-completion or retail/broad-homebrew compatibility claim follows from
this checkpoint. Required post-merge verification: **None**.
