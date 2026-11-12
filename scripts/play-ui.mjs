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
    assert.equal(await page.locator("#mode").count(), 0);
    assert(await page.locator("#empty-state").isVisible());
    assert(await page.locator("#start").isDisabled());
    assert(await page.locator("#pause").isHidden());
    assert(await page.locator("#restart").isDisabled());
    assert.equal(await page.locator("#error").textContent(), "");
    assert.equal(await page.locator(".setup-steps > li").count(), 3);
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
    await automaticBootSelections(page, firmware, iso);
    await context.setOffline(true);
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
    // #105/#109/#111: BIOS-only Start/Restart executes documented pre-write,
    // idle SF=0 and legacy DSTP firmware, checking actual guest results offline.
    for (const [name, base] of [
      ["smpc-sf-direct", 0x0601a000], ["smpc-sf-activity", 0x0601a000],
      ["smpc-idle-zero", 0x0601b000], ["smpc-idle-prewrite", 0x0601b000],
      ["scu-dstp-access", 0x0600d000],
    ]) {
      const expected = (await readFile(`tests/fixtures/${name}.tsv`, "utf8")).trim().split(/\r?\n/).map(v => parseInt(v,16));
      await page.locator("#bios").setInputFiles({...firmware,buffer:await rom(name)});
      await ready();
      await page.locator("#view-play").click();
      const resetState = () => page.evaluate(base => [window.biosMachine.instruction_count(), window.biosMachine.read_long(base), window.biosMachine.smpc_fault_diagnostics()], base);
      assert.deepEqual(await resetState(), ["0", 0, undefined]);
      for (let boot = 0; boot < 2; boot++) {
        if (boot === 0) await page.locator("#start").click();
        else {
          const reset = await page.locator("#restart").evaluate((button, base) => {
            button.click();
            return [window.biosMachine.instruction_count(), window.biosMachine.read_long(base), window.biosMachine.smpc_fault_diagnostics()];
          }, base);
          assert.deepEqual(reset, ["0", 0, undefined]);
        }
        await page.waitForFunction(({base, length}) => window.biosMachine.read_long(base+(length-1)*4) === 0x600d, {base, length:expected.length});
        const records = await page.evaluate(({base, length}) => Array.from({length}, (_,i) => window.biosMachine.read_long(base+i*4)), {base, length:expected.length});
        assert.deepEqual(records, expected);
        assert.equal(await page.locator("#error").textContent(), "");
        assert.equal(await page.locator("#disc-selection").textContent(), "No disc selected.");
      }
    }
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
    await page.locator("#bios").setInputFiles([]);
    await page.locator("#bios").setInputFiles(firmware);
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
      await page.locator("#bios").setInputFiles([]);
      await failed("Saturn");
      assert(await page.locator("#start").isDisabled());
      assert.equal(await page.locator("#disc-selection").textContent(), mounted);
      await page.locator("#bios").setInputFiles(firmware);
      await ready();
      assert.equal(await page.locator("#error").textContent(), "");
      assert.equal(await page.locator("#disc-selection").textContent(), mounted);
    }
    await page.locator("#bios").setInputFiles([]);
    await drop(files.slice(0,2));
    await failed("Missing CUE file reference");
    assert(await page.locator("#start").isDisabled());
    await drop(files);
    await ready();
    const selected = await page.locator("#disc-selection").textContent();
    for (const file of files) assert(selected.includes(file.name));
    assert(selected.includes("Loaded locally"));
    assert(await page.locator("#pause").isHidden());
    // Adding firmware automatically switches paths without losing the disc.
    await page.locator("#bios").setInputFiles(firmware);
    await ready();
    assert.match(await page.locator("#boot-method").textContent(), /Auto · User-provided BIOS/);
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
    await page.locator("#bios").setInputFiles([]);
    await ready();
    assert.equal(await page.locator("#disc-selection").textContent(), selected);
    // Raw BIN alone is rejected. A valid CHD replaces it via the same picker.
    await page.locator("#disc").setInputFiles({name:"raw.bin",mimeType:"application/octet-stream",buffer:audio});
    await failed("lone raw BIN");
    assert(await page.locator("#start").isDisabled());
    const chd = await readFile("tests/fixtures/chd/mixed-cdlz.chd");
    await page.locator("#disc").setInputFiles({name:"local.chd",mimeType:"application/octet-stream",buffer:chd});
    await ready();
    const chdSelection = await page.locator("#disc-selection").textContent();
    assert.match(chdSelection, /local.chd/);
    await page.locator("#bios").setInputFiles(firmware);
    await ready();
    assert.equal(await page.locator("#disc-selection").textContent(), chdSelection);
    await page.locator("#bios-clear").click();
    await ready();
    assert.equal(await page.locator("#disc-selection").textContent(), chdSelection);
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
    await page.locator("#bios").setInputFiles({name:"invalid.bin",mimeType:"application/octet-stream",buffer:Buffer.alloc(1)});
    await failed("received 1 bytes");
    assert(await page.locator("#start").isDisabled());
    await page.locator("#bios").setInputFiles(firmware);
    await ready();
    await page.locator("#bios").setInputFiles([]);
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
      for (const id of ["disc","disc-clear","bios","bios-clear","start","fullscreen","view-debug"]) {
        const box = await page.locator(`#${id}`).boundingBox();
        assert(box.x >= 0 && box.x+box.width <= width && box.height >= 44, `${id} at ${width}`);
      }
    }
    await framePacing(browser, url, bios);
    assert.deepEqual(errors,[]);
    assert(requests.every(r => r.url.startsWith(url) && r.method === "GET" && r.body === null));
  } finally { await context.close(); }
}

// Exercise selection transitions through the production UI. Delayed/rejected
// File reads control host timing only; all resets and guest execution use Wasm.
async function automaticBootSelections(page, firmware, iso) {
  const input = id => page.locator(`#${id}`);
  const disc = {name:"retained.iso", mimeType:"application/octet-stream", buffer:iso};
  const ready = () => page.waitForFunction(() => !document.getElementById("start").disabled);
  const snapshot = () => page.evaluate(async () => {
    const {emulator:m} = await import("./app.js");
    return {mode:m.mode(), count:m.instruction_count(), frames:m.frames(), pc:m.special_registers()[0]};
  });
  const blocked = async pattern => {
    assert(await input("start").isDisabled());
    assert(await input("restart").isDisabled());
    assert(await input("step").isDisabled());
    assert(await input("frame").isDisabled());
    assert.equal((await snapshot()).count, "0");
    if (pattern) assert.match(await input("error").textContent(), pattern);
  };
  await page.evaluate(() => {
    const original = File.prototype.arrayBuffer;
    window.selectionReads = {};
    File.prototype.arrayBuffer = function () {
      if (this.name.startsWith("reject")) return Promise.reject(new Error("selection read rejected"));
      if (this.name === "short.bin") return Promise.resolve(new ArrayBuffer(1));
      if (!this.name.startsWith("pending")) return original.call(this);
      return new Promise((resolve, reject) => {
        window.selectionReads[this.name] = async fail => {
          if (fail) reject(new Error("stale read rejected"));
          else resolve(await original.call(this));
        };
      });
    };
    window.restoreSelectionReads = () => { File.prototype.arrayBuffer = original; };
  });
  try {
    // Fail closed both with and without a retained valid disc.
    for (const withDisc of [false, true]) {
      if (withDisc) { await input("disc").setInputFiles(disc); await ready(); }
      const selectedDisc = await input("disc-selection").textContent();
      for (const [name, buffer, error] of [
        ["invalid.bin", Buffer.alloc(1), /received 1 bytes/],
        ["reject.bin", firmware.buffer, /selection read rejected/],
        ["short.bin", firmware.buffer, /did not contain 524288 bytes/],
      ]) {
        await input("bios").setInputFiles({...firmware, name, buffer});
        await page.waitForFunction(() => Boolean(document.getElementById("error").textContent));
        await blocked(error);
        assert.equal(await input("disc-selection").textContent(), selectedDisc);
        await input("reset").evaluate(button => button.click());
        await blocked(error);
      }
      await input("bios").setInputFiles({...firmware,name:"pending-reset.bin"});
      await blocked();
      assert.match(await input("boot-method").textContent(), /BIOS loading/);
      await input("reset").evaluate(button => button.click());
      await blocked(/BIOS read cancelled/);
      await page.evaluate(() => window.selectionReads["pending-reset.bin"]());
      await blocked(/BIOS read cancelled/);
      await input("bios-clear").click();
      if (withDisc) {
        await ready();
        assert.equal((await snapshot()).mode, "BIOS-less HLE disc");
        assert.equal(await input("disc-selection").textContent(), selectedDisc);
      } else await blocked();
    }
    // An independently completing disc read must retain the BIOS error and
    // validated medium, rather than silently booting it through HLE.
    await input("disc").setInputFiles({...disc,name:"pending-error.iso"});
    await input("bios").setInputFiles({...firmware,buffer:Buffer.alloc(1)});
    await page.evaluate(() => window.selectionReads["pending-error.iso"]());
    await blocked(/received 1 bytes/);
    assert.match(await input("disc-selection").textContent(), /Loaded locally/);
    // Newer accepted firmware wins over either a late success or rejection.
    for (const reject of [false, true]) {
      await input("bios").setInputFiles({...firmware,name:"pending-old.bin"});
      await blocked();
      await input("bios").setInputFiles(firmware);
      await ready();
      const latest = await snapshot();
      await page.evaluate(reject => window.selectionReads["pending-old.bin"](reject), reject);
      assert.deepEqual(await snapshot(), latest);
      assert.equal(await input("error").textContent(), "");
      assert.match(await input("bios-selection").textContent(), /original-loop/);
    }
    // Disc replacement/clear retains the firmware, including an outstanding read.
    await input("disc-clear").click();
    await ready();
    assert.equal((await snapshot()).mode, "User-provided BIOS");
    assert.equal(await input("disc-selection").textContent(), "No disc selected.");
    await input("disc").setInputFiles({...disc,name:"pending-disc.iso"});
    await blocked();
    await input("disc-clear").click();
    await ready();
    await page.evaluate(() => window.selectionReads["pending-disc.iso"]());
    assert.equal(await input("disc-selection").textContent(), "No disc selected.");
    assert.equal((await snapshot()).mode, "User-provided BIOS");
    assert.equal(await input("bios").evaluate(el => el.files[0].name), firmware.name);
    await input("disc").setInputFiles(disc);
    await ready();
    await input("start").click();
    await page.waitForFunction(() => document.getElementById("pause").textContent === "Pause");
    // Selection changes stop an active guest and never resume playback.
    await input("bios").setInputFiles({...firmware,name:"pending-active.bin"});
    await blocked();
    await input("bios-clear").click();
    await ready();
    const hle = await snapshot();
    await page.evaluate(() => window.selectionReads["pending-active.bin"]());
    assert.deepEqual(await snapshot(), hle);
    assert(await input("pause").isHidden());
    assert.equal(hle.mode, "BIOS-less HLE disc");
    // The fixture is explicit, clears files, survives Reset, and cannot Play.
    await input("view-debug").click();
    await input("fixture").click();
    assert.match(await input("boot-method").textContent(), /Debug · HLE bootstrap fixture/);
    assert(await input("start").isDisabled());
    assert(await input("restart").isDisabled());
    await input("run").click();
    assert.equal(await input("r0").textContent(), "0x00000004");
    await input("reset").click();
    assert.equal((await snapshot()).pc, 0x06000000);
    await input("view-play").click();
    assert(await input("start").isDisabled());
    await input("bios").setInputFiles([]); // Return to empty ordinary Play.
    await blocked();
    assert.equal(await input("disc-selection").textContent(), "No disc selected.");
  } finally { await page.evaluate(() => window.restoreSelectionReads()); }
}

// Preserve 60 Hz phase across rounded RAF timestamps and faster displays.
async function framePacing(browser,url,firmware) {
  const context=await browser.newContext();
  try {
    await context.addInitScript(()=>{
      let pending;
      window.requestAnimationFrame=callback=>{pending=callback;return 1};
      window.deliverFrame=timestamp=>{const cb=pending;pending=null;cb(timestamp)};
    });
    const page=await context.newPage();
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.goto(url);
    // RAF belongs to this test driver; readiness polling uses real timers.
    await page.waitForFunction(()=>!document.getElementById('controls').disabled,undefined,{polling:10});
    await page.locator('#settings > summary').evaluate(button=>button.click());
    await page.locator('#bios').setInputFiles({name:'original-loop.bin',mimeType:'application/octet-stream',buffer:firmware});
    await page.waitForFunction(()=>!document.getElementById('start').disabled,undefined,{polling:10});
    await page.evaluate(async()=>{
      const {emulator}=await import('./app.js');
      emulator.advance_frame();
      window.pacingCalls=0;
      // Count requests independently of core speed; the real Canvas presenter,
      // audio draining, status and public playback controls still run.
      emulator.advance_frame=()=>{window.pacingCalls++};
    });
    const results=[];
    for(const quantum of [.1,1])for(const hz of [30,60,120,144]) {
      await page.locator(results.length?'#pause':'#start').evaluate(button=>button.click());
      const base=1000+results.length*10000;
      const data=await page.evaluate(({hz,quantum,base})=>{
        window.deliverFrame(base);
        const start=window.pacingCalls;
        let maxPerCallback=0;
        for(let i=1;i<=hz*3;i++) {
          const before=window.pacingCalls;
          const timestamp=base+Math.round(i*1000/hz/quantum)*quantum;
          window.deliverFrame(timestamp);
          maxPerCallback=Math.max(maxPerCallback,window.pacingCalls-before);
          const presented=window.pacingCalls;
          window.deliverFrame(timestamp);
          if(window.pacingCalls!==presented)throw new Error(`Duplicate RAF advanced at ${hz} Hz, ${quantum} ms rounding`);
        }
        const frames=window.pacingCalls-start;
        const beforeStall=window.pacingCalls;
        window.deliverFrame(base+3500);
        const afterStall=window.pacingCalls;
        window.deliverFrame(base+3500);
        return {frames,maxPerCallback,stall:afterStall-beforeStall,duplicate:window.pacingCalls-afterStall};
      },{hz,quantum,base});
      assert.equal(data.frames,Math.min(hz,60)*3,JSON.stringify({hz,quantum,...data}));
      assert.equal(data.maxPerCallback,1);assert.equal(data.stall,1);assert.equal(data.duplicate,0);
      await page.locator('#pause').evaluate(button=>button.click());
      const paused=await page.evaluate(base=>{
        const before=window.pacingCalls;window.deliverFrame(base+3600);window.deliverFrame(base+3700);return window.pacingCalls-before;
      },base);
      assert.equal(paused,0);
      results.push({hz,quantum,...data});
    }
    assert.deepEqual(errors,[]);
    assert.equal(await page.locator('#error').textContent(),'');
  } finally {await context.close()}
}
