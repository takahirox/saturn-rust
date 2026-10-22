// Original MIT SH-2 guest; only its instructions write the observed records.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function cdCopy(page) {
  const original = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/cd-copy.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.trim().split(/\s+/).map(word => parseInt(word, 16));
    words.forEach((word, i) => original.writeUInt16BE(word, at + i * 2));
  }
  let previous;
  for (const payload of [0x1357, 0x1357, 0x2468, 0x2468]) {
    const image = Buffer.from(original);
    image.writeUInt16BE(payload, 0x100);
    await page.locator("#bios").setInputFiles({ name: "original-cd-copy.bin", mimeType: "application/octet-stream", buffer: image });
    await page.waitForFunction(() => document.getElementById("status").textContent.startsWith("Resetting"));
    const result = await page.evaluate(async () => {
      const { emulator } = await import("./app.js");
      emulator.mount_disc(new Uint8Array(2048 * 100));
      for (let i = 0; i < 100 && emulator.read_long(0x0600A80C) !== 0x600D; i++) emulator.run_steps(10000);
      return {
        trace: Array.from({ length: 46 * 4 }, (_, i) => emulator.read_long(0x0600A000 + i * 4)),
        data: Array.from({ length: 4 }, (_, i) => emulator.read_long(0x0600A800 + i * 4)),
        exceptions: emulator.exception_count(),
      };
    });
    assert.equal(result.exceptions, "0");
    assert.deepEqual(result.data, [payload, 0x5678, payload, 0x600D]);
    const read = (command, word = 0) => result.trace[command * 4 + word];
    for (const [command, expected] of [[4,255],[7,0],[19,255],[20,0],[30,255],[32,1],[37,0],[43,1]]) {
      assert.equal(read(command) & 255, expected, `CD selector status${command}`);
    }
    for (const [command, word, expected] of [[8,3,32],[9,1,1],[12,1,1],[14,1,1],[21,3,0],
      [22,3,32],[23,1,0],[25,1,1],[33,3,32],[38,3,0],[45,1,0]]) {
      assert.equal(read(command, word), expected, `CD selector response${command}:${word}`);
    }
    assert.equal(read(17) & 0x8000, 0x8000);
    for (const command of [3,5,6,18,29,31,35,41,42]) assert.equal(read(command) & 0x8000, 0);
    if (previous) assert.deepEqual(result.trace, previous);
    previous = result.trace;
  }
}
