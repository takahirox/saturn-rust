// Play UI acceptance uses only repository-authored media and firmware.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function playUi(browser, url) {
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    const requests = [], errors = [];
    page.on("request", r => requests.push({url:r.url(), method:r.method(), body:r.postData()}));
    page.on("pageerror", error => errors.push(error.message));
    await page.goto(url);
    await page.waitForFunction(() => !document.getElementById("controls").disabled);
    assert.equal(await page.title(), "Saturn Disc Player");
    assert.equal(await page.locator("#mode").inputValue(), "disc");
    assert(await page.locator("#empty-state").isVisible());
    assert(await page.locator("#start").isDisabled());
    assert(await page.locator("#pause").isHidden());
    assert(await page.locator("#restart").isDisabled());
    assert.equal(await page.locator("#error").textContent(), "");
    assert.equal(await page.locator(".setup-steps > li").count(), 4);
    assert.equal(await page.locator("#audio-status").textContent(), "Audio: disabled");
    assert.equal(requests.some(r => /\.(iso|chd|bin)$/.test(r.url)), false);
    const idle = await page.evaluate(async () => {
      const {emulator} = await import("./app.js");
      return [String(emulator.frames()), String(emulator.instruction_count())];
    });
    assert.deepEqual(idle, ["0", "0"]);
    const iso = await readFile("web/hardware-lab.iso");
    const audio = await readFile("web/hardware-lab-audio.bin");
    const cue = await readFile("web/hardware-lab.cue");
    const files = [
      {name:"hardware-lab.cue", bytes:[...cue]},
      {name:"hardware-lab.iso", bytes:[...iso]},
      {name:"hardware-lab-audio.bin", bytes:[...audio]},
    ];
    const drop = selection => page.evaluate(selection => {
      const transfer = new DataTransfer();
      for (const file of selection) transfer.items.add(new File([new Uint8Array(file.bytes)], file.name));
      const zone = document.getElementById("disc-drop");
      zone.dispatchEvent(new DragEvent("dragover", {bubbles:true, cancelable:true, dataTransfer:transfer}));
      zone.dispatchEvent(new DragEvent("drop", {bubbles:true, cancelable:true, dataTransfer:transfer}));
    }, selection);
    const ready = () => page.waitForFunction(() => !document.getElementById("start").disabled);
    const failed = pattern => page.waitForFunction(pattern => new RegExp(pattern).test(document.getElementById("error").textContent), pattern);
    const rom = async name => {
      const bytes = Buffer.alloc(524288);
      for (const line of (await readFile(`tests/fixtures/${name}.hex`, "utf8")).trim().split(/\r?\n/)) {
        if (line.startsWith("#")) continue;
        const [offset, ...words] = line.split(/\s+/).map(word => parseInt(word,16));
        words.forEach((word,i) => bytes.writeUInt16BE(word,offset+i*2));
      }
      return bytes;
    };
    const bios = await rom("bios-only");
    const firmware = {name:"original-loop.bin",mimeType:"application/octet-stream",buffer:bios};
    await context.setOffline(true);
    // No firmware, wrong-sized firmware and rejected local reads cannot launch.
    await page.locator("#mode").selectOption("bios");
    assert(await page.locator("#start").isDisabled());
    await page.locator("#bios").setInputFiles({...firmware,buffer:Buffer.alloc(1)});
    await failed("received 1 bytes");
    assert(await page.locator("#start").isDisabled());
    await page.evaluate(() => {
      const original = File.prototype.arrayBuffer;
      File.prototype.arrayBuffer = function () {
        if (this.name === "rejected.bin") return Promise.reject(new Error("test read rejection"));
        if (this.name !== "pending.bin") return original.call(this);
        return new Promise(resolve => { window.finishBiosRead = async () => resolve(await original.call(this)); });
      };
      window.restoreBiosRead = () => { File.prototype.arrayBuffer = original; };
    });
    await page.locator("#bios").setInputFiles({...firmware,name:"rejected.bin"});
    await failed("test read rejection");
    assert(await page.locator("#start").isDisabled());
    await page.locator("#reset").evaluate(button => button.click());
    assert(await page.locator("#start").isDisabled());
    assert.match(await page.locator("#error").textContent(), /test read rejection/);
    // Cancelling a pending BIOS read cannot revive it in either mode.
    await page.locator("#bios").setInputFiles({...firmware,name:"pending.bin"});
    assert(await page.locator("#start").isDisabled());
    assert.match(await page.locator("#player-status").textContent(), /Loading image/);
    await page.locator("#mode").selectOption("disc");
    await page.evaluate(async () => { await window.finishBiosRead(); window.restoreBiosRead(); });
    assert(await page.locator("#start").isDisabled());
    await page.locator("#mode").selectOption("bios");
    assert(await page.locator("#start").isDisabled());
    await page.locator("#bios").setInputFiles(firmware);
    await ready();
    assert.match(await page.locator("#player-status").textContent(), /BIOS ready/);
    assert.match(await page.locator("#launch-status").textContent(), /Disc optional/);
    assert.match(await page.locator("#status").textContent(), /Resetting/);
    await page.evaluate(async () => { window.biosMachine = (await import("./app.js")).emulator; });
    const state = () => page.evaluate(() => ({
      count:window.biosMachine.instruction_count(), counter:window.biosMachine.read_long(0x06001000),
      frames:window.biosMachine.frames(), mode:window.biosMachine.mode(),
    }));
    assert.deepEqual(await state(), {count:"0", counter:0, frames:"0", mode:"User-provided BIOS"});
    await page.locator("#start").click();
    await page.waitForFunction(() => window.biosMachine.read_long(0x06001000) > 0);
    assert.match(await page.locator("#player-status").textContent(), /Running/);
    assert(await page.locator("#empty-state").isHidden());
    await page.locator("#pause").click();
    const paused = await state();
    assert(Number(paused.count) > 100);
    assert(Number(paused.frames) > 0);
    await page.waitForTimeout(50);
    assert.deepEqual(await state(), paused, "Pause stops guest execution");
    await page.locator("#pause").click();
    await page.waitForFunction(count => Number(window.biosMachine.instruction_count()) > Number(count), paused.count);
    // Observe the synchronous UI reset before its next animation frame executes.
    const restarted = await page.locator("#restart").evaluate(button => {
      button.click();
      return [window.biosMachine.instruction_count(), window.biosMachine.read_long(0x06001000), window.biosMachine.frames()];
    });
    assert.deepEqual(restarted, ["0",0,"0"]);
    await page.waitForFunction(() => window.biosMachine.read_long(0x06001000) > 0);
    await page.locator("#view-debug").click();
    assert(Number(await page.locator("#instructions").textContent()) > 100);
    await page.locator("#reset").click();
    assert.equal(await page.locator("#pause").textContent(), "Resume");
    assert.equal((await state()).count, "0");
    await page.locator("#step").click();
    assert.equal((await state()).count, "1");
    await page.locator("#frame").click();
    assert(Number((await state()).frames) > 0);
    assert.equal(await page.locator("#disc").evaluate(input => input.files.length), 0);
    assert.equal(await page.locator("#disc-selection").textContent(), "No disc selected.");
    // A separate existing MIT firmware queries real CD hardware and records its
    // no-media result. Start, rather than host stepping, must reach that result.
    await page.locator("#bios").setInputFiles({...firmware,buffer:await rom("external-startup")});
    await ready();
    await page.locator("#view-play").click();
    await page.locator("#start").click();
    await page.waitForFunction(() => window.biosMachine.read_long(0x060100FC) === 0xE001);
    assert.equal(await page.locator("#error").textContent(), "");
    assert.equal(await page.locator("#disc-selection").textContent(), "No disc selected.");
    // A sleeping guest with no wake source can still be paused. Resume must
    // remain disabled honestly until a reset makes the CPU executable again.
    const sleeping = Buffer.from(bios);
    sleeping.writeUInt16BE(0x001B,0x100); // SLEEP, with reset SR masking IRQs
    await page.locator("#bios").setInputFiles({...firmware,buffer:sleeping});
    await ready();
    await page.locator("#start").click();
    await page.waitForFunction(() => document.getElementById("status").textContent.startsWith("Sleeping"));
    assert.match(await page.locator("#player-status").textContent(), /Sleeping/);
    assert(await page.locator("#pause").isEnabled());
    await page.locator("#pause").click();
    assert(await page.locator("#pause").isDisabled());
    assert.match(await page.locator("#launch-status").textContent(), /Sleeping/);
    assert.doesNotMatch(await page.locator("#player-status").textContent(), /Load a disc/);
    // Faults are reported honestly and cannot enable Start or Resume.
    const fault = Buffer.from(bios);
    fault.writeUInt16BE(0xD001,0x100); fault.writeUInt16BE(0x2000,0x102);
    fault.writeUInt32BE(0x05FE00CC,0x108);
    await page.locator("#bios").setInputFiles({...firmware,buffer:fault});
    await ready();
    await page.locator("#start").click();
    await failed("Unsupported Saturn device/register");
    assert.match(await page.locator("#status").textContent(), /Faulted/);
    assert(await page.locator("#pause").isDisabled());
    await page.locator("#bios").setInputFiles(firmware);
    await ready();
    // Invalid optional media must stay rejected through Reset and mode changes.
    await page.locator("#disc").setInputFiles({name:"invalid.iso",mimeType:"application/octet-stream",buffer:Buffer.alloc(1)});
    await failed("complete 2048-byte-sector ISO");
    assert(await page.locator("#start").isDisabled());
    await page.locator("#reset").evaluate(button => button.click());
    assert(await page.locator("#start").isDisabled());
    await page.locator("#mode").selectOption("disc");
    await page.locator("#mode").selectOption("bios");
    assert(await page.locator("#start").isDisabled());
    await page.locator("#disc").setInputFiles([]);
    await ready();
    assert.equal(await page.locator("#error").textContent(), "");
    // This is a valid medium with deliberately unsupported HLE boot metadata.
    // Returning to BIOS must reuse it without asking for another file read.
    const noHle = Buffer.from(iso);
    noHle[0] = 0;
    await page.locator("#disc").setInputFiles({name:"no-hle.iso",mimeType:"application/octet-stream",buffer:noHle});
    await ready();
    const mounted = await page.locator("#disc-selection").textContent();
    assert.match(mounted, /Loaded locally/);
    for (let attempt = 0; attempt < 2; attempt++) {
      await page.locator("#mode").selectOption("disc");
      await failed("Saturn");
      assert(await page.locator("#start").isDisabled());
      assert.equal(await page.locator("#disc-selection").textContent(), mounted);
      await page.locator("#mode").selectOption("bios");
      await ready();
      assert.equal(await page.locator("#error").textContent(), "");
      assert.equal(await page.locator("#disc-selection").textContent(), mounted);
    }
    await page.locator("#mode").selectOption("disc");
    await drop(files.slice(0,2));
    await failed("Missing CUE file reference");
    assert(await page.locator("#start").isDisabled());
    await drop(files);
    await ready();
    const selected = await page.locator("#disc-selection").textContent();
    for (const file of files) assert(selected.includes(file.name));
    assert(selected.includes("Loaded locally"));
    assert(await page.locator("#pause").isHidden());
    // Step 2 works before Step 3: selecting optional firmware does not lose disc.
    await page.locator("#bios").setInputFiles(firmware);
    await ready();
    assert.match(await page.locator("#bios-selection").textContent(), /unused in HLE/);
    await page.locator("#mode").selectOption("bios");
    await ready();
    assert.equal(await page.locator("#bios").evaluate(input => input.files[0].name), firmware.name);
    assert.equal(await page.locator("#disc-selection").textContent(), selected);
    await page.locator("#start").click();
    await page.waitForFunction(() => document.getElementById("pause").textContent === "Pause");
    assert(await page.locator("#start").isHidden());
    await page.locator("#pause").click();
    assert.match(await page.locator("#player-status").textContent(), /Paused/);
    await page.locator("#view-debug").click();
    assert.match(await page.locator("#active-mode").textContent(), /User-provided BIOS/);
    await page.locator("#view-play").click();
    assert.equal(await page.locator("#pause").textContent(), "Resume");
    await page.locator("#restart").click();
    await page.waitForFunction(() => document.getElementById("pause").textContent === "Pause");
    // Visibility pauses and releases input, without a later automatic resume.
    await page.evaluate(() => {
      Object.defineProperty(document, "hidden", {configurable:true,value:true});
      document.dispatchEvent(new Event("visibilitychange"));
      delete document.hidden;
      document.dispatchEvent(new Event("visibilitychange"));
    });
    assert.equal(await page.locator("#pause").textContent(), "Resume");
    await page.locator("#mode").selectOption("disc");
    await ready();
    assert.equal(await page.locator("#disc-selection").textContent(), selected);
    // Raw BIN alone is rejected. A valid CHD replaces it via the same picker.
    await page.locator("#disc").setInputFiles({name:"raw.bin",mimeType:"application/octet-stream",buffer:audio});
    await failed("lone raw BIN");
    assert(await page.locator("#start").isDisabled());
    const chd = await readFile("tests/fixtures/chd/mixed-cdlz.chd");
    await page.locator("#disc").setInputFiles({name:"local.chd",mimeType:"application/octet-stream",buffer:chd});
    await ready();
    assert.match(await page.locator("#disc-selection").textContent(), /local.chd/);
    // A replacement cancels a slow read, including after the new image validates.
    await page.evaluate(() => {
      const original = File.prototype.arrayBuffer;
      File.prototype.arrayBuffer = function () {
        if (this.name !== "slow.iso") return original.call(this);
        return new Promise(resolve => { window.finishSlowUi = async () => resolve(await original.call(this)); });
      };
      window.restoreUiRead = () => { File.prototype.arrayBuffer = original; };
    });
    await page.locator("#disc").setInputFiles({name:"slow.iso",mimeType:"application/octet-stream",buffer:iso});
    assert(await page.locator("#start").isDisabled());
    await page.locator("#disc").setInputFiles({name:"replacement.iso",mimeType:"application/octet-stream",buffer:iso});
    await ready();
    await page.evaluate(async () => { await window.finishSlowUi(); window.restoreUiRead(); });
    assert.match(await page.locator("#disc-selection").textContent(), /replacement.iso/);
    assert(await page.locator("#pause").isHidden());
    // BIOS mode never falls back to HLE after missing/invalid firmware.
    await page.locator("#mode").selectOption("bios");
    await page.locator("#bios").setInputFiles({name:"invalid.bin",mimeType:"application/octet-stream",buffer:Buffer.alloc(1)});
    await failed("received 1 bytes");
    assert(await page.locator("#start").isDisabled());
    await page.locator("#bios").setInputFiles(firmware);
    await ready();
    await page.locator("#mode").selectOption("disc");
    await ready();
    // Issue #99: the unchanged MIT Hardware Lab CUE references lower-case
    // names. The picker and drop must both boot the original bytes under
    // different ASCII spelling, without a sidecar, converter or request.
    const caseFiles = files.map((file, i) => ({...file,
      name: i === 1 ? "HARDWARE-LAB.ISO" : i === 2 ? "Hardware-Lab-Audio.BIN" : file.name,
    }));
    const picker = selection => page.locator("#disc").setInputFiles(selection.map(file => ({
      name:file.name, mimeType:"application/octet-stream", buffer:Buffer.from(file.bytes),
    })));
    const guest = () => page.evaluate(async () => {
      const {emulator:m, advanceFrame} = await import("./app.js");
      for (let i=0; i<40 && m.read_long(0x06010020)!==0x600D; i++) m.run_steps(10000);
      m.advance_frame();
      const pcm = Array.from(m.audio_samples(8192));
      advanceFrame();
      const frame = m.framebuffer();
      const canvas = document.getElementById("screen").getContext("2d");
      return {fields:Array.from({length:11},(_,i)=>m.read_long(0x06010000+i*4)),
        mode:m.mode(), stereo:pcm.some((v,i)=>i%2===0 && v!==pcm[i+1]),
        wasm:Array.from(frame.slice(0,4)), canvas:Array.from(canvas.getImageData(0,0,1,1).data)};
    });
    const checkGuest = async () => {
      const result = await guest();
      assert.deepEqual(result.fields.slice(0,6), [1,1,84,35,42,393984]);
      assert.equal(result.fields[8], 0x600D);
      assert.equal(result.fields[10], 0x900);
      assert.equal(result.mode, "BIOS-less HLE disc");
      assert(result.stereo);
      assert.deepEqual(result.wasm, [255,255,255,255]);
      assert.deepEqual(result.canvas, result.wasm);
    };
    const beforeCaseRequests = requests.length;
    for (const select of [picker, drop]) {
      await select(caseFiles);
      await ready();
      assert.equal(await page.locator("#error").textContent(), "");
      assert.match(await page.locator("#disc-selection").textContent(), /Hardware-Lab-Audio.BIN/);
      await checkGuest();
      // Retained-medium reset needs no selected bytes to be sent again.
      await page.locator("#restart").click();
      await ready();
      await checkGuest();
      // No exact winner: two ASCII candidates are explicitly ambiguous.
      await select([...caseFiles, {...caseFiles[2], name:"HARDWARE-LAB-AUDIO.BIN"}]);
      await failed("Ambiguous CUE file reference");
      assert(await page.locator("#start").isDisabled());
      await select(caseFiles.slice(0,2));
      await failed("Missing CUE file reference");
      assert(await page.locator("#start").isDisabled());
      // An exact track wins over an earlier invalid case-fold candidate.
      const wrongAudio = {...caseFiles[2], bytes:[0]};
      await select([...caseFiles.slice(0,2), wrongAudio, files[2]]);
      await ready();
      await checkGuest();
    }
    assert.equal(requests.length, beforeCaseRequests, "Local CUE resolution must make no network requests");
    // Disc-first layout: setup precedes display on narrow screens; every control
    // remains in bounds and the idle display is 4:3 without changing its pixels.
    for (const [width,height] of [[1440,1000],[1024,900],[768,1024],[390,844],[320,700]]) {
      await page.setViewportSize({width,height});
      const layout = await page.evaluate(() => {
        const screen = document.getElementById("screen").getBoundingClientRect();
        const setup = document.getElementById("setup").getBoundingClientRect();
        return {overflow:document.documentElement.scrollWidth > innerWidth, ratio:screen.width/screen.height,
          before:setup.bottom <= screen.top, beside:setup.left >= screen.right};
      });
      assert.equal(layout.overflow,false,`overflow at ${width}`);
      assert(Math.abs(layout.ratio-4/3)<0.005);
      assert(width <= 900 ? layout.before : layout.beside);
      for (const id of ["disc","bios","mode","start","fullscreen","view-debug"]) {
        const box = await page.locator(`#${id}`).boundingBox();
        assert(box.x >= 0 && box.x+box.width <= width && box.height >= 44, `${id} at ${width}`);
      }
    }
    assert.deepEqual(errors,[]);
    assert(requests.every(r => r.url.startsWith(url) && r.method === "GET" && r.body === null));
  } finally { await context.close(); }
}
