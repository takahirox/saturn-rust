// Original MIT guest: independent SH-2 manual/power reset and BSC completion.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function sh2ResetBus(page) {
  const rom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sh2-reset-bus.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => rom.writeUInt16BE(word, at + i * 2));
  }
  for (const manual of [true, false]) {
    for (const payload of [0x12345678, 0x89ABCDEF]) {
      const image = Buffer.from(rom);
      let controls = 0, operands = 0;
      for (let at = 0; at < image.length; at += 4) {
        const word = image.readUInt32BE(at);
        if (word === 0x5A7F) {
          controls++;
          if (!manual) image.writeUInt32BE(0x5A5F, at);
        }
        if (word === 0x12345678) { operands++; image.writeUInt32BE(payload, at); }
      }
      assert.deepEqual([controls, operands], [2, 2]);
      const expected = manual ? [2, payload, 0x7C, 0, 0xFF, 0x600D] : [1, 0, 0xFC, 0, 0xDF, 0x600D];
      for (let reset = 0; reset < 2; reset++) {
        const actual = await page.evaluate(async image => {
          const {emulator} = await import("./app.js");
          emulator.reset_bios(Uint8Array.from(image));
          emulator.run_steps(10000);
          return {
            words: [0x0601E000, 0x0601E100].map(base => Array.from({length: 6}, (_, i) => emulator.read_long(base + i * 4))),
            exceptions: [emulator.exception_count(), emulator.slave_exception_count()]
          };
        }, [...image]);
        assert.deepEqual(actual, {words: [expected, expected], exceptions: ["0", "0"]});
      }
    }
  }
  console.log("SH7604 reset-bus guest passed: both CPUs, byte-wide buffered write completed on manual reset and aborted on power reset, distinct reset vectors, retained/initialized BSC and cache control, changed operands and repeated cold boots in production Wasm.");
}
