// Original MIT dual-SH-2 CS0 guest in rebuilt production Wasm.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function cs0Transfers(page) {
  const rom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/cs0.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v,16));
    words.forEach((v,i) => rom.writeUInt16BE(v,at+i*2));
  }
  for (const waits of [0,1,2]) for (const payload of [0x11223344,0x89ABCDEF]) for (const omit of [false,true]) {
    const image = Buffer.from(rom);
    image.writeUInt32BE(payload,0x1F000);
    let changed = 0;
    for (let at=0;at<image.length;at+=4) if (image.readUInt32BE(at)===0xA55A0001) {
      image.writeUInt32BE(0xA55A0000+waits,at); changed++;
    }
    assert.equal(changed,2);
    if (omit) {
      let n=0;
      for(let at=0x8000;at<0x8100;at+=2) if(image.readUInt16BE(at)===0x66A2) {
        image.writeUInt16BE(9,at); n++;
      }
      assert.equal(n,16);
    }
    const traces=await page.evaluate(async image => {
      const {Emulator}=await import("./pkg/saturn_rust.js");
      const m=new Emulator();
      try {
        return Array.from({length:2},()=>{
          m.reset_bios(Uint8Array.from(image));
          for(let i=0;i<5;i++) m.run_steps(10000);
          return {words:[0x0601C000,0x0601C100].map((a,cpu)=>Array.from({length:cpu?6:5},(_,i)=>m.read_long(a+i*4))),
            sync:m.read_long(0x00212000),exceptions:[m.exception_count(),m.slave_exception_count()]};
        });
      } finally {m.free();}
    },[...image]);
    // high(2)+low(2)+32 long reads+high(2). Each grouped read costs
    // 4/5/7 states for W0=0/1/2. NOP control costs one state per ROM read.
    const ticks=(omit?[10,12,16]:[16,20,28])[waits];
    const value=omit?0:payload;
    const common=[ticks,value,0x13579BDF,(value^0x13579BDF)>>>0];
    for(const trace of traces) assert.deepEqual(trace,{words:[[...common,0x600D],[...common,0x51,0x600D]],sync:0xA3,exceptions:["0","0"]});
  }
  console.log("CS0 guest passed: both CPUs computed ROM/low-RAM payloads, BSTROM FRT 16/20/28, omitted-read 10/12/16, synchronization, operand mutations and repeated cold boots in production Wasm.");
}
