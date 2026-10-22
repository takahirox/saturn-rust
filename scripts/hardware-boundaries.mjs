// Independent original guest evidence for SH7604 memory and SMPC boundaries.
// The browser uploads local synthetic BIOSes and observes actual Wasm execution.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

async function rom(name) {
  const image = Buffer.alloc(524288);
  for (const line of (await readFile(`tests/fixtures/${name}.hex`, "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at,...words] = line.split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((word,i)=>image.writeUInt16BE(word,at+i*2));
  }
  return image;
}
async function load(page,name,image) {
  await page.locator("#bios").setInputFiles({name:`original-${name}.bin`,mimeType:"application/octet-stream",buffer:image});
  await page.waitForFunction(() => document.getElementById("status").textContent.startsWith("Resetting"));
}

export async function hardwareBoundaries(page) {
  const original = await rom("bsc-memory");
  const vectors = (await readFile("tests/fixtures/bsc-memory.tsv","utf8")).trim().split(/\r?\n/).filter(l=>!l.startsWith("#"));
  for (const line of vectors) {
    const [name,...words] = line.split(/\s+/), values=words.map(v=>parseInt(v,16));
    const image=Buffer.from(original);
    [0xA55A0401,0xA55A0040,0xA55A0000].forEach((before,i)=>{
      const needle=Buffer.alloc(4);needle.writeUInt32BE(before);
      const at=original.indexOf(needle);
      assert(at>=0 && original.indexOf(needle,at+1)<0);
      image.writeUInt32BE(0xA55A0000+values[i],at);
    });
    await load(page,name,image);
    const run=()=>page.evaluate(async()=>{
      const {emulator}=await import("./app.js");
      emulator.run_steps(4000);
      return {traces:[0x0600E800,0x0600E900].map(base=>Array.from({length:6},(_,i)=>emulator.read_long(base+i*4))),
        exceptions:[emulator.exception_count(),emulator.slave_exception_count()]};
    });
    const result=await run();
    assert.deepEqual(result.exceptions,["0","0"],name);
    for (const trace of result.traces) assert.deepEqual(trace,values.slice(3),name);
    await page.locator("#reset").click();
    assert.deepEqual(await run(),result,`${name} cold reset`);
  }
  const mutated=Buffer.from(original), needle=Buffer.from([0x12,0x34,0x56,0]);
  let mutations=0;
  for (let at=original.indexOf(needle);at>=0;at=original.indexOf(needle,at+4)) {
    mutated.writeUInt32BE(0x87654300,at);mutations++;
  }
  assert.equal(mutations,4);
  await load(page,"bsc-memory-mutation",mutated);
  assert.deepEqual(await page.evaluate(async()=>{
    const {emulator}=await import("./app.js");emulator.run_steps(4000);
    return [0x0600E800,0x0600E900].map(base=>[emulator.read_long(base+12),emulator.read_long(base+16),emulator.read_long(base+20)]);
  }),[[0x87654308,0x87654308,0x600D],[0x87654308,0x87654308,0x600D]]);

  const smpc=await rom("smpc-boundaries");
  const expected=(await readFile("tests/fixtures/smpc-boundaries.tsv","utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
  await load(page,"smpc-boundaries",smpc);
  const runSmpc=()=>page.evaluate(async count=>{
    const {emulator}=await import("./app.js");
    emulator.set_buttons(0x11);emulator.set_port2_buttons(0x88);
    emulator.set_3d_pad(1,true,0x12,0x34,0x56,0x78);
    const marker=0x0600EC00+(count-1)*4;
    for (let i=0;i<100;i++) {
      emulator.run_steps(10000);
      if (emulator.read_long(marker)===0x600D) break;
    }
    return {trace:Array.from({length:count},(_,i)=>emulator.read_long(0x0600EC00+i*4)),exceptions:emulator.exception_count()};
  },expected.length);
  const result=await runSmpc();
  assert.equal(result.exceptions,"0");assert.deepEqual(result.trace,expected);
  await page.locator("#reset").click();assert.deepEqual(await runSmpc(),result);
  // Leave the existing synthetic browser gamepad/focus checks a released pad.
  await page.evaluate(async()=>{const {emulator}=await import("./app.js");emulator.set_buttons(0);emulator.set_port2_buttons(0);emulator.set_3d_pad(1,false,128,128,0,0);});
}
