// Original MIT guest; identical ROM/media and controls to tests/cd_reports.rs.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

export async function cdReports(page) {
  const original = Buffer.alloc(524288);
  for (const line of (await readFile('tests/fixtures/cd-reports.hex', 'utf8')).split(/\r?\n/)) {
    if (!line || line.startsWith('#')) continue;
    const [address, ...words] = line.split(/\s+/).map(w => parseInt(w, 16));
    words.forEach((w, i) => original.writeUInt16BE(w, address + i * 2));
  }
  const cue = await readFile('tests/fixtures/cd-reports.cue', 'utf8');
  for (const [position, changed, omit] of [[181, false, false], [187, true, false], [181, false, true]]) {
    const image = Buffer.from(original);
    image.writeUInt16BE(position, 0x100);
    if (omit) image.writeUInt16BE(0, 0x104);
    const raw = await readFile(`tests/fixtures/cd-reports${changed ? '-changed' : ''}.bin`);
    const traces = await page.evaluate(async ({ image, raw, cue }) => {
      const { emulator } = await import('./app.js');
      const traces = [];
      for (const preview of [false, false, true, true]) {
        emulator.reset_bios(Uint8Array.from(image));
        emulator.mount_cue(cue, 'cd-reports.bin', new Uint32Array([raw.length]), Uint8Array.from(raw));
        const steps = preview ? 17 : 10000;
        let qPreviews = 0;
        for (let i = 0; i < 1500000 / steps && emulator.read_long(0x0600EFFC) !== 0x600D; i++) {
          if (preview) {
            emulator.read_word(0x05890024); // Must not release CR4.
            const value = emulator.read_long(0x05818000);
            if (value !== emulator.read_long(0x25818000)) throw new Error('Debugger consumed CD FIFO');
            if (value === 0x41010100) qPreviews++;
          }
          emulator.run_steps(steps);
        }
        if (preview && qPreviews === 0) throw new Error('No pending guest Q transfer previewed');
        traces.push({ records: Array.from({ length: 1024 }, (_, i) => emulator.read_long(0x0600E000 + i * 4)), exceptions: emulator.exception_count() });
      }
      return traces;
    }, { image: [...image], raw: [...raw], cue });
    traces.slice(1).forEach(trace => assert.deepEqual(trace, traces[0]));
    assert.equal(traces[0].exceptions, '0');
    const read = at => traces[0].records[at / 4];
    const report = slot => Array.from({ length: 4 }, (_, i) => read(slot * 16 + i * 4));
    assert.equal(read(0xFFC), 0x600D);
    assert.deepEqual(report(2), [0x0180, 0x4101, 0x0100, position]);
    assert.deepEqual(report(6), omit ? [0x0180, 0x4101, 0x0100, position] : [0x02FF, 0xFFFF, 0xFFFF, 0xFFFF]);
    const paused = omit ? position : 150;
    assert.deepEqual(report(10), [0x0180, 0x4101, 0x0100, paused]);
    assert.deepEqual(report(13), [0x06FF, 0xFFFF, 0xFFFF, 0xFFFF]);
    assert.deepEqual(report(18), [0x09FF, 0xFFFF, 0xFFFF, 0xFFFF]);
    assert.deepEqual(report(22), [0x0180, 0x4101, 0x0100, 173]);
    const initial = [0x4101, 0x0100, position - 150, 0, position];
    const invalid = [0xFFFF, 0xFFFF, 0xFFFF, 0x00FF, 0xFFFF];
    [initial, omit ? initial : invalid, [0x4101, 0x0100, paused - 150, 0, paused], invalid, invalid, [0x4101, 0x0100, 23, 0, 173]].forEach((words, q) => {
      assert.deepEqual(Array.from({ length: 5 }, (_, i) => read(0x800 + q * 32 + i * 4)), words);
      assert.equal(read(0x814 + q * 32), words.reduce((a, b) => a + b, 0));
    });
    assert.equal(read(0xFE0), changed ? 0xA6A6 : 0x6969);
    assert.equal(report(28)[1], 1);
  }
  console.log('CD reports passed: guest commands/home/error recovery/Q transfers and sums, changed FAD/media, omitted stop, passive Machine/Wasm debugger reads and repeated cold boots.');
}
