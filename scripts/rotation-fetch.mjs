// Original MIT guest and independent literal oracle, shared bytes with native.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function rotationFetch(page, modes = ["tile", "bitmap", "rbg1"]) {
  for (const mode of modes) {
    const original = Buffer.alloc(524288);
    for (const line of (await readFile(`tests/fixtures/rotation-fetch-${mode}.hex`, "utf8")).trim().split(/\r?\n/)) {
      if (line.startsWith("#")) continue;
      const [at, ...words] = line.split(/\s+/).map(w => parseInt(w, 16));
      words.forEach((word, i) => original.writeUInt16BE(word, at + i * 2));
    }
    const golden = (await readFile(`tests/fixtures/rotation-fetch-${mode}.tsv`, "utf8")).trim().split(/\r?\n/).map(w => parseInt(w, 16));
    for (let boot = 0; boot < 2; boot++) {
      for (const initial of [0x1111, 0x3333]) {
        for (const [threshold, write] of [[842, true], [700, true], [842, false]]) {
          const image = Buffer.from(original);
          image.writeUInt16BE(initial, 0x100);
          image.writeUInt16BE(threshold, 0x102);
          image.writeUInt16BE(Number(write), 0x104);
          const actual = await page.evaluate(async rom => {
            const { emulator, advanceFrame } = await import("./app.js");
            emulator.reset_bios(Uint8Array.from(rom));
            for (let i = 0; i < 400 && emulator.read_long(0x0600DC0C) !== 0x600D; i++) emulator.run_steps(5000);
            if (emulator.read_long(0x0600DC0C) !== 0x600D) throw new Error("Rotation image guest did not complete");
            const records = Array.from({ length: 4 }, (_, i) => emulator.read_long(0x0600DC00 + i * 4));
            advanceFrame();
            return {
              records,
              frame: Array.from(emulator.framebuffer()),
              canvas: Array.from(document.getElementById("screen").getContext("2d").getImageData(0, 0, 320, 224).data),
            };
          }, [...image]);
          const context = `${mode}, boot=${boot}, initial=${initial.toString(16)}, threshold=${threshold}, write=${write}`;
          assert(actual.records[0] >= threshold && actual.records[0] < 854, context);
          if (threshold === 842 && write) assert.equal(actual.records[0], golden[0], context);
          const value = write ? 0x2222 : initial;
          assert.deepEqual(actual.records.slice(1), [value, initial + value, golden[3]], context);
          // Literal coordinates/palette and equal-edge contract, independent of
          // core address generation. No host setup, result or pixel injection.
          const expected = [];
          for (let y = 0; y < 224; y++) for (let x = 0; x < 320; x++) {
            const changed = write && (y > 16 || y === 16 && (threshold === 700 || x !== 0));
            expected.push(...(changed ? [0, 255, 0, 255] : initial === 0x1111 ? [255, 0, 0, 255] : [0, 0, 255, 255]));
          }
          assert.deepEqual(actual.frame, expected, `Wasm ${context}`);
          assert.deepEqual(actual.canvas, expected, `Canvas ${context}`);
        }
      }
    }
  }
  console.log("Rotation image acquisition passed: RBG0 tile/bitmap and RBG1, retained first dot, source/timing/omitted-write controls, repeated cold boots and production Wasm/Canvas pixels.");
}
