// Original MIT SH-2/media fixture. Independent literal IEC matrix codewords.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const codewords = [
 [56,0,46,22,0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,39,59,11,23],
 [56,1,45,20,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,44,32,12,16],
 [56,2,40,18,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,39,34,61,56],
 [56,3,43,16,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,28,11,39,0],
];
const changed = [
 [56,0,46,22,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,41,20,58,23],
 [56,1,45,20,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,13,54,62,5],
 [56,2,40,18,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,26,46,45,9],
 [56,3,43,16,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,53,43,58,36],
];
export async function cdRw(page) {
 const original = await readFile('tests/fixtures/cd-rw.iso');
 const audio = await readFile('tests/fixtures/cd-rw-audio.bin');
 const cue = await readFile('tests/fixtures/cd-rw.cue','utf8');
 const transfers = [];
 for (const [variant,omit] of [['',false],['-changed',false],['-corrected',false],['-bad',false],['-header-bad',false],['',true]]) {
  const iso = Buffer.from(original);
  if (omit) iso.writeUInt32BE(0x0400,21*2048+0x1f00);
  const sub = await readFile(`tests/fixtures/cd-rw${variant}.srw`);
  const error = variant.endsWith('bad');
  const expected = (variant === '-changed' ? changed : codewords).map(p=>[...p]);
  if (error) expected[0] = [...sub.subarray(12,36)];
  const traces = await page.evaluate(async ({iso,audio,sub,cue}) => {
   const {emulator} = await import('./app.js');
   const traces = [];
   for (let boot=0;boot<2;boot++) {
    emulator.reset_cue(cue,'cd-rw.iso\0cd-rw-audio.bin\0cd-rw.srw',new Uint32Array([iso.length,audio.length,sub.length]),Uint8Array.from([...iso,...audio,...sub]));
    for(let i=0;i<400 && emulator.read_long(0x060103fc)!==0x600d;i++) {
     emulator.read_long(0x05818000); // Passive inspection cannot duplicate/consume a FIFO word.
     emulator.read_word(0x05890024);
     emulator.run_steps(10000);
    }
    emulator.run_steps(10000);
    traces.push({records:Array.from({length:256},(_,i)=>emulator.read_long(0x06010000+i*4)), exceptions:emulator.exception_count(), samples:[...emulator.audio_samples(8192)]});
   }
   return traces;
  },{iso:[...iso],audio:[...audio],sub:[...sub],cue});
  assert.deepEqual(traces[1],traces[0]);
  const {records:r,samples} = traces[0];
  assert.equal(r[0x3fc/4],0x600d);
  assert(r[0x300/4]>=4);assert.equal(r[0x308/4],omit?0:4);assert.equal(r[0x304/4],omit?0:2);
  if(!omit) expected.forEach((pack,k)=>{
   const words=Array.from({length:12},(_,i)=>pack[i*2]*256+pack[i*2+1]);
   assert.deepEqual(r.slice(k*32,k*32+12),words,`pack${k}/${variant}`);
   assert.equal(r[k*32+12],words.reduce((a,b)=>a+b,0));
   assert.equal(r[k*32+13],Number(error&&k===0));
   assert.equal(r[k*32+14],pack[1]);assert.equal(r[k*32+15],12);
  });
  assert(samples.length>512);
  samples.slice(-512).forEach((v,i)=>assert.equal(v,i%2?-2000:1000));
  transfers.push(r.slice(0,128));
 }
 assert.notDeepEqual(transfers[0],transfers[1]);
 assert.deepEqual(transfers[0],transfers[2]);
 const atomic = await page.evaluate(async ({cue,iso,audio}) => {
  const {emulator}=await import('./app.js');
  const before=emulator.read_long(0x060103fc);
  let rejected=false;
  try {
   emulator.mount_cue(cue,'cd-rw.iso\0cd-rw-audio.bin\0cd-rw.srw',new Uint32Array([iso.length,audio.length,11]),Uint8Array.from([...iso,...audio,...Array(11).fill(0)]));
  } catch { rejected=true; }
  return [rejected,before,emulator.read_long(0x060103fc)];
 },{cue,iso:[...original],audio:[...audio]});
 assert.deepEqual(atomic,[true,0x600d,0x600d],'Failed mount must preserve executing guest state');
 // Exercise the production local-file UI, including the missing companion.
 await page.locator('#mode').selectOption('disc');
 const file = (name,buffer)=>({name,mimeType:'application/octet-stream',buffer});
 const files=[file('cd-rw.cue',Buffer.from(cue)),file('cd-rw.iso',original),file('cd-rw-audio.bin',audio)];
 await page.locator('#disc').setInputFiles(files);
 await page.waitForFunction(()=>document.getElementById('error').textContent.includes('Missing SRW1'));
 const requests=[];const listener=req=>requests.push(req.url());page.on('request',listener);
 await page.locator('#disc').setInputFiles([...files,file('cd-rw.srw',await readFile('tests/fixtures/cd-rw.srw'))]);
 await page.waitForFunction(()=>document.getElementById('error').textContent==='');
 const loaded=await page.evaluate(async()=>{
  const {emulator}=await import('./app.js');
  for(let i=0;i<400 && emulator.read_long(0x060103fc)!==0x600d;i++)emulator.run_steps(10000);
  return [emulator.read_long(0x060103fc),emulator.read_long(0x06010000)];
 });
 assert.deepEqual(loaded,[0x600d,0x3800]);
 page.off('request',listener);assert.deepEqual(requests,[],'Local companion data must stay in the browser');
 console.log('R-W passed: original guest CPU/DMA packs, SCDQ/A-bus IRQs, literal words/checksums/errors, changed/omitted/corrected/bad controls, stereo, cold boots and local-file privacy.');
}
