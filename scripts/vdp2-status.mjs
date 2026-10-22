// Original MIT guest. ST-058-R2 §2.5; ST-097 table2.1.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function vdp2Status(page) {
  const original = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/vdp2-status.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(word => parseInt(word, 16));
    words.forEach((word, i) => original.writeUInt16BE(word, at + i * 2));
  }
  const traces = await page.evaluate(async rom => {
    const { Emulator } = await import("./pkg/saturn_rust.js");
    const m = new Emulator(), results = [];
    try {
      for (const operand of [0, 0, 0x8000, 0x8000]) {
        const image = Uint8Array.from(rom);
        image[0x100] = operand >> 8;
        m.reset_bios(image);
        for (let i = 0; i < 100 && m.read_long(0x0600E02C) !== 0x600D; i++) m.run_steps(5000);
        results.push({ words: Array.from({ length: 12 }, (_, i) => m.read_long(0x0600E000 + i * 4)), exceptions: m.exception_count() });
      }
      return results;
    } finally { m.free(); }
  }, [...original]);
  for (let i = 0; i < traces.length; i++) {
    const blank = i < 2 ? 8 : 0;
    assert.deepEqual(traces[i], { words: [0, 0, blank, 0, 8, 1, 8, 0, 3, blank, 3, 0x600D], exceptions: "0" });
  }
}
