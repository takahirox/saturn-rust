// Original guest fixtures exercise actual transferable renderer messages.
// No private firmware/media or synthetic-to-game performance conversion.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { once } from "node:events";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { resolve } from "node:path";
import { parseArgs } from "node:util";
import { chromium } from "playwright";

const { values } = parseArgs({ options: { pkg: { type: "string", default: "web/pkg" }, "interleave-batches": { type: "boolean", default: false } } });
const assets = new Map(), sha256 = {};
for (const [url, file] of [
  ["/pkg/saturn_rust.js", resolve(values.pkg, "saturn_rust.js")],
  ["/pkg/saturn_rust_bg.wasm", resolve(values.pkg, "saturn_rust_bg.wasm")],
  ...["video-worker.js", "video-render-client.js", "playback-worker.js", "playback-client.js", "sound-client.js", "sound-worker.js", "worker-emulator.js"].map(name => ["/" + name, resolve("web", name)]),
]) {
  let bytes = await readFile(file);
  if (url === "/worker-emulator.js" && values["interleave-batches"]) {
    const ordered = "this.commands = this.commands.then(execute, execute);";
    assert(bytes.toString().includes(ordered), "interleaving mutant must alter the command chain");
    bytes = Buffer.from(bytes.toString().replace(ordered, "this.commands = execute();"));
  }
  assets.set(url, bytes);
  sha256[url] = createHash("sha256").update(bytes).digest("hex");
}
const fixtures = [];
for (const [name, marker] of [["rotation-fetch-tile", 0x0600DC0C], ["rotation-fetch-rbg1", 0x0600DC0C],
  ["sprite-rotation16", 0x0600DE18], ["sprite-interlace", 0x0600DF34], ["scsp-pcm", 0x0600ED00]]) {
  const bytes = Buffer.alloc(524288);
  for (const line of (await readFile(`tests/fixtures/${name}.hex`, "utf8")).split(/\r?\n/)) {
    if (!line.trim() || line.startsWith("#")) continue;
    const [at, ...words] = line.trim().split(/\s+/).map(word => parseInt(word, 16));
    words.forEach((word, i) => bytes.writeUInt16BE(word, at + i * 2));
  }
  fixtures.push({ name, marker, rom: [...bytes] });
}
const server = createServer((req, res) => {
  const path = new URL(req.url, "http://localhost").pathname;
  if (req.method !== "GET") { res.writeHead(405).end(); return; }
  if (path === "/") { res.setHeader("Content-Type", "text/html"); res.end("<!doctype html><title>Video Worker verification</title>"); return; }
  const bytes = assets.get(path);
  if (!bytes) { res.writeHead(404).end(); return; }
  res.setHeader("Content-Type", path.endsWith(".wasm") ? "application/wasm" : "text/javascript");
  res.end(bytes);
});
let browser;
try {
  server.listen(0, "127.0.0.1"); await once(server, "listening");
  browser = await chromium.launch({ headless: true, channel: process.env.BROWSER_CHANNEL });
  const page = await browser.newPage();
  const origin = `http://127.0.0.1:${server.address().port}`;
  await page.route("**/*", route => route.request().url().startsWith(origin + "/") && route.request().method() === "GET" ? route.continue() : route.abort());
  await page.goto(origin + "/");
  const result = await page.evaluate(async fixtures => {
    const { default: init, Emulator } = await import("./pkg/saturn_rust.js");
    const { WorkerEmulator } = await import("./worker-emulator.js");
    await init();
    const same = (a, b, label) => { if (a.length !== b.length || a.some((x, i) => x !== b[i])) throw new Error(label); };
    const rows = [];
    for (const { name, rom, marker } of fixtures) {
      const reference = new Emulator();
      // Match the producer's queued video scheduler for legacy debugger
      // frame calls. The separate playback oracle compares exact raster
      // boundaries with immediate rendering.
      if (!reference.enable_video_journal(true)) throw new Error("reference renderer setup");
      let failure;
      const remote = new WorkerEmulator({ onError: error => { failure = String(error); } });
      try {
        await remote.ready;
        reference.reset_bios(Uint8Array.from(rom));
        await remote.reset_bios(Uint8Array.from(rom));
        const compare = async () => {
          if (failure) throw new Error(failure);
          const [registers, special, instructions, clocks, hashes, pixels, width, height, audio] = await Promise.all([
            remote.registers(), remote.special_registers(), remote.instruction_count(), remote.peripheral_clocks(),
            remote.loop_state_hashes(), remote.framebuffer(), remote.video_width(), remote.video_height(), remote.audio_samples(8192),
          ]);
          same(registers, reference.registers(), "registers"); same(special, reference.special_registers(), "special");
          if (instructions !== reference.instruction_count() || clocks !== reference.peripheral_clocks()) throw new Error(`${name}: CPU time changed: instructions=${instructions}/${reference.instruction_count()}, clocks=${clocks}/${reference.peripheral_clocks()}`);
          if (hashes.length !== 22 || reference.loop_state_hashes().length !== 22) throw new Error("Full device hash coverage missing");
          same(hashes, reference.loop_state_hashes(), "all22 device hashes");
          same(pixels, reference.framebuffer(), "partial/full image");
          if (width !== reference.video_width() || height !== reference.video_height()) throw new Error("geometry");
          same(audio, reference.audio_samples(8192), "PCM");
        };
        for (const count of [1, 3, 19, 127, 1000, 10000]) {
          reference.run_steps(count); await remote.run_steps(count); await compare();
        }
        for (let i = 0; i < 4; i++) {
          reference.advance_frame(); await remote.advance_frame(); await compare();
        }
        if (await remote.read_long(marker) !== reference.read_long(marker)) throw new Error("guest marker");
        if (name === "rotation-fetch-tile") {
          const before = await remote.instruction_count();
          const counters = Array.from({length:8193}, () => remote.instruction_count());
          // A callback from the first chunk queues a real reset. Every earlier
          // queued read must still observe the pre-reset Machine, including the
          // final chunk; a later JS task cannot overtake it.
          const reset = counters[0].then(() => remote.reset_bios(Uint8Array.from(rom)));
          const values = await Promise.all(counters);
          await reset;
          if (values.some(value => value !== before)) throw new Error("Later reset overtook earlier debugger reads");
          reference.reset_bios(Uint8Array.from(rom));
          await compare();
        }
        // Distinct port-1 triggers catch accidental right/left swapping. A
        // sparse port-2 update must not require a port-1 object to exist.
        reference.set_3d_pad(1, true, 3, 200, 47, 191);
        remote.set_3d_pad(1, true, 3, 200, 47, 191);
        reference.set_buttons(0x80); remote.set_buttons(0x80);
        reference.set_3d_pad_digital(1, 0x40); remote.set_3d_pad_digital(1, 0x40);
        await compare();
        const backup = Uint8Array.from({ length: 32768 }, (_, i) => i * 17 & 255);
        reference.import_backup_ram(backup); await remote.import_backup_ram(backup);
        same(await remote.export_backup_ram(), reference.export_backup_ram(), "backup import/export");
        let rejected = false;
        try { await remote.import_backup_ram(new Uint8Array(7)); } catch { rejected = true; }
        if (!rejected) throw new Error("invalid backup was accepted");
        same(await remote.export_backup_ram(), backup, "rejected import must preserve backup");
        try { reference.reset_bios(new Uint8Array()); } catch { /* Expected native rejection. */ }
        rejected = false;
        try { await remote.reset_bios(new Uint8Array()); } catch { rejected = true; }
        if (!rejected) throw new Error("invalid BIOS was accepted");
        await compare();
        same(await remote.export_backup_ram(), backup, "backup across rejected boot");
        rows.push({ name, partialStepQueries: 6, fixedFrameQueries: 4, fullHashes: 22, input: true, backup: true, rejectedBoot: true });
      } finally { remote.free(); reference.free(); }
    }
    return rows;
  }, fixtures);
  assert.equal(result.length, fixtures.length);
  console.log(JSON.stringify({ browser: browser.version(), sha256, results: result }, null, 2));
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }
