# Bounded CD streaming and full-buffer notification

Related to #93 and #17. This increment corrects a missed full-buffer transition
in the existing drive scheduler. It adds an original MIT guest after the
unchanged [ExternalBios firmware](external-startup.md); it adds no host loader,
HLE service, disc signature branch or library interception.

## Primary contract and revisions

These public primary PDFs were downloaded and read directly. PDF pages are
one based. The corresponding printed page numbers are given explicitly.

| Reference | Sections used |
| --- | --- |
| [CD Communication Interface ST-162-062094](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-162-062094.pdf) | §§3.1–3.4, PDF15–20/printed27–32: word FIFO, clear-zero HIRQ, command classes, acceptance and transfer termination; §4.2, PDF25–26/printed37–38: current FAD, range completion and automatic full-buffer pause/resume; §§5.3.3–5.3.4, PDF33–35/printed45–47: connectors, errors and selected sector ownership; §8.2.1 function 1.10, PDF69/printed81: GET/PUT counts; §8.2.6 function 6.1, PDF80/printed92: 200-sector pool; §8.2.7, PDF83–86/printed95–98: GET/DELETE/PUT/copy; §8.2.8 function 8.6, PDF89/printed101: AbortFile |
| [SCU ST-097-R5-072694](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-097-R5-072694.pdf) | §2.1, PDF32–42/printed16–26: DMA data flow; §2.2/table2.1 and DMA-end, PDF43/49, printed27/33; §3.2, PDF57–66/printed41–50: retained parameters, priority and address update; §3.6, PDF77–88/printed61–72: A-bus selects and waits |
| [SCU final corrections ST-210-110194](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-210-110194.pdf) | §3 items1/2/4, PDF8/printed4: legal endpoints; items8/12, PDF9/printed5: A-to-B CPU restrictions and startup; items15–25, PDF10–12/printed6–8: register restrictions, add values, activation and descriptors |
| [Hitachi SH7604 hardware manual](https://github.com/luckyseoul/saturn-docs/blob/main/files/sh7604.pdf) | §7.10, PDF218–219/printed202–203: cached execution and internal peripheral accesses while an external master owns the bus; §7.11.2, PDF226–227/printed210–211: separate buses and cache coherence; §11.2.1, PDF314/printed298 and §11.3, PDF320/printed304: FRT and ordered high/low byte reads |
| [VDP2 ST-058-R2-060194](https://github.com/luckyseoul/saturn-docs/blob/main/files/ST-058-R2-060194.pdf) | §7.2, PDF193–194/printed175–176, figs7.4–7.5: back-screen VRAM, RGB555 and single-color mode |

ST-210 supersedes the older SCU register descriptions: the guest does not read
transfer-count/DSTA, write IST/DSTOP, enable A-bus preread, use three DMA levels
simultaneously, or write to a DMA channel while active. Its route is fixed-source
CD A-bus to incrementing work RAM-H, with legal add value2 and destination
update. The A-to-B startup rule is preserved, not generalized to this A-to-RAM
route. The wider SCU endpoint evidence limitation in #81/#82 remains unchanged.

## Operation and ownership matrix

| Stage | Permitted simultaneous work and exclusion | Publication, cancellation and evidence |
| --- | --- | --- |
| Drive Play `10` | Acquires sectors independently of accepted host GET/DMA and available selector commands. A second drive command may change/stop that acquisition; this is not another parallel drive | A sector is routed at a reference-clock deadline, then current FAD advances. Pool exhaustion holds the next FAD; no sector is decoded/published beyond capacity. §4.2 requires PAUSE/BFUL when filling the buffer and automatic continuation after space becomes available |
| Selector ESEL jobs | Connection/filter setup and queries remain available during host I/O. Commands of one end-flag class are sequenced; ECPY copy/move is a distinct engine | Existing routing, copy deadlines and connector restrictions remain. No promise of arbitrary same-selector or same-class parallelism is added. §§3.2/5.3.3–4 and existing CDSEL vectors establish this boundary |
| Pool and partitions | 200 shared slots, not 200 per partition. Guest slow pass retains 198 sectors in partition2, leaving two for stream partition1 | Every stored sector has its own retained identity and FAD. PUT's unwritten contents are unspecified and are never used as expected data. Full pool means no additional allocation; range end must still report BFUL even though there will be no next read |
| Host GET/Delete | One DATATRNS transaction. GET/GETDELETE/PUT/TOC/Q/file-info cannot replace an accepted FIFO; conflicting host commands WAIT. Queries `50/51/54` are independent | Accepted GET payload and selected IDs survive incoming drive sectors. GetDelete releases selected IDs at DataEnd, including an interrupted transfer; it cannot remove a shifted neighboring sector. DRDY clears and sector-I/O EHST sets there. §§3.4/5.3.4/8.2.7 |
| SCU DMA | Runs from the accepted FIFO through real bus grants. CPU external accesses contend; cached instructions and local FRT accesses may execute while DMA owns the external bus | Source words are consumed at physical completion, destination increments on publication, and DMA-end follows the last write. The unchanged firmware handler counts/acknowledges each completion; no transfer is completed by the test host |
| Master SH-2 | Guest discovers root records, schedules the stream, consumes prior chunks, sums bytes and writes actual VDP2 VRAM. A warmed register-only kernel runs during DMA; uncached IRQ-counter reads wait for the external bus | Cache coherence is the guest's responsibility: shared firmware IRQ counters use `26010204`, not its cached alias. FRT is read as two ordered bytes through TEMP. RAM stores after the kernel can wait even though its arithmetic already executed |
| HIRQ and SCU | HIRQ latches independently of its mask; SCU A-bus rearm and CPU vector acknowledgment remain separate. The guest polls masked CSCT/BFUL/PEND; DMA-end uses real interrupts | Clear only owned factors. New full-buffer vectors enable a latched masked BFUL, acknowledge it, check FIFO WAIT and prevent duplicate/lost sector release. SCU endpoint timing remains an explicit functional convention, not silicon proof |
| VDP2 and sound time | Raster and sound engines advance with reference/main clocks during guest waits, DMA and SLEEP | The guest's computed prefix/full sums change the real back-screen word. Native raster and production Canvas show changed pixels before PEND. This scenario selects video; it adds no audio generator or sound-conformance claim |
| Abort/reset | AbortFile interrupts an accepted directory/drive job and posts EFLS. FIFO and drive/selector cancellation belong to their respective reset domains | Original guest aborts before directory acquisition, rejects an unavailable FID and rearms. Device vectors reset immediately before/at full and after partial GET/DataEnd; cold/SYSRES and active slow-FIFO reset discard volatile work while retaining physical media. Existing pending-copy, medium-change and no-media vectors remain |

## Failure and smallest correction

Inherited `bf76a3f` checks capacity **before** reading the next sector. When a
sector consumes the last free slot, it publishes CSCT but leaves PLAY/BFUL
unchanged. Only a later failed acquisition enters PAUSE/BFUL. At the end of a
range, that later acquisition never runs, so BFUL is missing indefinitely.
This contradicts §4.2's buffer-full transition, rather than an arbitrary demand
for a measured optical-controller response time.

The correction in [CdBlock](../src/cd.rs) checks occupancy after actual routing
and FAD advancement, at the same accepted-sector event. It sets buffer-paused,
PAUSE and BFUL while preserving the existing next-FAD, range completion, flags,
FIFO identity and clock phase. Freeing a sector through DataEnd lets the
existing next drive deadline resume that FAD. At end of range it stays stopped;
BFUL and PEND may both be set. No SCU/bus implementation change is needed.

The [device oracle](../tests/cd.rs) retains 198 sectors and starts a two- or
three-sector range. Four 1075-clock commands have accumulated 4300 clocks.
Relative to Play command completion, sector 1 arrives at 174861 reference clocks,
sector 2 at 354022. At354021 the masked CSCT/BFUL value is 0; at 354022 it is12,
then PAUSE/FAD152, free 0 and partition count 2 remain visible after command
latency. Inherited main reports4 at 354022. A two-sector range also proves the
missing notification cannot be excused by waiting for another drive tick.

The same fences run with advance chunks 1/3/17/10000 and passive FIFO/CR4 reads.
Reset, partial FIFO, masked IRQ, WAIT and rearm vectors use the same medium and
literal register/data expectations. The existing SCU physical-word, retained
source/destination, preemption and reset vectors remain independent oracles.

## Guest and independent results

[Original builder](../tests/fixtures/build_cd_stream.py),
[assembly](../tests/fixtures/cd-stream.s),
[native assertions](../tests/cd_stream.rs) and
[Wasm/Chromium assertions](../scripts/cd-stream.mjs) use the unchanged original
firmware, actual IP/AIP and first-read FID2. They supply only physical media and
firmware. Root LSN comes from hardware-fetched firmware metadata; the guest
finds `STREAM.DAT;1` by comparing actual ISO record names. No extent or file
contents are supplied through a host API.

| Medium | Root / data LSN | Bytes / sectors | Guest byte sum | Final RGB555 / RGBA |
| --- | --- | --- | --- | --- |
| cd-stream | 20 / 64 | 12291 / 7 | 1566892 | E8AC / 99,41,214,255 |
| cd-stream-moved | 27 / 88 | 16387 / 9 | 2089312 | E160 / 0,90,198,255 |

Root record order and asset bytes differ. The binary is identical. Each medium
carries an accessible MIT license. Both native and JS independently specify
`((i*31+seed) XOR (i>>7) XOR (floor(i/2048)*19)) & 255`, with seeds 11/73 and
literal lengths. The final sector contains three file bytes; DMA consumes one
four-byte FIFO unit, verifies its padding byte and leaves the rest of the 64 KiB
canary area intact. DataEnd returns3 after those two host words, and 1024 for a
full sector. Earlier data is processed while later sectors are scheduled.

The fast pass drains sectors without exhausting the pool. The slow pass fills
the two remaining slots, captures CSCT/BFUL immediately after sector 2, consumes
one with DMA and waits for refill at the retained FAD. It also fills the pool
on the final acquisition, exercising BFUL with PEND. All sector FADs, complete
DMA output arrays, both sums, source/destination registers, FIFO termination and
27/31 actual DMA interrupts are independently checked. One initial firmware
A-bus interrupt is acknowledged exactly once.

Each 64-byte record at `06071000` contains 16 guest-written words: sector index,
pre-GET HIRQ, stored sector FAD, partition count, pre-DMA IRQ count, pre-DMA FRT,
free slots during GET, post-computation FRT, previous-chunk sum, cached-kernel
FRT duration, DataEnd count, published DMA destination, post-DataEnd HIRQ,
post-DMA IRQ count, destination base and requested DMA length. Commands and
actual responses occupy 8-word records at `06074000`; global fields at
`06070000` contain discovered metadata, phase, sums, immediate full-buffer
sample, trace ends and success/error status. These are guest diagnostics, not
production success registers. Existing passive debugger APIs only inspect them.

A cache-warmed 16-iteration register kernel and local FRT accesses run after GO;
normal full-sector records measure 12–13 FRT ticks, with a deliberately broader
6–99-tick functional bound allowing the short final DMA's interrupt. This is
not an optical timing measurement. Uncached publication then waits for bus
ownership. Full previous-sector sums and independently expected changing
back-screen pixels are observed with master instruction and raster progress
before the finite Play ends. Two frame boundaries expose the completed raster
field; advancing only one can still return the field begun before the color
write. Final native framebuffer and Canvas comparisons cover every pixel.

## Deterministic policies and remaining limits

Command 1075, seek 26875 reference clocks, default data 150 Hz, selector raw-sector
copy 2352 clocks, one offered GET word, simultaneous drive-before-copy ordering,
RGB555-to-eight-bit expansion and the existing raster latches are deterministic
emulator policies. The primary sources specify functional ordering and allowed
operations, not those measured physical latencies or all electrical phases.
No new wall timer, callback scheduling or host data-transfer shortcut is added.
The full guest snapshot at 2000000 steps is compared across chunks 1/3/17/10000
natively and10000/17 in Wasm, including passive inspection, clocks, CPU state,
records, fetched bytes and pixels.

Full cross-chip arbitration, arbitrary asynchronous CDC/GFS calls, real Sega
BIOS/SDK ABI and commercial games are not verified. Complete connector lifetime,
command-specific timing, optical mechanics, every error/contention permutation
and the wider #17 hardware scope remain pending. The old synchronous runtime
reader and all its assertions remain. See [executed validation](validation-issue-93.md)
for the inherited control, complete checks, fixture hashes and pre-merge CI limit.
