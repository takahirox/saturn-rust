# Executable external-firmware startup contract

Related to #85, #83 and #17. The original MIT
[builder](../tests/fixtures/build_external_startup.py) emits one 512 KiB ROM and
three original Saturn-IP/ISO9660 discs. The ROM executes SH-2 instructions to
read IP, descriptors, root records and file identifier 2 through CD MMIO and SCU
DMA. It loads the original AIP and enters it; that AIP jumps to the loaded file.
No host filesystem parser, HLE installation, executable preload, signature
recognition or result-writing hook participates in ExternalBios. All existing
Rust hardware scheduling and browser selection APIs are reused unchanged.

This establishes synthetic, firmware-executed discovery within the bounds below.
It does not establish stock Sega BIOS boot, authenticated retail media, broad
homebrew support, BIOS library compatibility or commercial-game compatibility.
No Sega security/area object, firmware, SDK routine or commercial media is included.

## Primary references and revision boundaries

PDF pages are one based. These sections were inspected in the linked primary
PDFs, rather than inferred from the fixture or the independent HLE parser.

| Reference | Exact location | Contract used |
| --- | --- | --- |
| [Disc format ST-040-R4-051795](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-040-R4-051795.pdf) | §3.1 printed14/PDF18; §3.2.1, table3.3 printed17–19/PDF21–23 | System area LSN0–15; descriptors begin LSN16; `CD001`, descriptor types, both-endian volume/block fields and root record |
| Same | §3.2.2, tables3.6/3.8 printed20–21/PDF24–25 | Record length, extended-attribute length, extent LSN, data length, flags, unit/gap, volume sequence and identifier length |
| Same | §§4.1–4.3 printed25–29/PDF29–33; §4.5 table4.3 printed30/PDF34 | IP structure, hardware ID, area symbols, IP size, master/slave stacks, first-read destination; CD ignores IP first-read size |
| Same | §§4.6–4.7, fig4.1 printed31/PDF35; appendix `smpsys.lnk` printed34/PDF38 | First-read file is loaded, **not automatically executed**; enter AIP after area group; IP linkage base `06002000` |
| [ECMA-119, 2nd edition, December1987](https://ecma-international.org/wp-content/uploads/ECMA-119_2nd_edition_december_1987.pdf) | §§6.7.1/6.8.1–6.8.2, §§7.2–7.3, §8.4, §§9.1/9.3 | Descriptor sequence, sector-contained records/zero tail padding, root self/parent, both-byte orders, PVD fields and directory record structure/order |
| [CD I/F ST-162-062094](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-162-062094.pdf) | terminology printed24/PDF12; §3.1 printed27–28/PDF15–16; §§3.2–3.4 printed29–32/PDF17–20 | FAD=LSN+150; file identifiers are directory record sequence numbers, 0=self/1=parent; CMOK/DRDY/CSCT/PEND/EHST; command serialization, response/REJECT/WAIT and FIFO/DataEnd |
| Same | §§4.2–4.3; §5.5; §8.2.1 functions2/3/5 printed77–80/PDF65–68; §8.2.2 function1; §8.2.3 function1; §8.2.6 function2; §8.2.7 function4 | TOC/lead-out, software reset, play range, device connection, partition count, GetDelete and transfer termination |
| [Boot ROM ST-079B-R3-011895](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-079B-R3-011895.pdf) | §§1–2 printed3–5/PDF7–9; §3.2 printed7/PDF11 | Reset/media boot ownership and boot-code checks; not a specification of this synthetic ROM's private diagnostic ABI |
| [SH7604 Rev.4](https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf), [SMPC ST-169-R1-072694](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-169-R1-072694.pdf) | SH7604 §4.2.2/table4.3/§5; SMPC table1.1/§2.3, SSHON/SNDOFF/CDON/CDOFF, SYSRES, INTBACK | Reset PC/SP, VBR/SR, guest exceptions/RTE; timed CPU/device ownership, SF/COMREG and status interrupts |
| [SCU ST-097-R5-072694](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-097-R5-072694.pdf), [ST-210-110194](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-210-110194.pdf) | ST-097 table2.1/§§3.2/3.5/3.6; ST-210 item7 and DMA corrections15–27 | DMA registers/fixed FIFO source, IRQ masks/vectors/A-bus acknowledgement; prohibited IST writes and corrected DMA rules |

ST-040 §4.3 prints an implausible `60020000H` lower-base value for first-read
addresses. The appendix links SYSID at `06002000`, and its own example loads at
`06010000`; the implementation uses that consistent high-work-RAM interpretation
and conservatively requires a destination strictly above IP end. It does not
copy the apparent typo into a RAM address. IP offset F4 is explicitly ignored for
CD, despite the older fixture's size there. ISO directory Data Length controls
loading, including partial final sectors. All three discs set F4 to the aligned,
intentionally contradictory `DEADBEEC`.

The first-read contract selects **record sequence identifier 2**, the first root
child after self/parent. It is neither a filename of `2` nor an arbitrary search
for a convenient executable. A directory/associated/multi-extent record there
fails; the loader never skips it to select GAME.BIN elsewhere. A renamed first
regular child still boots. With destination zero, loading is disabled and an
original AIP can run without a first-read file.

The AIP entry is `06002000 + E00 + 20 * area_count`, from the system-ID/security/
area-group layout in §§4.1/4.5/4.6. Supported symbols are distinct J/T/U/E followed
by spaces, one to four symbols. This synthetic path validates those metadata
bounds; it deliberately supplies no proprietary area/security code, authentication
or region enforcement. It does not claim that metadata checks substitute for the
boot-code checks required of a retail disc. The SMPC revision here is R1-072694.
The CD I/F PDF filename is ST-162-062094 (its pages are headed System Library
User's Guide); ST-162-R1-092994 is a different System Library revision, not an
interchangeable command reference. ST-210's prohibition on IST writes supersedes
older system-program examples. The [SCU endpoint limitation](sh2-interrupt-entry.md#reconciled-cpuendpoint-boundary)
remains in force.

## Supported discovery and memory bounds

The original [SH-2 generator](../tests/fixtures/build_external_discovery.py) and
[expanded assembly](../tests/fixtures/external-startup.s) contain every decision.
Multi-byte fields are decoded with guest byte instructions, including unaligned
records. ISO little- and big-endian copies must agree.

| Metadata | Accepted subset and validation |
| --- | --- |
| IP | Exact 16-byte hardware ID; reserved D0–DF/E4/F8/FC zero; area symbols above; IP size 1000–8000, longword aligned; complete IP fetched to06002000 |
| Stacks | E8/EC specify aligned SPs in060E0000–060FE000. Zero gives master 06002000 and slave 06001000, the ends of the default stack areas in §4.3. Both CPU stacks are established by guest firmware; slave remains parked in ROM |
| Descriptors | Bounded scan of LSN16–31; one version1 PVD, version1 terminator; type 0/2 descriptors can be passed over. Reserved/partition/duplicate PVD types fail. Volume18–450000 sectors and no larger than actual TOC lead-out minus150; single-volume set/sequence 1, block 2048, file-structure version 1 |
| Root | PVD record length 34, directory flag 2, no XAR/unit/gap, volume sequence 1/name 00; extent after descriptor set; size one to eight complete sectors, within declared volume |
| Records | Even length>=34, nonempty name fitting the record including required padding, no sector crossing; zero sector-tail bytes; flags only existence/directory; XAR/unit/gap 0, volume sequence 1; both-endian extents/data lengths within volume. Self and parent must match the root extent/size and names00/01 |
| First-read | Identifier2 must be regular and after root extent, length 1–65536 bytes, extent/rounded sector span inside volume; no XAR/interleave/multi-extent/associated/protected/record formats. No optional generic filesystem browsing |
| Destination | Aligned, strictly above actual IP end. Full byte interval must end at/below06010000, or lie within06020000–060D0000. This protects vector/default-stack RAM below IP, live diagnostics/log/metadata/staging RAM06010000–06020000, supplied stacks/guards and final system-owned 4 KiB |

These resource bounds are synthetic firmware policy, not new Saturn limits.
Unsupported optional ISO extensions fail within bounded loops. Boot/supplementary
descriptors are skipped structurally; their alternate filesystem semantics are
not implemented. System-use bytes are ignored; they cannot alter addressing.
This is not a full ISO conformance checker: dates, title text, path-table contents,
filename collation and unrelated file contents are not interpreted. The discs
nevertheless include consistent path tables and original licensed extra files.

## Executed stages and transaction fences

Cache-through aliases are used for I/O, with caches disabled. The inherited
power-on BSC waits, reference-clock device deadlines and ordinary bus grants
remain active. No helper advances time or writes a success marker.

| Stage | Ordinary guest operations and fence |
| --- | --- |
| Reset/control | ROM PC/SP00000200/060FF000; master VBR06000000, ICR.VECMD1; vectors47/4B/50 and illegal/slot/address traps. SF/COMREG execute SSHON02, SNDOFF07, CDOFF09, CDON08; timed completions and slave mailbox5353; status-only INTBACK10 generates vector47/RTE |
| Video | TVMD8000, back-screen red; only the executing disc program changes it to green on success |
| CD setup | 0401 software reset, clear HIRQ, enable CMOK interrupt, AIACK1/IMS377F; masked response poll for0000 then unmask/wait vector50; handler clears CMOK/rearms AIACK, then CD IRQ disabled for transfer polls |
| Medium | 0200 TOC, consume all 204 words through DATATRNS; record first track41000096 and actual lead-out; 0600 DataEnd; 3000 connects device/filter0 |
| Each sector | Clear CSCT/PEND; issue1080 with actual LBA+150 and length1; poll HIRQ PEND, not repeated status commands. 5100 must report exactly one buffered sector. Require/clear EHST, issue6300 GetDelete, require DRDY; SCU DMA0 fixed source05818000, destination safe aligned RAM, count 2048, add2/mode7/GO101 |
| DMA/host end | Vector4B increments a monotonic guest counter; wait for precisely the next completion. Only then issue0600; require 1024 transferred words, DRDY cleared/EHST set. Delete removes consumed sectors, so the next read cannot reuse old partition data |
| Parsing/loading | Fetch complete IP, scan descriptors, fetch whole root including sector boundaries, validate records; derive extent and size. Each executable sector goes to06017000 staging; guest byte-copy transfers exactly min(2048, remaining), leaving final sector padding outside the destination |
| Entry | All reads/copies must finish before phase 5. Master requests the IP-derived slave SP, waits for guest acknowledgement, loads its own IP-derived SP, JMPs to loaded AIP. AIP BSR/PR records its real entry then jumps to first-read destination. The loaded program records fetched PC/SP/VBR, checks actual device/IRQ context and computes 48 |
| Reset | Cold reset retains selected immutable ROM/medium and clears transactions/volatile RAM. Optional inherited ROM operand7F00 requests one guest SYSRES0D using backup flag5A; SYSRES restarts discovery with retained backup/medium |

The command, sector-completion and DMA polls each have a 200000-iteration bound.
REJECT/WAIT, absent sectors, aborted/reset reads, count/fence errors and decode
failure cannot reach AIP. Diagnostic E001 means absent medium; E002 invalid or
unsupported metadata/memory bounds; E003 command/transfer timeout or I/O failure;
E004 is the ROM illegal/slot/address trap (for example selecting a text file as
the first-read program). BAD is an actually executing guest's failed oracle.
After a partial copy, failure parks in ROM; stale or partial code is never entered
by the loader. A failure after DMA activation can leave hardware work pending;
cold/SYSRES restart cancels that state before a new attempt.

These results are diagnostic ABI values, not BIOS services or hardware status
codes. Firmware log06010400 contains request CR1–4 then response CR1–4, 32 bytes
per command; metadata06010300 records IP/entry/stacks/destination/volume/root/
file/length/copied/phase/read-count/handshake/descriptor-end. AIP/assigned-slave-SP
records are060102F0/F4. Error/success is060100FC. The bounded maximum of72 sector
reads plus setup fits the reserved log space. There is no host consumer of this
ABI in production. The harness observes it read-only.

## Reproduction and evidence

| Original disc | Root LBA/bytes; first-child record offset | File LBA/bytes | Destination / master SP | Areas; descriptor sectors; total sector DMA IRQs |
| --- | --- | --- | --- | --- |
| external-startup | 20/2048;68 | 21/4096 | 06004000 / 060FE000 | U;16–17;7 |
| external-startup-moved | 27/4096;92 | 35/4093 | 06030000 / 060FC000 | JT;16–18 (boot record before PVD);9 |
| external-startup-long | 31/4096;116 | 48/6145 | 06050000 / 060FA000 | JTUE;16–17;10 |

Self-record system-use lengths vary the first-child position. The latter roots
contain 48 additional original files and actual zero padding at a directory-sector
boundary. All three use the identical ROM. Native and production Wasm compare
complete executable/IP bytes, literal command/status tuples, IRQ counts, loaded
length, AIP/PC/SP/VBR, FIFO/SCU registers and unchanged padding. They relocate
records/bytes again and mutate an executed MOV count, changing the actual guest
sum from 48 to 31. Chunks 1/3/17/10000 and passive FIFO/response inspection preserve
complete clock/CPU/log state for every layout. Original #83 UI/privacy checks are retained, and Canvas comparison now covers
every 320×224 RGBA byte. Separate original abort/reset ROM probes issue their
command while a real play seek is pending; they cannot serve a valid boot oracle.

```sh
python3 tests/fixtures/build_external_startup.py --rom-output target/external-startup.bin
python3 tests/fixtures/build_external_startup.py --check --rom-output target/external-startup.bin
cargo test --locked --test external_startup
npm run test:browser -- --group=externalDiscovery
npm run test:browser -- --group=externalStartup
```

The reproducible [inherited control](../tests/fixtures/build_external_fixed.py)
retains #83's ROM and disc as `external-startup-fixed.hex/.iso`. Before modifying
the active firmware, an independently constructed valid disc moved its unchanged
first-read bytes to LBA35 and changed size to4093. The frozen ROM succeeds on its
original disc, then still requests FAD171 on the moved disc, loads zeros and
reports E002 without entry. The independent HLE parser confirms that moved disc
is valid; that check is not part of ExternalBios execution. Both oracles preserve
this control. Hashes, actual automation results and limitations are in
[Issue #85 validation](validation-issue-85.md); #83's historical fixed-layout
results remain in [its validation record](validation-issue-83.md).

The existing 1075-clock command response, 26875-clock seek, SMPC rounding, DMA bus
cadence and SCU endpoint tie policy are emulator conventions, not measurements
of optical/pin/oscillator timing. Full arbitration, CD corner cases, proprietary
security/area execution, regional enforcement, Sega BIOS services and #17's
hardware completion remain outside this finite discovery increment. The
independent HLE loader still supports its own broader bounded layouts, including
XAR/interleaving; that is separate evidence and performs no firmware work.

In browser User-provided BIOS mode, select the original raw ROM and medium in
either order. Both stay in tab memory; Reset/Restart retains them; changing mode
clears selections. Invalid ROM stops with no HLE fallback. Local bytes are not
uploaded or persisted. Native uses `reset(Boot::ExternalBios(&rom))` plus
`mount_media`; Wasm uses the same `reset_bios`/`mount_disc`/`mount_cue` paths.
