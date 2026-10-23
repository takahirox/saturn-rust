// Original MIT diagnostic bytes shared with tests/rotation_reload.rs.
// Independent §6.3/6.4 oracle; guest builds all tables/bitmap and drives MMIO.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function rotationReload(page) {
  for (const parameter of [0, 1]) {
    const image = Buffer.alloc(524288);
    for (const line of (await readFile(`tests/fixtures/rotation-reload-${parameter}.hex`, "utf8")).split(/\r?\n/)) {
      if (!line || line.startsWith("#")) continue;
      const [at, ...words] = line.split(/\s+/).map(w => parseInt(w, 16));
      words.forEach((w, i) => image.writeUInt16BE(w, at + i * 2));
    }
    const records = (await readFile(`tests/fixtures/rotation-reload-${parameter}.tsv`, "utf8")).trim().split(/\r?\n/).map(w => parseInt(w, 16));
    for (const delta of [1, 1, 2]) {
      image.writeUInt32BE(delta << 16, 0x100);
      const actual = await page.evaluate(async rom => {
        const { emulator, advanceFrame } = await import("./app.js");
        emulator.reset_bios(Uint8Array.from(rom));
        for (let i = 0; i < 2000 && emulator.read_long(0x0600DD24) !== 0x600D; i++) emulator.run_steps(500);
        if (emulator.read_long(0x0600DD24) !== 0x600D) throw new Error("Rotation reload guest did not complete");
        advanceFrame();
        const canvas = document.getElementById("screen").getContext("2d");
        return {
          records: Array.from({ length: 10 }, (_, i) => emulator.read_long(0x0600DD00 + i * 4)),
          frame: Array.from(emulator.framebuffer()),
          canvas: Array.from(canvas.getImageData(0, 0, 320, 224).data),
        };
      }, [...image]);
      // Explicit line schedule authored independently of Rust/core helpers.
      const expected = [];
      const expand = n => n << 3 | n >> 2;
      for (let y = 0; y < 224; y++) {
        let sx, sy, k;
        if (y < 128) [sx, sy, k] = [10 + delta * y, 0, 2 + y];
        else if (y < 160) [sx, sy, k] = [100 + delta * (y - 128), 4, 8 + y - 128];
        else if (y < 192) [sx, sy, k] = [100 + delta * 31 + 3 * (y - 159), 4, 39 + 2 * (y - 159)];
        else if (y === 192) [sx, sy, k] = [20, 8, 16];
        else if (y === 193) [sx, sy, k] = [30, 10, 20];
        else [sx, sy, k] = [40 + 3 * (y - 194), 12 + y - 194, 24 + 2 * (y - 194)];
        for (let x = 0; x < 320; x++) {
          const column = (sx + x + k % 4) % 512;
          expected.push(expand(column % 32), expand(sy % 32), expand(Math.floor(column / 32)), 255);
        }
      }
      assert.deepEqual(actual.records, records, `parameter=${parameter} delta=${delta}`);
      assert.deepEqual(actual.frame, expected, `Wasm parameter=${parameter} delta=${delta}`);
      assert.deepEqual(actual.canvas, expected, `Canvas parameter=${parameter} delta=${delta}`);
    }
  }
}
