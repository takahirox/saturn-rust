// Original MIT IF/ID guest and independent stage oracle in production Wasm.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function sh2Fetch(page) {
  const rom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sh2-fetch.hex", "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v,16));
    words.forEach((word,i) => rom.writeUInt16BE(word,at+i*2));
  }
  const golden = (await readFile("tests/fixtures/sh2-fetch.tsv","utf8")).trim().split(/\r?\n/).map(v => parseInt(v,16));
  for (let control=0;control<5;control++) {
    const image = Buffer.from(rom), expected = [...golden];
    if (control === 1) {
      let found=0;
      for(let at=0x200;at<0x7000;at+=2) if(image.readUInt32BE(at)===0xE0032C02) {image[at+1]=11;found++;}
      assert.equal(found,1);
      for(const i of [2,3,6,7,9,11,13]) expected[2*i+1]=11;
    } else if(control===2 || control===3) {
      const before=control===2?0x7601:0x66C2, after=control===2?0x7602:9;
      for(let at=0x8000;at<0x9000;at+=2) if(image.readUInt16BE(at)===before) image.writeUInt16BE(after,at);
      let found=0;
      for(let at=0x200;at<0x7000;at+=4) if(image.readUInt32BE(at)===before) {image.writeUInt32BE(after,at);found++;}
      assert(found>0);
      if(control===2) for(const i of [0,1,4,5,8,10,12]) expected[2*i+1]=64;
      else {
        for(const i of [2,3,6,7,9,11,13]) expected[2*i+1]=0;
        for(const [i,ticks] of [[3,4],[7,7],[9,11],[11,11],[13,9]]) expected[2*i]=ticks;
      }
    } else if(control===4) {
      let found=0;
      for(let at=0x200;at<0x7000;at+=4) if(image.readUInt32BE(at)===0xA55A0080) {image.writeUInt32BE(0xA55A0040,at);found++;}
      assert.equal(found,2); expected[16]=9;expected[18]=11;
    }
    for(let boot=0;boot<2;boot++) {
      const result=await page.evaluate(async ({image,count})=>{
        const {emulator}=await import("./app.js");
        emulator.reset_bios(Uint8Array.from(image));
        for(let i=0;i<3;i++) emulator.run_steps(10000);
        return {
          words:[0x0600D800,0x0600D900].map(base=>Array.from({length:count},(_,i)=>emulator.read_long(base+i*4))),
          handoff:[emulator.read_long(0x0600D800+count*4),emulator.read_long(0x0600D804+count*4),emulator.read_long(0x0600D900+count*4)],
          exceptions:[emulator.exception_count(),emulator.slave_exception_count()],
        };
      },{image:[...image],count:expected.length});
      assert.deepEqual(result,{words:[expected,expected],handoff:[0x600D,0x12345679,0x12345679],exceptions:["1","0"]},`control ${control}, boot ${boot}`);
    }
  }
  console.log("SH-2 IF/ID guest passed: both CPUs, resident/cold cache, both alignments, P1/CS0 waits, computed data, omitted loads, arithmetic/wait controls, NMI handoff and repeated cold boots.");
}
