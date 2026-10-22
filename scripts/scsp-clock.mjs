// Original ST-077 table2.2/§3.2/§5 fig5.1 guest; adjacent fixture MIT license.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function scspClock(page) {
  const original = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/scsp-clock.hex", "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((v, i) => original.writeUInt16BE(v, at + i * 2));
  }
  const operand = parseInt((await readFile("tests/fixtures/scsp-clock.tsv", "utf8")).trim(), 16);
  for (const [value, late] of [[0x2345, 0x11A2], [0x3456, 0x1A2B], [0x1234, 0x91A]]) {
    const image = Buffer.from(original);
    image.writeUInt16BE(value, operand);
    for (let repeat = 0; repeat < 2; repeat++) {
      const result = await page.evaluate(async ({ bytes, late }) => {
        const { emulator } = await import("./app.js");
        emulator.reset_bios(Uint8Array.from(bytes));
        for (let i = 0; i < 10; i++) emulator.run_steps(10000);
        const audio = emulator.audio_samples(8192);
        let stereo = false;
        for (let i = 0; i + 1 < audio.length; i += 2) {
          stereo ||= audio[i] === late * 2 && audio[i + 1] === late * 2;
        }
        return { records: [emulator.read_long(0x0600EA00), emulator.read_long(0x0600EA04)],
          input: emulator.read_long(0x05A01000) >>> 16, stereo, halted: emulator.sound_halted(),
          exceptions: emulator.exception_count() };
      }, { bytes: [...image], late });
      assert.deepEqual(result, { records: [0x091A0000 + late, 0x600D], input: value,
        stereo: true, halted: false, exceptions: "0" });
    }
    const stats = await page.evaluate(async () => {
      const { emulator } = await import("./app.js");
      const { AudioOutput } = await import("./audio.js");
      const output = new AudioOutput();
      try {
        await output.enable(); output.setPaused(false);
        emulator.run_steps(10000);
        output.deliver(emulator.audio_samples(8192));
        for (let i = 0; i < 200; i++) {
          if (output.stats.rendered > 128 && output.stats.state === "underflow") break;
          await new Promise(resolve => setTimeout(resolve, 25));
        }
        return { ...output.stats };
      } finally { await output.context?.close(); }
    });
    assert.equal(stats.state, "underflow"); assert(stats.rendered > 128);
    assert.equal(stats.dropped, 0);
    assert(Math.abs(stats.peak - late * 2 / 32768) < 0.00001);
  }
  console.log("SCSP instruction clocks passed: real 68k uploads DSP program, observes early effect, changes sound RAM before late read; authored arithmetic/stereo, operand/control changes, cold resets and production AudioWorklet delivery in Wasm.");
}

// ST-228 p.31: only the last STR to an EFREG is valid. Original licensed
// 68k-uploaded DSP program computes the samples and all guest records.
export async function scspEffects(page) {
  const original = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/scsp-effects.hex", "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((v, i) => original.writeUInt16BE(v, at + i * 2));
  }
  const operands = (await readFile("tests/fixtures/scsp-effects.tsv", "utf8")).trim().split(/\s+/).map(v => parseInt(v, 16));
  for (const [positive, negative, finalLeft, right] of [
    [0x4000, 0xE000, -4096, 8192], [0x2400, 0xA000, -12288, 4608], [0x6000, 0xC000, -8192, 12288],
  ]) {
    for (const omitFinal of [false, true]) {
      const image = Buffer.from(original);
      image.writeUInt16BE(positive, operands[0]);
      image.writeUInt16BE(negative, operands[1]);
      assert.equal(image.readUInt16BE(operands[2]), 0x1002);
      if (omitFinal) image.writeUInt16BE(2, operands[2]);
      const left = omitFinal ? right : finalLeft;
      for (let repeat = 0; repeat < 2; repeat++) {
        const result = await page.evaluate(async ({ bytes, left, right }) => {
          const { emulator } = await import("./app.js");
          emulator.reset_bios(Uint8Array.from(bytes));
          for (let i = 0; i < 10; i++) emulator.run_steps(10000);
          emulator.audio_samples(8192);
          emulator.run_steps(10000);
          const audio = emulator.audio_samples(8192);
          return { records: [0, 4, 8].map(i => emulator.read_long(0x0600EB00 + i)),
            stereo: audio.length >= 128 && audio.every((v, i) => v === (i % 2 ? right : left)),
            halted: emulator.sound_halted(), exceptions: emulator.exception_count() };
        }, { bytes: [...image], left, right });
        assert.deepEqual(result, { records: [((left & 65535) * 65536 + right) >>> 0, 0, 0x600D],
          stereo: true, halted: false, exceptions: "0" });
      }
      const stats = await page.evaluate(async () => {
        const { emulator } = await import("./app.js");
        const { AudioOutput } = await import("./audio.js");
        const output = new AudioOutput();
        try {
          await output.enable(); output.setPaused(false);
          emulator.run_steps(10000);
          output.deliver(emulator.audio_samples(8192));
          for (let i = 0; i < 200; i++) {
            if (output.stats.rendered > 128 && output.stats.state === "underflow") break;
            await new Promise(resolve => setTimeout(resolve, 25));
          }
          return { ...output.stats };
        } finally { await output.context?.close(); }
      });
      assert.equal(stats.state, "underflow"); assert(stats.rendered > 128);
      assert.equal(stats.dropped, 0);
      assert(Math.abs(stats.peak - Math.max(Math.abs(left), right) / 32768) < 0.00001);
    }
  }
  console.log("SCSP effect stores passed: real 68k/DSP replace positive, negative and zero values; three operand pairs, omitted-store control, cold resets, exact stereo and production AudioWorklet delivery.");
}
