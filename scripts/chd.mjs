// Original MIT media/guest and full-byte native-equivalent oracles. No uploads.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
function rom(text, count, audio) {
  const out=Buffer.alloc(524288);
  for(const line of text.split(/\r?\n/).filter(l=>l&&!l.startsWith('#'))){
    const [at,...words]=line.split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>out.writeUInt16BE(w,at+i*2));
  }
  out.writeUInt16BE(count,0x100);out.writeUInt16BE(audio,0x102);return out;
}
function toc(starts, leadout) {
  const out=Buffer.alloc(408,255);
  starts.forEach(([control,fad],i)=>out.writeUInt32BE(control*0x1000000+fad,i*4));
  out.set([starts[0][0],1,0,0],396);out.set([starts.at(-1)[0],starts.length,0,0],400);
  out.writeUInt32BE(0x01000000+leadout,404);return out;
}
export async function chd(page) {
  const source=await readFile('tests/fixtures/chd/probe.hex','utf8');
  const data=await readFile('tests/fixtures/cd-stream.iso');
  const iso=await readFile('web/hardware-lab.iso'),audio=await readFile('web/hardware-lab-audio.bin');
  const cue=await readFile('tests/fixtures/chd/mixed.cue','utf8');
  const gapsCue=await readFile('tests/fixtures/chd/gaps.cue','utf8');
  const mode2=await readFile('tests/fixtures/mode2.bin');
  const requests=[];page.on('request',r=>requests.push({method:r.method(),body:r.postData()}));
  const probe=(image,expected,{cue=null,gaps=false,files=null,names=null,audioFad=expected.length===data.length?0:214}={})=>page.evaluate(async args=>{
    const {emulator:m}=await import('./app.js');
    m.reset_bios(Uint8Array.from(args.rom));
    if(args.cue){
      const files=args.files||(args.gaps?[args.expected,args.audio,args.mode2]:[args.expected,args.audio]);
      const bytes=files.flat();
      m.mount_cue(args.cue,args.names||(args.gaps?'data.iso\0audio.bin\0mode2.bin':'data.iso\0audio.bin'),new Uint32Array(files.map(f=>f.length)),Uint8Array.from(bytes));
    }else m.mount_disc(Uint8Array.from(args.image));
    for(let i=0;i<800&&m.read_long(0x0601fffc)!==0x600d;i++)m.run_steps(10000);
    if(m.read_long(0x0601fffc)!==0x600d)throw new Error(`probe timeout ${m.special_registers()[0].toString(16)}`);
    const bytes=(at,n)=>Array.from({length:n},(_,i)=>(m.read_long((at+i)&~3)>>>(24-(at+i)%4*8))&255);
    const toc=bytes(0x06020000,408),data=bytes(0x06021000,args.expected.length);
    for(let i=0;i<20;i++)m.run_steps(10000);
    return {toc,data,pcm:Array.from(m.audio_samples(8192)),mode:m.mode(),hle:m.read_long(0x06000320),fifo:m.read_long(0x25818000)};
  },{image:[...image],expected:[...expected],audio:[...audio],mode2:[...mode2],cue,gaps,files:files?.map(f=>[...f]),names,rom:[...rom(source,expected.length/2048,audioFad)]});
  const check=(got,expected,table,hasAudio)=>{
    assert.deepEqual(Buffer.from(got.data),expected,'complete first-track FIFO bytes');
    assert.deepEqual(Buffer.from(got.toc),table,'all 408 guest-transferred TOC bytes');
    assert.equal(got.mode,'User-provided BIOS');assert.equal(got.hle,0);assert.equal(got.fifo,0xFFFFFFFF);
    assert.equal(got.pcm.some(v=>v!==0),hasAudio);if(hasAudio)assert(got.pcm.some((v,i)=>i%2===0&&v!==got.pcm[i+1]));
  };
  const dataToc=toc([[0x41,150]],278),mixedToc=toc([[0x41,150],[1,214]],222);
  const baseline=await probe(data,data);check(baseline,data,dataToc,false);
  const mixed=await probe(iso,iso,{cue});check(mixed,iso,mixedToc,true);
  for(const codec of ['default','cdlz','cdzl','cdfl','none']){
    const a=await probe(await readFile(`tests/fixtures/chd/data-${codec}.chd`),data);
    check(a,data,dataToc,false);assert.deepEqual(a,baseline);
    const b=await probe(await readFile(`tests/fixtures/chd/mixed-${codec}.chd`),iso);
    check(b,iso,mixedToc,true);assert.deepEqual(b,mixed,'identical full stereo samples and guest bytes');
  }
  const gaps=await probe(iso,iso,{cue:gapsCue,gaps:true});
  check(gaps,iso,toc([[0x41,150],[1,216],[0x41,225]],228),true);
  assert.deepEqual(await probe(await readFile('tests/fixtures/chd/gaps-cdlz.chd'),iso),gaps);
  const mode1Raw=await readFile('tests/fixtures/large-media-mode1.raw');
  const xaRaw=await readFile('tests/fixtures/large-media-xa.raw');
  const rawData=Buffer.concat(Array.from({length:mode1Raw.length/2352},(_,i)=>mode1Raw.subarray(i*2352+16,i*2352+2064)));
  const rawBaseline=await probe(mode1Raw,rawData,{cue:await readFile('tests/fixtures/chd/raw.cue','utf8'),files:[mode1Raw,xaRaw,mode2,audio],names:'mode1.bin\0xa.bin\0mode2.bin\0audio.bin',audioFad:169});
  check(rawBaseline,rawData,toc([[0x41,150],[0x41,159],[0x41,168],[1,169]],177),true);
  assert.deepEqual(await probe(await readFile('tests/fixtures/chd/raw-cdlz.chd'),rawData,{audioFad:169}),rawBaseline);
  // Standard optical subcode is not the already-deinterleaved SRW1 boundary.
  // Mount it standalone and retain every data/audio byte without requiring SRW1.
  const rwIso=await readFile('tests/fixtures/cd-rw.iso'),rwAudio=await readFile('tests/fixtures/cd-rw-audio.bin');
  const rwCue='FILE "data.iso" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nFILE "audio.bin" BINARY\nTRACK 02 AUDIO\nINDEX 01 00:00:00';
  const rwExpected=await probe(rwIso,rwIso,{cue:rwCue,files:[rwIso,rwAudio]});
  assert.deepEqual(await probe(await readFile('tests/fixtures/chd/subcode-cdlz.chd'),rwIso),rwExpected);
  const changed=Buffer.from(data);changed[64*2048]^=0x1F;
  // Changed bytes must propagate through the same firmware FIFO, never a cached source image.
  const changedResult=await probe(await readFile('tests/fixtures/chd/changed-cdlz.chd'),changed);
  assert.deepEqual(Buffer.from(changedResult.data),changed);

  // Exercise the production picker, HLE retained-media Reset/Restart and errors.
  await page.selectOption('#mode','disc');
  const chd=await readFile('tests/fixtures/chd/mixed-cdlz.chd');
  const select=async(buffer,name='ordinary.chd')=>{
    await page.locator('#disc').setInputFiles({name,mimeType:'application/octet-stream',buffer});
    await page.waitForFunction(()=>document.getElementById('status').textContent==='Ready'||document.getElementById('error').textContent.includes('CHD')||document.getElementById('error').textContent.includes('Select one'));
  };
  await select(chd);assert.equal(await page.locator('#error').textContent(),'');
  const lab=()=>page.evaluate(async()=>{
    const {emulator:m}=await import('./app.js');
    for(let i=0;i<40&&m.read_long(0x06010020)!==0x600d;i++)m.run_steps(10000);
    return {fields:Array.from({length:11},(_,i)=>m.read_long(0x06010000+i*4)),pcm:Array.from(m.audio_samples(8192)),mode:m.mode()};
  });
  const result=await lab();assert.deepEqual(result.fields.slice(0,6),[1,1,84,35,42,393984]);
  assert.equal(result.fields[8],0x600d);assert.equal(result.fields[10],0x900);assert(result.pcm.some(v=>v!==0));assert.equal(result.mode,'BIOS-less HLE disc');
  for(const button of ['#reset','#restart']){await page.click(button);assert.equal((await lab()).fields[8],0x600d);}
  const variants=[[await readFile('tests/fixtures/chd/self-cycle.chd'),/cyclic self/]];
  const flac=await readFile('tests/fixtures/chd/mixed-cdfl.chd');
  const flacStart=flac.indexOf(Buffer.from([0xFF,0xF8]),124);
  assert(flacStart>=124,'MAME fixture contains a FLAC frame');
  const mono=Buffer.from(flac);mono[flacStart+3]=0x08;variants.push([mono,/stereo 16-bit/]);
  const parent=Buffer.from(chd);parent[104]=1;variants.push([parent,/parent/]);
  const codec=Buffer.from(chd);codec.write('cdzs',16);variants.push([codec,/codec/]);
  const huge=Buffer.from(chd);huge.writeUInt32BE(0xFFFFFFFF,56);variants.push([huge,/hunks/]);
  const corrupt=Buffer.from(chd);corrupt[124]^=255;variants.push([corrupt,/CHD/]);
  variants.push([chd.subarray(0,150),/CHD/],[Buffer.alloc(124),/CHD/]);
  for(const [bytes,message] of variants){
    await select(bytes);assert.match(await page.locator('#error').textContent(),message);
    assert(await page.locator('#run').isDisabled());
    const stopped=await page.evaluate(async()=>{const {emulator:m}=await import('./app.js');return [m.instruction_count(),m.read_long(0x06010020)];});
    assert.deepEqual(stopped,['0',0],'failed load discarded stale guest state');
  }
  await select(Buffer.alloc(18*2048),'lonely.bin');assert.match(await page.locator('#error').textContent(),/BIN needs its CUE/);
  await select(chd);await page.locator('#disc').setInputFiles([]);
  assert(await page.locator('#run').isDisabled(),'cancelled replacement has no stale medium');
  assert.equal(requests.filter(r=>r.method!=='GET'||r.body!==null).length,0);
}
