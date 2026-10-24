// Original MIT SH-2 bytes, identical to native. Independent §2.1/6.3 oracle.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function rotationHighResolution(page) {
  for (const mode of ["tile", "bitmap", "rbg1", "coefficient"]) {
    const original = Buffer.alloc(524288);
    const text = await readFile(`tests/fixtures/rotation-hi-${mode}.hex`, "utf8");
    for (const line of text.trim().split(/\r?\n/)) {
      if (line.startsWith("#")) continue;
      const [at, ...words] = line.split(/\s+/).map(w => parseInt(w, 16));
      words.forEach((word, i) => original.writeUInt16BE(word, at + i * 2));
    }
    const golden = (await readFile(`tests/fixtures/rotation-hi-${mode}.tsv`, "utf8")).trim().split(/\r?\n/).map(w => parseInt(w, 16));
    for (let boot = 0; boot < 2; boot++) for (const tv of [0x8002, 0x8003]) {
      for (const [seed, shift, enabled, parameter] of [[1, 0, true, 0], [17, 7, true, 1], [1, 0, false, 0]]) {
        const image = Buffer.from(original);
        const enable = enabled ? mode === "rbg1" ? 0x30 : 0x10 : 0;
        for (const [at, value] of [[0x100, tv], [0x102, seed], [0x104, shift], [0x106, enable], [0x108, mode === "rbg1" ? 0 : parameter]]) image.writeUInt16BE(value, at);
        const actual = await page.evaluate(async rom => {
          const { emulator, advanceFrame } = await import("./app.js");
          emulator.reset_bios(Uint8Array.from(rom));
          for (let i = 0; i < 300 && emulator.read_long(0x0600DE1C) !== 0x600D; i++) emulator.run_steps(10000);
          if (emulator.read_long(0x0600DE1C) !== 0x600D) throw new Error("High-resolution rotation guest did not complete");
          const records = Array.from({ length: 8 }, (_, i) => emulator.read_long(0x0600DE00 + i * 4));
          advanceFrame();
          const canvas = document.getElementById("screen");
          return { records, width: canvas.width, height: canvas.height,
            frame: Array.from(emulator.framebuffer()),
            canvas: Array.from(canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data) };
        }, [...image]);
        const context = `${mode}, boot=${boot}, tv=${tv.toString(16)}, seed=${seed}, shift=${shift}, enabled=${enabled}`;
        const base = mode === "bitmap" || mode === "coefficient" ? 0x8000 : 0;
        assert.deepEqual(actual.records, [512 * (base + seed) + 130816, base + seed, base + 511 + seed, shift * 65536, tv, enable, mode === "coefficient" ? 65536 : 0, golden[7]], context);
        if (tv === 0x8002 && seed === 1 && enabled) assert.deepEqual(actual.records, golden, context);
        const width = tv === 0x8002 ? 640 : 704;
        assert.equal(actual.width, width, context);
        assert.equal(actual.height, 224, context);
        const expected = [];
        const expand = c => ((c & 31) << 3) | ((c & 31) >> 2);
        for (let y = 0; y < 224; y++) for (let x = 0; x < width; x++) {
          const color = enabled ? Math.floor(x / 2) + seed + shift : 0;
          expected.push(expand(color), expand(color >> 5), expand(color >> 10), 255);
        }
        assert.deepEqual(actual.frame, expected, `Wasm ${context}`);
        assert.deepEqual(actual.canvas, expected, `Canvas ${context}`);
      }
    }
  }
  console.log("High-resolution rotation passed: 640/704 full images, RBG0 A/B tile/bitmap, fixed-bank RBG1, horizontal coefficients, guest records, operand/enable controls, cold boots and production Wasm/Canvas.");
}
