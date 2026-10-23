// Original MIT SH-2/SCU DSP guest; identical ROM and literal records as native.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function dspArithmetic(page) {
  const rom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/dsp-arithmetic.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((v, i) => rom.writeUInt16BE(v, at + i * 2));
  }
  const baseline = (await readFile("tests/fixtures/dsp-arithmetic.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  for (const variant of [0, 1, 2, 3]) {
    const image = Buffer.from(rom), expected = [...baseline];
    if (variant === 1) {
      image.writeUInt32BE(0x7ffffffe, 0x1f000);
      expected.splice(2, 5, 0, 0, 0, 0x7fffffff, 0x7fff);
      expected.splice(7, 5, 0x480000, 0x480000, 0x480000, 0x80000000, 0x8000);
    } else if (variant === 2) {
      image.writeUInt32BE(0x80000000, 0x1f000);
      expected.splice(2, 5, 0x400000, 0x400000, 0x400000, 0x80000001, 0xffff8000);
      expected.splice(7, 5, 0x400000, 0x400000, 0x400000, 0x80000002, 0xffff8000);
    } else if (variant === 3) {
      const needle = Buffer.from([0x10, 0x04, 0x32, 0x09]), positions = [];
      for (let at = image.indexOf(needle); at >= 0; at = image.indexOf(needle, at + 1)) positions.push(at);
      assert.equal(positions.length, 4);
      image.writeUInt32BE(0x00043209, positions[0]);
      expected.splice(2, 5, 0, 0, 0, 0x7fffffff, 0x7fff);
    }
    const traces = await page.evaluate(async ({ image, length }) => {
      const { Emulator } = await import("./pkg/saturn_rust.js");
      const m = new Emulator();
      try {
        return Array.from({ length: 2 }, () => {
          m.reset_bios(Uint8Array.from(image));
          const reset = [m.read_long(0x05fe0080), m.read_long(0x05fe00a4) & 0x20];
          // Resume after an actual bounded pause with guest memory inspection.
          m.run_steps(17);
          const paused = m.read_long(0x0600ec00);
          if (m.read_long(0x0600ec00) !== paused) throw new Error("Paused record changed");
          m.run_steps(9983);
          const words = Array.from({ length }, (_, i) => m.read_long(0x0600ec00 + i * 4));
          m.run_steps(1000);
          const settled = Array.from({ length }, (_, i) => m.read_long(0x0600ec00 + i * 4));
          return { reset, words, settled, exceptions: m.exception_count(), last: m.last_exception(), status: m.status() };
        });
      } finally { m.free(); }
    }, { image: [...image], length: expected.length });
    for (const trace of traces) {
      assert.deepEqual(trace.reset, [0, 0]);
      assert.deepEqual(trace.words, expected, `DSP arithmetic variant ${variant}`);
      assert.deepEqual(trace.settled, expected);
      assert.equal(trace.exceptions, "2");
      assert.match(trace.last, /^Vector 69 · saved PC 0x[0-9A-F]{8}$/);
      assert.equal(trace.status, "Sleeping — waiting for an interrupt");
    }
  }
  console.log("SCU DSP arithmetic guest passed: ADD/SUB/48-bit AD2 overflow replacement, repeated halted reads, parallel results, two end interrupts/RTE acknowledgements, operand mutations, omitted-ADD control and repeated cold boots in production Wasm.");
}
