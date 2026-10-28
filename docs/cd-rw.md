# Timed R–W subcode and host packs

Related to #75 and #17. This implements the built-in decoder/host interface;
#17 remains open. A CD+G renderer and MPEG/CD-TEXT applications are separate.

## Primary contract and revisions

The later [Sega CD I/F §4.3](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/prgg/sysg/cdif/hon/p04_10.htm)
and its **figure4.5** (`grp/i069_01.png`) supersede the physical buffer count in
[ST-162-062094](https://antime.kapsi.fi/sega/files/ST-162-062094.pdf), printed
pp.39–40 (PDF pages27–28, figure4.4). The old manual describes 23 packs;
the later manual specifies **24 physical packs / six packets**. The later
§4.3(6) separately retains **23 unacquired packs** as the overwrite-recovery
condition. These numbers describe different things.

[Later §7.2.4(2), figure `i088_04.png`](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/prgg/sysg/cdif/hon/p07_11.htm)
specifies four packs/frame, 24 bytes/pack, 12 bus words/pack. In every byte,
R,S,T,U,V,W occupy bits5,4,3,2,1,0; bits7–6 are zero. Byte0 precedes byte1 in a
big-endian DATATRNS word. Header symbols0–1, header parity2–3, payload4–19 and
pack parity20–23 retain their positions in all 24 transferred bytes.

[IEC 60908:1999 §§17.6,19.1–19.9 and reproduced figures19–24](https://decode-orc.github.io/analogue-video-specifications/efm/IEC-60908-1999/IEC-60908-1999/)
were checked against the diagrams, including figure22's permutation and seven
pack delays, figure23's header-only Q input and figure24's P-before-Q order.
These are six-bit **Reed–Solomon parity** codes, despite Sega's shorthand “CRC”.
They are independent of channel-P/channel-Q status and CD-ROM ECC.

For both codes GF(64) uses polynomial `x^6+x+1` (`0x43`), primitive element2.
The P check rows are `H[r,i] = alpha^(r*(23-i))`, r=0..3, i=0..23.
The Q rows are `H[r,i] = alpha^(r*(3-i))`, r=0..1, i=0..3.
P corrects at most two symbols, Q at most one header symbol. All syndrome
rows must agree before a correction is applied. Final P and Q checks must both
pass; a Q header correction cannot conceal an invalid full pack. No finite RS
code guarantees detection of every error pattern beyond its correction radius.
Tests use frozen nonzero codewords, every single-symbol position/magnitude,
every pair of symbol positions, P-valid/Q-invalid input and selected
three-symbol header/payload patterns with no accepted correction.

[Later §8.2.3 function3.2](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/prgg/sysg/cdif/hon/p08_13.htm)
defines PACK bit0 and OVER bit1, and WAIT for an empty buffer. The existing raw
command interface exposes length12 in CR2, zero in CR3 and flags in CR4.
`0x2001` sets TRNS/DRDY only for a populated pack; it never invents zero packs
for media without subchannel bytes. A supplied ZERO-mode codeword is valid data.

## Caller-supplied media boundary

The one new representation is **SRW1**. It is not a claim to support raw `.sub`,
CDG, CD-TEXT or a new optical-disc container.

| Offset | Field |
| --- | --- |
| 0 | Four ASCII bytes `SRW1` |
| 4 | First absolute FAD, unsigned big-endian u32 |
| 8 | Frame count, unsigned big-endian u32 |
| 12 | Exactly count×96 bytes; four consecutive 24-symbol packs per frame |

The caller supplies symbols **after disc deinterleaving/permutation and before
P/Q correction**. FAD labels the reconstructed packet's disc position, with the
seven-pack interleave delay already compensated. The caller must perform IEC
figure22's permutation/delays if converting an interleaved optical stream.
The emulator does not apply that transformation again. In particular, these
bytes are uncorrected codewords, rather than corrected display instructions.

`Media::with_subchannel` accepts a borrowed bounded byte slice and returns a
validated owned medium. CUE can explicitly reference one companion using
`REM SATURN_RW "filename.srw"`. Resolution uses only the supplied file table;
there is no filesystem discovery, URL interpretation, automatic basename search
or network loader. `Media::cue_owned`, production Wasm `reset_cue`/`mount_cue`
and the local multi-file UI use this same parser. Select the CUE, all BINARY
files and the named SRW1 companion together; the browser accepts `.srw`.
Ordinary ISO/BIN/CUE without this REM retain their prior behavior.

Nonempty count≤450000, checked count×96+12 length, checked FAD addition,
zero upper symbol bits, and mapping of **every** frame to stored audio are
validated before publishing the attachment. Synthesized gaps, data tracks,
lead-out and out-of-disc FADs are rejected. Duplicate REM declarations, duplicate
names, a companion also used as a BINARY track, missing names, malformed quotes,
truncation, trailing bytes and overflowing counts fail. Existing aggregate CUE
limits (99 supplied files, 1058400000 bytes, 1 MiB CUE) still apply. The maximum
SRW1 payload is 43200000 bytes. Loading does not partially replace a mounted
medium: native construction and Wasm `mount_cue` validate first. Wasm `reset_cue`
retains its established failed-boot behavior of stopping old execution, rather
than continuing an old guest after an invalid new selection.

## Acquisition, pre-roll and publication

Sega §4.3(5) starts acquisition two 75 Hz frames before PLAY; figure4.5 starts
decoding after eight packs have been collected. §4.3(4) gives a 3.2 ms decode/
transfer interval. The drawing specifies ordering and steady four packs/frame,
but does not assign an exact oscillator phase or rational placement of the
remaining frame slack. No measured physical drive latency is claimed.

The deterministic convention is:

- Explicit CD-DA Play with decode enabled adds `ceil(2*HZ/75)=716643` reference
  clocks of pre-roll to the existing 26875-clock virtual seek. The acquisition scheduler captures four packs at each of
  `seek+ceil(k*HZ/75)`, k=1,2, filling an eight-pack window before PLAY. The
  already deinterleaved boundary compensates disc delay; pre-roll cannot publish
  a pack. Cancellation discards the window and produces nothing later.
- Entering CD-DA PLAY starts a fresh stream at the requested/current FAD and
  clears the pack ring. Enabling decoding alone, pause, seek and scan do not
  clear the retained ring. Unchanged-start PLAY while already decoding keeps
  the stream and only updates its end; a new start or repeat starts decoding
  again. Resume uses the existing retained-position, immediate PLAY convention.
- For stream pack n, publication occurs at `PLAY + 85998 + ceil(n*HZ/300)`
  reference clocks, HZ=26874100. Acquisition continues on separate 75 Hz
  deadlines and the decoder consumes its bounded eight-pack input window. Thus the first explicit-play publication is
  clock829516 after command completion. The 3.2 ms initial decode deadline
  follows pre-roll; steady deadlines distribute the frame slack uniformly.
  This is an emulator phase convention, not a claim that figure4.5 specifies
  equal spacing to oscillator precision.
- Acquisition has its own monotonically timed FAD/pack position. Host polling,
  passive inspection, audio sample draining and output mute cannot drive it.
  The main scheduler includes every decoder deadline. No pack is produced in
  PAUSE, SEEK, SCAN or a data track. Data→audio begins decoding; audio→data stops
  production and retains already decoded packs. End/repeat follows the retained
  playback range. No new R–W interrupt source is invented: existing 75 Hz SCDQ
  reporting notifies the guest, with independently latched CMOK/DRDY and SCU IRQ.

Before/at/after tests cover 100 publications; identical fixed-horizon traces use
clock chunks1/3/17/10000 and passive DATATRNS/CR4 inspection. Device tests also
cover transitions, range retention, repeat and separate audio consumption.

## Ring, transfers and initialization

The ring has 24 physical 24-byte slots. A command pins the oldest acquisition
sequence until DataEnd; it does not reserve an immutable copy. Production keeps
writing while a transfer is outstanding. A delayed reader therefore sees the
current contents of the physical slot, including overwrite of untransferred
words. Words already transferred are not replayed. Error flags describe the slot
and overrun state when the command completes. On DataEnd the pointer advances
to `max(acquired+1, written-23)`, recovering to the 23 newest unacquired packs.
At exactly 24 accumulated packs there is no overrun; the next overwrite sets it.

DATATRNS competition returns WAIT without consuming a pack. CPU halfword and
longword reads and fixed-source SCU DMA use the same cursor. Reads beyond
12 words return FFFF without incrementing the valid count. DataEnd reports the
CD-side valid word count (bounded at12), clears DRDY and does not raise EHST
for a subcode transfer. A canceled partial transfer still advances the pack
pointer. The later §8.2.1 DataEnd table requires CD-side count > host count for an
interrupted GET. R–W therefore counts one next word offered to DATATRNS: a
one-word partial read reports2, full/excess reads report12. The exact physical
prefetch depth is unspecified; one word is the deterministic convention. Existing
sector/TOC transfer counting is outside this scoped change.
The host's documented ≤16 packs/frame and ≤3.2 ms transfer limits are programming
requirements, not an artificial host polling throttle.

[CDC_CdInit §8.2.1](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/prgg/sysg/cdif/hon/p08_11.htm)
and [§5.5 initialization domains](https://github.com/luckyseoul/saturn-docs/blob/main/files/ssddv25/segahtml/prgg/sysg/cdif/hon/p05_10.htm)
retain the existing init bit1/no-change semantics. Ordinary initialization pauses
and cancels scheduled decoding, preserving queued packs and an active transfer.
Software reset, CD power OFF/ON and Machine reset discard decoder/ring/transfer
state. Tray opening/media replacement discard media-specific R–W data and cancel
an outstanding R–W DRDY; unrelated sector transfer domains retain their existing
rules. Starting fresh decoding invalidates an old outstanding R–W transfer along
with the old ring. These invalidations prevent stale slot bytes from appearing
under a new stream's sequence numbers. Periodic SCDQ remains the general Q/report
source and is not evidence that a queued old R–W pack survived a reset.

## Independent guest evidence

`python3 tests/fixtures/build_cd_rw.py --check` reproduces the original MIT guest
source, HLE-bootable ISO, CUE, stereo audio and five parity-bearing companions.
The builder solves parity matrices by Gaussian elimination; the production
decoder independently solves syndrome error locations. Native and browser
oracles freeze all four original and four changed nonzero codewords. They do not
obtain expected checksums or sequences from a production encoder/decoder.

The SH-2 installs SCDQ/A-bus and DMA-end handlers, acknowledges HIRQ and rearms
the SCU external input, enables decoding, plays audio, retrieves four packs using
alternating CPU/DMA transfers and records all words, checksums, flags, sequences
and DataEnd counts. Omitted enable returns WAIT and publishes zero retrieved
packs. Corrected header/payload input reproduces clean records; uncorrectable
header/payload input sets PACK and changes records. Changed clean payload changes
records. All variants repeat cold boots. The exact final stereo stream remains
`[1000,-2000]`; existing suite WebAudio assertions remain required.
