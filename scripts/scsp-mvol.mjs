// Original MIT fixture/media (tests/fixtures/LICENSE), Sega sound FAQ MVOL;
// ST-077 §4.2 pp.80–88. Expected arithmetic never imports core gain helpers.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function scspMvol(page) {
  const original = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/scsp-mvol.hex","utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at,...words]=line.split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((v,i)=>original.writeUInt16BE(v,at+i*2));
  }
  const operands=Object.fromEntries((await readFile("tests/fixtures/scsp-mvol.tsv","utf8")).trim().split(/\r?\n/).map(line=>{
    const [name,at]=line.split(/\s+/);return [name,parseInt(at,16)];
  }));
  const cue=await readFile("tests/fixtures/mvol.cue","utf8");
  const data=await readFile("tests/fixtures/mvol-data.bin");
  const audio=await readFile("tests/fixtures/mvol-audio.bin");
  const media=Buffer.concat([data,audio]);
  const expected=(v,g)=>g===0 ? 0 : Math.trunc(Math.max(-32768,Math.min(32767,v))*Math.round(32768*2**((g-15)/2))/32768);
  function authored(samples,mode,gain,wave,coef,cdLeft) {
    assert(samples.length>128);
    if (!gain) { assert(samples.every(v=>v===0));return; }
    const pcm=[wave*256,-wave*256,-wave*512,wave*512];
    let found=false;
    for (let phase=0;phase<4 && !found;phase++) {
      for (let cd=0;cd<(mode<2 ? 1 : 18000) && !found;cd++) {
        found=true;
        for (let i=0;i<samples.length/2;i++) {
          const p=pcm[(phase+i)%4];
          const dry=mode===0 || mode===3;
          const wet=mode===1 || mode===3;
          const left=(dry ? p/16 : 0)+(wet ? p*coef/32768 : 0)+(mode>=2 ? cdLeft : 0);
          const right=(dry ? p : 0)+(mode>=2 ? -1000-cd-i : 0);
          if(samples[i*2]!==expected(left,gain) || samples[i*2+1]!==expected(right,gain)) {found=false;break;}
        }
      }
    }
    assert(found,`MVOL mode=${mode}, gain=${gain}, samples=${samples.slice(0,16)}`);
  }
  const variant=(mode,gain,wave=16,coef=0x4000,omit=false,noMute=false)=>{
    const image=Buffer.from(original);
    image[operands.wave]=wave;
    image.writeUInt16BE(coef,operands.coef);
    image.writeUInt16BE([0xe800,0xff,0,0xe8ff][mode],operands.pcm);
    if(mode<2) {image.writeUInt16BE(0,operands.ext_left);image.writeUInt16BE(0,operands.ext_right);}
    image.writeUInt16BE(gain,operands.sound_volume);
    assert.equal(image[operands.restore],0xe0);
    image[operands.restore+1]=gain;
    if(noMute) image[operands.mute+1]=gain;
    if(omit) {
      image.set([0x4e,0x71,0x4e,0x71,0x4e,0x71,0x4e,0x71],operands.sound_volume-2);
      image.set([0,9],operands.restore+2);
    }
    return image;
  };
  async function execute(image,changedAudio=false,fullTrace=false) {
    const input=Buffer.from(media);
    if(changedAudio) for(let at=data.length;at<input.length;at+=4) input.writeInt16LE(2048,at);
    return page.evaluate(async ({bytes,cue,media,lengths,fullTrace})=>{
      const { emulator }=await import("./app.js");
      emulator.reset_bios(Uint8Array.from(bytes));
      if(emulator.audio_generated()!=="0" || emulator.audio_dropped()!=="0" || emulator.audio_samples(8192).length) throw new Error("MVOL cold reset retained audio");
      emulator.mount_cue(cue,"mvol-data.bin\0mvol-audio.bin",Uint32Array.from(lengths),Uint8Array.from(media));
      const trace=[];
      for(let i=0;i<40;i++) {
        emulator.run_steps(10000);
        const block=emulator.audio_samples(8192);
        if(fullTrace) trace.push(...block);
        if(!fullTrace && emulator.read_long(0x06009100)===4) break;
      }
      assertDone();
      function assertDone() {if(emulator.read_long(0x06009100)!==4) throw new Error("MVOL guest stalled");}
      emulator.run_steps(10000);
      const samples=Array.from(emulator.audio_samples(8192));
      // Verified guest PCM is retained solely for production worklet delivery.
      window.mvolPcm=Int16Array.from(samples);
      return {samples,trace,records:[0,4,0x14,0x18,0x1c].map(at=>emulator.read_long(0x06009100+at)),
        instructions:emulator.sound_instruction_count(),generated:emulator.audio_generated(),
        dropped:emulator.audio_dropped(),halted:emulator.sound_halted(),exceptions:emulator.exception_count(),
        geometry:[emulator.read_long(0x05a00808),emulator.read_long(0x06009120)],
        register:emulator.read_long(0x05b00400) >>> 16};
    },{bytes:[...image],cue,media:[...input],lengths:[data.length,audio.length],fullTrace});
  }
  async function worklet(gain,samples) {
    const stats=await page.evaluate(async ()=>{
      const { AudioOutput }=await import("./audio.js");
      const output=new AudioOutput();
      try {
        await output.enable();output.setPaused(false);
        output.deliver(window.mvolPcm);
        for(let i=0;i<200;i++) {
          if(output.stats.rendered>128 && output.stats.state==="underflow") break;
          await new Promise(resolve=>setTimeout(resolve,25));
        }
        const playing={...output.stats};
        output.setMuted(true);const muted=output.stats.state;
        output.setMuted(false);
        await output.context.suspend();
        for(let i=0;i<100 && output.stats.state!=="blocked";i++) await new Promise(resolve=>setTimeout(resolve,10));
        return {playing,muted,blocked:output.stats.state};
      } finally {await output.context?.close();}
    });
    assert.equal(stats.playing.state,"underflow");
    assert(stats.playing.received>=samples.length/2);
    assert(stats.playing.rendered>128);assert(stats.playing.underflow>0);
    assert.equal(stats.playing.dropped,0);
    assert.equal(stats.muted,"muted");assert.equal(stats.blocked,"blocked");
    const peak=Math.max(...samples.map(Math.abs))/32768;
    assert(Math.abs(stats.playing.peak-peak)<0.00001,`worklet MVOL=${gain}, peak=${stats.playing.peak}, expected=${peak}`);
  }
  for(let mode=0;mode<4;mode++) {
    for(const gain of [0,1,13,14,15]) {
      let previous;
      for(let repeat=0;repeat<2;repeat++) {
        const result=await execute(variant(mode,gain));
        authored(result.samples,mode,gain,16,0x4000,1024);
        assert.equal(result.records[0],4);assert.equal(result.records[1],45000);
        assert(result.records[2]>0 && result.records[3]>result.records[2] && result.records[4]>result.records[3]);
        assert.equal(result.register & 0x20f,0);
        assert.deepEqual(result.geometry,[0x5a5a0000,0x5a5a]);
        assert.equal(result.halted,false);assert.equal(result.exceptions,"0");assert.equal(result.dropped,"0");
        if(previous) assert.deepEqual(result,previous);
        previous=result;
      }
      if(mode===3) await worklet(gain,previous.samples);
    }
  }
  for(const [wave,coef,changedAudio] of [[8,0x2000,false],[24,0x4000,true],[16,0,false]]) {
    const result=await execute(variant(3,13,wave,coef),changedAudio);
    authored(result.samples,3,13,wave,coef,changedAudio ? 2048 : 1024);
  }
  const omitted=await execute(variant(3,13,16,0x4000,true));
  assert(omitted.samples.every(v=>v===0));
  const muted=await execute(variant(3,15),false,true);
  const continuous=await execute(variant(3,15,16,0x4000,false,true),false,true);
  assert.equal(muted.instructions,continuous.instructions);
  assert.equal(muted.generated,continuous.generated);
  assert.deepEqual(muted.records,continuous.records);
  assert.equal(muted.trace.length,continuous.trace.length);
  const differences=[];
  for(let i=0;i<muted.trace.length;i+=2) {
    if(muted.trace[i]!==continuous.trace[i] || muted.trace[i+1]!==continuous.trace[i+1]) {
      assert.equal(muted.trace[i],0);assert.equal(muted.trace[i+1],0);differences.push(i);
    }
  }
  assert(differences.length>100);
  assert(differences.every((v,i)=>!i || v===differences[i-1]+2));
  const restored=differences.at(-1)+2;
  assert(muted.trace.length-restored>128);
  assert.deepEqual(muted.trace.slice(restored),continuous.trace.slice(restored));
  console.log("SCSP MVOL passed: shared PCM/executing DSP/runtime mixed-track CD-DA amplitudes at 0/1/13/14/F, real SH-2/68k byte writes, source mutations, omitted-write control, repeated cold boots, exact mute/resume progress and production AudioWorklet peaks/underflow/mute/blocked/drop states.");
}
