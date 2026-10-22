// Original sound-reset guest, MC68000UM §§5.5/6.3.1 and ST-169 SNDON/SNDOFF.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

async function fixture(name) {
  const image = Buffer.alloc(524288);
  for (const line of (await readFile(`tests/fixtures/${name}.hex`, "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => image.writeUInt16BE(word, at + i * 2));
  }
  return image;
}
export async function m68kReset(page) {
  const original = await fixture("m68k-reset");
  const vectors = (await readFile("tests/fixtures/m68k-reset.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  for (const changed of [false, true]) {
    const image = Buffer.from(original), expected = [...vectors];
    if (changed) {
      const needle = Buffer.from([0x26, 0x3C, 0x13, 0x57, 0, 0x13]), at = image.indexOf(needle);
      assert(at >= 0 && image.indexOf(needle, at + 1) < 0);
      image.writeUInt32BE(0x13579BDF, at + 2);
      expected[3] = 0x13579BDF;
    }
    await page.locator("#bios").setInputFiles({ name: `original-68k-reset-${changed}.bin`, mimeType: "application/octet-stream", buffer: image });
    await page.waitForFunction(() => document.getElementById("status").textContent.startsWith("Resetting"));
    const run = () => page.evaluate(async () => {
      const { emulator } = await import("./app.js");
      for (let i = 0; i < 10; i++) emulator.run_steps(10000);
      return { records: Array.from({ length: 19 }, (_, i) => emulator.read_long(0x0600EB00 + i * 4)),
        halted: emulator.sound_halted(), exceptions: emulator.exception_count() };
    });
    const result = await run();
    assert.deepEqual(result.records, expected);
    assert.equal(result.halted, false); assert.equal(result.exceptions, "0");
    await page.locator("#reset").click();
    assert.deepEqual(await run(), result, "68k reset guest repeated cold reset");
  }
  const phases = await fixture("m68k-reset-phases");
  for (const operand of [0x13579BDF, 0x2468ACE0, null]) {
    const image = Buffer.from(phases);
    if (operand === null) {
      const needle = Buffer.from([0x25, 0x62]), at = image.indexOf(needle);
      assert(at >= 0 && image.indexOf(needle, at + 1) < 0);
      image.writeUInt16BE(9, at); // omit the guest vector write
    } else image.writeUInt32BE(operand, 0x2202);
    for (let repeat = 0; repeat < 2; repeat++) {
      const result = await page.evaluate(async bytes => {
        const { emulator } = await import("./app.js");
        emulator.reset_bios(Uint8Array.from(bytes));
        for (let i = 0; i < 10; i++) emulator.run_steps(10000);
        return { record: emulator.read_long(0x0600ED00), halted: emulator.sound_halted(),
          exceptions: emulator.exception_count() };
      }, [...image]);
      assert.deepEqual(result, { record: operand ?? 0xBAD, halted: false, exceptions: "0" });
    }
  }
  const traced = await fixture("m68k-traced-reset");
  for (const operand of [0x13579BDF, 0x2468ACE0]) {
    const image = Buffer.from(traced);
    image.writeUInt32BE(operand, 0x2202);
    for (let repeat = 0; repeat < 2; repeat++) {
      const result = await page.evaluate(async bytes => {
        const { emulator } = await import("./app.js");
        emulator.reset_bios(Uint8Array.from(bytes));
        for (let i = 0; i < 10; i++) emulator.run_steps(10000);
        return { records: Array.from({length: 4}, (_, i) => emulator.read_long(0x0600EE00 + i * 4)),
          halted: emulator.sound_halted(), exceptions: emulator.exception_count() };
      }, [...image]);
      assert.deepEqual(result, { records: [0xA700, 0x20C, operand, 0x600D], halted: false, exceptions: "0" });
    }
  }
  console.log("68k reset guest passed: actual SH-2 SNDON/SNDOFF, new reset SSP/PC, retained D0-D7/A0-A6/USP, control state, changed pre-reset operand and cold resets through production Wasm.");
  console.log("68k reset bus phases passed: SH-2 changes the sound PC vector after SNDON returns; the independent 68k runs the changed entry, including changed arithmetic operand and repeated cold resets through production Wasm.");
  console.log("Traced 68k RESET passed: pulse completion precedes the guest trace handler; saved SR/PC and retained operand records, changed operand and cold resets through production Wasm.");
}
