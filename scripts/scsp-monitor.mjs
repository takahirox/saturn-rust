// ST-077 fig.4.3 / p.89. Original guest PCM/monitor results in production Wasm.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function scspMonitor(page) {
  const original = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/scsp-monitor.hex", "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => original.writeUInt16BE(word, at + i * 2));
  }
  const vectors = (await readFile("tests/fixtures/scsp-monitor.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  const replace = (image, before, after) => {
    const needle = Buffer.from(before), at = image.indexOf(needle);
    assert(at >= 0 && image.indexOf(needle, at + 1) < 0);
    Buffer.from(after).copy(image, at);
  };
  for (const variant of ["original", "loop", "waveform", "unkeyed"]) {
    const image = Buffer.from(original), expected = [...vectors];
    if (variant === "loop") {
      replace(image, [0x33, 0xFC, 0x20, 0, 0, 0x10, 3, 0xE4], [0x33, 0xFC, 0x30, 0, 0, 0x10, 3, 0xE4]);
      replace(image, [0x33, 0xFC, 0x20, 1, 0, 0x10, 3, 0xE6], [0x33, 0xFC, 0x30, 1, 0, 0x10, 3, 0xE6]);
      expected[62] = 0x01800180; expected[63] = 0x01800000;
    }
    if (variant === "waveform") {
      replace(image, [0x33, 0xFC, 0x11, 0, 0, 1, 0x20, 0], [0x33, 0xFC, 0x09, 0, 0, 1, 0x20, 0]);
    }
    if (variant === "unkeyed") {
      replace(image, [0x33, 0xFC, 0x18, 0x21, 0, 0x10, 0, 0], [0x33, 0xFC, 0, 0x21, 0, 0x10, 0, 0]);
      expected.fill(0);
    }
    await page.locator("#bios").setInputFiles({ name: `original-scsp-monitor-${variant}.bin`, mimeType: "application/octet-stream", buffer: image });
    await page.waitForFunction(() => document.getElementById("status").textContent.startsWith("Resetting"));
    const run = () => page.evaluate(async () => {
      const { emulator } = await import("./app.js");
      for (let i = 0; i < 20; i++) emulator.run_steps(10000);
      const pcm = emulator.audio_samples(8192);
      return { records: Array.from({ length: 64 }, (_, i) => emulator.read_long(0x05A05000 + i * 4)),
        done: emulator.read_long(0x06007004), exceptions: emulator.exception_count(),
        halted: emulator.sound_halted(), instructions: emulator.sound_instruction_count(),
        tail: Array.from(pcm.slice(-128)) };
    });
    const result = await run();
    assert.equal(result.done, 0x600D); assert.equal(result.exceptions, "0");
    assert.equal(result.halted, false); assert(BigInt(result.instructions) > 3000n);
    assert.deepEqual(result.records, expected, variant);
    const left = variant === "unkeyed" ? 0 : variant === "waveform" ? 0x0900 : 0x1100;
    const right = variant === "unkeyed" ? 0 : variant === "loop" ? 0x1300 : 0x1200;
    assert.deepEqual(result.tail, Array.from({ length: 128 }, (_, i) => i % 2 ? right : left));
    await page.locator("#reset").click();
    assert.deepEqual(await run(), result, `${variant} cold reset`);
    // Deliver newly generated guest samples through the real production
    // AudioWorklet. No authored/canned PCM is supplied to the output pipeline.
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
    assert(Math.abs(stats.peak - Math.max(left, right) / 32768) < 0.00001);
  }
  console.log("SCSP slot-monitor guest passed: all 32 selectors, current PCM loop addresses, word/byte lanes, guest loop/waveform mutations, cold resets and production WebAudio.");
}
