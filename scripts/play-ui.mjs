// Disc-first UI acceptance uses only repository-authored media and firmware.
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
    const bios = Buffer.alloc(524288);
    bios.writeUInt32BE(0x100,0); bios.writeUInt32BE(0x06100000,4);
    bios.writeUInt16BE(0xAFFE,0x100); bios.writeUInt16BE(0x0009,0x102);
    const firmware = {name:"original-loop.bin",mimeType:"application/octet-stream",buffer:bios};
    await context.setOffline(true);
    // Firmware alone allows debugging, but Play still needs a mounted disc.
    await page.locator("#mode").selectOption("bios");
    await page.locator("#bios").setInputFiles(firmware);
    await page.waitForFunction(() => document.getElementById("bios-selection").textContent.includes("loaded locally"));
    assert(await page.locator("#start").isDisabled());
    assert.match(await page.locator("#player-status").textContent(), /Load a disc to begin/);
    assert.match(await page.locator("#launch-status").textContent(), /Select a valid disc/);
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
    await failed("Missing or ambiguous CUE file reference");
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
