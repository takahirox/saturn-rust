// Original MIT interrupt guest; literal oracle is H Rev4 table5.8/fig5.7.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
export async function interruptEntry(page) {
  const rom=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/interrupt-entry.hex","utf8")).trim().split(/\r?\n/)) {
    if(line.startsWith("#")) continue;
    const [at,...words]=line.split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((w,i)=>rom.writeUInt16BE(w,at+i*2));
  }
  const patch=(image,before,after,width)=>{
    let found=0;
    for(let at=0x200;at<0x7000;at+=2) {
      if((width===2?image.readUInt16BE(at):image.readUInt32BE(at))!==before)continue;
      if(width===2)image.writeUInt16BE(after,at);else image.writeUInt32BE(after,at);
      found++;
    }
    assert.equal(found,1,`unique patch ${before.toString(16)}`);
  };
  for(let control=0;control<5;control++) {
    const image=Buffer.from(rom);
    if(control===1) {patch(image,0xE020,0xE030,2);patch(image,0x0000E607,0x0000E60B,4);patch(image,0x00000600,0x00000A00,4);}
    if(control===2) patch(image,0x13579BDF,0x2468ACE0,4);
    if(control===3) patch(image,0xE0082100,0xE0002100,4);
    if(control===4) patch(image,0x00000101,0x00000100,4);
    for(let boot=0;boot<2;boot++) {
      const got=await page.evaluate(async image=>{
        const {emulator}=await import("./app.js");emulator.reset_bios(Uint8Array.from(image));
        emulator.run_steps(10000);
        return {words:[0x0600D000,0x0600D100].map(base=>Array.from({length:7},(_,i)=>emulator.read_long(base+i*4))),
          data:emulator.read_long(0x0600D204),exceptions:[emulator.exception_count(),emulator.slave_exception_count()]};
      },[...image]);
      const record=[control===1?51:35,0x50,0xC000050C,control===1?0xA0:0x60,control===1?11:7,0x600D];
      const expected=control===3?{words:[Array(7).fill(0),Array(7).fill(0)],data:0,exceptions:["0","0"]}:
        control===4?{words:[[...record,0],Array(7).fill(0)],data:0,exceptions:["1","0"]}:
        {words:[[...record,0x4B],[...record,0]],data:control===2?0x2468ACE0:0x13579BDF,exceptions:["2","1"]};
      assert.deepEqual(got,expected,`interrupt control ${control}, boot ${boot}`);
    }
  }
  console.log("Interrupt guest passed: literal FRT timestamps, saved PC/SR, masks, RTE, both CPUs, SCU vector acquisition, timing/source controls, omitted triggers and repeated cold boots.");
}
