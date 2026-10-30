// Original MIT synthetic firmware, exercised through the production Wasm API.
// Literal layouts/oracles are independent of the assembler and emulator parser.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { rawMode1 } from "./raw-media-fixture.mjs";

const discs = [
  {name:"external-startup",extent:21,size:4096,destination:0x06004000,stack:0x060FE000,aip:0x06002E20,root:20,rootSize:2048,record:68,sectors:[0,1,16,17,20,21,22]},
  {name:"external-startup-moved",extent:35,size:4093,destination:0x06030000,stack:0x060FC000,aip:0x06002E40,root:27,rootSize:4096,record:92,sectors:[0,1,16,17,18,27,28,35,36]},
  {name:"external-startup-long",extent:48,size:6145,destination:0x06050000,stack:0x060FA000,aip:0x06002E80,root:31,rootSize:4096,record:116,sectors:[0,1,16,17,31,32,48,49,50,51]},
];
function decode(source) {
  const rom=Buffer.alloc(524288);
  for(const line of source.trim().split(/\r?\n/)) {
    if(line.startsWith("#"))continue;
    const [at,...words]=line.split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((word,i)=>rom.writeUInt16BE(word,at+i*2));
  }
  return rom;
}
function trace(sectors) {
  return [
    [0x0401,0,0,0,0x0180,0x4101,0x0100,150],
    [0,0,0,0,0x0180,0x4101,0x0100,150],
    [0x0200,0,0,0,0x4180,204,0,0],
    [0x0600,0,0,0,0x0100,204,0x0100,150],
    [0x3000,0,0,0,0x0180,0x4101,0x0100,150],
    ...sectors.flatMap(lba=>[
      [0x1080,lba+150,0x80,1,0x0400,0x4101,0x0100,lba+150],
      [0x5100,0,0,0,0x0180,0,0,1],
      [0x6300,0,0,1,0x4180,0x4101,0x0100,lba+151],
      [0x0600,0,0,0,0x0100,1024,0x0100,lba+151],
    ]),
  ].flat();
}
const dual32=(bytes,at,value)=>{bytes.writeUInt32LE(value,at);bytes.writeUInt32BE(value,at+4);};
function patch(rom,before,after) {
  let count=0;
  for(let at=0x200;at<rom.length-4;at+=2)if(rom.readUInt32BE(at)===before){rom.writeUInt32BE(after,at);count++;}
  assert.equal(count,1);
}
export async function externalDiscovery(page) {
  const rom=decode(await readFile("tests/fixtures/external-startup.hex","utf8"));
  const images=await Promise.all(discs.map(d=>readFile(`tests/fixtures/${d.name}.iso`)));
  const run=async (image,disc,{firmware=rom,raw=false,restart=false,chunk=10000,inspect=false,fixedSteps=0,resetPhase=0}={})=>page.evaluate(async args=>{
    const {Emulator}=await import("./pkg/saturn_rust.js");
    const m=new Emulator();
    try {
      const rom=Uint8Array.from(args.rom);
      m.reset_bios(rom);
      if(args.raw)m.mount_cue('FILE "d" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00',"d",new Uint32Array([args.image.length]),Uint8Array.from(args.image));
      else m.mount_disc(Uint8Array.from(args.image));
      if(args.resetPhase) {
        let reached=false;
        for(let i=0;i<1000000;i++) {
          m.step();
          if(m.read_long(0x0601032C)===args.resetPhase && (args.resetPhase!==4 || m.read_long(0x06010328)===2048) && (m.read_word(0x25890008)&1)===0){reached=true;break;}
        }
        if(!reached)throw new Error("pending phase not reached");
        if(m.read_long(0x060102F0)!==0)throw new Error("early AIP entry");
        m.reset_bios(rom);
        if(m.read_long(0x06010328)!==0 || m.read_long(0x06004000)!==0)throw new Error("stale reset data");
      }
      const advance=()=>{
        let left=args.fixedSteps || 2000000;
        while(left>0) {
          const n=Math.min(left,args.chunk);m.run_steps(n);left-=n;
          if(args.inspect){m.read_long(0x25818000);m.read_word(0x25890024);}
          if(!args.fixedSteps && m.read_long(0x060100FC)!==0)break;
        }
      };
      advance();
      if(args.restart){m.reset_bios(rom);advance();}
      const words=(address,count)=>Array.from({length:count},(_,i)=>m.read_long(address+i*4));
      const bytes=(address,count)=>Array.from({length:count},(_,i)=>m.read_long((address+i)&~3) >>> (24-((address+i)%4)*8) &255);
      return {marker:m.read_long(0x060100FC),records:words(0x06010000,12),metadata:words(0x06010300,15),
        aip:m.read_long(0x060102F0),slaveStack:m.read_long(0x060102F4),exceptions:m.exception_count(),mode:m.mode(),hle:m.read_long(0x06000320),
        trace:words(0x06010400,(5+4*args.disc.sectors.length)*8),file:bytes(args.disc.destination,args.disc.size),
        tail:bytes(args.disc.destination+args.disc.size,4),ip:bytes(0x06002000,4096),
        fifo:m.read_long(0x25818000),hirq:m.read_word(0x25890008)&0x82,
        dma:words(0x25FE0000,2),slaveRegisters:Array.from(m.slave_registers()),
        ...(args.fixedSteps?{clocks:m.peripheral_clocks(),registers:Array.from(m.registers()),special:Array.from(m.special_registers()),
          instructions:m.instruction_count(),slaveInstructions:m.slave_instruction_count(),slavePc:m.slave_pc(),log:words(0x06010000,0x1400/4)}:{})};
    } finally {m.free();}
  },{rom:[...firmware],image:[...image],disc,raw,restart,chunk,inspect,fixedSteps,resetPhase});
  function success(got,disc,image) {
    assert.equal(got.marker,0x600D);
    assert.deepEqual(got.records,[disc.destination+4,disc.stack,0x06000000,48,1,disc.sectors.length,1,0x5353,0x8000,1024,0x41000096,0xB0000009]);
    assert.equal(got.aip,disc.aip+4);
    assert.equal(got.slaveStack,0x06001000);assert.equal(got.slaveRegisters[15],0x06001000);
    assert.equal(got.exceptions,String(disc.sectors.length+2));
    assert.equal(got.mode,"User-provided BIOS");assert.equal(got.hle,0);
    assert.deepEqual(got.file,[...image.subarray(disc.extent*2048,disc.extent*2048+disc.size)],"every executable byte");
    assert.deepEqual(got.tail,[0,0,0,0],"do not copy final sector padding");
    assert.deepEqual(got.ip,[...image.subarray(0,4096)],"actual on-disc AIP and IP");
    assert.deepEqual(got.trace,trace(disc.sectors));
    assert.deepEqual(got.metadata,[4096,disc.aip,disc.stack,0x06001000,disc.destination,64,disc.root,disc.rootSize,disc.extent,disc.size,disc.size,5,disc.sectors.length,0,disc.name.endsWith("moved")?19:18]);
    assert.equal(got.hirq,0x80);assert.equal(got.fifo,0xFFFFFFFF);
    assert.deepEqual(got.dma,[0x05818000,0x06017000]);
  }
  for(const [i,disc] of discs.entries()) {
    const image=images[i];
    for(const restart of [false,true])success(await run(image,disc,{restart}),disc,image);
    let reference;
    for(const chunk of [1,3,17,10000]) for(const inspect of [false,true]) {
      const got=await run(image,disc,{chunk,inspect,fixedSteps:1000000});success(got,disc,image);
      if(reference)assert.deepEqual(got,reference,"chunk/inspection must not change state");reference=got;
    }
    // Relocate again; mutate an actually fetched MOV #4,R3 into MOV #3,R3.
    const changed=Buffer.from(image),payload=Buffer.from(image.subarray(disc.extent*2048,disc.extent*2048+disc.size));
    changed.fill(0,disc.extent*2048,(disc.extent+Math.ceil(disc.size/2048))*2048);
    payload.copy(changed,40*2048);dual32(changed,disc.root*2048+disc.record+2,40);
    const at=payload.indexOf(Buffer.from([0xE0,0,0xE3,4]));assert(at>=0);changed[40*2048+at+3]=3;
    const got=await run(changed,disc);
    assert.equal(got.marker,0xBAD);assert.equal(got.records[3],31);assert.equal(got.metadata[8],40);
    assert.deepEqual(got.file,[...changed.subarray(40*2048,40*2048+disc.size)]);
  }
  for(let control=0;control<47;control++) {
    let image=Buffer.from(images[0]);const pvd=16*2048,root=20*2048,file=root+68;
    switch(control) {
      case 0:image[0]=88;break;
      case 1:image[0x40]=32;break;
      case 2:image[0x41]=85;break;
      case 3:image[0x42]=74;break;
      case 4:image.write("JTUEJ",0x40);break;
      case 5:image[0x40]=90;break;
      case 6:image.writeUInt32BE(0xFFC,0xE0);break;
      case 7:image.writeUInt32BE(0x8004,0xE0);break;
      case 8:image.writeUInt32BE(0x1001,0xE0);break;
      case 9:image.writeUInt32BE(0x06010000,0xE8);break;
      case 10:image.writeUInt32BE(0x060FE002,0xEC);break;
      case 11:image.writeUInt32BE(0x06002000,0xF0);break;
      case 12:image.writeUInt32BE(0x06010000,0xF0);break;
      case 13:image.writeUInt32BE(0xFFFFFFFC,0xF0);break;
      case 14:image[pvd+1]=88;break;
      case 15:image[pvd+6]=2;break;
      case 16:dual32(image,pvd+80,65);break;
      case 17:image[pvd+84]=1;break;
      case 18:image[pvd+128]=1;break;
      case 19:image[pvd+156]=33;break;
      case 20:image[pvd+189]=1;break;
      case 21:dual32(image,pvd+158,16);break;
      case 22:dual32(image,pvd+166,0);break;
      case 23:dual32(image,pvd+166,18432);break;
      case 24:image[root+33]=1;break;
      case 25:image[root+67]=0;break;
      case 26:image[file]=33;break;
      case 27:image[file+32]=255;break;
      case 28:image[file+25]=2;break;
      case 29:image.fill(0,file,root+2048);break;
      case 30:dual32(image,file+10,0);break;
      case 31:dual32(image,file+10,65537);break;
      case 32:dual32(image,file+2,64);break;
      case 33:dual32(image,file+2,20);break;
      case 34:image[file+1]=1;break;
      case 35:image[file+26]=1;break;
      case 36:image[file+27]=1;break;
      case 37:image[file+25]=0x80;break;
      case 38:image[file+25]=4;break;
      case 39:image[0xE4]=1;break;
      case 40:image[0xD0]=1;break;
      case 41:image[0xF8]=1;break;
      case 42:image[file]=45;break;
      case 43:image[file+28]=2;break;
      case 44:image[root+40]=1;break;
      case 45:dual32(image,pvd+80,18);break;
      case 46:image=image.subarray(0,24*2048);break;
    }
    const got=await run(image,discs[0]);
    assert.equal(got.marker,0xE002,`metadata ${control}`);assert.equal(got.aip,0);assert.equal(got.records[0],0);assert.equal(got.metadata[10],0);
  }
  for(const [before,after] of [[0x3000,0x3100],[0x1080,0x1180],[0x6300,0x6100]]) {
    const firmware=Buffer.from(rom);patch(firmware,before,after);
    const got=await run(images[0],discs[0],{firmware});assert.equal(got.marker,0xE003);assert.equal(got.aip,0);assert.equal(got.records[0],0);
  }
  for(const [name,opcode] of [["abort",0x7500],["read-reset",0x0401]]) {
    const firmware=decode(await readFile(`tests/fixtures/external-startup-${name}.hex`,"utf8"));
    const got=await run(images[0],discs[0],{firmware});
    assert.equal(got.marker,0xE003);assert.equal(got.aip,0);assert.equal(got.records[0],0);assert.equal(got.metadata[10],0);
    assert.equal(got.trace[5*8],0x1080);assert.equal(got.trace[5*8+4],0x0400);assert.equal(got.trace[6*8],opcode);
  }
  const raw=rawMode1(images[0]);raw.fill(0xB7,22*2352+16,22*2352+2064);
  const failedRead=await run(raw,discs[0],{raw:true});
  assert.equal(failedRead.marker,0xE003);assert.equal(failedRead.metadata[10],2048);assert.equal(failedRead.aip,0);assert.equal(failedRead.records[0],0);
  for(const resetPhase of [2,3,4])success(await run(images[0],discs[0],{resetPhase}),discs[0],images[0]);
  // First-read identity is directory order, not GAME.BIN spelling. IP defaults,
  // explicit Stack-S and a selected non-program must affect actual execution.
  for(let control=0;control<3;control++) {
    const image=Buffer.from(images[0]);image.write("BOOT.BIN;1",20*2048+101);
    if(control===0){image.writeUInt32BE(0,0xE8);const at=image.indexOf(Buffer.from([6,15,224,0]),21*2048);assert(at<23*2048);image.writeUInt32BE(0x06002000,at);}
    if(control===1)image.writeUInt32BE(0x060EE000,0xEC);
    if(control===2){dual32(image,20*2048+70,23);dual32(image,20*2048+78,(await readFile("tests/fixtures/LICENSE")).length);}
    const got=await run(image,discs[0]);assert.equal(got.marker,control===2?0xE004:0x600D);
    if(control===0)assert.equal(got.records[1],0x06002000);
    if(control===1)assert.equal(got.slaveRegisters[15],0x060EE000);
    if(control===2){assert.equal(got.records[0],0);assert.equal(got.metadata[8],23);}
  }
  for(const children of [0,1,2]) {
  const disabled=Buffer.from(images[0]);disabled.writeUInt32BE(0,0xF0);if(children===0)disabled.fill(0,20*2048+68,21*2048);
  if(children===2)disabled.fill(0,20*2048,21*2048);
  [0xD103,0xD004,0x2102,0x001B,0xAFFE,0x0009,0x0009,0x0009,0x2601,0x00FC,0,0x600D].forEach((word,i)=>disabled.writeUInt16BE(word,0xE20+i*2));
  const noFirstRead=await run(disabled,discs[0]);assert.equal(noFirstRead.marker,children===2?0xE002:0x600D);assert.equal(noFirstRead.metadata[10],0);assert(noFirstRead.file.every(b=>b===0));assert.equal(noFirstRead.metadata[12],5);
  }
  // Same independently validated moved-file negative control as inherited main:
  // frozen ROM first succeeds on its original disc, then fails solely on layout.
  const fixed=decode(await readFile("tests/fixtures/external-startup-fixed.hex","utf8"));
  const original=await readFile("tests/fixtures/external-startup-fixed.iso");
  assert.equal((await run(original,discs[0],{firmware:fixed})).marker,0x600D);
  const differentBios=await run(images[0],discs[0],{firmware:fixed});assert.equal(differentBios.marker,0xE002);assert.equal(differentBios.records[0],0);
  const moved=Buffer.from(original),program=Buffer.from(original.subarray(21*2048,21*2048+4093));
  moved.fill(0,21*2048,23*2048);program.copy(moved,35*2048);
  dual32(moved,20*2048+70,35);dual32(moved,20*2048+78,4093);
  const inherited=await run(moved,discs[0],{firmware:fixed});
  assert.equal(inherited.marker,0xE002);assert.equal(inherited.records[0],0);assert(inherited.file.every(b=>b===0));
  assert.equal(inherited.trace[5*8+1],171,"inherited ROM still reads fixed FAD171");
  console.log("External discovery passed: same ROM, 3 layouts, full executable/IP bytes and literal hardware records, chunk/inspection equivalence, 47 metadata controls, actual instruction/extent mutations, decode failure, cancellation/reset and inherited-main control.");
}
