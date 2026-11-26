#!/usr/bin/env python3
"""Build the original MIT-licensed reset-vector CD cover diagnostic."""
import argparse
from pathlib import Path
import struct
from build_scu import assembler

HERE = Path(__file__).resolve().parent
RECORDS = 0x06010000
HIRQ = 0x25890008
CR1 = 0x25890018
FIFO = 0x25818000


def command(cr1, cr2=0, cr3=0, cr4=0):
    return (f"load {cr1},r0\nload {cr2},r1\nload {cr3},r2\nload {cr4},r3\n"
            "bsr command\nnop\n")


def record(reg=0):
    return f"mov.l r{reg},@r11\nadd #4,r11\n"


def read(address, width="w"):
    return f"load {address},r4\nmov.{width} @r4,r0\n" + ("extu.w r0,r0\n" if width == "w" else "")


def acknowledge(mask):
    return f"load {HIRQ},r4\nload {0xFFFF ^ mask},r0\nmov.w r0,@r4\n"


def build():
    source = f"""
        load 0x00180001,r1
        mov #0x5a,r0
        mov.b r0,@r1
        load {RECORDS},r8
        mov #0,r9
        mov r8,r12
        add #12,r12
        mov r8,r10
        add #16,r10
        mov r8,r11
        add #4,r11
        load 0x434F5645,r0
    """ + record()
    source += command(0x0401) + record() + command(0x3000)
    # Ack startup causes, then poll the drive through normal periodic reports.
    source += acknowledge(0xFFFF) + """
    cycle:
        mov r10,r11
    await_open:
        bsr periodic
        nop
        load 0x600,r5
        cmp/eq r5,r1
        bf await_open
    """ + record()
    source += read(HIRQ) + "mov #32,r5\nand r5,r0\n" + record()
    source += acknowledge(0x20) + read(HIRQ) + "mov #32,r5\nand r5,r0\n" + record()
    source += command(0x0400, 0xFFFF) + record()
    source += command(0x0200) + record() + command(0x0300) + record()
    source += """
    await_close:
        bsr periodic
        nop
        load 0x600,r5
        cmp/eq r5,r1
        bt await_close
    """ + record()
    source += """
        tst r1,r1
        bt busy
        bra complete
        nop
    busy:
    """
    source += command(0x0200) + record() + command(0x0300) + record()
    source += """
    await_ready:
        bsr periodic
        nop
        load 0x100,r5
        cmp/eq r5,r1
        bf await_ready
    """
    source += command(0x0200) + record() + read(FIFO, "l") + record()
    source += command(0x0600) + command(0x0300) + record(3)
    source += acknowledge(4) + command(0x1080, 150, 0x80, 1)
    source += f"""
        load {HIRQ},r4
    await_sector:
        mov.w @r4,r0
        tst #4,r0
        bt await_sector
    """
    source += command(0x6300, 0, 0, 1) + read(FIFO, "l") + record() + command(0x0600)
    source += f"""
    complete:
        mov.l @r12,r0
        add #1,r0
        mov.l r0,@r12
        add #52,r10
        bra cycle
        nop

    periodic:
        add #1,r9
        mov.l r9,@r8
        load {HIRQ},r4
        load 0x400,r5
    await_report:
        mov.w @r4,r0
        tst r5,r0
        bt await_report
    """
    source += acknowledge(0x400)
    source += f"""
        load {CR1},r4
        mov.w @r4,r0
        extu.w r0,r0
        load {CR1 + 12},r4
        mov.w @r4,r3
        mov r0,r1
        load 0xf00,r5
        and r5,r1
        rts
        nop

    command:
        load {CR1},r4
        mov.w r0,@r4
        add #4,r4
        mov.w r1,@r4
        add #4,r4
        mov.w r2,@r4
        add #4,r4
        mov.w r3,@r4
        load {HIRQ},r4
    await_cmok:
        mov.w @r4,r0
        tst #1,r0
        bt await_cmok
        load {CR1},r4
        mov.w @r4,r0
        extu.w r0,r0
        load {CR1 + 12},r4
        mov.w @r4,r3
        extu.w r3,r3
        rts
        nop
    """
    code = assembler.assemble(source, base=0x200)
    rom = bytearray(0x200) + code
    struct.pack_into(">II", rom, 0, 0x200, 0x06100000)
    rom.extend(bytes(-len(rom) % 16))
    lines = ["# Original reset-vector CD cover diagnostic. MIT: adjacent LICENSE.",
             "# Unspecified bytes are zero."]
    for address in range(0, len(rom), 16):
        words = struct.unpack_from(">8H", rom, address)
        if any(words):
            lines.append(f"{address:04X} " + " ".join(f"{word:04X}" for word in words))
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    fixture = HERE / "cd-cover.hex"
    if args.check:
        assert fixture.read_text() == build(), "Rebuild cd-cover.hex"
    else:
        fixture.write_text(build())
