// Original MIT firmware, shared native/production-Wasm semantics and controls.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

async function rom(name) {
  const image=Buffer.alloc(524288);
  for (const line of (await readFile(`tests/fixtures/scu-dstp-${name}.hex`,"utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at,...words]=line.split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((word,i)=>image.writeUInt16BE(word,at+i*2));
  }
  return [...image];
}

export async function scuDstp(page) {
  const disc=[...await readFile("web/star-collector.iso")];
  for (const name of ["read","write","access"]) {
    const image=await rom(name);
    const expected=(await readFile(`tests/fixtures/scu-dstp-${name}.tsv`,"utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
    let reference;
    for (const chunk of [1,127,10000]) {
      for (const media of [false,true]) {
        const result=await page.evaluate(async ({image,disc,media,chunk,count})=>{
          const {emulator:m}=await import("./app.js");
          m.clear_media();
          if (media) m.mount_disc(Uint8Array.from(disc));
          m.reset_bios(Uint8Array.from(image));
          if (m.dstp_trace()!==undefined) throw Error("cold reset retained developer trace");
          m.set_dstp_trace(media);
          for (let i=0;i<10000;i+=chunk) {
            m.run_steps(Math.min(chunk,10000-i));
            m.registers();m.dstp_trace(); // observation must not change progress
          }
          const trace=m.dstp_trace();
          for (let i=0;i<100;i++) m.read_long(0x25fe0060);
          if (trace!==m.dstp_trace()) throw Error("debug inspection recorded guest cycles");
          return {records:Array.from({length:count},(_,i)=>m.read_long(0x0600d000+i*4)),
            clocks:m.peripheral_clocks(),instructions:m.instruction_count(),exceptions:m.exception_count(),
            registers:[...m.registers()],status:m.status(),irq:m.read_long(0x25fe00a4)&0xe00,trace};
        },{image,disc,media,chunk,count:expected.length});
        assert.deepEqual(result.records,expected);
        assert.equal(result.exceptions,name==="access"?"3":"0");
        assert.equal(result.status,"Sleeping — waiting for an interrupt");
        assert.equal(result.irq,0);
        if (media) {
          assert.match(result.trace,/source=Master SH-2 PC=Some\(200002/);
          assert.equal(result.trace.split("\n").length,name==="write"?64:name==="read"?1:35);
          if (name==="read") assert.match(result.trace,/direct read.*value=None.*result=Ok\(00000000\)/);
          if (name==="write") assert.match(result.trace,/buffered write write.*value=Some\(A55A1234\)/);
          if (name==="access") {
            assert.match(result.trace,/address=0x05FE0060/);
            assert.match(result.trace,/address=0x25FE0060/);
          }
        } else assert.equal(result.trace,undefined);
        delete result.trace;
        reference??=result;
        assert.deepEqual(result,reference);
      }
    }
  }
  const image=await rom("refill");
  const negative=await page.evaluate(async image=>{
    const {emulator:m}=await import("./app.js");
    m.reset_bios(Uint8Array.from(image));m.set_dstp_trace(true);
    let message,repeated;
    try {m.run_steps(10000);} catch(e) {message=String(e);}
    const clocks=m.peripheral_clocks();
    try {m.run_steps(1);} catch(e) {repeated=String(e);}
    return {message,repeated,trace:m.dstp_trace(),unchanged:clocks===m.peripheral_clocks(),status:m.status()};
  },image);
  assert.match(negative.message,/0x05FE0064/);
  assert.equal(negative.message,negative.repeated);
  assert(negative.unchanged);
  assert.match(negative.status,/Faulted/);
  assert.match(negative.trace,/operand=Some\(\(05FE006C, 00000004\)\) cache line-fill read.*address=0x05FE0060.*result=Ok\(00000000\)/);
  console.log("DSTP guests passed: reads, ignored writes, aliases, cache refill negative control, bounded local diagnostics, DMA IRQ/data, media/no-media, chunk parity and cold resets.");
}
