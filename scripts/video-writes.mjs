// Original MIT guest; same ROM, controls and independent bounds as native tests.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
export async function videoWrites(page) {
  const original=Buffer.alloc(524288);
  for(const line of (await readFile('tests/fixtures/video-writes.hex','utf8')).trim().split(/\r?\n/)) {
    if(line.startsWith('#')) continue;
    const [address,...words]=line.split(/\s+/);let at=parseInt(address,16);
    for(const word of words){original.writeUInt16BE(parseInt(word,16),at);at+=2;}
  }
  let normalTimes;
  for(const [payload,color,omit] of [[0x11223344,0xFF,false],[0x55667788,0xFF00,false],[0x11223344,0xFF,true]]) {
    const image=Buffer.from(original);image.writeUInt32BE(payload,0x100);image.writeUInt32BE(color,0x108);
    if(omit) {
      image.writeUInt16BE(0,0x104);let count=0;
      for(let at=0x200;at<0x1000;at+=2) if(image.readUInt16BE(at)===0x24C2){image.writeUInt16BE(9,at);count++;}
      assert.equal(count,64);
    }
    for(let repeat=0;repeat<2;repeat++) {
      const result=await page.evaluate(async bytes=>{
        const {emulator,advanceFrame}=await import('./app.js');
        emulator.set_pal(false);emulator.reset_bios(Uint8Array.from(bytes));
        for(let i=0;i<4000&&emulator.read_long(0x0600E150)!==0x600D;i++) emulator.run_steps(500);
        const records=[0x0600E000,0x0600E100].flatMap(base=>Array.from({length:21},(_,i)=>emulator.read_long(base+i*4)));
        records.push(emulator.read_long(0x0600E200),emulator.read_long(0x0600E204));
        const done=emulator.read_long(0x0600E208),exceptions=emulator.exception_count(),slaveExceptions=emulator.slave_exception_count();
        advanceFrame();
        const f=emulator.framebuffer(),ctx=document.getElementById('screen').getContext('2d');
        return {records,done,exceptions,slaveExceptions,row:Array.from(f.slice(16*320*4,17*320*4)),
          canvas:Array.from(ctx.getImageData(0,16,320,1).data)};
      },[...image]);
      assert.equal(result.done,0x600D);assert.equal(result.exceptions,'0');assert.equal(result.slaveExceptions,'0');
      const times=[];
      for(const base of [0,21]) {
        for(let n=0;n<4;n++) {
          const r=result.records.slice(base+n*5,base+n*5+5);times.push(r[0]);
          assert(r[0]>=(omit?1:32)&&r[0]<200,`CPU FRT ${r}`);
          assert.equal(r[1],omit?0:payload);assert(r[2]>=1&&r[2]<2000);
          assert.deepEqual(r.slice(3),[omit?0:payload,omit?0:payload]);
        }
        assert.equal(result.records[base+20],0x600D);
      }
      if(!omit){if(!normalTimes)normalTimes=times;else assert.deepEqual(times,normalTimes);}
      else times.forEach((t,i)=>assert(t+24<normalTimes[i],`write budget ${t}/${normalTimes[i]}`));
      assert.equal(result.records[42],omit?0x00FF0000:color);assert(result.records[43]>=300&&result.records[43]<600);
      assert.deepEqual(result.canvas,result.row);
      const pixels=Array.from({length:320},(_,x)=>result.row.slice(x*4,x*4+4));
      const black=pixels.flatMap((p,x)=>p.every((v,i)=>v===[0,0,0,255][i])?[x]:[]);
      if(omit){assert.equal(black.length,0);pixels.forEach(p=>assert.deepEqual(p,[0,0,255,255]));}
      else {
        // Eight clocks between words / four clocks per dot = two black dots.
        // Old blue is removed by high word; new red/green arrives in low word.
        assert.equal(black.length,2);assert.equal(black[1],black[0]+1);assert(black[0]>=50&&black[0]<300);
        pixels.forEach((p,x)=>assert.deepEqual(p,x<black[0]?[0,0,255,255]:x<=black[1]?[0,0,0,255]:color===0xFF?[255,0,0,255]:[0,255,0,255]));
      }
    }
  }
  console.log('Video writes passed: dual SH-2/DMAC guest FRT/readback, separate palette-word Canvas publication, changed payloads, omitted writes and repeated cold boots.');
}
