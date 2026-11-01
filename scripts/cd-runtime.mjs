// Original MIT runtime reader on the production Wasm, after #85 ROM discovery.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { rawMode1 } from "./raw-media-fixture.mjs";

const O=0x06070000;
function firmware(source) {
  const rom=Buffer.alloc(524288);
  for(const line of source.trim().split(/\r?\n/)) {
    if(line.startsWith("#"))continue;
    const [at,...words]=line.split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((w,i)=>rom.writeUInt16BE(w,at+i*2));
  }
  return rom;
}
function assets(layout) {
  const [a,b]=layout===0?[5003,2051]:[4096,6145];
  return [Buffer.from(Array.from({length:a},(_,i)=>(i*17+3+(i>>>8))&255)),
    Buffer.from(Array.from({length:b},(_,i)=>((i*29+7)^(i>>>4))&255))];
}
const dual32=(image,at,value)=>{image.writeUInt32LE(value,at);image.writeUInt32BE(value,at+4);};
export async function cdRuntime(page) {
  const rom=firmware(await readFile("tests/fixtures/external-startup.hex","utf8"));
  const images=await Promise.all(["cd-runtime","cd-runtime-moved"].map(n=>readFile(`tests/fixtures/${n}.iso`)));
  const executable=await readFile("tests/fixtures/cd-runtime.bin");
  const requests=[];
  const observe=request=>requests.push({method:request.method(),body:request.postData()});
  page.on("request",observe);
  const run=(image,{bios=rom,raw=false,restart=false,sysres=false,chunk=10000,fixed=0,inspect=false,draw=false,resetTransfer=false,noMedia=false}={})=>page.evaluate(async args=>{
    const {emulator:m,advanceFrame}=await import("./app.js");
    const O=0x06070000,rom=Uint8Array.from(args.bios);
    m.reset_bios(rom);
    if(args.noMedia)m.clear_media();
    else if(args.raw)m.mount_cue('FILE "d" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00',"d",new Uint32Array([args.image.length]),Uint8Array.from(args.image));
    else m.mount_disc(Uint8Array.from(args.image));
    if(args.sysres)m.import_backup_ram(new Uint8Array(32768).fill(0xA5));
    const advance=()=>{
      let left=args.fixed||6000000;
      while(left>0) {
        const n=Math.min(left,args.chunk);m.run_steps(n);left-=n;
        if(args.inspect){m.read_long(0x25818000);m.read_word(0x25890024);}
        if(!args.fixed && (m.read_long(O+0xFC)!==0 || m.read_long(0x060100FC)!==0))return;
      }
    };
    if(args.resetTransfer) {
      let reached=false;
      for(let i=0;i<1600000;i++) {
        m.step();
        if(m.read_long(O+4)===2 && (m.read_word(0x25890008)&2)!==0){reached=true;break;}
      }
      if(!reached)throw new Error("runtime FIFO boundary not reached");
      m.reset_bios(rom);
      if(m.read_long(O)!==0 || m.read_long(0x25818000)!==0xFFFFFFFF)throw new Error("stale runtime reset bytes");
    }
    advance();
    if(args.restart){m.reset_bios(rom);advance();}
    const words=(at,n)=>Array.from({length:n},(_,i)=>m.read_long(at+i*4));
    const bytes=(at,n)=>Array.from({length:n},(_,i)=>(m.read_long((at+i)&~3)>>>(24-((at+i)%4)*8))&255);
    const end=m.read_long(O+0x28)&0x1FFFFFFF;
    const result={records:words(O,0x100/4),bootError:m.read_long(0x060100FC),aip:m.read_long(0x060102F0),
      loaded:m.read_long(0x06010328),program:bytes(0x06020000,args.executableLength),
      handles:words(O+0x800,14),buffers:bytes(0x06080000,65536),
      mode:m.mode(),hle:m.read_long(0x06000320),fifo:m.read_long(0x25818000),hirq:m.read_word(0x25890008)&0x82,
      dma:words(0x25FE0000,2),irq:[m.read_long(0x06010204),m.read_long(0x06010208),m.read_long(0x25FE00A4)&0x10800],
      trace:words(0x06072000,end>=0x06072000&&end<0x06074000?(end-0x06072000)/4:0),
      ...(args.sysres?{backup:Array.from(m.export_backup_ram())}:{}),
      ...(args.fixed?{clocks:m.peripheral_clocks(),registers:Array.from(m.registers()),special:Array.from(m.special_registers()),
        instructions:m.instruction_count(),slaveRegisters:Array.from(m.slave_registers()),log:words(O,0x4000/4)}:{})};
    if(args.draw) {
      advanceFrame();advanceFrame();
      result.canvas=Array.from(document.getElementById("screen").getContext("2d").getImageData(0,0,320,224).data);
    }
    return result;
  },{image:[...image],bios:[...bios],raw,restart,sysres,chunk,fixed,inspect,draw,resetTransfer,noMedia,executableLength:executable.length});
  function success(got,layout,[a,b]) {
    const read=offset=>got.records[offset/4];
    assert.equal(got.bootError,0);assert.equal(read(0xFC),0x600D);
    assert.equal(read(0),0x06020004);assert.equal(read(4),3);assert.equal(got.aip,0x06002E24);
    assert.equal(got.loaded,executable.length);assert.deepEqual(Buffer.from(got.program),executable);
    assert.equal(got.mode,"User-provided BIOS");assert.equal(got.hle,0);
    assert.equal(read(12),0xFFFFFFFF);assert.equal(read(16),layout===0?3:4);
    assert.deepEqual([read(0x18),read(0x1C)],[2,3],"primary interrupted-GET count relation");
    assert.deepEqual([read(0x20),read(0x24)],[a.length,b.length]);
    assert.deepEqual([read(0x60),read(0x64),read(0x68)],[0xFFFFFFFF,0,0xFFFFFFFF]);
    const counts=[3,2050,Math.min(2057,a.length-2046),7,a.length,13,b.length];
    assert.deepEqual(got.records.slice(0x40/4,0x40/4+7),counts);
    const expected=Buffer.alloc(65536,0xA5);
    for(const [slot,data] of [[0,a.subarray(0,3)],[1,b.subarray(0,2050)],[2,a.subarray(2046,2046+counts[2])],
      [3,b.subarray(1,8)],[4,a],[5,a.subarray(0,13)],[6,b]])data.copy(expected,4+slot*0x2000);
    assert.deepEqual(Buffer.from(got.buffers),expected,"complete bytes, tails and neighboring RAM");
    const sum=[...a,...b].reduce((s,v)=>s+v,0);assert.equal(read(8),sum);
    const handles=layout===0?[[2,64,a.length,13,1,1],[3,72,b.length,b.length,1,2]]:
      [[3,80,a.length,13,1,1],[2,96,b.length,b.length,1,2]];
    assert.deepEqual(got.handles.slice(0,6),handles[0]);assert.deepEqual(got.handles.slice(8,14),handles[1]);
    assert.equal(got.fifo,0xFFFFFFFF);assert.equal(got.hirq,0x80);
    assert.deepEqual(got.dma,[0x05818000,layout===0?0x06060000:0x06060800]);assert.deepEqual(got.irq,[16+layout,1,0]);
    const trace=Array.from({length:got.trace.length/8},(_,i)=>got.trace.slice(i*8,i*8+8));
    assert.deepEqual(trace.slice(0,5),[
      [0,0,0,0,0x180,0x4101,0x100,191],[0x100,0,0,0,0x180,2,0,0],
      [0x400,0xFFFF,0,0x201,0x180,0x4101,0x100,191],[0x480,0xFFFF,0,0xFFFF,0x180,0x4101,0x100,191],
      [0x400,0xFFFF,0,0x600,0xFF00,0x4101,0x100,191]]);
    assert(trace.some(r=>r[0]===0x7400 && r[2]===0x1700 && r[4]===0xFF00));
    assert(trace.some(r=>r[0]===0x7500 && (r[4]&0x8000)===0));
    assert(trace.every(r=>r[4]!==0x8180));
    if(got.canvas) {
      const c=shift=>{const v=(sum>>>shift)&31;return (v<<3)|(v>>>2);};
      const pixel=[c(0),c(5),c(10),255];
      assert.deepEqual(Buffer.from(got.canvas),Buffer.from(Array.from({length:320*224*4},(_,i)=>pixel[i%4])),"entire actual Canvas influenced by runtime assets");
    }
  }
  try {
    for(const [layout,image] of images.entries()) {
      success(await run(image,{draw:true}),layout,assets(layout));
      success(await run(image,{restart:true}),layout,assets(layout));
      let reference;
      for(const chunk of [1,3,17,10000]) {
        const got=await run(image,{chunk,inspect:chunk!==10000,fixed:2600000});success(got,layout,assets(layout));
        if(reference)assert.deepEqual(got,reference,"chunking and passive inspection");reference=got;
      }
    }
    const bios=Buffer.from(rom);bios.writeUInt32BE(1,0x7F00);
    const sysres=await run(images[1],{bios,sysres:true});success(sysres,1,assets(1));
    assert.equal(sysres.backup[0],0x5A);assert(sysres.backup.slice(1).every(v=>v===0xA5));
    success(await run(images[0],{resetTransfer:true}),0,assets(0));
    for(let control=0;control<5;control++) {
      const image=Buffer.from(images[0]);
      if(control===0)image[48*2048+101]=88;
      if(control===1)image[48*2048+68]=33;
      if(control===2)image.fill(0,48*2048+68,49*2048);
      if(control===3)dual32(image,48*2048+70,127);
      if(control===4)image[20*2048+147]=88;
      const got=await run(image);
      assert.equal(got.bootError,0);assert.equal(got.records[0],0x06020004);
      assert.equal(got.records[0xFC/4],[0xE090,0xE092,0xE090,0xE092,0xE093][control]);
      assert(got.buffers.every(v=>v===0xA5));assert.equal(got.handles[3],0);
    }
    const raw=rawMode1(images[0]);raw.fill(0xB7,65*2352+16,65*2352+2064);
    const bad=await run(raw,{raw:true});assert.equal(bad.records[0xFC/4],0xE092);assert.equal(bad.records[1],2);
    assert.equal(bad.bootError,0);assert(bad.buffers.every(v=>v===0xA5));assert.equal(bad.handles[3],0);
    const empty=await run(images[0],{noMedia:true});assert.equal(empty.bootError,0xE001);assert.equal(empty.records[0],0);
    const changed=Buffer.from(images[0]);changed[64*2048]^=0x1F;
    const expected=assets(0);expected[0][0]^=0x1F;
    success(await run(changed,{draw:true}),0,expected);
    success(await run(images[0]),0,assets(0));
    assert(requests.every(r=>r.method==="GET" && r.body===null),"local firmware/disc bytes are never uploaded");
  } finally {page.off("request",observe);}
  console.log("Runtime CD passed: 2 layouts/same application, all bytes/handles/canaries, interrupted GET, Canvas, chunks/inspection, abort, errors, reset and SYSRES.");
}
