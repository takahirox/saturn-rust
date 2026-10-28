// Original ST-077 §3.2/fig3.2 real-guest waveform mutation. MIT: fixture LICENSE.
import { scspPipeline } from "./scsp-pipeline.mjs";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function scspPcm(page) {
  await scspPipeline(page);
  const original = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/scsp-pcm.hex", "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v,16));
    words.forEach((v,i) => original.writeUInt16BE(v,at+i*2));
  }
  for (const [a,b] of [[0x2000,0x6000],[0x1000,0x5000],[0x800,0x2800]]) {
    const image = Buffer.from(original);
    let mutations = 0;
    for (let at=0x4100;at<0x4400;at+=2) {
      if (image.readUInt16BE(at)===0x33FC && image.readUInt32BE(at+4)===0x1000) {
        image.writeUInt16BE(image.readUInt16BE(at+2)===0x2000 ? a : b,at+2);
        mutations++;
      }
    }
    assert.equal(mutations,3);
    for (let repeat=0;repeat<2;repeat++) {
      const result = await page.evaluate(async ({bytes,a,b}) => {
        const { emulator } = await import("./app.js");
        emulator.reset_bios(Uint8Array.from(bytes));
        for (let i=0;i<20;i++) emulator.run_steps(10000);
        const audio = emulator.audio_samples(8192);
        const midpoint = (a+b)/2;
        let mid=false, first=false, second=false, valid=true;
        for (let i=0;i+1<audio.length;i+=2) {
          mid ||= audio[i]===midpoint && audio[i+1]===midpoint;
          first ||= audio[i]===a; second ||= audio[i]===b;
          valid &&= audio[i]===audio[i+1] && [0,a,b,midpoint].includes(audio[i]);
        }
        return {done:emulator.read_long(0x0600ED00),mid,first,second,valid,
          halted:emulator.sound_halted(),exceptions:emulator.exception_count()};
      }, {bytes:[...image],a,b});
      assert.deepEqual(result,{done:0x600D,mid:true,first:true,second:true,valid:true,halted:false,exceptions:"0"});
    }
    const stats = await page.evaluate(async () => {
      const { emulator } = await import("./app.js");
      const { AudioOutput } = await import("./audio.js");
      const output = new AudioOutput();
      try {
        await output.enable(); output.setPaused(false);
        emulator.run_steps(10000);
        output.deliver(emulator.audio_samples(8192));
        for (let i=0;i<200;i++) {
          if (output.stats.rendered>128 && output.stats.state==="underflow") break;
          await new Promise(resolve=>setTimeout(resolve,25));
        }
        return {...output.stats};
      } finally { await output.context?.close(); }
    });
    assert.equal(stats.state,"underflow"); assert(stats.rendered>128);
    assert.equal(stats.dropped,0);
    assert(Math.abs(stats.peak-b/32768)<0.00001);
  }
  const keyon = parseInt((await readFile("tests/fixtures/scsp-pcm.tsv","utf8")).trim(),16);
  const absent = Buffer.from(original); absent.writeUInt16BE(0x1020,keyon);
  const result = await page.evaluate(async bytes=>{
    const { emulator } = await import("./app.js");
    emulator.reset_bios(Uint8Array.from(bytes));
    for (let i=0;i<20;i++) emulator.run_steps(10000);
    return {done:emulator.read_long(0x0600ED00),silent:emulator.audio_samples(8192).every(v=>v===0),
      halted:emulator.sound_halted(),exceptions:emulator.exception_count()};
  },[...absent]);
  assert.deepEqual(result,{done:0x600D,silent:true,halted:false,exceptions:"0"});
  const arbitration = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/scsp-pcm-arbitration.hex","utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at,...words] = line.split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((v,i)=>arbitration.writeUInt16BE(v,at+i*2));
  }
  let previous;
  for (let repeat=0;repeat<3;repeat++) {
    const result = await page.evaluate(async bytes=>{
      const { emulator } = await import("./app.js");
      emulator.reset_bios(Uint8Array.from(bytes)); emulator.run_steps(10000);
      return { records:Array.from({length:9},(_,i)=>emulator.read_long(0x0600EE00+i*4)),
        exceptions:emulator.exception_count() };
    },[...arbitration]);
    assert.equal(result.exceptions,"0");
    assert.deepEqual(result.records.slice(6),[0x1230,0x1428,0x600D]);
    for (let n=0;n<2;n++) {
      const quiet=result.records[n], pcm=result.records[2+n], noise=result.records[4+n];
      assert(quiet>=30 && quiet<70); assert(pcm>=65 && pcm<130);
      assert(pcm>=quiet+20); assert(Math.abs(noise-quiet)<=8);
    }
    if (previous) assert.deepEqual(result,previous);
    previous=result;
  }
  console.log("SCSP PCM read phases passed: actual 68k waveform writes yield arithmetic interpolation midpoints, three operand mutations, omitted-KEYON control, guest FRT/DMA waits for all32 PCM voices versus silent-EG/noise suppression, repeated native-equivalent Wasm boots and production AudioWorklet peak/render/underflow/drop assertions.");
}
