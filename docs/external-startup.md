# Executable external-firmware startup contract

Related to #83 and #17. The original MIT
[firmware builder](../tests/fixtures/build_external_startup.py) produces a genuine
512 KiB ROM with power-on and manual-reset vectors. Its companion ISO contains
an original 4096-byte first-read file at LBA21–22, also described by the ISO9660
root directory and Saturn IP. No Sega firmware, SDK routine or security object
is included. This is a synthetic hardware integration scenario, not a replacement
BIOS library or evidence that stock firmware or commercial games boot.

## Primary references and revision boundaries

The following sections were checked in the actual PDFs. PDF page numbers are
one based; printed page numbers are separate.

| Reference | Checked location | Applicable contract |
| --- | --- | --- |
| [Hitachi SH7604 hardware manual, Rev.4](https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf) | §4.2.2, printed 70 / PDF 86; exception table 4.3; §5 | Fetch PC/SP from reset vectors, VBR=0, SR.I=15; ordinary guest vector setup and exception/RTE |
| [SMPC ST-169-R1-072694](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-169-R1-072694.pdf) | table 1.1 printed 3 / PDF 13; §2.3 printed 21 / PDF 31; SSHON/SNDOFF/CDON/CDOFF printed 23/26/27/28; SYSRES printed 29 / PDF 39; INTBACK printed 36 / PDF 46 | Slave/sound reset ownership, CD access enabled at power-on, SF/COMREG protocol, timed completion, status-only interrupt, backup retention |
| [SCU ST-097-R5-072694](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-097-R5-072694.pdf) | table 2.1 printed 27 / PDF 43; §3.2 printed 41 / PDF 57; §3.5 printed 57 / PDF 73; §3.6 printed 61 / PDF 77 | DMA registers, fixed-source transfer, IMS masks, interrupt vectors/levels and A-bus acknowledgement |
| [SCU final specifications ST-210-110194](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-210-110194.pdf) | item 7 printed 4 / PDF 8; DMA corrections 15–27 | IST writes prohibited; final DMA rules supersede older register descriptions |
| [CD I/F ST-162-062094](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-162-062094.pdf) | §3.1 printed 27–28 / PDF 15–16; §3.4 printed 32 / PDF 20; §4.2–4.3; §5.5 / PDF 37; §8.2.1 initialization printed 78–79 / PDF 66–67; §8.2.7 transfer / PDF 84 | CMOK/DRDY/EHST, FIFO, command/response, independent drive/selector/host operations, software reset and actual sector transfer |
| [Disc format ST-040-R4-051795](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-040-R4-051795.pdf) | §§4.1/4.3, printed 25–29 / PDF 29–33 | IP metadata, executable load address/size and stack fields in the original ISO |
| [Boot ROM ST-079B-R3-011895](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-079B-R3-011895.pdf) | §1 printed 3 / PDF 7 | Firmware starts on reset and owns hardware initialization/media boot; no specification of this synthetic program's private ABI |

The SMPC revision actually used here is R1-072694, as in the existing conformance
matrix, rather than assuming that the Issue's R3-061694 URL identifies the same
text. ST-162-R1-092994 is the **System Library** manual; ST-162-062094 above is
the **CD I/F** manual. They are not interchangeable command specifications.
The older System Program instructions to write IST do not override ST-210 item 7.
The [existing CPU/SCU reconciliation](sh2-interrupt-entry.md#reconciled-cpuendpoint-boundary)
continues to apply: functional endpoint consumption/tie behavior is a convention,
not newly established SCU electrical evidence.

## Stages, accesses and fences

Addresses below use the SH-2 cache-through alias for external I/O. Cache is
disabled; the inherited BSC power-on waits are sufficient for this fixture.
There is no new instantaneous hardware service or synthesized guest result.
Native and browser harnesses bound startup to 2,000,000 step attempts. Individual
commands use the scheduled deadlines below; the optional SYSRES variant waits
through the existing 2,687,410-reference-clock reset interval. A missing fence
cannot be turned into success by advancing or inspecting the machine.

| Stage | Executing guest operations | Fence and independently checked consequence |
| --- | --- | --- |
| Reset | PC/SP at ROM 0/4 = `00000200/060FF000`; identity read at BCR1 distinguishes CPUs; master sets VBR=`06000000`, CCR=0 and ICR.VECMD=1 | Scheduled reset-vector data phases precede instruction execution; slave stays held until SSHON; no HLE pointers are installed |
| CPU/device control | SF=`20100063`, COMREG=`2010001F`: SSHON 02, SNDOFF 07, CDOFF 09, CDON 08; poll SF after each, record OREG31 | Existing reference-clock completions: CPU commands 807 and CD commands 1075 clocks; the slave executes its own reset vectors and publishes `5353`; CD reset discards prior FIFO/selector/command state while retaining the medium |
| Status/interrupt setup | Master installs vectors 47/4B/50; IMS=`B77F`; IREG0/1/2=`1/0/F0`, INTBACK 10 | Status-only INTBACK completes at 8600 reference clocks, generates SCU vector 47, and handler/RTE increments a guest counter; sound-off status byte=`35` |
| Observable video setup | TVMD=`8000`, BKTAU/BKTAL=0; red back-screen word in VDP2 VRAM | The disc guest checks TVMD, then changes the actual back-screen word to green on success; browser waits for two completed fields before checking Canvas |
| CD initialization | CR1–4 at `25890018/1C/20/24`; issue 0401 software reset and wait CMOK; clear HIRQ, enable CMOK in HIRQMSK; AIACK=1, IMS=`377F`; issue 0000 | Guest SR.I temporarily masks acceptance during the response poll, then unmasks and waits for vector 50; handler clears CMOK and rearms AIACK; no race between polling and IRQ acknowledgement |
| TOC | Issue 0200, read 204 words through DATATRNS=`25818000`, issue 0600 | Literal TOC first word pair=`41000096` (data control/ADR41, FAD150); DataEnd reports 204 words; no-media rejects TOC and publishes error E001 without entry |
| Disc acquisition | Issue 3000 to connect device to filter 0; clear CSCT/PEND; issue 1080/00AB/0080/0002 | Play two sectors from FAD 171; wait PEND rather than continuously issuing status commands; query 5100 returns exactly 2 buffered sectors |
| Transfer | Issue 6300/0000/0000/0002; SCU DMA0 source=`05818000`, destination=`06004000`, count 4096, address-add 2, mode 7, enable/GO=`101` | Existing scheduled FIFO reads and separate RAM writes copy the actual sectors; vector 4B is a transaction fence; DataEnd 0600 returns 2048 words and clears DRDY/sets EHST |
| Handoff | ROM sums all 1024 loaded longwords, compares its build-time checksum, establishes SP=`060FE000`, executes JMP to `06004000` | Loaded program uses BSR/PR to independently record entry PC=`06004004`; checks VBR/SP, all three IRQ counts, slave mailbox, video state, transfer count, TOC and its own first instruction; computes 7+11+13+17=48; only its instructions write 600D |
| Restart | Local cold reset reuses the selected ROM/medium; optional ROM operand at 7F00 requests one guest SYSRES 0D via backup flag 5A | Cold reset clears volatile memory, CPUs, CD mask/FIFO, pending commands and DMA. SYSRES retains ROM/medium and battery state. Neither path invokes boot_disc or installs HLE |

The nine literal request/response tuples are asserted independently in
[native](../tests/external_startup.rs) and [production Wasm](../scripts/external-startup.mjs).
The firmware writes its ordered transaction log at `06010400`; each tuple has
request CR1–4 followed by response CR1–4. It is a **guest transaction log**, not
a claim to record every physical bus edge. The actual MMIO and transfer are
performed by ordinary SH-2 instructions and existing scheduled bus/device code.

## Deterministic conventions and remaining omissions

The firmware's fixed first-read LBA, 4096-byte size, destination, stack, checksum
and result locations are its own documented ABI. Production code does not inspect
those addresses, ROM signatures or success words. The ISO's first-read entry is
original; this firmware does not implement a general ISO9660 boot-file search,
execute proprietary area/security blocks, authenticate media, or provide BIOS
libraries/menu services. CD media objects can represent original unauthenticated
data; no retail security behavior is claimed.

The inherited 1075-clock CD response, 26875-clock seek, reference-clock SMPC
rounding, SCU DMA bus cadence and external-vector endpoint policies remain
explicit emulator conventions. They are not new measurements of optical, pin or
oscillator timing. Existing deadlines remain scheduled, including reset/abort
cancellation; no host wall-clock callbacks replace them. Full bus/display/sound
arbitration, all CD corner cases, regional/security startup and stock Sega BIOS
and commercial-title compatibility remain separate hardware work.

The observed production defect was loss of the physical medium on an external
cold reset. Retaining the immutable media object while reconstructing the CD
block fixes the reset boundary; keeping a prior CD block would incorrectly retain
its transactions. No additional unsupported port needed a fabricated value for
this bounded trace. Existing MMIO/CPU/SCU/CD behavior proved sufficient for the
first handoff on inherited main, and is reused without relaxing faults.

## Reproduction and local selection

```sh
python3 tests/fixtures/build_external_startup.py --rom-output target/external-startup.bin
python3 tests/fixtures/build_external_startup.py --check --rom-output target/external-startup.bin
cargo test --locked --test external_startup
npm run test:browser -- --group=externalStartup
```

In the browser, select User-provided BIOS, then the original raw ROM and ISO in
either order. Both stay in tab memory. Reset/Restart retains them. Changing mode
clears both selections; replacing/clearing the disc explicitly discards the old
medium before reset. Invalid ROM stops in ExternalBios with no executable ROM
and no HLE fallback. Native callers use `reset(Boot::ExternalBios(&rom))` and
`mount_media`; subsequent external resets retain that medium. Wasm uses the
same `reset_bios`, `mount_disc`/`mount_cue` paths. `clear_media` is a host selection
operation before cold reset, not a guest electrical tray command. No local bytes
are uploaded or persisted to browser storage.
