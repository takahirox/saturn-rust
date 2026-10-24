// Original MIT diagnostic and authored records, shared with native Machine tests.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
export async function slaveRaster(page) {
  const original=Buffer.alloc(524288);
  for(const line of (await readFile('tests/fixtures/slave-raster.hex','utf8')).trim().split(/\r?\n/)) {
    if(line.startsWith('#')) continue;
    const [address,...words]=line.split(/\s+/);let at=parseInt(address,16);
    for(const word of words){original.writeUInt16BE(parseInt(word,16),at);at+=2;}
  }
  for(const [h,v,masked,omit] of [[3,5,false,false],[23,41,false,false],[3,5,true,false],[3,5,false,true]]) {
    const image=Buffer.from(original);image.writeUInt32BE(h,0x100);image.writeUInt32BE(v,0x104);
    if(masked) image.writeUInt32BE(0xF0,0x108);
    if(omit) {
      let count=0;
      for(let at=0x200;at<0x300;at+=2) if(image.readUInt16BE(at)===0x2100){count++;if(count===2)image.writeUInt16BE(9,at);}
      assert.equal(count,3);
    }
    for(let repeat=0;repeat<2;repeat++) {
      const result=await page.evaluate(async bytes=>{
        const {emulator}=await import('./app.js');
        emulator.set_pal(false);emulator.reset_bios(Uint8Array.from(bytes));
        for(let i=0;i<600&&emulator.read_long(0x0600E044)!==0x600D;i++) {
          emulator.run_steps(500);
          // Read-only debugger inspections do not manufacture IRQs or records.
          emulator.read_word(0x25F80004);
        }
        return {r:Array.from({length:18},(_,i)=>emulator.read_long(0x0600E000+i*4)),
          master:emulator.exception_count(),slave:emulator.slave_exception_count()};
      },[...image]);
      const {r}=result;
      assert.equal(result.master,'0');
      if(masked||omit) {assert.deepEqual([r[0],r[8],r[17]],[0,0,0]);assert.equal(result.slave,'0');}
      else {
        assert.equal(r[17],0x600D);assert.equal(result.slave,'4');
        assert.deepEqual([r[0],r[1],r[8],r[9],r[16]],[2,h+14,2,v+22,h+v+36]);
        assert.equal(r[2],r[4],'HBlank re-entry within same line');
        assert.equal(r[3]&4,4);assert.equal(r[5]&4,4);
        assert.equal(r[11]&8,8);assert.equal(r[13]&8,8);
        assert.equal(r[10],r[12],'VBlank re-entry within same line');
        assert(r[10]>=224&&r[10]<263);
      }
    }
  }
  console.log('Slave raster passed: SMPC startup, live HBlank/VBlank re-entry, guest arithmetic, SR masking, omitted COMREG and repeated cold boots.');
}
