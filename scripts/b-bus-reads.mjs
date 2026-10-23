// Same original MIT ROM, operands and controls as tests/b_bus_reads.rs.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function bBusReads(page) {
  const original = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/b-bus-reads.hex", "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [address, ...words] = line.split(/\s+/);
    let at = parseInt(address, 16);
    for (const word of words) { original.writeUInt16BE(parseInt(word,16),at); at += 2; }
  }
  const golden = (await readFile("tests/fixtures/b-bus-reads.tsv","utf8")).trim().split(/\r?\n/).map(s=>parseInt(s,16));
  let normalTimes;
  for (const [a,b,omit] of [[0xA1B2,0xC3D4,false],[0x1122,0x3344,false],[0xA1B2,0xC3D4,true]]) {
    const image=Buffer.from(original);
    image.writeUInt16BE(a,0x100); image.writeUInt16BE(b,0x102);
    if(omit) {
      let count=0;
      for(let at=0x200;at<0x2000;at+=2) if(image.readUInt16BE(at)===0x6041) {
        image.writeUInt16BE(0x60C3,at); count++;
      }
      assert.equal(count,80);
    }
    for(let repeat=0;repeat<2;repeat++) {
      const result=await page.evaluate(async bytes=>{
        const {emulator}=await import("./app.js");
        emulator.set_pal(false); emulator.reset_bios(Uint8Array.from(bytes));
        for(let i=0;i<100 && emulator.read_long(0x0600E190)!==0x600D;i++) emulator.run_steps(5000);
        return {records:[0x0600E000,0x0600E100].map(base=>Array.from({length:37},(_,i)=>emulator.read_long(base+i*4))),
          exceptions:emulator.exception_count(),slaveExceptions:emulator.slave_exception_count(),slaveInstructions:emulator.slave_instruction_count()};
      },[...image]);
      const times=[];
      for(const actual of result.records) {
        for(const i of [3,11,19,27,35]) {
          times.push(actual[i]);
          assert(actual[i]>=(omit?10:32)&&actual[i]<(omit?64:128),`${actual}`); actual[i]=0;
        }
        const expected=[a,b,(a*65536+b)>>>0,0];
        for(let i=0;i<4;i++) expected.push(a>>>8,a&255,b>>>8,b&255,a,b,(a*65536+b)>>>0,0);
        expected.push(0x600D);
        if(a===0xA1B2) assert.deepEqual(expected,golden);
        assert.deepEqual(actual,expected);
      }
      if(!omit) { if(!normalTimes) normalTimes=times; else assert.deepEqual(times,normalTimes); }
      else times.forEach((t,i)=>assert(t+20<normalTimes[i]));
      assert.equal(result.exceptions,"0"); assert.equal(result.slaveExceptions,"0");
      assert(BigInt(result.slaveInstructions)>100n);
    }
  }
  console.log("B-bus read bridge passed: original guest on both SH-2s, sound/video lanes, guest FRT, changed operands, replaced-read control and repeated cold boots.");
}
