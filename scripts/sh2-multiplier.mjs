// Original MIT guest; same bytes and primary-derived oracle as the native test.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function sh2Multiplier(page) {
  const rom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sh2-multiplier.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.trim().split(/\s+/).map(v => parseInt(v,16));
    words.forEach((word,i) => rom.writeUInt16BE(word,at+i*2));
  }
  const vectors = (await readFile("tests/fixtures/sh2-multiplier.tsv","utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
  for (const variant of ["original", "fast", "operand", "spacing", "omitted"]) {
    const image = Buffer.from(rom), expected = [...vectors], start = 0x400C;
    if (variant === "fast") {
      const at=image.indexOf(Buffer.from([0xEB,0,0x2B,0xB8]));
      assert(at>=0);
      image[at+1]=1;
    } else if (variant === "operand") {
      for(let i=0;i<16;i++)image[start+i*4+1]=0x2F;
      expected[1]=49;
    } else if (variant === "spacing") {
      image.copyWithin(start+128,start+64,start+74);
      for(let i=0;i<16;i++)image.set([0x22,0x1F,0,9,0,9,0x06,0x1A],start+i*8);
      expected[0]=11;
    } else if (variant === "omitted") {
      for(let i=0;i<16;i++)image.set([0,9],start+i*4);
      expected[0]=7;expected[1]=0;
    }
    await page.locator("#bios").setInputFiles({name:`original-sh2-multiplier-${variant}.bin`,mimeType:"application/octet-stream",buffer:image});
    await page.waitForFunction(()=>document.getElementById("status").textContent.startsWith("Resetting"));
    const run=()=>page.evaluate(async(count)=>{
      const {emulator}=await import("./app.js");
      for(let i=0;i<30;i++)emulator.run_steps(10000);
      return {words:[0x0600E900,0x0600EB00].map(base=>Array.from({length:count},(_,i)=>emulator.read_long(base+i*4))),
        exceptions:[emulator.exception_count(),emulator.slave_exception_count()]};
    },vectors.length);
    const actual=await run();
    assert.deepEqual(actual,{words:[expected,expected],exceptions:[variant==="fast"?"1":"0","0"]},variant);
    await page.locator("#reset").click();
    assert.deepEqual(await run(),actual,`${variant} cold boot`);
  }
}
