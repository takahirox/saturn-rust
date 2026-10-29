// Original MIT firmware + media, executed by the production Wasm and local UI.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function externalStartup(page) {
  const rom=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/external-startup.hex","utf8")).trim().split(/\r?\n/)) {
    if(line.startsWith("#"))continue;
    const [at,...words]=line.split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((word,i)=>rom.writeUInt16BE(word,at+i*2));
  }
  const image=await readFile("tests/fixtures/external-startup.iso");
  const requests=[];
  const observe=request=>requests.push({method:request.method(),body:request.postData()});
  page.on("request",observe);
  const expected=[0x06004004,0x060FE000,0x06000000,48,1,1,1,0x5353,0x8000,2048,0x41000096,0xB0000009];
  const trace=[
    [0x0401,0,0,0,0x0180,0x4101,0x0100,150],
    [0,0,0,0,0x0180,0x4101,0x0100,150],
    [0x0200,0,0,0,0x4180,204,0,0],
    [0x0600,0,0,0,0x0100,204,0x0100,150],
    [0x3000,0,0,0,0x0180,0x4101,0x0100,150],
    [0x1080,171,0x0080,2,0x0400,0x4101,0x0100,171],
    [0x5100,0,0,0,0x0180,0,0,2],
    [0x6300,0,0,2,0x4180,0x4101,0x0100,173],
    [0x0600,0,0,0,0x0100,2048,0x0100,173],
  ].flat();
  const run=async()=>page.evaluate(async()=>{
    const {emulator:m,advanceFrame}=await import("./app.js");
    for(let i=0;i<200 && m.read_long(0x060100FC)===0;i++)m.run_steps(10000);
    advanceFrame(); advanceFrame();
    return {records:Array.from({length:12},(_,i)=>m.read_long(0x06010000+i*4)),
      trace:Array.from({length:72},(_,i)=>m.read_long(0x06010400+i*4)),
      marker:m.read_long(0x060100FC),mode:m.mode(),exceptions:m.exception_count(),
      smpc:[0x248,0x25C,0x264,0x260,0x218].map(off=>m.read_long(0x06010000+off)),
      hle:m.read_long(0x06000320),pixel:[...document.getElementById("screen").getContext("2d").getImageData(0,0,1,1).data]};
  });
  // Both orders of actual local File selections; Restart/Reset retain both.
  for(const discFirst of [false,true]) {
    await page.locator("#mode").selectOption("hle");
    await page.locator("#mode").selectOption("bios");
    const loadRom=()=>page.locator("#bios").setInputFiles({name:"original-bios.bin",mimeType:"application/octet-stream",buffer:rom});
    const loadDisc=()=>page.locator("#disc").setInputFiles({name:"original-disc.iso",mimeType:"application/octet-stream",buffer:image});
    if(discFirst) {await loadDisc();await loadRom();} else {await loadRom();await loadDisc();}
    await page.waitForFunction(()=>document.getElementById("error").textContent==="");
    for(let boot=0;boot<2;boot++) {
      const got=await run();
      assert.deepEqual(got,{records:expected,trace,marker:0x600D,mode:"User-provided BIOS",exceptions:"3",smpc:[2,7,9,8,0x35],hle:0,pixel:[0,255,0,255]},`local order ${discFirst}, boot ${boot}`);
      if(boot===0)await page.locator(discFirst?"#restart":"#reset").click();
    }
  }
  for(let control=0;control<6;control++) {
    const bytes=Buffer.from(image),firmware=Buffer.from(rom);
    if(control===1)bytes[21*2048+0xF03]=8;
    if(control===2)bytes[21*2048]^=1;
    if(control===3) {
      let count=0;
      for(let at=0x200;at<firmware.length-4;at+=2)if(firmware.readUInt32BE(at)===0x3000){firmware.writeUInt32BE(0x3100,at);count++;}
      assert.equal(count,1);
    }
    if(control===4)firmware.writeUInt32BE(1,0x7F00);
    const got=await page.evaluate(async({rom,image,control})=>{
      const {Emulator}=await import("./pkg/saturn_rust.js");const m=new Emulator();
      m.reset_bios(Uint8Array.from(rom));
      if(control!==0)m.mount_disc(Uint8Array.from(image));
      if(control===5) {
        try{m.reset_bios(new Uint8Array(8));}catch(error){return {error:String(error),mode:m.mode(),marker:m.read_long(0x060100FC)};}
        throw new Error("short BIOS accepted");
      }
      for(let i=0;i<200 && m.read_long(0x060100FC)===0;i++)m.run_steps(10000);
      return {marker:m.read_long(0x060100FC),entry:m.read_long(0x06010000),backup:m.export_backup_ram()[0],mode:m.mode()};
    },{rom:[...firmware],image:[...bytes],control});
    if(control===5){assert.match(got.error,/512 KiB/);assert.equal(got.marker,0);assert.equal(got.mode,"User-provided BIOS");}
    else assert.deepEqual(got,{marker:control===4?0x600D:control===1||control===2?0xE002:0xE001,entry:control===4?0x06004004:0,backup:control===4?0x5A:0,mode:"User-provided BIOS"},`negative/reset control ${control}`);
  }
  // The existing HLE path still installs its own services; no external ROM survives.
  const star=await readFile("web/star-collector.iso");
  const hle=await page.evaluate(async image=>{
    const {emulator:m}=await import("./app.js");m.reset_disc(Uint8Array.from(image));m.run_steps(10000);
    return {mode:m.mode(),service:m.read_long(0x06000320)};
  },[...star]);
  assert.equal(hle.mode,"BIOS-less HLE disc");assert(hle.service>0);
  assert(requests.every(r=>r.method==="GET" && r.body===null),"local images must not be uploaded");
  assert.equal(await page.evaluate(()=>localStorage.length+sessionStorage.length),0);
  page.off("request",observe);
  console.log("External startup passed: both local selection orders, reset retention, literal CD/IRQ/DMA/entry records, Canvas, sector/command/no-media/invalid-ROM controls, guest SYSRES and HLE isolation.");
}
