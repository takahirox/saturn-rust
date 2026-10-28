// Original synthetic guest execution through the production Wasm and Canvas.
// Expected pixels/registers are authored from ST-013 §§2/4 and ST-058 §§2–4.
import { vdp1Vbe } from "./vdp1-vbe.mjs";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

async function rom(name) {
  const bytes = Buffer.alloc(524288);
  for (const line of (await readFile(`tests/fixtures/${name}.hex`, "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [address, ...words] = line.split(/\s+/);
    let at = parseInt(address, 16);
    for (const word of words) { bytes.writeUInt16BE(parseInt(word, 16), at); at += 2; }
  }
  return bytes;
}
function patch(bytes, before, after, count = 1) {
  const changed = Buffer.from(bytes), locations = [];
  for (let i = 0; i <= changed.length - 4; i++) if (changed.readUInt32BE(i) === before) locations.push(i);
  assert.equal(locations.length, count);
  for (const i of locations) changed.writeUInt32BE(after, i);
  return changed;
}
async function run(page, bytes, base, count, pixels = false) {
  return page.evaluate(async ({ bytes, base, count, pixels }) => {
    const { emulator, advanceFrame } = await import("./app.js");
    emulator.set_pal(false);
    emulator.reset_bios(Uint8Array.from(bytes));
    for (let i = 0; i < 100 && emulator.read_long(base + (count - 1) * 4) !== 0x600D; i++) emulator.run_steps(5000);
    const records = Array.from({ length: count }, (_, i) => emulator.read_long(base + i * 4));
    if (!pixels) return { records };
    advanceFrame();
    const f = emulator.framebuffer(), width = emulator.video_width(), height = emulator.video_height();
    const canvas = document.getElementById("screen").getContext("2d");
    return { records, width, height, length: f.length,
      rows: [0, 479].map(y => Array.from(f.slice(y * width * 4, y * width * 4 + 16))),
      canvas: [0, 479].map(y => Array.from(canvas.getImageData(0, y, 4, 1).data)) };
  }, { bytes: [...bytes], base, count, pixels });
}
export async function videoTransactions(page) {
  await vdp1Vbe(page);
  const eraseLatch = await rom("vdp1-erase-latch");
  for (const [a,b,left,right,write] of [[0x801F,0x83E0,0x0402,0x0803,1],
    [0xFC00,0xFFFF,0x0202,0x0603,1],[0x801F,0x83E0,0x0402,0x0803,0]]) {
    const image=Buffer.from(eraseLatch);
    [a,b,left,right,write].forEach((word,i)=>image.writeUInt16BE(word,0x100+i*2));
    // Independently authored ST-013 ch.4/§4.4 literals, shared guest bytes
    // with native tests. The host only loads the ROM and inspects results.
    const first=[208*0x8000+32*a,0x8000,a,a,0x8000,a,0x8000,0x8000,0x8000];
    const second=!write?first:left===0x0402?
      [208*0x8000+32*b,0x8000,0x8000,b,b,b,b,0x8000,0x8000]:
      [208*0x8000+32*b,0x8000,0x8000,b,0x8000,b,0x8000,0x8000,0x8000];
    const expected=[...first,...second,0x1010,0,0,0,0x600D];
    const rows=Array.from({length:6},(_,y)=>Array.from({length:40},(_,x)=>{
      const inside=!write?x>=8&&x<24&&y>=1&&y<3:
        x>=(left===0x0402?16:8)&&x<(left===0x0402?32:24)&&y>=2&&y<4;
      return !inside?[0,0,0,255]:!write?[255,0,0,255]:b===0x83E0?[0,255,0,255]:[255,255,255,255];
    }).flat());
    for(let repeat=0;repeat<2;repeat++) {
      const result=await page.evaluate(async image=>{
        const {emulator,advanceFrame}=await import("./app.js");
        emulator.set_pal(false);emulator.reset_bios(Uint8Array.from(image));
        for(let i=0;i<100&&emulator.read_long(0x0600E758)!==0x600D;i++)emulator.run_steps(5000);
        const records=Array.from({length:23},(_,i)=>emulator.read_long(0x0600E700+i*4));
        advanceFrame();
        const f=emulator.framebuffer(),ctx=document.getElementById("screen").getContext("2d");
        return {records,exceptions:emulator.exception_count(),
          rows:Array.from({length:6},(_,y)=>Array.from(f.slice(y*320*4,y*320*4+160))),
          canvas:Array.from({length:6},(_,y)=>Array.from(ctx.getImageData(0,y,40,1).data))};
      },[...image]);
      assert.deepEqual(result.records,expected);assert.equal(result.exceptions,"0");
      assert.deepEqual(result.rows,rows);assert.deepEqual(result.canvas,rows);
    }
  }
  console.log("VDP1 erase capture passed: guest MMIO A/B sequence, physical-buffer checksums, literal framebuffer/Canvas pixels, color/coordinate changes, omitted writes and repeated cold boots.");
  const cycleSlots = await rom("vdp2-cycle-slots");
  for (const slot of [0,7]) for (let repeat=0;repeat<2;repeat++) for (const initial of [0x1111,0x3333]) {
    for (const [threshold,write] of [[826,true],[700,true],[826,false]]) {
      const image=Buffer.from(cycleSlots);
      image.writeUInt16BE(initial,0x100);image.writeUInt16BE(threshold,0x102);
      image.writeUInt16BE(Number(write),0x104);
      if(slot===7) {image.writeUInt16BE(0xEEEE,0x106);image.writeUInt16BE(0xEEE4,0x108);}
      const result=await page.evaluate(async image=>{
        const {emulator,advanceFrame}=await import("./app.js");
        emulator.reset_bios(Uint8Array.from(image));
        for(let i=0;i<100 && emulator.read_long(0x0600DF14)!==0x600D;i++)emulator.run_steps(5000);
        const records=Array.from({length:4},(_,i)=>emulator.read_long(0x0600DF08+i*4));
        // This is production presentation. The guest parks during active
        // display; advanceFrame completes that field and presents its pixels.
        advanceFrame();
        const f=emulator.framebuffer(),ctx=document.getElementById("screen").getContext("2d");
        return {records,exceptions:emulator.exception_count(),
          rows:[0,15,16,17,223].map(y=>Array.from(f.slice(y*320*4,(y+1)*320*4))),
          canvas:[0,15,16,17,223].map(y=>Array.from(ctx.getImageData(0,y,320,1).data))};
      },[...image]);
      const [h,v,word,done]=result.records;
      assert(h<842 && (threshold===700?h<822:h>822),`HCNT ${h}`);
      assert(v>=15 && v<=16);assert.equal(word,write?0x2222:initial);assert.equal(done,0x600D);
      assert.equal(result.exceptions,"0");
      const old=initial===0x1111?[255,0,0,255]:[0,0,255,255],green=[0,255,0,255];
      const rows=[0,15,16,17,223].map(y=>Array.from({length:320},()=>
        write && (y>16 || y===16 && threshold===700)?green:old).flat());
      assert.deepEqual(result.rows,rows);assert.deepEqual(result.canvas,rows);
    }
  }
  console.log("VDP2 cycle slots passed: pre/post-acquisition guest writes, source mutations, omitted writes, cold resets and production Canvas pixels.");
  const heldWord = await rom("vdp2-word-retention");
  for (const initial of [0x1111,0x1111,0x3333,0x3333]) {
    const image = Buffer.from(heldWord);image.writeUInt16BE(initial,0x100);
    const result = await page.evaluate(async image => {
      const {emulator} = await import("./app.js");
      emulator.reset_bios(Uint8Array.from(image));
      for(let i=0;i<100 && emulator.read_long(0x0600DF04)!==0x600D;i++)emulator.run_steps(5000);
      const f=emulator.framebuffer();
      return {done:emulator.read_long(0x0600DF04),exceptions:emulator.exception_count(),
        rows:[0,15,16,17,223].map(y=>Array.from(f.slice(y*320*4,(y+1)*320*4)))};
    },[...image]);
    const old=initial===0x1111?[255,0,0,255]:[0,0,255,255],green=[0,255,0,255];
    const row = color => Array.from({length:320},()=>color).flat();
    assert.deepEqual(result,{done:0x600D,exceptions:"0",rows:[old,old,old,green,green].map(row)});
  }
  const preclip = await rom("vdp1-preclip");
  for (const disable of [false, false, true, true]) {
    const image = Buffer.from(preclip);
    image.writeUInt16BE(disable ? 0x800 : 0, 0x100);
    const colors = disable ? [0,0,0,0] : [0x8003,0x8004,0x8005,0x8006];
    assert.deepEqual((await run(page,image,0x0600E600,9)).records,[...colors,0,2,12,0x2000,0x600D]);
  }
  const character=await rom("vdp1-character-fetch");
  for (const [texture,color] of [[0x2222,0x8012],[0x5555,0x8015]]) {
    const bytes=Buffer.from(character);bytes.writeUInt16BE(texture,0x100);
    const expected=[...Array(3).fill(0x8011),...Array(4).fill(color),...Array(4).fill(0x8014),
      2,12,0x2000,0x600D];
    for (let repeat=0;repeat<2;repeat++) {
      assert.deepEqual((await run(page,bytes,0x0600E500,15)).records,expected);
    }
  }
  console.log("VDP1 character fetch passed: already-fetched nibbles survive a guest texture write during scaling; the next command reloads both changed words, with operand mutations and cold resets in Wasm.");
  const lookup = await rom("vdp1-lookup-fetch");
  for (const [oldColor,newColor] of [[0x8000,0x83E0],[0xFC00,0x8010]]) {
    const bytes=Buffer.from(lookup);
    bytes.writeUInt16BE(oldColor,0x100);bytes.writeUInt16BE(newColor,0x102);
    const expected=[...Array(4).fill(oldColor+1),...Array.from({length:16},(_,i)=>newColor+i),
      2,12,0x2000,0x600D];
    for (let repeat=0;repeat<2;repeat++) {
      assert.deepEqual((await run(page,bytes,0x0600E400,24)).records,expected);
    }
  }
  console.log("VDP1 lookup fetch passed: guest palette writes during 504x4 scaling preserve the current table and change the next command, all sixteen entries, status/IRQ, operand mutations and cold resets in Wasm.");
  const effects = await rom("vdp1-effects-timing");
  let referenceEffectsTimes;
  for (let repeat = 0; repeat < 2; repeat++) {
    const times = [];
    for (const [mode, pixel] of [[0,0x8010],[1,0xBC00],[2,0x8008],
      [3,0xBC08],[4,0x8010],[6,0x8008],[7,0xBC08]]) {
      const bytes = Buffer.from(effects);
      bytes.writeUInt16BE(0xC0|mode,0x100);
      const {records:r} = await run(page,bytes,0x0600E300,8);
      assert.deepEqual(r.slice(1),[pixel,pixel,pixel,2,0x2000,68,0x600D]);
      times.push(r[0]);
    }
    assert.equal(times[0],times[2]);assert.equal(times[4],times[5]);
    for (const [a,b,min,max] of [[4,0,14,20],[1,0,78,84],[3,0,78,84],[6,4,78,84]]) {
      assert(times[a]-times[b]>=min && times[a]-times[b]<=max,`${times}`);
    }
    if (referenceEffectsTimes) assert.deepEqual(times,referenceEffectsTimes);
    else referenceEffectsTimes=times;
  }
  const changedEffects=Buffer.from(effects);
  changedEffects.writeUInt16BE(0x83E0,0x102);
  assert.deepEqual((await run(page,changedEffects,0x0600E300,8)).records.slice(1),
    [0x83E0,0x83E0,0x83E0,2,0x2000,68,0x600D]);
  console.log("VDP1 effects timing passed: guest FRT distinguishes Gouraud table fetches, replacement/half-luminance and six-clock shadow/translucency; pixels, status, draw IRQ, source mutation and cold resets in Wasm.");
  const commands = await rom("vdp1-command-fetch");
  for (const color of [0x801F, 0x83E0]) {
    for (let repeat = 0; repeat < 2; repeat++) {
      assert.deepEqual((await run(page, patch(commands, 0x801F, color), 0x0600E100, 5)).records,
        [color, color, 8, 2, 0x600D]);
    }
  }
  const beforeStart = Buffer.from(commands), needle = Buffer.from([0x24, 0x01, 0x25, 0x61]);
  const at = beforeStart.indexOf(needle);
  assert(at >= 0 && beforeStart.indexOf(needle, at + 1) < 0);
  beforeStart.set([0x25, 0x61, 0x24, 0x01], at);
  assert.deepEqual((await run(page, beforeStart, 0x0600E100, 5)).records, [0, 0, 0, 2, 0x600D]);
  const ownership = await rom("vdp1-memory-ownership");
  let referenceTimes;
  for (let repeat = 0; repeat < 2; repeat++) for (const color of [0x801F, 0x83E0]) {
    const times = [];
    for (const framebuffer of [false, true]) {
      let bytes = patch(ownership, 0x801F, color);
      if (!framebuffer) bytes = patch(bytes, 0x25C81000, 0x25C01000);
      const {records} = await run(page, bytes, 0x0600E200, 5);
      assert.deepEqual(records.slice(1), [color, color, 2, 0x600D]);
      assert(records[0] > 500 && records[0] < 2000);
      times.push(records[0]);
    }
    assert(times[1] > times[0] + 50, `VDP1 framebuffer/VRAM FRT intervals: ${times}`);
    if (!referenceTimes) referenceTimes = times; else assert.deepEqual(times, referenceTimes);
  }
  console.log("VDP1 memory ownership passed: guest post-fetch CTRL write, early-write negative case, framebuffer/VRAM FRT comparison, real pixels, color mutations and cold resets in production Wasm.");
  const draw = await rom("vdp1-progress");
  const expected = (await readFile("tests/fixtures/vdp1-progress.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v,16));
  for (let repeat = 0; repeat < 2; repeat++) assert.deepEqual((await run(page, draw, 0x0600D800,10)).records, expected);
  for (const [before,after,indices] of [[0x801F,0x83E0,[0]],[0x1234,0x2468,[6,7]]]) {
    const e = [...expected]; for (const i of indices) e[i] = after;
    assert.deepEqual((await run(page, patch(draw,before,after),0x0600D800,10)).records,e);
  }
  const monitor = await rom("exclusive-monitor");
  const colors = [255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255];
  for (const [mode,width] of [[0x8004,320],[0x8005,352],[0x8006,640],[0x8007,704]]) {
    for (let repeat = 0; repeat < 2; repeat++) {
      const r = await run(page,patch(monitor,0x8006,mode),0x0600DC00,5,true);
      assert.deepEqual(r.records,[8,0,480,mode,0x600D]);
      assert.deepEqual([r.width,r.height,r.length],[width,480,width*480*4]);
      assert.deepEqual(r.rows,[colors,colors]); assert.deepEqual(r.canvas,r.rows);
    }
  }
  const changed = await run(page,patch(monitor,0x25F80080,0x25F80084,2),0x0600DC00,5,true);
  assert.deepEqual(changed.rows[0],[255,0,0,255,0,0,0,255,0,0,255,255,0,0,0,255]);
  assert.deepEqual(changed.canvas,changed.rows);
  const grants = await rom("vram-grants");
  for (let repeat = 0; repeat < 2; repeat++) {
    const { records: r } = await run(page,grants,0x0600DE00,7);
    assert.equal(r[6],0x600D);
    for (const i of [0,2,4]) assert(r[i] > 0 && r[i] < 60, `${r}`);
    for (const i of [1,3,5]) assert(r[i] >= 80 && r[i] < 220, `${r}`);
  }
  const { records: released } = await run(page,patch(grants,0x25F80020,0x25F80022),0x0600DE00,7);
  assert.equal(released[6],0x600D);
  for (const value of released.slice(0,6)) assert(value > 0 && value < 60, `${released}`);
  const fetchRom = await rom("vdp2-fetch");
  const rows = [0,15,17,31,33,47,49,63,65,79,81,95,97,111,113,127,128,129,130,
    144,145,160,161,176,177,191,193,207,209,223];
  for (const changed of [false,false,true]) {
    const bytes = changed ? patch(fetchRom,0x801F,0x83E0,5) : fetchRom;
    const result = await page.evaluate(async ({bytes,rows}) => {
      const {emulator,advanceFrame} = await import("./app.js");
      emulator.reset_bios(Uint8Array.from(bytes));
      for (let i=0;i<500 && emulator.read_long(0x0600DF00)!==0x600D;i++) emulator.run_steps(500);
      if (emulator.read_long(0x0600DF00)!==0x600D) throw new Error("Display-fetch guest did not complete");
      advanceFrame();
      const f=emulator.framebuffer(),canvas=document.getElementById("screen").getContext("2d");
      return {exceptions:Number(emulator.exception_count()),
        pixels:rows.flatMap(y=>[159,319].map(x=>Array.from(f.slice((y*320+x)*4,(y*320+x)*4+4)))),
        canvas:rows.flatMap(y=>[159,319].map(x=>Array.from(canvas.getImageData(x,y,1,1).data)))};
    },{bytes:[...bytes],rows});
    const expected=rows.flatMap(y=>{
      const visible=!(y>=17&&y<=47 || y>=65&&y<=79 || y>=97&&y<=111 || y===129);
      const color=visible?(changed||y>=145&&y<=160||y>=193&&y<=207?[0,255,0,255]:[255,0,0,255]):[0,0,0,255];
      return [color,color];
    });
    assert.equal(result.exceptions,0);
    assert.deepEqual(result.pixels,expected);assert.deepEqual(result.canvas,expected);
  }
  console.log("Display fetch passed: guest bandwidth starvation/recovery, wrong-bank grants, split A0/A1, 8/16-bit read requirements, line-window and per-line coefficient capture, per-dot coefficient bank ownership, cold resets, pixel operand mutation and production Wasm/Canvas.");
  console.log("Video transactions passed: partial VDP1 dots/ENDR/status, bounded VBlank erase and operand changes; all four exclusive monitor modes, 480-line pixels/counters/Wasm/Canvas and NBG1 mutation; split VRAM grants, HBlank stalls, ignored high-resolution slots and disabled-screen release.");
}
