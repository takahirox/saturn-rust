// Original MIT guest, H §7 and P TAS.B. Same production Wasm as the player.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
export async function busTransactions(page) {
  const bytes = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/bus-transactions.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => bytes.writeUInt16BE(word, at + i * 2));
  }
  for (const increments of [32, 32, 17]) {
    const image = Buffer.from(bytes), at = image.indexOf(Buffer.from([0xE6, 32]));
    assert(at >= 0 && image.indexOf(Buffer.from([0xE6, 32]), at + 1) < 0);
    image[at + 1] = increments;
    const actual = await page.evaluate(async image => {
      const { emulator } = await import("./app.js");
      emulator.reset_bios(Uint8Array.from(image));
      emulator.run_steps(10000);
      return {
        records: [0x0600ED00, 0x0600ED10].map(base => [emulator.read_long(base), emulator.read_long(base + 4), emulator.read_long(base + 8), emulator.read_long(base + 12)]),
        counter: emulator.read_long(0x0600F004),
        lock: emulator.read_long(0x0600F000),
        exceptions: [emulator.exception_count(), emulator.slave_exception_count()],
        dma: [emulator.read_long(0x0600ED30), ...Array.from({length:3}, (_,i) => emulator.read_long(0x0600ED20+i*4)),
          ...Array.from({length:6}, (_,i) => emulator.read_long(0x0600ED40+i*4))]
      };
    }, [...image]);
    assert.deepEqual(actual, {records: [[increments, 0x600D, 6, 0], [increments, 0x600D, 6, 0]], counter: increments * 2, lock: 0, exceptions: ["3", "0"], dma: [0x600D, 1, 1, 1, 0x11223344, 0xAABBCCDD, 0x11223344, 0xAABBCCDD, 0x11223344, 0xAABBCCDD]});
  }
  console.log("Bus transaction guest passed: independent SH-2s, TAS mutual exclusion across delayed data phases, six SDRAM MRS encodings, unaligned MRS exemption, all three SCU DMA channels with IRQs and physical RAM/sound/VRAM routes, changed loop operand and cold reset through production Wasm.");
}
