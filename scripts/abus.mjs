// Original MIT guest; expected FRT and data vectors from ST-097 §3.2.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
export async function abusTiming(page) {
  const bytes=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/abus.hex","utf8")).split(/\r?\n/)) {
    if(!line || line.startsWith("#")) continue;
    const [address,...words]=line.split(/\s+/);
    words.forEach((w,i)=>bytes.writeUInt16BE(parseInt(w,16),parseInt(address,16)+i*2));
  }
  const expected=(await readFile("tests/fixtures/abus.tsv","utf8")).trim().split(/\r?\n/).map(w=>parseInt(w,16));
  for(const changed of [false,false,true]) {
    const image=Buffer.from(bytes), golden=[...expected];
    if(changed) {
      const needle=Buffer.from([0,0x30,0,0]),at=image.indexOf(needle);
      assert(at>=0 && image.indexOf(needle,at+1)<0);
      image.writeUInt32BE(0x00400000,at); golden[2]=12;
    }
    const actual=await page.evaluate(async image=>{
      const {emulator}=await import("./app.js");
      emulator.reset_bios(Uint8Array.from(image));
      emulator.run_steps(10000);
      return {records:Array.from({length:13},(_,i)=>emulator.read_long(0x0600D700+i*4)),
        exceptions:emulator.exception_count()};
    },[...image]);
    assert.deepEqual(actual,{records:golden,exceptions:"0"});
  }
  console.log("A-bus guest passed: programmed waits, narrow bus, bounded/unbounded burst, precharge, reset and changed wait operand through production Wasm.");
}
