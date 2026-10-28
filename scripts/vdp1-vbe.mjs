// Original MIT guest executed through rebuilt production Wasm and Canvas.
// Literal ST-013 ch.4/§4.4 expectations, independent of emulator execution.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function vdp1Vbe(page) {
  const original=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/vdp1-vbe.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [address,...words]=line.split(/\s+/);
    words.forEach((word,i)=>original.writeUInt16BE(parseInt(word,16),parseInt(address,16)+i*2));
  }
  for (const [a,b,left,right,enable] of [[0x801F,0x83E0,0x0402,0x0803,8],
    [0xFC00,0xFFFF,0x0202,0x0603,8],[0x801F,0x83E0,0x0402,0x0803,0]]) {
    const image=Buffer.from(original);
    [a,b,left,right,enable].forEach((v,i)=>image.writeUInt16BE(v,0x100+i*2));
    const empty=[240*0x8000,...Array(8).fill(0x8000)];
    const first=!enable?empty:[208*0x8000+32*a,0x8000,a,a,0x8000,a,0x8000,0x8000,0x8000];
    const second=!enable?empty:left===0x0402?
      [208*0x8000+32*b,0x8000,0x8000,b,b,b,b,0x8000,0x8000]:
      [208*0x8000+32*b,0x8000,0x8000,b,0x8000,b,0x8000,0x8000,0x8000];
    const expected=[...first,...second,...first,0x1010,0,0,0,5,5];
    const rows=Array.from({length:6},(_,y)=>Array.from({length:40},(_,x)=>{
      const inside=enable && x>=(left===0x0402?16:8) && x<(left===0x0402?32:24) && y>=2 && y<4;
      return !inside?[0,0,0,255]:b===0x83E0?[0,255,0,255]:[255,255,255,255];
    }).flat());
    for(let repeat=0;repeat<2;repeat++) {
      const result=await page.evaluate(async image=>{
        const {emulator,advanceFrame}=await import("./app.js");
        emulator.set_pal(false);emulator.reset_bios(Uint8Array.from(image));
        for(let i=0;i<100 && emulator.read_long(0x0600E888)!==0x600D;i++)emulator.run_steps(5000);
        const records=Array.from({length:35},(_,i)=>emulator.read_long(0x0600E800+i*4));
        const exceptions=emulator.exception_count();
        advanceFrame();
        const f=emulator.framebuffer(),ctx=document.getElementById("screen").getContext("2d");
        return {records,exceptions,
          rows:Array.from({length:6},(_,y)=>Array.from(f.slice(y*320*4,y*320*4+160))),
          canvas:Array.from({length:6},(_,y)=>Array.from(ctx.getImageData(0,y,40,1).data))};
      },[...image]);
      assert.deepEqual(result.records.slice(0,33),expected);
      assert(result.records[33]>=8200 && result.records[33]<8450,`FRT blank interval ${result.records[33]}`);
      assert.equal(result.records[34],0x600D);
      assert.equal(result.exceptions,"10"); // five real IN and five OUT exception entries
      assert.deepEqual(result.rows,rows);assert.deepEqual(result.canvas,rows);
    }
  }
  console.log("VDP1 sampled VBE passed: real SCU raster IRQ/RTE, FRT, guest checksums of both buffers, legal disable, changed operands, omitted enable, repeated cold boots and production Canvas.");
}
