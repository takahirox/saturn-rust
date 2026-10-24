// Original MIT mixed-disc guest and independent data/palette expectations.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';

export async function dmaStartup(page) {
  const original=await readFile('tests/fixtures/dma-startup.iso');
  const audio=await readFile('tests/fixtures/dma-startup-audio.bin');
  const cue=await readFile('tests/fixtures/dma-startup.cue','utf8');
  for(const [source,cpu,trigger] of [[0x22223333,0x11111111,true],[0x33334444,0x44444444,true],[0x22223333,0x11111111,false]]) {
    const iso=Buffer.from(original);
    iso.writeUInt32BE(source,60*2048);
    iso.writeUInt32BE(cpu,21*2048+0x1800);
    iso.writeUInt32BE(Number(trigger),21*2048+0x1804);
    const media=Buffer.concat([iso,audio]);
    for(let repeat=0;repeat<2;repeat++) {
      const r=await page.evaluate(async ({cue,media,lengths})=>{
        const {emulator,advanceFrame}=await import('./app.js');
        emulator.set_pal(false);
        emulator.reset_cue(cue,'dma-startup.iso\0dma-startup-audio.bin',Uint32Array.from(lengths),Uint8Array.from(media));
        for(let i=0;i<300&&emulator.read_long(0x06010014)!==0x600D;i++) emulator.run_steps(10000);
        const records=Array.from({length:6},(_,i)=>emulator.read_long(0x06010000+i*4));
        const exceptions=emulator.exception_count();
        advanceFrame();advanceFrame();
        const frame=Array.from(emulator.framebuffer().slice(0,8*4));
        const canvas=Array.from(document.getElementById('screen').getContext('2d').getImageData(0,0,8,1).data);
        return {records,exceptions,frame,canvas};
      },{cue,media:[...media],lengths:[iso.length,audio.length]});
      const first=trigger?source:cpu,second=trigger?0x33332222:0;
      assert.deepEqual(r.records,[Number(trigger),first,second,(first+second)>>>0,trigger?4:0,0x600D]);
      assert.equal(r.exceptions,trigger?'1':'0');
      const palette=[[0,0,0,255],[255,0,0,255],[0,255,0,255],[0,0,255,255],[255,255,255,255]];
      const expected=Array.from({length:8},(_,x)=>palette[(first>>>((7-x)*4))&15]).flat();
      assert.deepEqual(r.frame,expected);
      assert.deepEqual(r.canvas,expected);
    }
  }
  console.log('DMA startup passed: Timer0/CD-FIFO/video overlap, guest records/source counts/end handler, changed operands, omitted trigger, repeated boots and Canvas.');
}
