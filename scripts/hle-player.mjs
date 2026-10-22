// Original SYS_EXECDMP caller, real local CUE/Wasm/Canvas/WebAudio path.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function hlePlayer(page, url) {
  const [iso, cue, audio] = await Promise.all([
    readFile("tests/fixtures/hle-player.iso"), readFile("tests/fixtures/hle-player.cue"),
    readFile("tests/fixtures/hle-player-audio.bin"),
  ]);
  await page.goto(url);
  await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  await page.locator("#view-debug").click();
  await page.locator("#settings > summary").click();
  await page.locator("#mode").selectOption("disc");
  const files = track => [
    {name:"hle-player.iso",mimeType:"application/octet-stream",buffer:iso},
    {name:"hle-player.cue",mimeType:"text/plain",buffer:cue},
    {name:"hle-player-audio.bin",mimeType:"application/octet-stream",buffer:track},
  ];
  const ready = async expected => {
    await page.waitForFunction(()=>document.getElementById("status").textContent==="Ready"&&document.getElementById("pc").textContent==="0x00000100");
    const result = await page.evaluate(async expected => {
      const {emulator}=await import("./app.js");
      const state=()=>[0,4,8].map(offset=>emulator.read_long(0x06000F00+offset));
      for(let i=0;i<100;i++) {
        emulator.advance_frame();
        if(state().every((v,j)=>v===expected[j])) {
          for(let k=0;k<3;k++)emulator.advance_frame();
          return {state:state(),returned:emulator.read_long(0x06009010),
            power:emulator.read_long(0x06000C00),backup:emulator.export_backup_ram()[0]};
        }
      }
      throw new Error(`SYS_EXECDMP startup failed: ${state()}, PC=${emulator.pc()}`);
    },expected);
    assert.deepEqual(result,{state:expected,returned:0,power:0x13579BDF,backup:42});
  };
  const frames = (count, buttons=null) => page.evaluate(async ({count,buttons}) => {
    const {emulator}=await import("./app.js");
    if(buttons!==null)emulator.set_buttons(buttons);
    for(let i=0;i<count;i++)emulator.advance_frame();
  },{count,buttons});
  const key = async (name,count=4) => {
    await page.locator("#screen").focus();
    await page.keyboard.down(name);await frames(count);await page.keyboard.up(name);await frames(2);
  };
  const image = async (track,playing) => {
    const result=await page.evaluate(async()=>{
      const {emulator,advanceFrame}=await import("./app.js");
      advanceFrame();
      const frame=emulator.framebuffer(),canvas=document.getElementById("screen").getContext("2d");
      const points=[[32,32],[40,32],[24,96],[32,100],[36,100],[38,100],[41,123],[43,123],[44,123],[279,127]];
      return {state:[0,4,8].map(o=>emulator.read_long(0x06000F00+o)),
        wasm:points.map(([x,y])=>Array.from(frame.slice((y*320+x)*4,(y*320+x)*4+4))),
        canvas:points.map(([x,y])=>Array.from(canvas.getImageData(x,y,1,1).data)),
        size:[emulator.video_width(),emulator.video_height()],returned:emulator.read_long(0x06009010)};
    });
    const expected=[[255,255,255,255],[8,16,16,255],[33,33,33,255]];
    const cursor=track===0?[255,0,0,255]:playing?[0,255,0,255]:[255,255,0,255];
    for(const x of [32,36,38,41,43,44])expected.push(x>=32+track*2&&x<38+track*2?cursor:[33,33,33,255]);
    expected.push([33,33,33,255]);
    assert.deepEqual(result.state,[track,Number(playing),track===0?1:3]);
    assert.deepEqual(result.size,[320,224]);assert.equal(result.returned,0);
    assert.deepEqual(result.wasm,expected);assert.deepEqual(result.canvas,expected);
  };
  const signal = async expected => {
    const result=await page.evaluate(async expected=>{
      const {emulator}=await import("./app.js");
      const {AudioOutput}=await import("./audio.js");
      // The production Canvas presenter drains its audio producer even while
      // paused. Generate the next guest buffers for the independent consumer.
      emulator.advance_frame();emulator.advance_frame();
      const pcm=emulator.audio_samples(8192);
      let nonzero=0;
      for(let i=0;i<pcm.length;i+=2) {
        if(pcm[i]!==0||pcm[i+1]!==0) {
          if(pcm[i]!==expected[0]||pcm[i+1]!==expected[1])throw new Error(`Unexpected player stereo: ${pcm[i]},${pcm[i+1]}`);
          nonzero++;
        }
      }
      const output=new AudioOutput();
      try {
        await output.enable();output.setPaused(false);output.deliver(pcm);
        for(let i=0;i<200;i++) {
          if(output.stats.rendered>128&&output.stats.peak>0)break;
          await new Promise(resolve=>setTimeout(resolve,25));
        }
        return {nonzero,stats:{...output.stats}};
      } finally {await output.context?.close();}
    },expected);
    assert(result.nonzero>128);assert(result.stats.rendered>128);
    assert(Math.abs(result.stats.peak-Math.max(...expected.map(Math.abs))/32768)<0.00001,JSON.stringify(result));
    assert.notEqual(result.stats.state,"blocked");
  };
  await page.locator("#disc").setInputFiles(files(audio));
  await ready([2,0,3]);
  await page.locator("#settings > summary").click();
  await image(2,false);
  await key("z");await image(2,true);await signal([4096,-8192]);
  await frames(4,16);await frames(2,0);await image(2,false);
  await page.evaluate(async()=>{const {emulator}=await import("./app.js");emulator.audio_samples(8192);});
  await frames(3);
  assert(await page.evaluate(async()=>{const {emulator}=await import("./app.js");return Array.from(emulator.audio_samples(8192)).every(v=>v===0);}));
  await key("Enter");await signal([4096,-8192]);
  await key("ArrowRight",8);await image(3,false); // one selection per press
  await page.evaluate(async()=>{const {emulator}=await import("./app.js");emulator.audio_samples(8192);});
  await key("z");await image(3,true);await signal([-12288,16384]);
  await key("ArrowLeft");await image(2,false);
  await key("ArrowLeft");await image(3,false); // wrap skips data track1
  await key("z",75);await image(3,false); // real end, then restart
  await page.evaluate(async()=>{const {emulator}=await import("./app.js");emulator.audio_samples(8192);});
  await key("z");await signal([-12288,16384]);
  await page.locator("#settings > summary").click();
  await page.locator("#reset").click();await ready([2,0,3]);await image(2,false);
  await page.locator("#settings > summary").click();
  // Actual browser Gamepad mapping supplies ID15/ID16 SMPC reports; the menu
  // consumes those timed packets, including analog axis edges and A/B bits.
  await page.locator("#screen").focus();
  for(const mapping of ["","standard"]) {
    await page.evaluate(async mapping=>{
      window.playerPad={connected:true,mapping,axes:[1,0],buttons:Array.from({length:16},()=>({pressed:false,value:0}))};
      Object.defineProperty(navigator,"getGamepads",{configurable:true,value:()=>window.playerPad?[window.playerPad]:[]});
      const {pollGamepads}=await import("./app.js");pollGamepads();
    },mapping);
    const axis=async x=>page.evaluate(async x=>{window.playerPad.axes[0]=x;const {pollGamepads}=await import("./app.js");pollGamepads();},x);
    const button=async index=>page.evaluate(async index=>{
      window.playerPad.buttons.forEach((button,i)=>{button.pressed=i===index;button.value=Number(i===index);});
      const {pollGamepads}=await import("./app.js");pollGamepads();
    },index);
    await frames(6);await image(3,false);await frames(4);await image(3,false);
    await axis(0);await frames(3);await axis(-1);await frames(6);await image(2,false);
    await axis(0);await frames(3);
    await page.evaluate(async()=>{const {emulator}=await import("./app.js");emulator.audio_samples(8192);});
    await button(0);await frames(4);await signal([4096,-8192]);
    await button(1);await frames(4);await image(2,false);
  }
  await page.evaluate(async()=>{window.playerPad=null;const {pollGamepads}=await import("./app.js");pollGamepads();});
  await page.locator("#settings > summary").click();
  const changed=Buffer.from(audio);
  for(let i=0;i<44100;i++){changed.writeInt16LE(8192,i*4);changed.writeInt16LE(-16384,i*4+2);}
  await page.locator("#disc").setInputFiles(files(changed));
  await ready([2,0,3]);
  await page.locator("#settings > summary").click();
  await key("z");await signal([8192,-16384]);
  await page.locator("#settings > summary").click();
  await page.locator("#disc").setInputFiles({name:"hle-player.iso",mimeType:"application/octet-stream",buffer:iso});
  await ready([0,0,1]);
  await page.locator("#settings > summary").click();
  for(const button of ["z","ArrowLeft","ArrowRight","Enter"])await key(button);
  await image(0,false);
  console.log("SYS_EXECDMP guest player passed: local CUE, unfinished INTBACK/FIFO, TOC selection, Canvas, keyboard/analog Gamepad packets, exact CD-DA/WebAudio, completion/restart, reset, changed audio and data-only media");
}
