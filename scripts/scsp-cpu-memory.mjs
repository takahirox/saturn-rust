// Original MIT guest: tests/fixtures/LICENSE. Same bytes and independent
// cycle-budget/audio expectations as tests/scsp_cpu_memory.rs.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function scspCpuMemory(page) {
  const original=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/scsp-cpu-memory.hex","utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at,...words]=line.split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((v,i)=>original.writeUInt16BE(v,at+i*2));
  }
  const offsets=(await readFile("tests/fixtures/scsp-cpu-memory.tsv","utf8")).trim().split(/\s+/).map(v=>parseInt(v,16));
  for (const [a,b] of [[0x2000,0x4000],[0x1000,0x3000],[0x800,0x2800]]) {
    const image=Buffer.from(original);
    image.writeUInt16BE(a,offsets[0]); image.writeUInt16BE(b,offsets[1]);
    let previous;
    for (let repeat=0;repeat<2;repeat++) {
      const result=await page.evaluate(async ({bytes,a,b})=>{
        const { emulator }=await import("./app.js");
        emulator.reset_bios(Uint8Array.from(bytes));
        for (let i=0;i<20;i++) emulator.run_steps(10000);
        const audio=emulator.audio_samples(8192);
        let first=false,second=false,valid=true;
        for (let i=0;i+1<audio.length;i+=2) {
          first ||= audio[i]===a && audio[i+1]===a/2;
          second ||= audio[i]===b && audio[i+1]===b/2;
          valid &&= [0,0x2000,a,b,(a+b)/2].includes(audio[i]) && [0,0x1000,a/2,b/2].includes(audio[i+1]);
        }
        return {records:[0,1,2].map(i=>emulator.read_long(0x0600ef00+i*4)),
          first,second,valid,exceptions:emulator.exception_count(),halted:emulator.sound_halted()};
      },{bytes:[...image],a,b});
      assert.deepEqual(result.records.slice(1),[0x20002000,0x600d]);
      // 512 * 26874100 / 5644800 / 8 >304 FRT counts, before refresh.
      assert(result.records[0]>=304 && result.records[0]<520,`DRAM cycle budget ${result.records}`);
      assert(result.first && result.second && result.valid);
      assert.equal(result.exceptions,"0"); assert.equal(result.halted,false);
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
        await output.context.suspend(); output.deliver(new Int16Array([1,1]));
        for (let i=0;i<100 && output.stats.state!=="blocked";i++) {
          await new Promise(resolve=>setTimeout(resolve,10));
        }
        return {rendered,blocked:output.stats.state};
      } finally { await output.context?.close(); }
    });
    assert.equal(stats.rendered.state,"underflow"); assert(stats.rendered.rendered>128);
    assert.equal(stats.rendered.dropped,0);
    assert(Math.abs(stats.rendered.peak-b/32768)<0.00001);
    assert.equal(stats.blocked,"blocked");
  }
  const absent=Buffer.from(original); absent.writeUInt16BE(9,offsets[2]);
  const result=await page.evaluate(async bytes=>{
    const { emulator }=await import("./app.js");
    emulator.reset_bios(Uint8Array.from(bytes));
    for (let i=0;i<2;i++) emulator.run_steps(10000);
    return {done:emulator.read_long(0x0600ef08),wave:emulator.read_long(0x05a01000),
      silent:emulator.audio_samples(8192).every(v=>v===0),exceptions:emulator.exception_count()};
  },[...absent]);
  assert.deepEqual(result,{done:0,wave:0,silent:true,exceptions:"0"});
  console.log("SCSP CPU memory guest passed: finite 512-cycle DRAM budget, SH-2 DMAC/68k shared operands, timed sample writes, executing PCM/DSP stereo, changed operands, omitted start, repeated cold boots and production AudioWorklet rendering/underflow/blocked assertions.");
}
