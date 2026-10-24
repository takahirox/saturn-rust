// Original MIT fixture and authored oracle shared with tests/sh2_load.rs.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function sh2Load(page) {
  const rom=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/sh2-load.hex","utf8")).split(/\r?\n/)) {
    if(!line || line.startsWith("#"))continue;
    const [at,...words]=line.trim().split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((word,i)=>rom.writeUInt16BE(word,at+i*2));
  }
  const vectors=(await readFile("tests/fixtures/sh2-load.tsv","utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
  for(const variant of ["original","operand","dependency","alignment"]) {
    const image=Buffer.from(rom),expected=[...vectors];
    if(variant==="operand") {
      const at=image.indexOf(Buffer.from([0xE0,3,0x2C,2]));assert(at>=0);image[at+1]=7;
      for(const i of [1,3,9])expected[i]=7;
      for(const i of [5,7,11])expected[i]=8;
    } else if(variant==="dependency") {
      for(let i=0;i<16;i++)image[0x480E+i*4]=0x79;
      expected[4]=5;expected[5]=3;
    } else if(variant==="alignment") {
      image.copyWithin(0x400E,0x400C,0x40F0);image.set([0,9],0x400C);expected[0]=7;
    }
    await page.locator("#bios").setInputFiles({name:`original-sh2-load-${variant}.bin`,mimeType:"application/octet-stream",buffer:image});
    await page.waitForFunction(()=>document.getElementById("status").textContent.startsWith("Resetting"));
    const run=()=>page.evaluate(async(count)=>{
      const {emulator}=await import("./app.js");emulator.run_steps(10000);
      return {words:[0x0600EC00,0x0600ED00].map(base=>Array.from({length:count},(_,i)=>emulator.read_long(base+i*4))),
        exceptions:[emulator.exception_count(),emulator.slave_exception_count()]};
    },vectors.length);
    const actual=await run();
    assert.deepEqual(actual,{words:[expected,expected],exceptions:["0","0"]},variant);
    await page.locator("#reset").click();assert.deepEqual(await run(),actual,`${variant} cold boot`);
  }
}
