// Original MIT guest, independent ST-077 figs4.18/4.30/4.31 oracle.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function scspPipeline(page) {
  const original=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/scsp-pipeline.hex","utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at,...words]=line.split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((v,i)=>original.writeUInt16BE(v,at+i*2));
  }
  const offsets=(await readFile("tests/fixtures/scsp-pipeline.tsv","utf8")).trim().split(/\s+/).map(v=>parseInt(v,16));
  for (const [a,b] of [[0x2000,0x4000],[0x6000,0xC000],[0xA000,0x2000]]) {
    for (const omit of [0,1,2]) {
      const image=Buffer.from(original);
      image.writeUInt16BE(a,offsets[0]); image.writeUInt16BE(b,offsets[1]);
      if (omit) image.writeUInt16BE(0,offsets[omit===1 ? 2 : 3]);
      const signed=v=>v>=32768 ? v-65536 : v;
      const left=omit ? 0 : signed(b)/2, right=signed(a)/2;
      let previous;
      for (let repeat=0;repeat<2;repeat++) {
        const result=await page.evaluate(async ({bytes,left,right})=>{
          const { emulator }=await import("./app.js");
          emulator.reset_bios(Uint8Array.from(bytes));
          for (let i=0;i<10;i++) emulator.run_steps(10000);
          const records=Array.from({length:4},(_,i)=>emulator.read_long(0x0600EF00+i*4));
          emulator.audio_samples(8192); emulator.run_steps(10000);
          const audio=emulator.audio_samples(8192);
          return {records,frames:audio.length/2,stereo:audio.length>=128 && audio.every((v,i)=>v===(i%2 ? right : left)),
            exceptions:emulator.exception_count(),halted:emulator.sound_halted()};
        },{bytes:[...image],left,right});
        assert.deepEqual(result.records,[right&65535,left&65535,0x30060000,0x600D]);
        assert(result.stereo); assert.equal(result.exceptions,"0");
        assert.equal(result.halted,false);
        if (previous) assert.deepEqual(result,previous);
        previous=result;
      }
      const stats=await page.evaluate(async ()=>{
        const { emulator }=await import("./app.js");
        const { AudioOutput }=await import("./audio.js");
        const output=new AudioOutput();
        try {
          await output.enable(); output.setPaused(false);
          emulator.run_steps(10000); output.deliver(emulator.audio_samples(8192));
          for (let i=0;i<200;i++) {
            if (output.stats.rendered>128 && output.stats.state==="underflow") break;
            await new Promise(resolve=>setTimeout(resolve,25));
          }
          const rendered={...output.stats};
          // The bounded producer reports a dropped oversize buffer, then
          // suspension exposes blocked state without changing guest clocks.
          output.deliver(new Int16Array(8193*2));
          const dropped=output.stats.dropped;
          await output.context.suspend(); output.deliver(new Int16Array([1,1]));
          for (let i=0;i<100 && output.stats.state!=="blocked";i++) await new Promise(resolve=>setTimeout(resolve,10));
          return {rendered,dropped,blocked:output.stats.state};
        } finally {await output.context?.close();}
      });
      assert.equal(stats.rendered.state,"underflow"); assert(stats.rendered.rendered>128);
      assert.equal(stats.rendered.dropped,0); assert.equal(stats.dropped,8193);
      assert(Math.abs(stats.rendered.peak-Math.max(Math.abs(left),Math.abs(right))/32768)<0.00001);
      assert.equal(stats.blocked,"blocked");
    }
  }
  console.log("SCSP pipeline passed: original SH-2/68k-uploaded DSP probes post-level MIXS publication and slot31 sample wrap; signed operand/send/KEYON mutations, cold boots, exact stereo and production AudioWorklet render/underflow/drop/blocked states.");
}
