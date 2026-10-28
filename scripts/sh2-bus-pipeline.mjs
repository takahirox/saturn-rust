// Original MIT SH7604 instruction and write-buffer guest, production Wasm.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function sh2BusPipeline(page) {
  const rom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sh2-bus-pipeline.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => rom.writeUInt16BE(word, at + i * 2));
  }
  const golden = (await readFile("tests/fixtures/sh2-bus-pipeline.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  for (const [width, changed] of [1, 2, 3].flatMap(width => [[width, false], [width, false], [width, true]])) {
    const image = Buffer.from(rom), expected = [...golden];
    let widths = 0;
    for (let at = 0; at < image.length; at += 4) {
      if (image.readUInt32BE(at) === 0xA55A00FC) { image.writeUInt32BE((0xA55A003C | width << 6) >>> 0, at); widths++; }
    }
    assert.equal(widths, 2);
    if (changed) {
      const changes = [0, 0];
      for (let at = 0; at < image.length - 4; at += 2) {
        if (image.readUInt32BE(at) === 0x0000E62A) { image[at + 3] = 0x3D; changes[0]++; }
        if (image.readUInt32BE(at) === 0xE0312900) { image[at + 1] = 0x52; changes[1]++; }
      }
      assert.deepEqual(changes, [2, 2]);
      expected[1] = 61;
      expected[6] = 0x52520000;
    }
    const actual = await page.evaluate(async ({image, count}) => {
      const {emulator} = await import("./app.js");
      emulator.reset_bios(Uint8Array.from(image));
      emulator.run_steps(10000);
      return {
        words: [0x0601D000, 0x0601D100].map(base => Array.from({length: count}, (_, i) => emulator.read_long(base + i * 4))),
        exceptions: [emulator.exception_count(), emulator.slave_exception_count()]
      };
    }, {image: [...image], count: expected.length});
    assert.deepEqual(actual, {words: [expected, expected], exceptions: ["0", "0"]});
  }
  console.log("SH7604 fetch/buffer guest passed: both SH-2s, uncached retained self-modifying instructions and branch refetch, FRT-timed peripheral overlap at all three bus widths, pipelined internal-source DMA, changed operands and repeated cold boots in production Wasm.");
}
