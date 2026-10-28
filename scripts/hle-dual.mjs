// Original MIT HLE disc and independent literal records, shared with native.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function hleDual(page) {
  const original = await readFile('tests/fixtures/hle-dual.iso');
  const golden = (await readFile('tests/fixtures/hle-dual.tsv', 'utf8')).trim().split(/\r?\n/)
    .map(line => { const [a, v, name] = line.split('\t'); return [parseInt(a, 16), parseInt(v, 16), name]; });
  const input = (image, offset, value) => image.writeUInt32BE(value, 21 * 2048 + 0x7000 + offset);
  const raster = (r, h, v) => {
    assert.deepEqual([r[0], r[1], r[8], r[9], r[16], r[17]], [2, h + 14, 2, v + 22, h + v + 36, 0x600D]);
    assert.equal(r[2], r[4]); assert.deepEqual([r[3] & 4, r[5] & 4], [4, 4]);
    assert.equal(r[10], r[12]); assert(r[10] >= 224 && r[10] < 263);
    assert.deepEqual([r[11] & 8, r[13] & 8], [8, 8]);
  };
  const run = async (image, { sysres = false, retain = false, short = false } = {}) => page.evaluate(async ({ bytes, addresses, sysres, retain, short }) => {
    const { Emulator } = await import('./pkg/saturn_rust.js');
    const m = new Emulator(), traces = [];
    try {
      m.import_backup_ram(new Uint8Array(32768).fill(0xA5));
      for (let repeat = 0; repeat < 2; repeat++) {
        m.reset_disc(Uint8Array.from(bytes));
        const tables = [], stacks = [];
        if (retain) {
          for (let epoch = 0; epoch < 4; epoch++) {
            const entry = epoch === 0 ? 0x06008000 : 0x06008040;
            const deadline = Number(m.peripheral_clocks()) + 4000000;
            while (m.slave_pc() !== entry && Number(m.peripheral_clocks()) < deadline) m.step();
            if (m.slave_pc() !== entry) throw new Error(`Missing slave entry ${epoch}`);
            tables.push(Array.from({ length: 128 }, (_, i) => m.read_long(0x06000400 + i * 4)));
            stacks.push(m.slave_registers()[15]); m.step();
          }
        }
        const deadline = short ? 700000 : sysres ? 22000000 : 8000000;
        while (m.read_long(0x0600E904) !== 0x600D && Number(m.peripheral_clocks()) < deadline) {
          m.run_steps(10000); m.read_word(0x25F80004); // Passive inspection only.
        }
        traces.push({ records: addresses.map(a => m.read_long(a)),
          raster: Array.from({ length: 18 }, (_, i) => m.read_long(0x0600E800 + i * 4)),
          power: m.read_long(0x0600E900), done: m.read_long(0x0600E904),
          exceptions: [m.exception_count(), m.slave_exception_count()], tables, stacks,
          backup: Array.from(m.export_backup_ram()).every(v => v === 0xA5) });
      }
      return traces;
    } finally { m.free(); }
  }, { bytes: [...image], addresses: golden.map(([a]) => a), sysres, retain, short });

  for (const stack of [0, 0x060FD000]) {
    const image = Buffer.from(original); image.writeUInt32BE(stack, 0xEC);
    for (const result of await run(image, { retain: true })) {
      const expected = golden.map(([a, v]) => stack && a >= 0x0600D000 && a < 0x0600D800 && (a & 0x100)
        && [0x20, 0x50].includes(a & 0xFF) ? stack - ((a & 0xFF) === 0x50 ? 8 : 0) : v);
      assert.deepEqual(result.records, expected);
      raster(result.raster, 3, 5);
      assert.deepEqual(result.stacks, Array(4).fill(stack || 0x06001000));
      for (const table of result.tables) assert.deepEqual(table, result.tables[0], 'retained vector table');
      assert.deepEqual(result.exceptions, ['7', '0']);
      assert.equal(result.power, 0); assert.equal(result.done, 0x600D); assert(result.backup);
    }
  }
  const changed = Buffer.from(original);
  for (const [off, value] of [[0, 9], [4, 17], [8, 23], [0x34, 23], [0x38, 41], [0x40, 0]]) input(changed, off, value);
  const changes = new Map([[0x0600E000, 69], [0x0600E004, 89], [0x0600E008, 86], [0x0600E010, 86], [0x0600E018, 86]]);
  for (let epoch = 0; epoch < 4; epoch++) {
    const base = 0x0600D000 + epoch * 0x200;
    changes.set(base + 0x28, 9); changes.set(base + 0x128, epoch === 0 ? 17 : 34);
    changes.set(base + 0x60, epoch === 0 ? 89 : 86); changes.set(base + 0x15C, epoch === 0 ? 54 : 64);
  }
  for (const result of await run(changed)) {
    assert.deepEqual(result.records, golden.map(([a, v]) => changes.get(a) ?? v));
    raster(result.raster, 23, 41); assert.equal(result.done, 0x600D);
  }
  const omitted = Buffer.from(original); input(omitted, 12, 0);
  for (const result of await run(omitted, { short: true })) {
    for (const a of [0x0600D040, 0x0600D140, 0x0600D064, 0x0600E000, 0x0600E004])
      assert.equal(result.records[golden.findIndex(([address]) => address === a)], 0);
    assert.deepEqual(result.exceptions, ['0', '0']); assert.equal(result.done, 0);
  }
  for (const mask of [0x10, 0xF0]) {
    const image = Buffer.from(original); image.writeUInt32BE(0x06006000, 0xE28); input(image, 0x3C, mask);
    for (const result of await run(image, { short: true })) {
      if (mask === 0x10) { raster(result.raster, 3, 5); assert.equal(result.done, 0x600D); }
      else { assert.deepEqual([result.raster[0], result.raster[8], result.raster[17]], [0, 0, 0]); assert.equal(result.done, 0); }
      assert.deepEqual(result.exceptions, ['0', '0']);
    }
  }
  const reset = Buffer.from(original); input(reset, 16, 1);
  for (const result of await run(reset, { sysres: true })) {
    assert.deepEqual(result.records, golden.map(([, v]) => v)); raster(result.raster, 3, 5);
    assert.equal(result.power, 0x12345678); assert.equal(result.done, 0x600D); assert(result.backup);
  }
  console.log('HLE dual CPU passed: literal startup, bidirectional capture/SLEEP/frame/ack/RTE, retained vectors, both PLL restarts, raster/SR controls, changed operands, omitted strobes, SYSRES and repeated boots.');
}
