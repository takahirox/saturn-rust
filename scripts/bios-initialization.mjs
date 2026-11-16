// Original MIT reset-vector SH-2/SCU-DSP/68000 firmware, no proprietary input.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function biosInitialization(page) {
  const image = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/bios-initialization.hex", "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => image.writeUInt16BE(word, at + i * 2));
  }
  const expected = (await readFile("tests/fixtures/bios-initialization.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  const run = async (bytes, chunk, resetPending = false) => page.evaluate(async ({bytes, chunk, resetPending, count}) => {
    const {emulator: m, advanceFrame} = await import("./app.js");
    (await m.clear_media());
    (await m.reset_bios(Uint8Array.from(bytes)));
    if (resetPending) {
      // Cold cancellation during active status acquisition, before the deferred
      // control takes effect. The guest's subsequent run must start from reset.
      for (let i = 0; i < 400000; i++) {
        (await m.run_steps(1));
        // The firmware writes its marker immediately after the queued store;
        // stop during status collection, without injecting a result.
        if ((await m.read_long(0x06020ff4)) === 1) break;
      }
      if ((await m.read_long(0x06020ff4)) !== 1) throw Error("pending status marker missing");
      (await m.reset_bios(Uint8Array.from(bytes)));
    }
    // Queue an ordered batch without a transport round trip after every
    // instruction. Each native run still has its original passive CR4 and
    // register reads before the next run; no guest operation is combined.
    for (let i = 0; i < 400000;) {
      const pending = [];
      for (let batch = 0; batch < 128 && i < 400000; batch++, i += chunk) {
        pending.push(m.run_steps(Math.min(chunk, 400000-i)));
        pending.push(m.read_word(0x25890024), m.registers());
      }
      await Promise.all(pending);
    }
    (await advanceFrame()); (await advanceFrame());
    const pixel = [...document.getElementById("screen").getContext("2d").getImageData(0,0,1,1).data];
    return {pixel, records: (await Promise.all(Array.from({length: count}, async (_, i) => (await m.read_long(0x06020000+i*4))))),
      marker: (await m.read_long(0x06020ffc)), sound: (await m.read_long(0x05a00804)),
      exceptions: (await m.exception_count()), status: (await m.status()), mode: (await m.mode()),
      clocks: (await m.peripheral_clocks())};
  }, {bytes: [...bytes], chunk, resetPending, count: expected.length});
  let reference;
  for (const chunk of [1, 127, 10000]) for (let boot = 0; boot < 2; boot++) {
    const got = await run(image, chunk, boot === 1);
    assert.deepEqual(got.records, expected);
    assert.equal(got.marker, 0x600d);
    assert.deepEqual(got.pixel, [165,16,0,255]);
    assert.equal(got.sound, 0x600d);
    assert.equal(got.exceptions, "0");
    assert.equal(got.mode, "User-provided BIOS");
    assert.match(got.status, /Sleeping/);
    reference ??= got;
    assert.deepEqual(got, reference);
  }
  const changed = Buffer.from(image); changed[0x103] = 9;
  const mutation = await run(changed, 127);
  const changedExpected = [...expected]; changedExpected[24] = 108;
  assert.deepEqual(mutation.records, changedExpected);
  assert.equal(mutation.marker, 0x600d);
  assert.deepEqual(mutation.pixel, [99,24,0,255]);
  const omitted = Buffer.from(image); omitted[0x104] = 7;
  const negative = await run(omitted, 10000);
  assert.equal(negative.marker, 0);
  assert.equal(negative.sound, 0);
  assert.equal(negative.exceptions, "0");
  console.log("BIOS initialization: real shared MMIO, DSP arithmetic, DMA data/IRQ, 68000 acknowledgement, WAIT/ESEL/CMOK/periodic NODISC, mutations, cancellation and chunk parity passed.");
}
