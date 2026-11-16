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

const { values } = parseArgs({ options: { pkg: { type: "string", default: "web/pkg" } } });
const assets = new Map(), sha256 = {};
for (const [url, file] of [
  ["/pkg/saturn_rust.js", resolve(values.pkg, "saturn_rust.js")],
  ["/pkg/saturn_rust_bg.wasm", resolve(values.pkg, "saturn_rust_bg.wasm")],
  ...["video-worker.js", "video-render-client.js"].map(name => ["/" + name, resolve("web", name)]),
]) {
  const bytes = await readFile(file);
  assets.set(url, bytes);
  sha256[url] = createHash("sha256").update(bytes).digest("hex");
}
const fixtures = [];
for (const [name, marker] of [["rotation-fetch-tile", 0x0600DC0C], ["rotation-fetch-rbg1", 0x0600DC0C],
  ["sprite-rotation16", 0x0600DE18], ["sprite-interlace", 0x0600DF34]]) {
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
    const { VideoRenderClient } = await import("./video-render-client.js");
    await init();
    if (!Emulator.prototype.enable_video_transport) throw new Error("Use --features kernel-experiment");
    const same = (left, right, label) => {
      if (left.length !== right.length || left.some((n, i) => n !== right[i])) throw new Error(label);
    };
    const results = [];
    for (const { name, marker, rom } of fixtures) {
      const reference = new Emulator(), producer = new Emulator(), client = new VideoRenderClient();
      let fields = 0, pixels = 0;
      try {
        await client.ready; client.activate();
        if (!producer.enable_video_transport(true)) throw new Error("Transport did not initialize");
        for (let boot = 0; boot < 2; boot++) {
          reference.reset_bios(Uint8Array.from(rom)); producer.reset_bios(Uint8Array.from(rom));
          let last, doneAt;
          const pending = [];
          const verify = async item => {
            const receipt = await item.promise;
            if (receipt.frames !== item.frames || receipt.width !== item.width || receipt.height !== item.height) throw new Error("Field geometry changed");
            same(receipt.pixels, item.pixels, "Worker pixels changed");
            pixels += receipt.pixels.length;
          };
          for (let i = 0; i < 34; i++) {
            await client.waitForCapacity(); client.activate();
            reference.advance_video_field(); producer.advance_video_field();
            same([...reference.loop_state_hashes()], [...producer.loop_state_hashes()], "Full device state changed");
            same([...reference.registers(), ...reference.special_registers()], [...producer.registers(), ...producer.special_registers()], "CPU state changed");
            same(reference.audio_samples(8192), producer.audio_samples(8192), "PCM changed");
            if (reference.frames() !== producer.frames() || reference.peripheral_clocks() !== producer.peripheral_clocks()) throw new Error("Guest time changed");
            pending.push({ promise: client.submit(Number(producer.frames())), frames: Number(reference.frames()),
              width: reference.video_width(), height: reference.video_height(), pixels: reference.framebuffer() });
            fields++;
            if (pending.length >= 2) await verify(pending.shift());
            last = reference.read_long(marker);
            if (last === 0x600D && doneAt === undefined) doneAt = i;
            // Complete two additional scanouts after the guest marker.
            if (doneAt !== undefined && i >= doneAt + 2) break;
          }
          while (pending.length) await verify(pending.shift());
          if (last !== 0x600D) throw new Error("Original guest did not finish");
        }
        if (client.pending.size || client.postedBytes !== client.acknowledgedBytes || client.peakPendingFrames > 2) throw new Error("Transport did not drain within its bound");
        results.push({ name, fields, pixels, chunks: client.chunkSequence, bytes: client.postedBytes,
          peakPendingFrames: client.peakPendingFrames, peakPendingBytes: client.peakPendingBytes });
      } finally { client.close(); producer.free(); reference.free(); }
    }
    return results;
  }, fixtures);
  assert.equal(result.length, fixtures.length);
  const badOrder = await page.evaluate(async () => {
    const worker = new Worker("./video-worker.js", { type: "module" });
    let timeout;
    try {
      return await new Promise((resolve, reject) => {
        timeout = setTimeout(() => reject(new Error("Out-of-order Worker did not terminate")), 15000);
        worker.onerror = event => reject(new Error(event.message));
        worker.onmessage = ({ data }) => {
          if (data.type === "ready") worker.postMessage({ type: "chunk", sequence: 1, bytes: new Uint8Array() });
          else if (data.type === "error") resolve(data.message);
          else reject(new Error("Out-of-order command was accepted"));
        };
      });
    } finally { clearTimeout(timeout); worker.terminate(); }
  });
  assert.equal(badOrder, "Video chunk order changed");
  await page.route("**/pkg/saturn_rust_bg.wasm", route => route.fulfill({ status: 200, contentType: "application/wasm", body: Buffer.from("invalid") }));
  const failedStartup = await page.evaluate(async () => {
    const { VideoRenderClient } = await import("./video-render-client.js");
    const client = new VideoRenderClient();
    let timeout;
    try {
      const outcome = await Promise.race([
        client.ready.then(() => "unexpected-ready", () => "rejected"),
        new Promise(resolve => { timeout = setTimeout(() => resolve("timeout"), 15000); }),
      ]);
      return outcome;
    } finally { clearTimeout(timeout); client.close(); }
  });
  assert.equal(failedStartup, "rejected", "Wasm initialization failures must reject readiness");
  console.log(JSON.stringify({ browser: browser.version(), cross_origin_isolated: await page.evaluate(() => crossOriginIsolated),
    contract: "original RBG/VDP1/interlace guests, two cold boots, full CPU/device/PCM and every-frame bytes; bounded two-field transport", sha256, badOrder, failedStartup, results: result }, null, 2));
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }
