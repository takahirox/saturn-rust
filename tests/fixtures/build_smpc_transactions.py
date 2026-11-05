#!/usr/bin/env python3
"""Original MIT SMPC command/byte-lane guests. No firmware/SDK dependency.
ST-169 §1.2 fig1.3, §2.2, §§2.3–2.5; SH7604 table4.6 and §7.11.2.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_smpc import byte, issue, idle, assembler
from build_clock import read, put16
from build_hle_system import record
from build_video_transactions import encode

HERE = Path(__file__).resolve().parent


# ST-169 §2.2 pp.17–19, figs2.5–2.7: every command is later than
# VBlank-in + 300us; INTBACK and continue precede VBlank-out. NTSC 320
# phi/8 FRT: 1200 ticks nominally spans 9600 clocks; allowing one
# tick of quantization still exceeds ceil(26.8741MHz * 300us)=8063.
WINDOW_CALL = 'load command_window,r3\njsr @r3\nnop\n'
WINDOW_EXPECTED = [1, 1, 8]


def command_window():
    # DISP=1 makes TVSTAT reflect real raster blanking (ST-058 §2.5).
    # First observe active display, then the next rising VBlank edge.
    timer = ('mov.b @r4,r0\nextu.b r0,r0\nshll8 r0\n'
             'mov.b @r5,r1\nextu.b r1,r1\nor r1,r0\n')
    return ('command_window:\nload 0x25f80004,r1\n'
            'window_active:\nmov.w @r1,r0\ntst #8,r0\nbf window_active\n'
            'window_blank:\nmov.w @r1,r0\ntst #8,r0\nbt window_blank\n'
            'load 0xfffffe12,r4\nload 0xfffffe13,r5\n' + timer +
            'mov r0,r6\nload 1200,r7\nwindow_delay:\n' + timer +
            'sub r6,r0\nextu.w r0,r0\ncmp/hs r7,r0\nbf window_delay\n'
            'mov r0,r6\nmovt r0\n' + record() +
            'load 2000,r7\ncmp/hs r6,r7\nmovt r0\n' + record() +
            'load 0x25f80004,r1\nmov.w @r1,r0\nmov #8,r2\nand r2,r0\n' +
            record() + 'rts\nnop\n')


def irq_operands():
    handler = assembler.assemble('load 0x2601a100,r12\nmov.l @r12,r13\nadd #1,r13\nmov.l r13,@r12\nrte\nnop\n', base=0x18000)
    return [(0x47 * 4, 1), (0x47 * 4 + 2, 0x8000)] + [
        (0x18000+i, int.from_bytes(handler[i:i+2], 'big')) for i in range(0, len(handler), 2)]


def reset_guests():
    files = {}
    # The host resets only after a real guest COMREG store and mailbox. No
    # diagnostic fault is used as a stopping point, and no command is reissued.
    for command in [0x17, 0x16, 0x10]:
        s = 'load 0x2601a000,r8\n' + put16(0x25f80000, 0x8000) + WINDOW_CALL
        s += byte(0x20100063, 1)
        values = {0x17: [0x12, 0x34, 0x56, 0x78],
                  0x16: [0x20, 0x24, 0x4a, 0x31, 0x12, 0x34, 0x56],
                  0x10: [1, 0, 0xf0]}[command]
        for i, value in enumerate(values):
            s += byte(0x20100001 + i * 2, value)
        s += byte(0x2010001f, command)
        s += read(0x20100063, 1) + read(0x2010005f, 1)
        s += byte(0x2601a200, 0x55)
        s += 'park:\nsleep\nbra park\nnop\n' + command_window()
        extra, _ = encode(f'smpc-reset-pending-{command:02x}', s)
        files.update(extra)
    # No commands until a complete raster window (>390k clocks) has elapsed.
    # Read cancelled SF/report/SR/IRQ first, then issue a fresh status request
    # and inspect battery-backed calendar/SMEM to catch stale persistent effects.
    s = 'load 0x2601c000,r8\n' + put16(0x25f80000, 0x8000) + WINDOW_CALL
    for at in [0x20100063, 0x2010005f, 0x20100061]:
        s += read(at, 1)
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x80,r2\nand r2,r0\n' + record()
    s += byte(0x20100063, 1)
    for i, value in enumerate([1, 0, 0xf0]):
        s += byte(0x20100001 + i * 2, value)
    s += byte(0x2010001f, 0x10) + idle('observe_status')
    for i in [0, 1, 2, 3, 4, 5, 6, 7, 12, 13, 14, 15]:
        s += read(0x20100021 + i * 2, 1)
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n' + command_window()
    extra, _ = encode('smpc-reset-observer', s)
    files.update(extra)
    expected = WINDOW_EXPECTED + [0, 0, 0, 0] + [0x40, 0x20, 0, 0x61, 1, 0, 0, 0, 0, 0, 0, 0, 0x600d]
    files['smpc-reset-observer.tsv'] = ''.join(f'{v:08X}\n' for v in expected).encode()
    return files


def timed_peripherals():
    s = ('load 0x2601d000,r8\n' + put16(0x25f80000, 0x8000) +
         put16(0xfffffee0, 1) +
         'load 0x25fe00a0,r1\nload 0xbf7f,r0\nmov.l r0,@r1\nmov #0,r0\nldc r0,sr\n')
    expected = []
    # All three INTBACK forms; repeat unoptimized/optimized collection so
    # the second optimization observes the retained collection measurement.
    count = 0
    for n, (status, mode) in enumerate([(1, 0), (1, 8), (0, 8), (0, 10), (0, 8)]):
        s += WINDOW_CALL + idle(f'claim_packet{n}') + byte(0x20100063, 1)
        expected += WINDOW_EXPECTED
        for i, value in enumerate([status, mode, 0xf0]):
            s += byte(0x20100001 + i * 2, value)
        s += byte(0x2010001f, 0x10)
        if status:
            count += 1
            s += f'load 0x2601a100,r1\nload {count},r2\nstatus_irq{n}:\nmov.l @r1,r0\ncmp/eq r2,r0\nbf status_irq{n}\n'
            s += read(0x20100063, 1) + read(0x20100061, 1) + read(0x2010005f, 1)
            s += read(0x20100033, 1)  # real Japan region
            expected += [int(mode != 0), 0x60 if mode else 0x40, 0x10, 1]
            if mode:
                # Continue immediately while still inside this same blank.
                s += ('load 0x25f80004,r1\nmov.w @r1,r0\nmov #8,r2\nand r2,r0\n' + record())
                expected += [8]
                s += byte(0x20100001, 0x81)
        if mode:
            count += 1
            s += f'load 0x2601a100,r1\nload {count},r2\npacket_irq{n}:\nmov.l @r1,r0\ncmp/eq r2,r0\nbf packet_irq{n}\n'
            s += read(0x20100063, 1) + read(0x20100061, 1) + read(0x2010005f, 1)
            for i in range(8):
                s += read(0x20100021 + i * 2, 1)
            expected += [0, 0xc0, 0x10, 0xf1, 2, 0xff, 0xff, 0xf1, 2, 0xff, 0xff]
        s += read(0x2601a100) + 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x80,r2\nand r2,r0\n' + record()
        expected += [count, 0]
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n' + command_window()
    files, _ = encode('smpc-timed-peripherals', s, irq_operands())
    files['smpc-timed-peripherals.tsv'] = ''.join(f'{v:08X}\n' for v in expected + [0x600d]).encode()
    return files


def byte_audit():
    """Independent byte acceptance/result guest; also runs on inherited main.

    ST-169 §2.2's SF-before-parameters sequence, §2.5 hexadecimal months,
    BCD calendar, §2.4 SMEM/status and all nonreserved port mode fields.
    Existing clock/reset/dual-CPU guests exercise the other command effects.
    """
    s = 'load 0x2601b000,r8\n' + put16(0x25f80000, 0x8000)
    expected = []
    calendars = [
        [0x19, 0x93, 0x5c, 0x31, 0x23, 0x59, 0x59],
        [0x20, 0x00, 0x22, 0x29, 0, 0, 0],
        [0x20, 0x24, 0x4a, 0x31, 0x12, 0x34, 0x56],
        [0x20, 0x99, 0x0b, 0x30, 0x01, 0x02, 0x03],
    ]
    for n, calendar in enumerate(calendars):
        s += WINDOW_CALL
        expected += WINDOW_EXPECTED
        s += idle(f'claim_time{n}') + byte(0x20100063, 1)
        for i, value in enumerate(calendar):
            s += byte(0x20100001 + i * 2, value)
        s += byte(0x2010001f, 0x16) + idle(f'time{n}')
        s += read(0x20100063, 1) + read(0x2010005f, 1)
        expected += [0, 0x16]
        # Change data operands on each iteration; expected status comes from
        # actual RTC/SMEM publication rather than a host-authored RAM result.
        memory = [value ^ n for value in [0x12, 0x34, 0x56, 0x78]]
        s += WINDOW_CALL
        expected += WINDOW_EXPECTED
        s += byte(0x20100063, 1)
        for i, value in enumerate(memory):
            s += byte(0x20100001 + i * 2, value)
        s += byte(0x2010001f, 0x17) + idle(f'memory{n}')
        s += WINDOW_CALL
        expected += WINDOW_EXPECTED
        s += byte(0x20100063, 1)
        for i, value in enumerate([1, 0, 0xf0]):
            s += byte(0x20100001 + i * 2, value)
        s += byte(0x2010001f, 0x10) + idle(f'calendar_status{n}')
        for i in [0, 1, 2, 3, 4, 5, 6, 7, 12, 13, 14, 15]:
            s += read(0x20100021 + i * 2, 1)
        expected += [0xc0] + calendar + memory
    for port1 in [0, 1, 3]:
        for port2 in [0, 1, 3]:
            for ope in [0, 2]:
                tag = f'{port1}_{port2}_{ope}'
                s += WINDOW_CALL
                expected += WINDOW_EXPECTED
                s += idle(f'claim_{tag}') + byte(0x00100063, 1)
                for i, value in enumerate([1, port2 << 6 | port1 << 4 | 8 | ope, 0xf0]):
                    s += byte(0x00100001 + i * 2, value)
                s += byte(0x0010001f, 0x10)
                # SF remains busy awaiting continue; SR.PDE distinguishes
                # the new result from the preceding (already broken) status.
                s += ('load 0x00100061,r1\n'+f'status_{tag}:\nmov.b @r1,r0\n'
                      f'tst #32,r0\nbt status_{tag}\n')
                s += read(0x00100063, 1) + read(0x00100061, 1)
                s += byte(0x00100001, 0x41) + read(0x00100063, 1)
                expected += [1, 0x60, 0]
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    files, _ = encode('smpc-byte-audit', s + command_window())
    files['smpc-byte-audit.tsv'] = ''.join(f'{v:08X}\n' for v in expected + [0x600d]).encode()
    return files


def issuer_guests():
    """Table2.5: real SSHON/reset entry, shared SF and slave COMREG stores.

    All issuers wait for VBlank+300us. Only the host observes/reports faults;
    each prohibited guest reaches a success marker on the inherited dispatch.
    """
    files = {}
    window = 'load 0x20000,r3\njsr @r3\nnop\n'
    park = 'park:\nsleep\nbra park\nnop\n'
    for command in [None, 2, 3, 0x0e, 0x0f, 0x10]:
        name = 'smpc-issuer-' + ('allowed' if command is None else f'{command:02x}')
        # The master sets the flag before SSHON can release slave reset.
        master = ('load 0x2601e200,r1\nmov.l @r1,r0\ntst r0,r0\nbt master\n'
                  'load 0x10000,r1\njmp @r1\nnop\nmaster:\n')
        master += byte(0x2601e203, 1) + 'load 0x2601e000,r8\n'
        master += put16(0x25f80000, 0x8000) + window + issue(2) + idle('slave_started')
        master += ('load 0x2601e204,r1\nwait_slave:\nmov.l @r1,r0\ntst r0,r0\nbt wait_slave\n')
        master += window + byte(0x20100063, 1)
        for i, value in enumerate([1, 0, 0xf0]):
            master += byte(0x20100001 + i * 2, value)
        master += byte(0x2010001f, 0x10) + idle('master_status')
        for i in [0, 1, 2, 3, 4, 5, 6, 7, 12, 13, 14, 15]:
            master += read(0x20100021 + i * 2, 1)
        master += read(0x20100063, 1) + read(0x2010005f, 1)
        master += 'load 0x600d,r0\n' + record() + park
        slave = 'load 0x060fe000,r15\nload 0x2601e100,r8\n' + idle('master_finished')
        # A permitted command in every guest is a control against blanket
        # rejection of all slave MMIO; master later verifies its real SMEM.
        slave += window + byte(0x20100063, 1)
        for i, value in enumerate([0x12, 0x34, 0x56, 0x78]):
            slave += byte(0x20100001 + i * 2, value)
        slave += byte(0x2010001f, 0x17) + idle('slave_memory')
        slave += read(0x20100063, 1) + read(0x2010005f, 1)
        slave += window + byte(0x20100063, 1)
        values = ([0x20, 0x24, 0x4a, 0x31, 0x12, 0x34, 0x56]
                  if command is None else [1, 0, 0xf0])
        for i, value in enumerate(values):
            slave += byte(0x20100001 + i * 2, value)
        slave += byte(0x2010001f, 0x16 if command is None else command)
        slave += idle('slave_command') + read(0x20100063, 1) + read(0x2010005f, 1)
        slave += byte(0x2601e207, 1) + 'load 0x600d,r0\n' + record() + park
        operands = []
        for base, source in [(0x10000, slave), (0x20000, command_window())]:
            code = assembler.assemble(source, base=base)
            operands += [(base+i, int.from_bytes(code[i:i+2], 'big')) for i in range(0, len(code), 2)]
        extra, _ = encode(name, master, operands)
        extra[name + '.s'] = ('; Master/reset entry at 0x200\n' + master +
                             '; Slave entry at 0x10000\n' + slave +
                             '; Shared window routine at 0x20000\n' + command_window()).encode()
        files.update(extra)
    # Independent results: the master must receive the calendar and memory
    # actually set by slave; status-only INTBACK is then master-issued.
    files['smpc-issuer-allowed.tsv'] = ''.join(f'{v:08X}\n' for v in
        WINDOW_EXPECTED * 2 + [0xc0, 0x20, 0x24, 0x4a, 0x31, 0x12, 0x34, 0x56,
                           0x12, 0x34, 0x56, 0x78, 0, 0x10, 0x600d]).encode()
    return files


def read_path_guests():
    """ST-169 fig1.3; SH7604 §§8.4.1/9.3.8: real cache/DMAC paths.

    OREG0..7 are individually readable. Disabling width rejection lets both
    the burst and refill succeed, with no unrelated write-only/unsupported
    byte to mask that regression. All setup is guest code.
    """
    from build_scu import store
    files = {}
    park = 'park:\nsleep\nbra park\nnop\n'
    # End completed probes with an explicit write-only read. The existing
    # bounded diagnostic exposes all IREG/pending state in both backends;
    # this observation itself must have no guest-visible side effects.
    snapshot = 'load 0x2010001f,r1\nmov.b @r1,r0\n' + byte(0x2601f080, 0x55) + park
    # SF is reserved but no command is issued: state cannot change with time.
    setup = 'load 0x2601f000,r8\n' + byte(0x20100063, 1)
    for i, value in enumerate([0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde]):
        setup += byte(0x20100001 + i*2, value)
    status = ''.join(read(at, 1) for at in [0x20100063, 0x20100061, 0x2010005f])
    # Fetch through P1 so enabling cache cannot fault on an instruction refill.
    vectors = [(0, 0x2000), (2, 0x0200)]
    for name, address, ccr in [('cache', 0x00100021, 0x11),
                               ('cache-through', 0x20100021, 0x11),
                               ('cache-disabled', 0x00100021, 0x15)]:
        s = setup + status + byte(0xfffffe92, ccr)
        s += f'load {address:#x},r1\nload 0x20001000,r3\njmp @r3\nnop\n'
        probe = ('mov.b @r1,r0\nextu.b r0,r0\n' + record() +
                 byte(0xfffffe92, 0) + status +
                 'load 0x600d,r0\n' + record() + snapshot)
        code = assembler.assemble(probe, base=0x1000)
        operands = vectors + [(0x1000+i, int.from_bytes(code[i:i+2], 'big'))
                              for i in range(0, len(code), 2)]
        guest = 'smpc-read-' + name
        extra, _ = encode(guest, s, operands)
        extra[guest + '.s'] = ('; Reset entry at P1 0x20000200\n' + s +
                              '; Byte probe at P1 0x20001000\n' + probe).encode()
        files.update(extra)
    for name, source in [('dma', 0x00100020), ('dma-ram', 0x2601f200)]:
        s = setup + status
        for i, value in enumerate([0x12345678, 0x89abcdef, 0x13579bdf, 0x2468ace0]):
            s += store(0x2601f200+i*4, value) + store(0x2601f100+i*4, 0xa5a5a5a5)
        # Auto-request, burst, 16-byte units, increment destination, four longs.
        for at, value in [(0xffffff80, source), (0xffffff84, 0x2601f100),
                          (0xffffff88, 4), (0xffffff8c, 0x4e11), (0xffffffb0, 1)]:
            s += store(at, value)
        s += ('load 0xffffff8c,r1\nwait_dma:\nmov.l @r1,r0\ntst #2,r0\nbt wait_dma\n'
              'mov #0,r0\n' + record() + 'load 0x20001800,r3\njmp @r3\nnop\n')
        # Vector10 entry publishes the failure context from the actual stack.
        handler = 'mov #10,r0\n' + record()
        handler += 'mov.l @r15,r0\n' + record() + 'mov r15,r1\nadd #4,r1\nmov.l @r1,r0\n' + record()
        handler += 'load 0x20001800,r3\njmp @r3\nnop\n'
        report = ''.join(read(at) for at in [0xffffffb0, 0xffffff80, 0xffffff84,
                                            0xffffff88, 0xffffff8c])
        report += status + ''.join(read(0x2601f100+i*4) for i in range(4))
        report += 'load 0x600d,r0\n' + record() + snapshot
        operands = vectors + [(40, 0x2000), (42, 0x1400)]
        for base, code_source in [(0x1400, handler), (0x1800, report)]:
            code = assembler.assemble(code_source, base=base)
            operands += [(base+i, int.from_bytes(code[i:i+2], 'big'))
                         for i in range(0, len(code), 2)]
        guest = 'smpc-read-' + name
        extra, _ = encode(guest, s, operands)
        extra[guest + '.s'] = ('; Reset entry at P1 0x20000200\n' + s +
                              '; Vector10 at P1 0x20001400\n' + handler +
                              '; Report at P1 0x20001800\n' + report).encode()
        files.update(extra)
    return files


def build():
    s = 'load 0x2601a000,r8\n'
    s += read(0x20100063, 1) + read(0x2010005f, 1)
    for i, value in enumerate([0x12, 0x34, 0x56, 0x78]):
        s += byte(0x20100001 + i * 2, value)
    s += issue(0x17) + read(0x20100063, 1) + read(0x2010005f, 1)
    s += idle('smem') + read(0x20100063, 1) + read(0x2010005f, 1)
    s += issue(9) + idle('cd_off')
    for i, value in enumerate([1, 0, 0xf0]):
        s += byte(0x20100001 + i * 2, value)
    s += issue(0x10) + idle('status')
    for address in [0x20100063, 0x2010005f, 0x20100033, 0x20100037,
                    0x20100039, 0x2010003b, 0x2010003d, 0x2010003f]:
        s += read(address, 1)
    # Poll the real SCU pending bit before allowing the actual IRQ handler.
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x80,r2\nand r2,r0\n' + record()
    s += 'load 0xfffffee0,r1\nmov #1,r0\nmov.w r0,@r1\n'
    s += 'load 0x25fe00a0,r1\nload 0xbf7f,r0\nmov.l r0,@r1\nmov #0,r0\nldc r0,sr\n'
    s += 'load 0x2601a100,r1\nirq_done:\nmov.l @r1,r0\ntst r0,r0\nbt irq_done\n' + record()
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x80,r2\nand r2,r0\n' + record()
    s += issue(8) + idle('cd_on') + issue(0x19) + idle('enable')
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    handler = assembler.assemble('load 0x2601a100,r12\nmov.l @r12,r13\nadd #1,r13\nmov.l r13,@r12\nrte\nnop\n', base=0x1800)
    operands = [(0x47 * 4, 0), (0x47 * 4 + 2, 0x1800)]
    operands += [(0x1800+i, int.from_bytes(handler[i:i+2], 'big')) for i in range(0,len(handler),2)]
    files, _ = encode('smpc-transactions', s, operands)
    expected = [0, 0, 1, 0x17, 0, 0x17, 0, 0x10, 1, 0x40,
                0x12, 0x34, 0x56, 0x78, 0x80, 1, 0, 0x600d]
    files['smpc-transactions.tsv'] = ''.join(f'{v:08X}\n' for v in expected).encode()
    # Each original fault case boots through real reset vectors. The guest
    # publishes a sentinel only if the prohibited operation silently succeeds.
    cases = {
        'read': 'load 0x0010001f,r1\nmov.b @r1,r0\n',
        'missing-sf': byte(0x0010001f, 9),
        'invalid-sf': byte(0x20100063, 0),
        'busy': issue(9) + byte(0x0010001f, 8),
        'command': byte(0x20100063, 1) + byte(0x0010001f, 0xff),
        'parameters': byte(0x20100063, 1) + byte(0x0010001f, 0x10),
        'word': byte(0x20100063, 1) + 'load 0x2010001e,r1\nmov #9,r0\nmov.w r0,@r1\n',
        'long': 'load 0x20100000,r1\nload 0x00120034,r0\nmov.l r0,@r1\n',
        'word-read': 'load 0x20100062,r1\nmov.w @r1,r0\n',
        # Inherited byte decomposition reaches write-only COMREG at +1f,
        # falsely describing this MOV.W operand as a one-byte access there.
        'comreg-word-read': 'load 0x0010001e,r1\nmov.w @r1,r0\n',
        'long-read': 'load 0x20100060,r1\nmov.l @r1,r0\n',
    }
    for name, source in cases.items():
        source += byte(0x2601a200, 0x55) + 'park:\nsleep\nbra park\nnop\n'
        extra, _ = encode('smpc-fault-' + name, source)
        files.update(extra)
    # H §§4.8.2–3: invalid alignment still issues exception bus cycles.
    # Those cycles remain longword operands at the ST-169 byte-only boundary.
    # Stack SR would change IREGs; the vector spans readable SR/SF registers,
    # so a byte-decomposed read cannot accidentally pass on a WriteOnly fault.
    for name, source, operands in [
        ('stack', 'trapa #32\n', [(4, 0x2010), (6, 0x0009)]),
        ('vector', 'load 0x200fffe1,r0\nldc r0,vbr\ntrapa #32\n', []),
    ]:
        source += byte(0x2601a200, 0x55) + 'park:\nsleep\nbra park\nnop\n'
        extra, _ = encode('smpc-exception-' + name, source, operands)
        files.update(extra)
    files.update(byte_audit())
    files.update(timed_peripherals())
    files.update(reset_guests())
    files.update(issuer_guests())
    files.update(read_path_guests())
    return files


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('--check', action='store_true'); args = p.parse_args()
    for name, data in build().items():
        path = HERE/name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        if name.endswith('.hex'):
            print(f'{"Verified" if args.check else "Built"} {name}; text SHA-256 {hashlib.sha256(data).hexdigest()}')
