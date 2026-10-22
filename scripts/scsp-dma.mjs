// Original ST-077 §3.2/fig4.66/tables4.39–40 guest; fixture MIT license.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function scspDma(page) {
  const original = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/scsp-dma.hex", "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((v, i) => original.writeUInt16BE(v, at + i * 2));
  }
  for (const value of [0x1230, 0x2340, 0x3450]) {
    const image = Buffer.from(original);
    image.writeUInt16BE(value, 0x6000);
    let previous;
    for (let repeat = 0; repeat < 2; repeat++) {
      const result = await page.evaluate(async ({ bytes, value }) => {
        const { emulator } = await import("./app.js");
        emulator.reset_bios(Uint8Array.from(bytes));
        for (let i = 0; i < 10; i++) emulator.run_steps(10000);
        const samples = emulator.audio_samples(8192);
        let positive = false, negative = false, valid = true;
        for (let i = 0; i + 1 < samples.length; i += 2) {
          positive ||= samples[i] === value && samples[i + 1] === value;
          negative ||= samples[i] === -value && samples[i + 1] === -value;
          valid &&= samples[i] === samples[i + 1] && [0, value, -value].includes(samples[i]);
        }
        return { records: Array.from({ length: 6 }, (_, i) => emulator.read_long(0x0600EC00 + i * 4)),
          positive, negative, valid, halted: emulator.sound_halted(), exceptions: emulator.exception_count() };
      }, { bytes: [...image], value });
      assert(result.records[0] >= 32 && result.records[0] < 100, `DMA FRT interval ${result.records[0]}`);
      assert.equal(result.records[1], value); assert.equal(result.records[2], 0x1428);
      assert.equal(result.records[3] & ~0x410, 0);
      assert.deepEqual(result.records.slice(4), [value, 0x600D]);
      assert(result.positive && result.negative && result.valid);
      assert.equal(result.halted, false); assert.equal(result.exceptions, "0");
      if (previous) assert.deepEqual(result, previous);
      previous = result;
    }
    const stats = await page.evaluate(async () => {
      const { emulator } = await import("./app.js");
      const { AudioOutput } = await import("./audio.js");
      const output = new AudioOutput();
      try {
        await output.enable(); output.setPaused(false);
        emulator.run_steps(10000);
        output.deliver(emulator.audio_samples(8192));
        for (let i = 0; i < 200; i++) {
          if (output.stats.rendered > 128 && output.stats.state === "underflow") break;
          await new Promise(resolve => setTimeout(resolve, 25));
        }
        return { ...output.stats };
      } finally { await output.context?.close(); }
    });
    assert.equal(stats.state, "underflow"); assert(stats.rendered > 128);
    assert.equal(stats.dropped, 0);
    assert(Math.abs(stats.peak - value / 32768) < 0.00001);
  }
  const control = Buffer.from(original);
  const start = control.indexOf(Buffer.from([0x26, 0x71]));
  assert(start >= 0 && control.indexOf(Buffer.from([0x26, 0x71]), start + 1) === -1);
  control.writeUInt16BE(9, start);
  const absent = await page.evaluate(async bytes => {
    const { emulator } = await import("./app.js");
    emulator.reset_bios(Uint8Array.from(bytes));
    for (let i = 0; i < 10; i++) emulator.run_steps(10000);
    return { done: emulator.read_long(0x0600EC14),
      coefficient: emulator.read_long(0x05B00700) >>> 16,
      silent: emulator.audio_samples(8192).every(v => v === 0),
      halted: emulator.sound_halted(), exceptions: emulator.exception_count() };
  }, [...control]);
  assert.deepEqual(absent, { done: 0, coefficient: 0, silent: true, halted: false, exceptions: "0" });
  console.log("SCSP shared-memory DMA passed: SH-2 FRT times 64 real words, actual 68k computes bipolar stereo from a transferred coefficient; mutations, cold resets and production AudioWorklet peak/underflow/drop assertions in Wasm.");
}
