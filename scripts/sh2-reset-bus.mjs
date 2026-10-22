// Original MIT guest: independent SH-2 manual/power reset and BSC completion.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function sh2ResetBus(page) {
  const rom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sh2-reset-bus.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => rom.writeUInt16BE(word, at + i * 2));
  }
  for (const manual of [true, false]) {
    for (const payload of [0x12345678, 0x89ABCDEF]) {
      const image = Buffer.from(rom);
      let controls = 0, operands = 0;
      for (let at = 0; at < image.length; at += 4) {
        const word = image.readUInt32BE(at);
        if (word === 0x5A7F) {
          controls++;
          if (!manual) image.writeUInt32BE(0x5A5F, at);
        }
        if (word === 0x12345678) { operands++; image.writeUInt32BE(payload, at); }
      }
      assert.deepEqual([controls, operands], [2, 2]);
      const expected = manual ? [2, payload, 0x7C, 0, 0xFF, 0x600D] : [1, 0, 0xFC, 0, 0xDF, 0x600D];
      for (let reset = 0; reset < 2; reset++) {
        const actual = await page.evaluate(async image => {
          const {emulator} = await import("./app.js");
          emulator.reset_bios(Uint8Array.from(image));
          emulator.run_steps(10000);
          return {
            words: [0x0601E000, 0x0601E100].map(base => Array.from({length: 6}, (_, i) => emulator.read_long(base + i * 4))),
            exceptions: [emulator.exception_count(), emulator.slave_exception_count()]
          };
        }, [...image]);
        assert.deepEqual(actual, {words: [expected, expected], exceptions: ["0", "0"]});
      }
    }
  }
  console.log("SH7604 reset-bus guest passed: both CPUs, byte-wide buffered write completed on manual reset and aborted on power reset, distinct reset vectors, retained/initialized BSC and cache control, changed operands and repeated cold boots in production Wasm.");

  const vectorRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sh2-vector-error.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => vectorRom.writeUInt16BE(word, at + i * 2));
  }
  for (const stack of [0x060FF000, 0x060FD000]) {
    const image = Buffer.from(vectorRom);
    image.writeUInt32BE(stack, 4);
    const traces = await page.evaluate(async image => {
      const { Emulator } = await import("./pkg/saturn_rust.js");
      const m = new Emulator();
      try {
        const traces = [];
        for (let reset = 0; reset < 2; reset++) {
          m.reset_bios(Uint8Array.from(image));
          assertReset();
          m.step();
          if (Number(m.peripheral_clocks()) < 16 || m.registers()[15] !== new DataView(Uint8Array.from(image).buffer).getUint32(4))
            throw new Error("Both reset-vector bus cycles must finish before the first instruction");
          m.run_steps(3000);
          traces.push({words: [0x0601F000, 0x0601F100].map(base => Array.from({length: 9}, (_, i) => m.read_long(base + i * 4))),
            exceptions: [m.exception_count(), m.slave_exception_count()], sp: [m.registers()[15], m.slave_registers()[15]]});
        }
        // A new boot selection cancels unsampled PC/SP, without publishing an
        // instruction or leaking an old vector completion into the new mode.
        m.reset_bios(Uint8Array.from(image));
        assertReset();
        m.reset_hle();
        m.step();
        if (m.registers()[0] !== 5 || m.special_registers()[0] !== 0x06000002 || m.instruction_count() !== "1")
          throw new Error("Canceled reset vector polluted a new boot selection");
        return traces;
        function assertReset() {
          if (!m.status().startsWith("Resetting") || m.special_registers()[0] !== 0 || m.registers()[15] !== 0 ||
              m.instruction_count() !== "0" || m.peripheral_clocks() !== "0")
            throw new Error("Cold reset published vectors or instructions before bus completion");
        }
      } finally { m.free(); }
    }, [...image]);
    for (const trace of traces) {
      assert.deepEqual(trace.exceptions, ["2", "2"]);
      assert.deepEqual(trace.sp, [stack, 0x060FE000]);
      trace.words.forEach((words, i) => {
        assert.deepEqual(words.slice(0, 4), [i ? 0x4101 : 0x4001, trace.sp[i] - 16, 0x8000, 0x51]);
        assert.equal(words[4], words[5]); // original TRAPA frame resumes at guest label
        assert.deepEqual(words.slice(6), [0x51, 0x600D, 0]);
      });
    }
  }
  console.log("SH-2 reset and exception guests passed in Wasm: unpublished cold vectors, completed vector waits, canceled startup, dual-CPU unaligned VBR repair and original TRAPA/RTE frames, changed reset SP and repeat boots.");

  const overrun = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sh2-overrun.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => overrun.writeUInt16BE(word, at + i * 2));
  }
  for (const native of [false, true]) for (const value of [42, 7]) {
    const image = Buffer.from(overrun);
    image.writeUInt16BE(0xEC00 | value, 0x5002);
    let controls = 0;
    for (let at = 0; at < image.length; at += 4) if (image.readUInt32BE(at) === 0x1000) {
      controls++;
      if (!native) image.writeUInt32BE(0, at);
    }
    assert.equal(controls, 2);
    const traces = await page.evaluate(async image => {
      const { Emulator } = await import("./pkg/saturn_rust.js");
      const m = new Emulator();
      try {
        return Array.from({length: 2}, () => {
          m.reset_bios(Uint8Array.from(image)); m.run_steps(3000);
          return {words: [0x0601F200, 0x0601F300].map(base => [0,4,8,0x20,0x24].map(offset => m.read_long(base+offset))),
            exceptions: [m.exception_count(), m.slave_exception_count()]};
        });
      } finally { m.free(); }
    }, [...image]);
    const words = [native ? 0 : 0x5002, 0, 0, value, 0x600D];
    assert.deepEqual(traces, Array(2).fill({words: [words, words], exceptions: Array(2).fill(native ? "0" : "1")}));
  }
  console.log("UBC overrun-fetch guests passed in Wasm: both CPUs, SH7000 compatible/native mode control, unused IF word, pre-execution saved PC, no compatible match flags, changed opcode and cold reset.");
}
