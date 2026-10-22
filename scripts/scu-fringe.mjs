// Original MIT guest. ST-097 §2.1 fig.2.1; SH7604 §6.3.4.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function scuFringe(page) {
  const bytes = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/scu-fringe.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => bytes.writeUInt16BE(word, at+i*2));
  }
  const golden = (await readFile("tests/fixtures/scu-fringe.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v,16));
  for (const mutate of [false, false, true, true]) {
    const image = Buffer.from(bytes), expected = [...golden];
    if (mutate) {
      const needle = Buffer.from([0x12,0x34,0x56,0x78]), offsets = [];
      for (let at=image.indexOf(needle); at>=0; at=image.indexOf(needle,at+1)) offsets.push(at);
      assert.equal(offsets.length,4);
      for (const at of offsets) image[at+3] = 0x5A;
      for (let i=3; i<1200; i+=5) {
        if ((expected[i]&255) === 0x78) expected[i] = (expected[i]&0xFFFFFF00 | 0x5A) >>> 0;
      }
    }
    const actual = await page.evaluate(async ({image,count}) => {
      const { emulator } = await import("./app.js");
      emulator.reset_bios(Uint8Array.from(image));
      for (let i=0; i<8; i++) emulator.run_steps(10000);
      return {records: Array.from({length:count}, (_,i) => emulator.read_long(0x0600D000+i*4)),
        exceptions: emulator.exception_count(), sp: emulator.registers()[15], status: emulator.status()};
    }, {image:[...image],count:expected.length});
    assert.deepEqual(actual.records, expected);
    assert.equal(actual.exceptions,"480");
    assert.equal(actual.sp,0x060FF000);
    assert.equal(actual.status,"Sleeping — waiting for an interrupt");
  }
  console.log("SCU fringe guest passed: byte-cycle UBC comparisons, direct/indirect DMA on all channels, 480 real handlers, source mutations and cold resets through production Wasm.");
}
