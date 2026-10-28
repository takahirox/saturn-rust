// Original guest bytes also run in tests/video_transactions.rs. Hosts only load,
// advance and inspect: literal RGB555, palette and priority oracles below.
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
async function rom(name) {
  const bytes=Buffer.alloc(524288);
  for(const line of (await readFile(`tests/fixtures/${name}.hex`,"utf8")).split(/\r?\n/)) {
    if(!line || line.startsWith("#"))continue;
    const [at,...words]=line.split(/\s+/);
    words.forEach((v,i)=>bytes.writeUInt16BE(parseInt(v,16),parseInt(at,16)+i*2));
  }
  return bytes;
}
export async function vdp1Hdtv(page) {
  const original=await rom("vdp1-hdtv");
  for(const [mode,width] of [[0x8004,320],[0x8005,352],[0x8006,640],[0x8007,704]]) {
    for(const [color,x,draw] of [[0x801F,0,1],[0x83E0,8,1],[0x801F,0,0]]) {
      const bytes=Buffer.from(original);
      [color,x,draw,mode].forEach((v,i)=>bytes.writeUInt16BE(v,0x100+i*2));
      const texture=[color,0x83E0,0,1,0x4001,0xFFFF,0xFC1F,0xFFE0,
        0x83FF,0xFC00,1,0,0x4001,0x801F,0xFFFF,0x83E0];
      const words=texture.map(v=>draw&&x===0?v:0);
      const expected=[...words,words.reduce((a,b)=>a+b,0),...(draw?[0xFFE0,0x83E0,0xFFE0,0x83E0]:[0,0,0,0]),
        0xABCD,0x1014,draw?2:0,draw?16:0,draw?0x2000:0,0x600D];
      const scale=width>=640?2:1;
      const samples=[0,1,2,3,4,476,477,478,479].flatMap(y=>
        [0,1,2,3,4,5,6,7,8,9,15,16,311,312,313,318,319,320,343,344,350,351]
          .filter(sx=>sx<width/scale).flatMap(sx=>Array.from({length:scale},(_,dx)=>[sx*scale+dx,y])));
      const rows=[
        [color===0x801F?[255,0,0,255]:[0,255,0,255],[0,255,0,255],[0,0,255,255],[255,0,255,255],
          [0,0,255,255],[255,255,255,255],[255,0,255,255],[0,255,255,255]],
        [[255,255,0,255],[0,0,255,255],[255,0,255,255],[0,0,255,255],[0,0,255,255],
          [255,0,0,255],[255,255,255,255],[0,255,0,255]]];
      const rgba=samples.map(([px,y])=>{
        const sx=Math.floor(px/scale);
        if(draw && y<4 && sx>=x && sx<x+8)return rows[Math.floor(y/2)][sx-x];
        if(draw && y>=476 && (sx>=312&&sx<320 || sx>=344&&sx<352))return rows[Math.floor((y-476)/2)][sx%8];
        return [0,0,255,255];
      });
      for(let repeat=0;repeat<2;repeat++) {
        const result=await page.evaluate(async ({bytes,samples})=>{
          const {emulator,advanceFrame}=await import("./app.js");
          emulator.set_pal(false);emulator.reset_bios(Uint8Array.from(bytes));
          for(let i=0;i<100&&emulator.read_long(0x0600EA68)!==0x600D;i++)emulator.run_steps(5000);
          const records=Array.from({length:27},(_,i)=>emulator.read_long(0x0600EA00+i*4));
          advanceFrame();
          const f=emulator.framebuffer(),width=emulator.video_width(),ctx=document.getElementById("screen").getContext("2d");
          return {records,width,height:emulator.video_height(),exceptions:emulator.exception_count(),
            pixels:samples.map(([x,y])=>Array.from(f.slice((y*width+x)*4,(y*width+x)*4+4))),
            canvas:samples.map(([x,y])=>Array.from(ctx.getImageData(x,y,1,1).data))};
        },{bytes:[...bytes],samples});
        assert.deepEqual(result.records,expected);assert.equal(result.exceptions,"0");
        assert.equal(result.width,width);assert.equal(result.height,480);
        assert.deepEqual(result.pixels,rgba);assert.deepEqual(result.canvas,rgba);
      }
    }
  }
  const erase=await rom("vdp1-hdtv-erase");
  for(const mode of [0x8004,0x8005,0x8006,0x8007]) {
    const bytes=Buffer.from(erase);bytes.writeUInt16BE(mode,0x100);
    const expected=[...(mode&1?[0x1234,0x1234,0x1234,0x1234,0xFFFF,0xFFFF]:
      [0x1234,0x1234,0xFFFF,0xFFFF,0xFFFF,0xFFFF]),0x1014,0,0,0,0xFFFF,0x600D];
    for(let repeat=0;repeat<2;repeat++) {
      const result=await page.evaluate(async bytes=>{
        const {emulator}=await import("./app.js");
        emulator.set_pal(false);emulator.reset_bios(Uint8Array.from(bytes));
        for(let i=0;i<100&&emulator.read_long(0x0600EB2C)!==0x600D;i++)emulator.run_steps(5000);
        return {records:Array.from({length:12},(_,i)=>emulator.read_long(0x0600EB00+i*4)),exceptions:emulator.exception_count()};
      },[...bytes]);
      assert.deepEqual(result.records,expected);assert.equal(result.exceptions,"0");
    }
  }
  console.log("VDP1 HDTV passed: all four exclusive outputs, guest commands/status/IRQ/checksum, RGB/palette/transparency/priority, adjacent colors/rows and last source edges, changed color/coordinate, omitted draw, cold boots, Canvas and VBE=0 automatic erase boundaries 29340/53136.");
}
