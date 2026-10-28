// Original MIT SH-2 media, identical to native; Sega VDP2 §§2.2/4.11.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";

export async function mosaic(page) {
  for (const mode of ["normal", "rotation", "rbg1"]) {
    const original = Buffer.alloc(524288);
    for (const line of (await readFile(`tests/fixtures/mosaic-${mode}.hex`, "utf8")).trim().split(/\r?\n/)) {
      if (line.startsWith("#")) continue;
      const [at, ...words] = line.split(/\s+/).map(w => parseInt(w, 16));
      words.forEach((word, i) => original.writeUInt16BE(word, at + i * 2));
    }
    assert.equal(createHash("sha256").update(original).digest("hex"), (await readFile(`tests/fixtures/mosaic-${mode}.sha256`, "utf8")).trim());
    const bit = mode === "rotation" ? 0x10 : 1;
    for (let boot = 0; boot < 2; boot++) for (const tv of [0x8080, 0x80c0]) {
      for (const [seed, mz, parameter] of [[1, bit, 0], [9, bit, 0], [1, 0x3300 | bit, 0], [9, 0x3300 | bit, mode === "rotation" ? 1 : 0], [1, 0x3300, 0]]) {
        const image = Buffer.from(original);
        for (const [at, value] of [[0x100, tv], [0x102, seed], [0x104, mz], [0x106, parameter]]) image.writeUInt16BE(value, at);
        const actual = await page.evaluate(async rom => {
          const { emulator, advanceFrame } = await import("./app.js");
          emulator.reset_bios(Uint8Array.from(rom));
          for (let i = 0; i < 300 && emulator.read_long(0x0600dc1c) !== 0x600d; i++) emulator.run_steps(10000);
          if (emulator.read_long(0x0600dc1c) !== 0x600d) throw new Error("Mosaic guest did not complete");
          const records = Array.from({ length: 8 }, (_, i) => emulator.read_long(0x0600dc00 + i * 4));
          const base64 = bytes => {
            let binary = "";
            for (let i = 0; i < bytes.length; i += 16384) binary += String.fromCharCode(...bytes.subarray(i, i + 16384));
            return btoa(binary);
          };
          advanceFrame(); advanceFrame();
          const frames = [];
          const canvas = document.getElementById("screen");
          for (let i = 0; i < 2; i++) {
            advanceFrame();
            frames.push({ frame: base64(emulator.framebuffer()), canvas: base64(canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data) });
          }
          return { records, width: canvas.width, height: canvas.height, frames };
        }, [...image]);
        const context = `${mode}, boot=${boot}, tv=${tv.toString(16)}, seed=${seed}, mz=${mz.toString(16)}, parameter=${parameter}`;
        const checksum = mode === "normal" ? 3072 * seed + 329728 : mode === "rbg1" ? 4096 * seed + 450560 : 2048 * seed + 67334144;
        assert.deepEqual(actual.records.slice(0, 5), [checksum, tv, mz, mode === "normal" ? 3 : mode === "rbg1" ? 0x30 : 0x10, parameter], context);
        assert.notEqual(actual.records[5] & 2, actual.records[6] & 2, `guest observed both fields: ${context}`);
        assert.equal(actual.records[7], 0x600d, context);
        const height = tv === 0x80c0 ? 448 : 224;
        assert.equal(actual.width, 320, context); assert.equal(actual.height, height, context);
        const expected = Buffer.alloc(320 * height * 4);
        const expand = n => ((n & 31) << 3) | ((n & 31) >> 2);
        for (let y = 0; y < height; y++) for (let x = 0; x < 320; x++) {
          let sx = x, sy = y;
          if (mz & bit) {
            const h = (mz >> 8 & 15) + 1, v = (mz >> 12) + 1;
            sx = Math.floor(x / h) * h;
            if (tv === 0x80c0) sy = Math.floor(y / 2);
            if (mode === "normal") sy = Math.floor(sy / v) * v;
          }
          if (mode !== "rbg1") sy %= 256;
          const companionY = y % 256;
          let color;
          if (mode === "normal" && !(sx < 16 && sy < 64)) color = x < 32 && companionY < 64 ? 3 * companionY + x + seed : 0;
          else color = sx < 32 && sy < 64 ? 3 * sy + sx + seed : 0;
          expected.set([expand(color), expand(color >> 5), 0, 255], (y * 320 + x) * 4);
        }
        for (const field of actual.frames) {
          assert.deepEqual(Buffer.from(field.frame, "base64"), expected, `Wasm ${context}`);
          assert.deepEqual(Buffer.from(field.canvas, "base64"), expected, `Canvas ${context}`);
        }
      }
    }
  }
  console.log("Mosaic passed: original guest records, both fields, independent normal/rotation source rows, A/B and RBG1, mixed density, changed source/size, omitted enable, cold boots, production Wasm and literal Canvas pixels.");
}
