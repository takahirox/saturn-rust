// Original MIT streaming application on production Wasm/Canvas; no host file API.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
const O=0x06070000;
function firmware(source){const rom=Buffer.alloc(524288);for(const line of source.trim().split(/\r?\n/)){if(line.startsWith("#"))continue;const [at,...words]=line.split(/\s+/).map(v=>parseInt(v,16));words.forEach((w,i)=>rom.writeUInt16BE(w,at+i*2));}return rom;}
function payload(layout){const n=layout?16387:12291,seed=layout?73:11;return Buffer.from(Array.from({length:n},(_,i)=>((i*31+seed)^(i>>>7)^(Math.floor(i/2048)*19))&255));}
function pixel(sum){const c=shift=>{const n=(sum>>>shift)&31;return(n<<3)|(n>>>2);};return[c(0),c(5),c(10),255];}
export async function cdStream(page){
 const rom=firmware(await readFile("tests/fixtures/external-startup.hex","utf8"));
 const images=await Promise.all(["cd-stream","cd-stream-moved"].map(n=>readFile(`tests/fixtures/${n}.iso`)));
 const executable=await readFile("tests/fixtures/cd-stream.bin");
 const requests=[];const observe=r=>requests.push({method:r.method(),body:r.postData()});page.on("request",observe);
 const run=(image,{chunk=10000,fixed=0,inspect=false,milestone=false,resetTransfer=false}={})=>page.evaluate(async args=>{
  const {emulator:m,advanceFrame}=await import("./app.js");
  const O=0x06070000,rom=Uint8Array.from(args.rom);m.reset_bios(rom);m.mount_disc(Uint8Array.from(args.image));
  const words=(at,n)=>Array.from({length:n},(_,i)=>m.read_long(at+i*4));
  const bytes=(at,n)=>Array.from({length:n},(_,i)=>(m.read_long((at+i)&~3)>>>(24-((at+i)%4)*8))&255);
  let intermediate=null;
  if(args.resetTransfer){let reached=false;for(let i=0;i<3000000;i++){m.step();if(m.read_long(O+4)===3&&(m.read_long(O+0x20)&12)===12&&(m.read_word(0x25890008)&2)!==0&&m.read_long(0x25818000)!==m.read_long(0x06080004)&&m.read_long(0x06010204)<m.read_long(O+0x40)){reached=true;break;}}if(!reached)throw new Error("slow FIFO not reached");m.reset_bios(rom);if(m.read_long(O)!==0||m.read_long(0x25818000)!==0xFFFFFFFF)throw new Error("stale reset publication");}
  if(args.milestone){let reached=false;for(let i=0;i<3000;i++){m.run_steps(1000);if(m.read_long(0x06071060)!==0){reached=true;break;}}if(!reached)throw new Error("guest computation milestone missing");
   const before=m.instruction_count();advanceFrame();advanceFrame();
   intermediate={phase:m.read_long(O+4),hirq:m.read_word(0x25890008),instructions:m.instruction_count()-before,frames:m.frames(),
    color:m.read_word(0x25E00000),canvas:Array.from(document.getElementById("screen").getContext("2d").getImageData(0,0,320,224).data)};
  }
  let left=args.fixed||8000000;while(left>0){const n=Math.min(left,args.chunk);m.run_steps(n);left-=n;if(args.inspect){m.read_long(0x25818000);m.read_word(0x25890024);}if(!args.fixed&&(m.read_long(O+0xFC)!==0||m.read_long(0x060100FC)!==0))break;}
  const end=m.read_long(O+0x24)&0x1FFFFFFF,commandEnd=m.read_long(O+0x2c)&0x1FFFFFFF;
  const result={fields:words(O,64),bootError:m.read_long(0x060100FC),mode:m.mode(),hle:m.read_long(0x06000320),aip:m.read_long(0x060102f0),
   program:bytes(0x06020000,args.codeLength),buffers:bytes(0x06080000,65536),fifo:m.read_long(0x25818000),
   records:words(0x06071000,end>=0x06071000&&end<0x06072000?(end-0x06071000)/4:0),
   commands:words(0x06074000,commandEnd>=0x06074000&&commandEnd<0x06078000?(commandEnd-0x06074000)/4:0),
   dma:words(0x25fe0000,2),irq:[m.read_long(0x06010204),m.read_long(0x06010208),m.read_long(0x25fe00a4)&0x10800],intermediate,
   ...(args.fixed?{clocks:m.peripheral_clocks(),registers:Array.from(m.registers()),special:Array.from(m.special_registers()),instructions:m.instruction_count(),slave:Array.from(m.slave_registers()),log:words(O,0x8000/4)}:{})};
  advanceFrame();advanceFrame();result.canvas=Array.from(document.getElementById("screen").getContext("2d").getImageData(0,0,320,224).data);return result;
 },{image:[...image],rom:[...rom],codeLength:executable.length,chunk,fixed,inspect,milestone,resetTransfer});
 function success(got,layout,data){
  const f=got.fields,n=Math.ceil(data.length/2048),sum=[...data].reduce((a,b)=>a+b,0);
  assert.equal(got.bootError,0);assert.equal(f[63],0x600D,`phase ${f[1]}, flags ${f[8].toString(16)}`);assert.equal(got.mode,"User-provided BIOS");assert.equal(got.hle,0);
  assert.equal(f[0],0x06020004);assert.equal(got.aip,0x06002e24);assert.deepEqual(Buffer.from(got.program),executable);
  assert.deepEqual(f.slice(2,7),[sum,sum,layout?88:64,data.length,n]);assert.equal(f[8]&12,12);assert.equal(f[20]&512,512);
  const expected=Buffer.alloc(65536,0xA5);for(const at of [4,0x8004]){data.copy(expected,at);expected[at+data.length]=0;}
  assert.deepEqual(Buffer.from(got.buffers),expected,"complete DMA bytes, partial final word, output canaries");
  assert.equal(got.records.length,n*2*16);
  for(let pass=0;pass<2;pass++)for(let i=0;i<n;i++){
   const r=got.records.slice((pass*n+i)*16,(pass*n+i+1)*16),length=i+1===n?4:2048;
   assert.equal(r[0],i);assert.equal(r[2],(layout?238:214)+i);assert(r[3]>=1&&r[3]<=(pass?2:n));
   assert.notEqual(r[5],r[7]);assert(r[9]>=6&&r[9]<100,`cached FRT ${r[9]}`);
   assert.equal(r[8],[...data.subarray(0,i*2048)].reduce((a,b)=>a+b,0));
   assert.equal(r[10],length===4?3:1024);assert.equal(r[11],0x06080004+pass*0x8000+i*2048+length);assert.equal(r[12]&0x82,0x80);assert.equal(r[13],r[4]+1);assert.equal(r[14],r[11]-length);assert.equal(r[15],length);
  }
  assert(got.records.slice(16,(n-1)*16).some((v,i,a)=>i%16===8&&v!==0&&(a[i-7]&16)===0));
  assert.deepEqual(got.irq,[13+2*n,1,0],"exact native-equivalent DMA/CD acknowledgments");
  assert.equal(f[18],16*[...data.subarray(0,(n-2)*2048)].reduce((a,b)=>a+b,0),"cached kernel uses prior transferred bytes");
  assert.equal(got.fifo,0xFFFFFFFF);assert.deepEqual(got.dma,[0x05818000,0x06088008+(n-1)*2048]);
  assert.deepEqual(Buffer.from(got.canvas),Buffer.from(Array.from({length:320*224*4},(_,i)=>pixel(sum)[i%4])),"all production Canvas pixels");
  const commands=Array.from({length:got.commands.length/8},(_,i)=>got.commands.slice(i*8,i*8+8));
  assert(commands.some(r=>r[0]===0x7500));assert(commands.some(r=>r[0]===0x7400&&r[4]===0xFF00));
  assert(commands.every(r=>r[4]!==0xFF00||r[0]===0x7400));assert(commands.every(r=>r[4]!==0x8180));
 }
 try{
  for(let layout=0;layout<2;layout++){const got=await run(images[layout],{milestone:layout===0,resetTransfer:layout===1});success(got,layout,payload(layout));
   if(got.intermediate){const mid=got.intermediate;assert.equal(mid.phase,2);assert.equal(mid.hirq&16,0);assert(mid.instructions>0);assert(Number(mid.frames)>0);
    const data=payload(0),colors=new Set();for(let i=1;i<7;i++)for(const end of [i*2048,(i-1)*2048+16])colors.add(pixel([...data.subarray(0,end)].reduce((a,b)=>a+b,0)).join(","));
    assert(mid.canvas.some((_,i,a)=>i%4===0&&colors.has(a.slice(i,i+4).join(","))),"real pixels changed from previously DMA-fetched bytes while later sectors remain in flight");
   }
  }
  const changed=Buffer.from(images[0]);changed[64*2048]^=0x1F;const data=payload(0);data[0]^=0x1F;success(await run(changed),0,data);
  const regular=await run(images[0],{fixed:2000000});success(regular,0,payload(0));
  assert.deepEqual(await run(images[0],{fixed:2000000,chunk:17,inspect:true}),regular,"same clock/CPU/RAM/pixels with passive reads and different chunks");
 }finally{page.off("request",observe);}
 assert.equal(requests.filter(r=>r.method!=="GET"||r.body!==null).length,0,"local firmware/media never uploaded");
}
