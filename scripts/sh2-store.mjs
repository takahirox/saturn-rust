// Original MIT fixture and authored oracle shared with tests/sh2_store.rs.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function sh2Store(page) {
  const rom=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/sh2-store.hex","utf8")).split(/\r?\n/)) {
    if(!line || line.startsWith("#"))continue;
    const [at,...words]=line.trim().split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((word,i)=>rom.writeUInt16BE(word,at+i*2));
  }
  const vectors=(await readFile("tests/fixtures/sh2-store.tsv","utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
  for(const variant of ["original","operand","dependency","alignment","omitted","address"]) {
    const image=Buffer.from(rom),expected=[...vectors];
    if(variant==="operand") {
      for(let at=0x200;at<0x3000;at++) {
        if(image[at]===0xE6 && image[at+1]===3 || image.subarray(at,at+4).equals(Buffer.from([0xE0,3,0x2D,2])))image[at+1]=11;
      }
      for(const i of [1,3,5,7,11])expected[i]=11;
    } else if(variant==="dependency") {
      for(let i=0;i<16;i++)image.set([0x2C,0x92],0x4C0E+i*4);
      expected[6]=7;expected[7]=7;
    } else if(variant==="address") {
      // Forwarding data does not remove the EX dependency on an address pointer.
      for(let at=0x200;at<0x3000;at+=2) {
        if(image.subarray(at,at+4).equals(Buffer.from([0xE0,3,0x2D,2])))image.set([0x60,0xC3],at);
      }
      for(let i=0;i<16;i++)image.set([0x6C,0xD2],0x4C0C+i*4);
      expected[6]=9;
    } else if(variant==="alignment") {
      image.copyWithin(0x400E,0x400C,0x40F0);image.set([0,9],0x400C);expected[0]=7;
    } else if(variant==="omitted") {
      for(let i=0;i<16;i++)image.set([0,9],0x440E+i*4);
      expected[2]=5;expected[3]=0xFFFFFFFF;
    }
    await page.locator("#bios").setInputFiles({name:`original-sh2-store-${variant}.bin`,mimeType:"application/octet-stream",buffer:image});
    await page.waitForFunction(()=>document.getElementById("status").textContent.startsWith("Resetting"));
    const run=()=>page.evaluate(async(count)=>{
      const {emulator}=await import("./app.js");emulator.run_steps(10000);
      return {words:[0x0600EE00,0x0600EF00].map(base=>Array.from({length:count},(_,i)=>emulator.read_long(base+i*4))),
        exceptions:[emulator.exception_count(),emulator.slave_exception_count()]};
    },vectors.length);
    const actual=await run();
    assert.deepEqual(actual,{words:[expected,expected],exceptions:["0","0"]},variant);
    await page.locator("#reset").click();assert.deepEqual(await run(),actual,`${variant} cold boot`);
  }
}
