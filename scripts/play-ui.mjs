// Play UI acceptance uses only repository-authored media and firmware.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { digitalInput } from "./digital-input.mjs";
import { waitForGuest } from "./browser-transport.mjs";

export async function playUi(browser, url) {
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    const requests = [], errors = [];
    page.on("request", r => requests.push({url:r.url(), method:r.method(), body:r.postData()}));
    page.on("pageerror", error => errors.push(error.message));
    await page.goto(url);
    await page.waitForFunction(() => !document.getElementById("controls").disabled);
    assert.equal(await page.title(), "saturn-rust · Saturn player");
    assert.equal(await page.locator("#mode").count(), 0);
    assert(await page.locator("#empty-state").isVisible());
    assert(await page.locator("#play").isDisabled());
    assert.equal(await page.locator("#play").textContent(), "Play");
    assert(await page.locator("#restart").isDisabled());
    assert.equal(await page.locator("#error").textContent(), "");
    assert.equal(await page.getByRole("group", { name: "Playback controls" }).getByRole("button", { name: "Play", exact: true }).count(), 1);
    assert.equal(await page.locator(".setup-steps").count(), 0);
    assert.equal(await page.locator("#audio-status").textContent(), "Audio: disabled");
    assert.equal(requests.some(r => /\.(iso|chd|bin)$/.test(r.url)), false);
    const idle = await page.evaluate(async () => {
      const {emulator} = await import("./app.js");
      return [String((await emulator.frames())), String((await emulator.instruction_count()))];
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
    const ready = () => page.waitForFunction(() => !document.getElementById("play").disabled);
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
    assert.match(await page.locator("#status").textContent(), /Resetting/);
    await page.evaluate(async () => { window.biosMachine = (await import("./app.js")).emulator; });
    const state = () => page.evaluate(async () => ({
      count:(await window.biosMachine.instruction_count()), counter:(await window.biosMachine.read_long(0x06001000)),
      frames:(await window.biosMachine.frames()), mode:(await window.biosMachine.mode()),
    }));
    assert.deepEqual(await state(), {count:"0", counter:0, frames:"0", mode:"User-provided BIOS"});
    await page.locator("#play").click();
    await waitForGuest(page, async () => (await window.biosMachine.read_long(0x06001000)) > 0);
    assert.match(await page.locator("#player-status").textContent(), /Running/);
    assert(await page.locator("#empty-state").isHidden());
    await page.locator("#play").click();
    const paused = await state();
    assert(Number(paused.count) > 100);
    assert(Number(paused.frames) > 0);
    await page.waitForTimeout(50);
    assert.deepEqual(await state(), paused, "Pause stops guest execution");
    await page.locator("#play").click();
    await waitForGuest(page, async count => Number((await window.biosMachine.instruction_count())) > Number(count), paused.count);
    // Observe the completed Worker reset at the boundary before playback resumes.
    const restarted = await restartRecord(page, 0x06001000);
    assert.deepEqual(restarted, ["0",0,"0"]);
    await waitForGuest(page, async () => (await window.biosMachine.read_long(0x06001000)) > 0);
    await page.locator("#view-debug").click();
    assert(Number(await page.locator("#instructions").textContent()) > 100);
    await page.locator("#reset").click();
    assert.equal(await page.locator("#play").textContent(), "Resume");
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
      if (await page.locator("#view-play").isVisible()) await page.locator("#view-play").click();
      const resetState = () => page.evaluate(async base => [(await window.biosMachine.instruction_count()), (await window.biosMachine.read_long(base)), (await window.biosMachine.smpc_fault_diagnostics())], base);
      assert.deepEqual(await resetState(), ["0", 0, undefined]);
      for (let boot = 0; boot < 2; boot++) {
        if (boot === 0) await page.locator("#play").click();
        else {
          const reset = await restartRecord(page, base, true);
          assert.deepEqual(reset, ["0", 0, undefined]);
        }
        await waitForGuest(page, async ({base, length}) => (await window.biosMachine.read_long(base+(length-1)*4)) === 0x600d, {base, length:expected.length});
        const records = await page.evaluate(async ({base, length}) => (await Promise.all(Array.from({length}, async (_,i) => (await window.biosMachine.read_long(base+i*4))))), {base, length:expected.length});
        assert.deepEqual(records, expected);
        assert.equal(await page.locator("#error").textContent(), "");
        assert.equal(await page.locator("#disc-selection").textContent(), "No disc selected.");
      }
    }
    // A separate existing MIT firmware queries real CD hardware and records its
    // no-media result. Start, rather than host stepping, must reach that result.
    await page.locator("#bios").setInputFiles({...firmware,buffer:await rom("external-startup")});
    await ready();
    await page.locator("#play").click();
    await waitForGuest(page, async () => (await window.biosMachine.read_long(0x060100FC)) === 0xE001);
    assert.equal(await page.locator("#error").textContent(), "");
    assert.equal(await page.locator("#disc-selection").textContent(), "No disc selected.");
    // A sleeping guest with no wake source can still be paused. Resume must
    // remain disabled honestly until a reset makes the CPU executable again.
    const sleeping = Buffer.from(bios);
    sleeping.writeUInt16BE(0x001B,0x100); // SLEEP, with reset SR masking IRQs
    await page.locator("#bios").setInputFiles({...firmware,buffer:sleeping});
    await ready();
    await page.locator("#play").click();
    await page.waitForFunction(() => document.getElementById("status").textContent.startsWith("Sleeping"));
    assert.match(await page.locator("#player-status").textContent(), /Sleeping/);
    assert(await page.locator("#play").isEnabled());
    await page.locator("#play").click();
    assert(await page.locator("#play").isDisabled());
    assert.match(await page.locator("#player-status").textContent(), /Sleeping/);
    assert.doesNotMatch(await page.locator("#player-status").textContent(), /Load a disc/);
    // Faults are reported honestly and cannot enable Start or Resume.
    const fault = Buffer.from(bios);
    fault.writeUInt16BE(0xD001,0x100); fault.writeUInt16BE(0x2000,0x102);
    fault.writeUInt32BE(0x05FE00CC,0x108);
    await page.locator("#bios").setInputFiles({...firmware,buffer:fault});
    await ready();
    await page.locator("#play").click();
    await failed("Unsupported Saturn device/register");
    assert.match(await page.locator("#status").textContent(), /Faulted/);
    assert(await page.locator("#play").isDisabled());
    await page.locator("#bios").setInputFiles(firmware);
    await ready();
    // Invalid optional media must stay rejected through Reset and mode changes.
    await page.locator("#disc").setInputFiles({name:"invalid.iso",mimeType:"application/octet-stream",buffer:Buffer.alloc(1)});
    await failed("complete 2048-byte-sector ISO");
    assert(await page.locator("#play").isDisabled());
    await page.locator("#reset").evaluate(button => button.click());
    assert(await page.locator("#play").isDisabled());
    await page.locator("#bios").setInputFiles([]);
    await page.locator("#bios").setInputFiles(firmware);
    assert(await page.locator("#play").isDisabled());
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
    assert.equal(await page.locator("#disc-state").textContent(), "Loaded locally");
    for (let attempt = 0; attempt < 2; attempt++) {
      await page.locator("#bios").setInputFiles([]);
      await failed("Saturn");
      assert(await page.locator("#play").isDisabled());
      assert.equal(await page.locator("#disc-selection").textContent(), mounted);
      await page.locator("#bios").setInputFiles(firmware);
      await ready();
      assert.equal(await page.locator("#error").textContent(), "");
      assert.equal(await page.locator("#disc-selection").textContent(), mounted);
    }
    await page.locator("#bios").setInputFiles([]);
    await drop(files.slice(0,2));
    await failed("Missing CUE file reference");
    assert(await page.locator("#play").isDisabled());
    await drop(files);
    await ready();
    const selected = await page.locator("#disc-selection").textContent();
    for (const file of files) assert(selected.includes(file.name));
    assert.equal(await page.locator("#disc-state").textContent(), "Loaded locally");
    assert.equal(await page.locator("#play").textContent(), "Play");
    // Adding firmware automatically switches paths without losing the disc.
    await page.locator("#bios").setInputFiles(firmware);
    await ready();
    assert.match(await page.locator("#boot-method").textContent(), /Auto · User-provided BIOS/);
    assert.equal(await page.locator("#bios").evaluate(input => input.files[0].name), firmware.name);
    assert.equal(await page.locator("#disc-selection").textContent(), selected);
    await page.locator("#play").click();
    await page.waitForFunction(() => document.getElementById("play").textContent === "Pause");
    await page.locator("#play").click();
    assert.match(await page.locator("#player-status").textContent(), /Paused/);
    await page.locator("#view-debug").click();
    assert.match(await page.locator("#active-mode").textContent(), /User-provided BIOS/);
    await page.locator("#view-play").click();
    assert.equal(await page.locator("#play").textContent(), "Resume");
    await page.locator("#restart").click();
    await page.waitForFunction(() => document.getElementById("play").textContent === "Pause");
    // Visibility pauses and releases input, without a later automatic resume.
    await page.evaluate(() => {
      Object.defineProperty(document, "hidden", {configurable:true,value:true});
      document.dispatchEvent(new Event("visibilitychange"));
      delete document.hidden;
      document.dispatchEvent(new Event("visibilitychange"));
    });
    assert.equal(await page.locator("#play").textContent(), "Resume");
    await page.locator("#bios").setInputFiles([]);
    await ready();
    assert.equal(await page.locator("#disc-selection").textContent(), selected);
    // Raw BIN alone is rejected. A valid CHD replaces it via the same picker.
    await page.locator("#disc").setInputFiles({name:"raw.bin",mimeType:"application/octet-stream",buffer:audio});
    await failed("lone raw BIN");
    assert(await page.locator("#play").isDisabled());
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
    assert(await page.locator("#play").isDisabled());
    await page.locator("#disc").setInputFiles({name:"replacement.iso",mimeType:"application/octet-stream",buffer:iso});
    await ready();
    await page.evaluate(async () => { await window.finishSlowUi(); window.restoreUiRead(); });
    assert.match(await page.locator("#disc-selection").textContent(), /replacement.iso/);
    assert.equal(await page.locator("#play").textContent(), "Play");
    // BIOS mode never falls back to HLE after missing/invalid firmware.
    await page.locator("#bios").setInputFiles({name:"invalid.bin",mimeType:"application/octet-stream",buffer:Buffer.alloc(1)});
    await failed("received 1 bytes");
    assert(await page.locator("#play").isDisabled());
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
      for (let i=0; i<40 && (await m.read_long(0x06010020))!==0x600D; i++) (await m.run_steps(10000));
      (await m.advance_frame());
      const pcm = Array.from((await m.audio_samples(8192)));
      (await advanceFrame());
      const frame = (await m.framebuffer());
      const canvas = document.getElementById("screen").getContext("2d");
      return {fields:(await Promise.all(Array.from({length:11},async (_,i)=>(await m.read_long(0x06010000+i*4))))),
        mode:(await m.mode()), stereo:pcm.some((v,i)=>i%2===0 && v!==pcm[i+1]),
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
      assert(await page.locator("#play").isDisabled());
      await select(caseFiles.slice(0,2));
      await failed("Missing CUE file reference");
      assert(await page.locator("#play").isDisabled());
      // An exact track wins over an earlier invalid case-fold candidate.
      const wrongAudio = {...caseFiles[2], bytes:[0]};
      await select([...caseFiles.slice(0,2), wrongAudio, files[2]]);
      await ready();
      await checkGuest();
    }
    assert.equal(requests.length, beforeCaseRequests, "Local CUE resolution must make no network requests");
    await playerLayouts(browser, page, url, iso);
    await playerAudio(browser, url);
    await framePacing(browser, url);
    await digitalInput(browser, url);
    assert.deepEqual(errors,[]);
    assert(requests.every(r => r.url.startsWith(url) && r.method === "GET" && r.body === null));
  } finally { await context.close(); }
}

// Inspect the real reset Machine before its first resumed instruction. The
// adapter hook observes the public play boundary and still starts the Worker.
async function restartRecord(page, base, diagnostics = false) {
  return page.locator("#restart").evaluate(async (button, {base, diagnostics}) => {
    const m = window.biosMachine, play = m.play.bind(m);
    const snapshot = new Promise((resolve, reject) => {
      m.play = async () => {
        m.play = play;
        try {
          resolve(await Promise.all([m.instruction_count(), m.read_long(base),
            diagnostics ? m.smpc_fault_diagnostics() : m.frames()]));
        } catch (error) { reject(error); throw error; }
        return play();
      };
    });
    button.click();
    return snapshot;
  }, {base, diagnostics});
}

// Exercise selection transitions through the production UI. Delayed/rejected
// File reads control host timing only; all resets and guest execution use Wasm.
async function automaticBootSelections(page, firmware, iso) {
  const input = id => page.locator(`#${id}`);
  const disc = {name:"retained.iso", mimeType:"application/octet-stream", buffer:iso};
  const ready = () => page.waitForFunction(() => !document.getElementById("play").disabled);
  const snapshot = () => page.evaluate(async () => {
    const {emulator:m} = await import("./app.js");
    return {mode:(await m.mode()), count:(await m.instruction_count()), frames:(await m.frames()), pc:(await m.special_registers())[0]};
  });
  const blocked = async pattern => {
    assert(await input("play").isDisabled());
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
    assert.equal(await input("disc-state").textContent(), "Loaded locally");
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
    // An accepted disc read can hand off to a newer BIOS reset before its
    // own reset reaches the mount call. Keep the independently accepted bytes.
    await input("disc").setInputFiles({...disc, name:"pending-stage.iso"});
    await page.waitForFunction(() => document.getElementById("main").getAttribute("aria-busy") === "false");
    await page.evaluate(async () => {
      const { emulator } = await import("./app.js");
      const resetBios = emulator.reset_bios.bind(emulator);
      emulator.reset_bios = bytes => {
        emulator.reset_bios = resetBios;
        return resetBios(bytes).catch(async error => {
          await new Promise(resolve => { window.releaseStagedReset = resolve; });
          throw error;
        });
      };
    });
    await page.evaluate(() => window.selectionReads["pending-stage.iso"]());
    await page.waitForFunction(() => Boolean(window.releaseStagedReset));
    await input("bios").setInputFiles(firmware);
    await waitForGuest(page, async () => (await import("./app.js")).emulator.state.mode === "User-provided BIOS");
    await page.evaluate(() => window.releaseStagedReset());
    await ready();
    assert.equal(await input("disc-state").textContent(), "Loaded locally");
    assert.equal(await input("disc-selection").textContent(), "pending-stage.iso");
    await input("bios-clear").click();
    await ready();
    assert.equal((await snapshot()).mode, "BIOS-less HLE disc");
    await input("bios").setInputFiles(firmware);
    await ready();
    await input("disc").setInputFiles(disc);
    await ready();
    await input("play").click();
    await page.waitForFunction(() => document.getElementById("play").textContent === "Pause");
    // Selection changes stop an active guest and never resume playback.
    await input("bios").setInputFiles({...firmware,name:"pending-active.bin"});
    await blocked();
    await input("bios-clear").click();
    await ready();
    const hle = await snapshot();
    await page.evaluate(() => window.selectionReads["pending-active.bin"]());
    assert.deepEqual(await snapshot(), hle);
    assert.equal(await input("play").textContent(), "Play");
    assert.equal(hle.mode, "BIOS-less HLE disc");
    // The fixture is explicit, clears files, survives Reset, and cannot Play.
    await input("view-debug").click();
    await input("fixture").click();
    assert.match(await input("boot-method").textContent(), /Debug · HLE bootstrap fixture/);
    assert(await input("play").isDisabled());
    assert(await input("restart").isDisabled());
    await input("run").click();
    assert.equal(await input("r0").textContent(), "0x00000004");
    await input("reset").click();
    assert.equal((await snapshot()).pc, 0x06000000);
    await input("view-play").click();
    assert(await input("play").isDisabled());
    await input("bios").setInputFiles([]); // Return to empty ordinary Play.
    await blocked();
    assert.equal(await input("disc-selection").textContent(), "No disc selected.");
  } finally { await page.evaluate(() => window.restoreSelectionReads()); }
}

// RAF schedules host input polling only. Real Worker fields retain their pace
// on slower/faster displays, rounded timestamps and duplicate callbacks.
async function framePacing(browser, url) {
  // Scheduler behavior must not assume CPU headroom on a shared CI host.
  // Actual real-time throughput is measured separately by benchmark-player.
  const firmware = Buffer.alloc(524288);
  for (const line of (await readFile("examples/runtime-loop.hex", "utf8")).split(/\r?\n/)) {
    if (!line.trim() || line.startsWith("#")) continue;
    const [at, ...words] = line.trim().split(/\s+/).map(word => parseInt(word, 16));
    words.forEach((word, i) => firmware.writeUInt16BE(word, at + i * 2));
  }
  const context = await browser.newContext();
  try {
    await context.addInitScript(() => {
      let pending;
      window.requestAnimationFrame = callback => { pending = callback; return 1; };
      window.deliverFrame = timestamp => {
        const callback = pending; pending = null; callback?.(timestamp);
      };
    });
    const page = await context.newPage(), errors = [];
    page.on("pageerror", error => errors.push(error.message));
    await page.goto(url);
    const settled = () => page.waitForFunction(() => document.getElementById("main").getAttribute("aria-busy") === "false", undefined, {polling:10});
    await page.waitForFunction(() => !document.getElementById("controls").disabled, undefined, {polling:10});
    await page.locator("#settings > summary").evaluate(button => button.click());
    await page.locator("#bios").setInputFiles({name:"original-loop.bin", mimeType:"application/octet-stream", buffer:firmware});
    await page.waitForFunction(() => !document.getElementById("play").disabled, undefined, {polling:10});
    await page.evaluate(async () => { window.pacingMachine = (await import("./app.js")).emulator; });
    for (const quantum of [0.1, 1]) for (const hz of [30, 60, 120, 144]) {
      const resumeFrom = await page.evaluate(() => pacingMachine.state.frames);
      await page.locator("#play").evaluate(button => button.click());
      await settled();
      // Require actual progress with RAF still withheld before measuring the
      // running cadence. Cold Wasm compilation/Worker scheduling can exceed
      // 250 ms on a shared host. Two completed fields also rule out observing
      // only one already admitted field; a RAF-dependent loop times out here.
      await page.waitForFunction(from => pacingMachine.state.frames >= from + 2,
        resumeFrom, {polling:10});
      const data = await page.evaluate(async ({hz, quantum}) => {
        const before = pacingMachine.state.frames, start = performance.now();
        // No RAF callback is delivered during this interval. Guest execution
        // must continue in its Worker even on a slow host or suspended display.
        await new Promise(resolve => setTimeout(resolve, 250));
        const withoutRaf = pacingMachine.state.frames - before;
        let callbacks = 0;
        const deliver = () => {
          const timestamp = Math.round(performance.now() / quantum) * quantum;
          window.deliverFrame(timestamp); window.deliverFrame(timestamp);
          callbacks++;
        };
        const timer = setInterval(deliver, 1000 / hz);
        await new Promise(resolve => setTimeout(resolve, 2000));
        clearInterval(timer);
        const elapsed = performance.now() - start, fields = pacingMachine.state.frames - before;
        // A burst of future RAF timestamps must never advance guest time.
        const burstFrom = pacingMachine.state.frames;
        for (let i = 0; i < 100; i++) window.deliverFrame(performance.now() + i * 500);
        return {fields, elapsed, callbacks, withoutRaf, burst: pacingMachine.state.frames - burstFrom};
      }, {hz, quantum});
      const fps = data.fields * 1000 / data.elapsed;
      assert(data.withoutRaf > 0, `Worker fields must progress without RAF: ${JSON.stringify({hz, quantum, ...data})}`);
      assert(fps > 0 && fps <= 62, JSON.stringify({hz, quantum, fps, ...data}));
      assert(data.callbacks > hz, "driver must deliver real rounded/duplicate RAF callbacks");
      assert.equal(data.burst, 0, "RAF cannot execute a guest field");
      await page.locator("#play").evaluate(button => button.click());
      await settled();
      const paused = await page.evaluate(async () => {
        const before = pacingMachine.state.frames;
        for (let i = 0; i < 10; i++) window.deliverFrame(performance.now() + i * 1000);
        await new Promise(resolve => setTimeout(resolve, 60));
        return pacingMachine.state.frames - before;
      });
      assert.equal(paused, 0);
    }
    assert.deepEqual(errors, []);
    assert.equal(await page.locator("#error").textContent(), "");
  } finally { await context.close(); }
}

async function playerLayouts(browser, desktop, url, iso) {
  const touch = await browser.newContext({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true, deviceScaleFactor: 3 });
  try {
    const mobile = await touch.newPage();
    await mobile.goto(url);
    await mobile.waitForFunction(() => !document.getElementById("controls").disabled);
    const longName = "long".repeat(59) + ".iso";
    const [chooser] = await Promise.all([
      mobile.waitForEvent("filechooser"),
      mobile.getByRole("button", { name: "Insert disc" }).tap(),
    ]);
    assert(chooser.isMultiple());
    await chooser.setFiles({ name: longName, mimeType: "application/octet-stream", buffer: iso });
    await mobile.waitForFunction(() => !document.getElementById("play").disabled);
    assert.equal(await mobile.locator("#disc-selection").textContent(), longName);
    assert(await mobile.getByRole("button", { name: "Change disc" }).isEnabled());
    const play = mobile.locator("#play"), help = mobile.locator("#show-help");
    await play.tap();
    await mobile.waitForFunction(() => document.getElementById("empty-state").hidden);
    await help.focus();
    await mobile.keyboard.press("Enter");
    const dialog = mobile.getByRole("dialog", { name: "Help" });
    assert(await dialog.isVisible());
    assert.equal(await mobile.evaluate(() => document.activeElement.textContent), "Close help");
    const machine = () => mobile.evaluate(async () => {
      const { emulator } = await import("./app.js");
      return [await emulator.instruction_count(), await emulator.frames()];
    });
    const paused = await machine();
    await mobile.keyboard.press("Escape");
    assert(await dialog.isHidden());
    assert.equal(await mobile.evaluate(() => document.activeElement.id), "show-help");
    assert(await help.evaluate(el => getComputedStyle(el).outlineStyle === "solid"));
    assert.deepEqual(await machine(), paused, "Closing Help never resumes");
    assert.equal(await play.textContent(), "Resume");

    const check = async (page, width, height, coarse, fullscreen = false) => {
      const result = await page.evaluate(fullscreen => {
        const $ = id => document.getElementById(id);
        const rect = node => node.getBoundingClientRect().toJSON();
        const picture = rect($("screen")), well = rect(document.querySelector(".display-well"));
        const controls = [...document.querySelectorAll("#touch-controls button")].map(node => {
          const b = rect(node);
          return { ...b, mask: Number(node.dataset.mask), hit: document.elementFromPoint(b.x + b.width / 2, b.y + b.height / 2) === node };
        });
        const visible = controls.some(b => b.width > 0);
        return {
          picture, well, controls, visible, dpr: devicePixelRatio, width: innerWidth, height: innerHeight,
          pointer: matchMedia("(pointer: coarse)").matches, overflow: document.documentElement.scrollWidth > innerWidth,
          media: rect($("setup")), toolbar: rect(document.querySelector(".player-toolbar")),
          exit: rect($("exit-fullscreen")), screenNative: [$("screen").width, $("screen").height],
          scrollable: getComputedStyle(document.body).touchAction === "auto",
          fitsFullscreen: !fullscreen || ($("player-shell").scrollHeight <= innerHeight && $("player-shell").scrollWidth <= innerWidth),
          labels: fullscreen ? [] : [...document.querySelectorAll(".player-toolbar button, .player-toolbar select, #volume, .media-slot button")]
            .map(rect),
        };
      }, fullscreen);
      assert.equal(result.width, width, "CSS viewport width");
      assert.equal(result.height, height, "CSS viewport height");
      assert.equal(result.pointer, coarse);
      assert.equal(result.dpr, coarse ? 3 : 1);
      assert.equal(result.visible, coarse);
      assert(!result.overflow);
      assert(result.scrollable);
      assert(result.fitsFullscreen, "Fullscreen layout fits its viewport");
      const [pixelW, pixelH] = result.screenNative;
      const aspect = pixelW === 320 && pixelH === 224 ? 4 / 3 : pixelW / pixelH;
      const { picture, well, controls } = result;
      assert(Math.abs(picture.width / picture.height - aspect) < .006);
      const disjoint = (a, b) => a.right <= b.left || b.right <= a.left || a.bottom <= b.top || b.bottom <= a.top;
      assert(picture.width > 140 && picture.left >= 0 && picture.right <= width);
      for (const box of result.labels) assert(box.left >= 0 && box.right <= width && box.height >= 44);
      for (const [i, box] of result.labels.entries()) {
        for (const other of result.labels.slice(i + 1)) assert(disjoint(box, other), "Player toolbar and media actions cannot overlap");
      }
      if (coarse) {
        assert.equal(controls.length, 13);
        for (const [i, box] of controls.entries()) {
          assert(box.width >= 44 && box.height >= 44 && box.left >= 0 && box.right <= width && box.top >= 0 && box.bottom <= height, JSON.stringify(box));
          assert(box.hit && disjoint(picture, box), JSON.stringify(box));
          for (const other of controls.slice(i + 1)) assert(disjoint(box, other), "Touch buttons cannot overlap");
        }
        if (width >= 640 && width > height) {
          assert(controls.find(b => b.mask === 2048).right <= picture.left);
          assert(controls.find(b => b.mask === 4096).left >= picture.right);
        } else assert(controls.every(b => b.top >= picture.bottom));
        for (const mask of [1024, 512, 256]) {
          assert(controls.find(b => b.mask === mask).bottom <= controls.find(b => b.mask === 64).top);
        }
      }
      if (fullscreen) {
        assert(picture.top >= 0 && picture.bottom <= height && disjoint(result.exit, picture));
        assert(result.exit.height >= 44 && result.exit.bottom <= height);
        assert(Math.abs(picture.width - Math.min(well.width, well.height * aspect)) < 1);
        if (coarse) assert(controls.every(b => disjoint(result.exit, b)));
      } else if (coarse) assert(result.media.top >= result.toolbar.bottom);
      else assert(result.media.left >= picture.right);
    };
    const fullscreenCheck = async (page, width, height, coarse) => {
      await page.locator("#fullscreen").click();
      await page.waitForFunction(() => document.fullscreenElement?.id === "player-shell" && document.activeElement.id === "screen");
      await check(page, width, height, coarse, true);
      await page.locator("#exit-fullscreen")[coarse ? "tap" : "click"]();
      await page.waitForFunction(() => !document.fullscreenElement && document.activeElement.id === "fullscreen");
    };
    for (const [width, height] of [[1280, 720], [1440, 900]]) {
      await desktop.setViewportSize({ width, height });
      await desktop.evaluate(() => scrollTo(0, 0));
      await check(desktop, width, height, false);
      await fullscreenCheck(desktop, width, height, false);
    }
    for (const [width, height] of [[320, 568], [390, 844], [768, 1024], [844, 390]]) {
      await mobile.setViewportSize({ width, height });
      await mobile.evaluate(() => scrollTo(0, 0));
      await check(mobile, width, height, true);
      await fullscreenCheck(mobile, width, height, true);
      assert.deepEqual(await machine(), paused);
    }
    await mobile.setViewportSize({ width: 320, height: 568 });
    await mobile.locator("#bios").setInputFiles({ name: "rejected".repeat(26) + ".bin", mimeType: "application/octet-stream", buffer: Buffer.alloc(1) });
    assert.match(await mobile.getByRole("alert").textContent(), /raw 512 KiB BIOS.*received 1 bytes/);
    assert.equal(await mobile.locator("#bios-state").textContent(), "Needs attention");
    assert(await play.isDisabled());
    await mobile.evaluate(() => scrollTo(0, 0));
    await check(mobile, 320, 568, true);
    await mobile.getByRole("button", { name: "Clear BIOS" }).tap();
    await mobile.waitForFunction(() => !document.getElementById("play").disabled);
    assert.equal(await mobile.evaluate(() => document.activeElement.id), "bios-choose");
    assert.equal(await mobile.locator("#disc-selection").textContent(), longName);
    assert.deepEqual(await mobile.evaluate(async () => ({
      local: localStorage.length, session: sessionStorage.length, databases: await indexedDB.databases(),
      caches: await caches.keys(), cookies: document.cookie,
    })), { local: 0, session: 0, databases: [], caches: [], cookies: "" });
  } finally { await touch.close(); }
}

async function playerAudio(browser, url) {
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    await page.addInitScript(() => {
      const resume = AudioContext.prototype.resume;
      window.audioGestures = [];
      AudioContext.prototype.resume = async function () {
        window.audioGestures.push(navigator.userActivation.isActive);
        if (window.audioGestures.length === 1) {
          await this.suspend();
          throw new DOMException("Blocked for recovery test", "NotAllowedError");
        }
        return resume.call(this);
      };
    });
    await page.goto(url);
    await page.waitForFunction(() => !document.getElementById("controls").disabled);
    await page.locator("#disc").setInputFiles("web/prism-courier.iso");
    await page.waitForFunction(() => !document.getElementById("play").disabled);
    await page.getByRole("button", { name: "Play", exact: true }).click();
    const recovery = page.locator("#audio-enable");
    await recovery.waitFor();
    await page.waitForFunction(() => document.getElementById("player-status").textContent.startsWith("Running"));
    assert.deepEqual(await page.evaluate(() => window.audioGestures), [true]);
    await recovery.click();
    await page.waitForFunction(() => document.getElementById("audio-enable").hidden);
    assert.deepEqual(await page.evaluate(() => window.audioGestures), [true, true]);
    await waitForGuest(page, async () => {
      const { audio } = await import("./app.js");
      return audio.stats.rendered > 128 && audio.stats.peak > .05;
    });
    await page.evaluate(async () => {
      const { audio } = await import("./app.js");
      const before = audio.context.createAnalyser(), after = audio.context.createAnalyser();
      audio.node.connect(before);
      audio.gain.connect(after);
      const input = new Float32Array(before.fftSize), output = new Float32Array(after.fftSize);
      const rms = samples => Math.sqrt(samples.reduce((sum, sample) => sum + sample * sample, 0) / samples.length);
      window.gainReading = () => {
        before.getFloatTimeDomainData(input);
        after.getFloatTimeDomainData(output);
        return { gain: audio.gain.gain.value, source: rms(input), output: rms(output) };
      };
    });
    const slider = page.getByRole("slider", { name: "Volume" });
    await slider.focus();
    await page.keyboard.press("Home");
    assert.equal(await slider.getAttribute("aria-valuetext"), "0%");
    for (const setting of [0, 25, 60, 100]) {
      await slider.fill(String(setting));
      assert.equal(await slider.getAttribute("aria-valuetext"), `${setting}%`);
      await page.waitForFunction(setting => {
        const { gain, source, output } = window.gainReading();
        return Math.abs(gain - setting / 100) < .001 && source > .01 && Math.abs(output / source - gain) < .025;
      }, setting);
    }
    await slider.fill("37");
    const mute = page.getByRole("button", { name: "Mute", exact: true });
    await mute.focus();
    await page.keyboard.press("Space");
    assert.equal(await mute.getAttribute("aria-pressed"), "true");
    assert.equal(await slider.inputValue(), "37");
    await page.waitForFunction(() => window.gainReading().output === 0);
    await page.keyboard.press("Space");
    assert.equal(await mute.getAttribute("aria-pressed"), "false");
    await page.waitForFunction(() => {
      const { gain, source, output } = window.gainReading();
      return Math.abs(gain - .37) < .001 && source > .01 && Math.abs(output / source - gain) < .025;
    });
    await page.locator("#play").click();
    await page.waitForFunction(() => document.getElementById("play").textContent === "Resume");
    const stats = await page.evaluate(async () => {
      const { audio } = await import("./app.js");
      return { paused: audio.paused, queued: audio.stats.queued, pending: audio.pending };
    });
    assert(stats.paused && stats.queued <= 8192 && stats.pending <= 8192, JSON.stringify(stats));
    await page.locator("#restart").click();
    await page.waitForFunction(() => document.getElementById("play").textContent === "Pause");
    assert.equal(await slider.inputValue(), "37");
  } finally { await context.close(); }
}
