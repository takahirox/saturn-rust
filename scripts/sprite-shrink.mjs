// Original guest bytes, independent literal samples, guest checksum and Canvas.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
export async function spriteShrink(page) {
  for (const [suffix,reds] of [["",[8,24,41,57]],["-changed",[74,90,107,123]],["-omitted",[8,24,41,57]],["-ordinary",[8,24,41,57]]]) {
    const fixture=suffix==="-ordinary"?"":suffix;
    const rom=Buffer.alloc(524288);
    for(const line of (await readFile(`tests/fixtures/sprite-shrink${fixture}.hex`,"utf8")).split(/\r?\n/)) {
      if(!line||line.startsWith("#"))continue;
      const [address,...words]=line.split(/\s+/);let at=parseInt(address,16);
      for(const word of words){rom.writeUInt16BE(parseInt(word,16),at);at+=2;}
    }
    let expected=(await readFile(`tests/fixtures/sprite-shrink${fixture}.tsv`,"utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
    if(suffix==="-ordinary") {
      let patches=0;
      for(let i=0x200;i<rom.length-4;i+=4) {
        if(rom.readUInt32BE(i)===0x1068){rom.writeUInt32BE(0x68,i);patches++;}
      }
      assert.equal(patches,7);
      const rows=[[1,3,5,7],[1,3,5,7],[1,3,5,7],[1,3,5,7],[1,3,5,7],
        [6,4,2,0],[6,4,2,0],[1,4,6],[1,4,6],[0,2,4,5,7],
        [0,1,2,2,3,4,5,5,6,7],[1,3,5,7]];
      const samples=rows.flat().map(i=>0x8000+i);
      expected=[...samples,0x1010,samples.reduce((a,b)=>a+b,0),0x600D];
    }
    const traces=await page.evaluate(async ({rom,count})=>{
      const {emulator,advanceFrame}=await import("./app.js");
      const traces=[];
      for(let repeat=0;repeat<2;repeat++) {
        emulator.reset_bios(Uint8Array.from(rom));
        for(let i=0;i<1200 && emulator.read_long(0x0600DFFC)!==0x600D;i++)emulator.run_steps(5000);
        emulator.advance_frame();advanceFrame();
        const f=emulator.framebuffer(),canvas=document.getElementById("screen").getContext("2d");
        traces.push({done:emulator.read_long(0x0600DFFC),
          words:Array.from({length:count},(_,i)=>emulator.read_long(0x0600D000+i*4)),
          pixels:Array.from(f.slice((20*320+10)*4,(20*320+15)*4)),
          canvas:Array.from(canvas.getImageData(10,20,5,1).data)});
      }
      return traces;
    },{rom:[...rom],count:expected.length});
    const pixels=[...reds,0].flatMap(red=>[red,0,0,255]);
    const trace={done:0x600D,words:expected,pixels,canvas:pixels};
    assert.deepEqual(traces,[trace,trace],`VDP1 sampling guest ${suffix||"original"}`);
  }
  console.log("VDP1 horizontal sampling passed: literal guest samples/checksums, changed texture, omitted draw, HSS-clear operands, repeated cold boots and production Canvas.");
}
