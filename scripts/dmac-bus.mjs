// Original MIT SH-2/DMAC guest. Execute the production player Wasm.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function dmacBus(page) {
  const original = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/dmac-bus.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(word => parseInt(word, 16));
    words.forEach((word, i) => original.writeUInt16BE(word, at + i * 2));
  }
  const golden = (await readFile("tests/fixtures/dmac-bus.tsv", "utf8")).trim().split(/\r?\n/).map(word => parseInt(word, 16));
  for (const changed of [false, false, true]) {
    const image = Buffer.from(original);
    if (changed) {
      let replacements = 0;
      for (let at = 0; at < image.length; at += 4) {
        if (image.readUInt32BE(at) === 0x11223344) {
          image.writeUInt32BE(0x91223344, at);
          replacements++;
        }
      }
      assert.equal(replacements, 2);
    }
    const actual = await page.evaluate(async image => {
      const { emulator } = await import("./app.js");
      emulator.reset_bios(Uint8Array.from(image));
      emulator.run_steps(10000);
      return {
        records: [0x0601C000, 0x0601C100].map(base => Array.from({ length: 12 }, (_, i) => emulator.read_long(base + i * 4))),
        exceptions: [emulator.exception_count(), emulator.slave_exception_count()]
      };
    }, [...image]);
    const expected = golden.map(word => changed ? new Map([
      [0x11223344, 0x91223344], [0x44332211, 0x44332291], [0x33441122, 0x33449122]
    ]).get(word) ?? word : word);
    assert.deepEqual(actual, { records: [expected, expected], exceptions: ["5", "5"] });
  }
  console.log("DMAC bus guest passed: both executing SH-2s, five real per-CPU IRQs, every transfer width, short overlapping 16-byte unit, byte/word decrement, cycle-steal/burst, changed source operands and repeated cold reset in production Wasm.");
}
