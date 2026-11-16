// Original MIT guest/media: tests/fixtures/LICENSE + build_large_media.py.
// Actual production picker, Wasm execution and AudioWorklet at project prefix.
import assert from "node:assert/strict";
import { waitForLoadedDisc } from "./browser-transport.mjs";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdtemp, open, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

export async function largeMedia(browser, url) {
  const directory = await mkdtemp(join(tmpdir(), "saturn-large-media-"));
  try {
    await promisify(execFile)("python3", ["tests/fixtures/build_large_media.py", "--check", "--output", directory]);
    const iso = join(directory, "large-media.iso");
    const cue = join(directory, "large-media.cue");
    const audio = join(directory, "large-media-audio.bin");
    for (const mixed of [false, true]) {
      // Isolate the large Wasm heaps and local File backing stores per selection.
      const context = await browser.newContext();
      const page = await context.newPage();
      const errors = [], requests = [];
      page.on("pageerror", error => errors.push(error.message));
      page.on("request", r => requests.push({ url:r.url(), method:r.method(), body:r.postData() }));
      try {
        await page.goto(url);
        await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
        await page.locator("#view-debug").click();
        await page.locator("#settings > summary").click();
        if (mixed) await page.locator("#audio-enable").click();
        await context.setOffline(true);
        // A genuine asynchronous local read cancelled by the Reset button must
        // never mount the pending bytes. A later valid selection must still work.
        await page.evaluate(() => {
          const original = File.prototype.arrayBuffer;
          File.prototype.arrayBuffer = function () {
            return new Promise(resolve => { window.finishLargeRead = async () => resolve(await original.call(this)); });
          };
          window.restoreLargeRead = () => { File.prototype.arrayBuffer = original; };
        });
        await page.locator("#disc").setInputFiles({ name:"pending.iso", mimeType:"application/octet-stream", buffer:Buffer.alloc(18*2048) });
        await page.waitForFunction(() => typeof window.finishLargeRead === "function");
        await page.locator("#reset").click();
        await page.evaluate(async () => { await window.finishLargeRead(); window.restoreLargeRead(); });
        assert.match(await page.locator("#status").textContent(), /Reset/);
        await page.locator("#disc").setInputFiles(mixed ? [cue, iso, audio] : iso);
        await waitForLoadedDisc(page, { timeout: 60000 });
        assert.equal(await page.locator("#error").textContent(), "");
        const execute = async (sysres = false) => page.evaluate(async (sysres) => {
          const { emulator } = await import("./app.js");
          if (sysres) {
            const old = (await emulator.export_backup_ram())[0];
            (await emulator.set_buttons(128));
            for(let i=0;i<80 && (await emulator.export_backup_ram())[0] === old;i++) (await emulator.run_steps(10000));
            (await emulator.set_buttons(0));
          }
          for(let i=0;i<80 && (await emulator.read_long(0x0601000C))!==0x600D;i++) (await emulator.run_steps(10000));
          return { sum:(await emulator.read_long(0x06010000)), dma:(await emulator.read_long(0x06010004)),
            irq:(await emulator.read_long(0x06010008)), done:(await emulator.read_long(0x0601000C)),
            first:(await emulator.read_long(0x06011000)), last:(await emulator.read_long(0x060117FC)),
            backup:(await emulator.export_backup_ram())[0], mode:(await emulator.mode()) };
        }, sysres);
        function check(result, backup) {
          assert.equal(result.sum, 393991); // 3*(1+...+512) + guest literal 7
          assert.equal(result.dma, 1); assert(result.irq>0);
          assert.equal(result.done, 0x600D);
          assert.equal(result.first, 3); assert.equal(result.last, 1536);
          assert.equal(result.backup, backup); assert.equal(result.mode, "BIOS-less HLE disc");
        }
        check(await execute(), 1);
        check(await execute(true), 2);
        await page.locator("#reset").click();
        check(await execute(), 3); // reuses immutable Wasm media without uploading
        if (mixed) {
          const samples = await page.evaluate(async () => {
            const { emulator, audio } = await import("./app.js");
            (await emulator.audio_samples(8192));
            for(let i=0;i<3;i++) (await emulator.advance_frame());
            const samples = (await emulator.audio_samples(8192));
            audio.setPaused(false); audio.deliver(samples);
            window.largeAudio = audio;
            return [...samples];
          });
          assert(samples.some((n,i) => i%2===0 && n===4096 && samples[i+1]===-2048));
          await page.waitForFunction(() => window.largeAudio.stats.rendered>0 && window.largeAudio.stats.peak>0);
        }
        // Structural rejection clears stale execution, then the original ISO
        // can be selected again even after a successful standard-size boot.
        await page.locator("#disc").setInputFiles({ name:"malformed.iso", mimeType:"application/octet-stream", buffer:Buffer.alloc(18*2048) });
        await page.waitForFunction(() => /missing Saturn/.test(document.getElementById("error").textContent));
        assert.equal(await page.locator("#run").isDisabled(), true);
        const resetState = await page.evaluate(async () => {
          const { emulator } = await import("./app.js");
          return [(await emulator.status()), (await emulator.instruction_count())];
        });
        assert.match(resetState[0], /Reset/); assert.equal(resetState[1], "0");
        assert.deepEqual(errors, []);
        assert(requests.every(r => r.method==="GET" && r.body===null && r.url.startsWith(url)));
      } finally { await context.close(); }
    }
    // Mutations change actual standard-size guest input, with the same picker.
    for(const mutation of ["literal", "sector", "omit"]) {
      const file = await open(iso, "r+");
      const at = mutation==="sector" ? 260000*2048+3 : 9003*2048+(mutation==="literal" ? 0x267 : 0x100);
      const original = Buffer.alloc(mutation==="omit" ? 2 : 1);
      await file.read(original, 0, original.length, at);
      await file.write(mutation==="omit" ? Buffer.from([0,9]) : Buffer.from([mutation==="literal" ? 11 : 9]), 0, original.length, at);
      const context = await browser.newContext();
      try {
        const page = await context.newPage();
        await page.goto(url);
        await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
        await page.locator("#view-debug").click(); await page.locator("#settings > summary").click();
        await page.locator("#disc").setInputFiles(iso);
        await waitForLoadedDisc(page, { timeout: 60000 });
        const result = await page.evaluate(async () => {
          const { emulator } = await import("./app.js");
          for(let i=0;i<80 && (await emulator.read_long(0x0601000C))!==0x600D;i++) (await emulator.run_steps(10000));
          return [(await emulator.read_long(0x06010000)), (await emulator.read_long(0x0601000C))];
        });
        if(mutation==="omit") assert.notEqual(result[1], 0x600D);
        else assert.deepEqual(result, [mutation==="literal" ? 393995 : 393997, 0x600D]);
      } finally {
        await context.close();
        await file.write(original, 0, original.length, at); await file.close();
      }
    }
    console.log("Standard-size media passed: real 552960000-byte ISO/CUE selections, late CD DMA/IRQ/sum, retained SYSRES/restart/backup, original CD-DA/AudioWorklet, cancellation/rejection and three input mutations.");
  } finally { await rm(directory, {recursive:true, force:true}); }
}
