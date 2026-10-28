// Same original bytes and independently authored MC68000UM 3.5/6.3.2 records as native.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function m68kIrq(page) {
  const original = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/m68k-irq.hex", "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => original.writeUInt16BE(word, at + i * 2));
  }
  for (const [operand, omit, result] of [[7, null, 0x10003], [9, null, 0x10004],
    [7, 0x2561, 0x10003], [7, 0x2461, 0x10003]]) {
    const image = Buffer.from(original);
    image.writeUInt32BE(operand, 0x2202);
    if (omit !== null) {
      const stores = [];
      for (let at = 0; at < 0x2000; at += 2) if (image.readUInt16BE(at) === omit) stores.push(at);
      assert.equal(stores.length, omit === 0x2461 ? 2 : 1);
      image.writeUInt16BE(9, stores.at(-1));
    }
    const expected = [0x2700, 0, 0x21e, 0, operand, 0, 1,
      ...(omit === null ? [0x2700, 0, 0x322, 1, result & 65535, 0, 2] : Array(7).fill(0))];
    for (let repeat = 0; repeat < 2; repeat++) {
      const actual = await page.evaluate(async bytes => {
        const { emulator } = await import("./app.js");
        emulator.reset_bios(Uint8Array.from(bytes));
        for (let i = 0; i < 3; i++) emulator.run_steps(10000);
        return { records: Array.from({length: 14}, (_, i) => emulator.read_word(0x05a00900 + i * 2)),
          result: emulator.read_long(0x05a00944), halted: emulator.sound_halted(), exceptions: emulator.exception_count() };
      }, [...image]);
      assert.deepEqual(actual, {records: expected, result: omit === null ? 0 : result, halted: false, exceptions: "0"});
    }
  }
  console.log("68k IRQ guest passed: SH-2 clear/reassert during DIVU at I=7, two guest handler records, computed result, saved SR/PC, no-clear/no-reassert controls, changed operand and repeated cold boots in production Wasm.");
}
