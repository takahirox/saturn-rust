# Star Collector: original Saturn homebrew

Selected target for Issue #7, before peripheral implementation: a silent, original
collect game using only the master SH-2, high work RAM, VDP2 NBG0 RGB bitmap and
SMPC direct port 1. Move the cyan square with arrows. Touch each yellow square;
five gold progress blocks and a green completion banner mean success. Enter
(Saturn Start) restarts from any state. The guest owns movement, collision,
score, completion, restart and all drawing. No host game logic is involved.

All files in this directory and the generated `web/star-collector.iso` are covered
by [LICENSE](LICENSE). There are no imported assets, fonts, music, SDKs, libraries,
Sega security objects or firmware. The small assembler/ISO writer is original
Python 3 standard-library code; Python (PSF license) is a build tool and is not
linked into the image. No cross compiler is required.

Build from the repository root:

```sh
python3 homebrew/star-collector/build.py
python3 homebrew/star-collector/build.py --check
```

`game.s` is big-endian SH-2 assembly for `0x06004000`, not a host executable.
`load constant,rN` expands to a PC-relative MOV.L, BRA/NOP over an aligned literal.
`bt.l`/`bf.l` expand to the opposite short conditional branch over BRA/NOP.
The build checks branch ranges, alignment and operand ranges; the encoder uses
Hitachi SH-1/SH-2 Programming Manual instruction encodings, without SDK code.

## Disc and boot contract

The reproducible 64-sector ISO has 2048-byte cooked sectors, an ISO9660 primary
volume at sector 16, terminator at 17, path tables at 18/19 and root at 20.
`GAME.BIN;1` is the first root file (file identifier 2); `LICENSE.TXT;1` follows.
The first 4096 bytes are an original Saturn IP system area: hardware identifier,
one U area, IP size `0x1000` at `0xE0`, master stack `0x060FF000` at `0xE8`,
first-read destination `0x06004000` at `0xF0`. AIP at `0xE20` jumps to that file.
The HLE loader copies IP to `0x06002000`, loads the first root file to the
header's first-read destination, then enters AIP at `0x06002E20`.

Sega security/region executables and the physical security ring are omitted.
This is a Saturn-targeted homebrew disc for the documented HLE boot contract,
not a pressed/stock-console authenticated disc. Stock boot ROM boot and other
emulators have not been validated. HLE implements no runtime BIOS services or
CD reads; all program data is loaded before execution. No custom host opcode,
magic game identifier, renderer hook or game-state replacement is used.

## Hardware contract

VDP2 registers: TVMD `0x05F80000=0x8000` (320×224), BGON `+0x20=0x0101`,
CHCTLA `+0x28=0x0032` (NBG0 512×256, 32768 RGB colors), MPOFN `+0x3C=0`,
ZMXN0/ZMYN0 `+0x78/+0x7C=0x00010000`, PRINA `+0xF8=1`.
Bitmap pixels at `0x05E00000` use 1024-byte pitch and RGB555 words. TVSTAT
`+4` VBLANK bit 3 is polled once per update. VRAM cycle arbitration is not
modeled by this emulator; the guest sets CYCA0L/U `+0x10/+0x12=0x4444/0xEEEE`
(four NBG0 bitmap fetch slots and four CPU slots).
SMPC PDR1 `0x20100075`, DDR1 `+4=0x60`, IOSEL `0x2010007D=1`, EXLE `+2=0`:
TH/TR=01 reads active-low directions; 10 reads Start/A/C/B. The guest inserts
a settling delay; the emulator approximates the electrical response as immediate.

Instruction families: NOP, MOV immediate/register, MOV.B/W/L indirect loads and
stores, MOV.L PC-relative, ADD immediate/register, SUB, SHLL/SHLL2/SHLL8,
CMP/EQ, CMP/HS, TST immediate, DT, BT/BF, BRA/BSR, JMP and RTS. Interrupts
are masked; no exceptions, slave SH-2, audio, SCU DMA, VDP1 or runtime CD access
are used. `0x06010000` contains five guest-written diagnostic longs (x, y, score,
completed, update count) for tests/debuggers; the host never writes this state.
These diagnostics are not an emulator ABI or a requirement for other discs.

See [architecture](../../docs/architecture.md) for manual references and exact
emulated limitations. Other Saturn homebrew is not claimed compatible.
