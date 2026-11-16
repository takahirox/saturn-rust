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

const { values } = parseArgs({ options: { pkg: { type: "string", default: "web/pkg" }, direct: { type: "boolean", default: false } } });
const assets = new Map(), sha256 = {};
for (const [url, file] of [
  ["/pkg/saturn_rust.js", resolve(values.pkg, "saturn_rust.js")],
  ["/pkg/saturn_rust_bg.wasm", resolve(values.pkg, "saturn_rust_bg.wasm")],
  ...["video-worker.js", "video-render-client.js", "playback-worker.js", "playback-client.js", "audio.js", "audio-worklet.js"].map(name => ["/" + name, resolve("web", name)]),
]) {
  const bytes = await readFile(file);
  assets.set(url, bytes);
  sha256[url] = createHash("sha256").update(bytes).digest("hex");
}
const rom = Buffer.alloc(524288);
for (const line of (await readFile("tests/fixtures/scsp-pcm.hex", "utf8")).split(/\r?\n/)) {
  if (!line.trim() || line.startsWith("#")) continue;
  const [at, ...words] = line.trim().split(/\s+/).map(word => parseInt(word, 16));
  words.forEach((word, i) => rom.writeUInt16BE(word, at + i * 2));
}
const server = createServer((req, res) => {
  const path = new URL(req.url, "http://localhost").pathname;
  if (req.method !== "GET") { res.writeHead(405).end(); return; }
  if (path === "/") { res.setHeader("Content-Type", "text/html"); res.end('<!doctype html><title>Playback audio verification</title><button id="audio">Enable audio</button><canvas id="screen"></canvas>'); return; }
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
  await page.evaluate(async ({ rom, direct }) => {
    const { PlaybackClient } = await import("./playback-client.js");
    const { AudioOutput } = await import("./audio.js");
    const output = new AudioOutput();
    const canvas = document.getElementById("screen"), context = canvas.getContext("2d");
    const state = { fields: 0, audioPackets: 0, queued: [], reports: 0, ticks: 0, gaps: [], failure: null, previous: null };
    const client = new PlaybackClient({
      directPresentation: direct,
      onError: error => { state.failure = String(error); },
      onAudio: ({ pcm }) => {
        state.audioPackets++;
        if (output.paused) {
          state.queued.push(pcm);
          if (state.queued.length >= 2) {
            const merged = new Int16Array(state.queued.reduce((sum, data) => sum + data.length, 0));
            let at = 0;
            for (const data of state.queued) { merged.set(data, at); at += data.length; }
            if (!output.start(merged)) throw new Error("Audio start was not accepted");
            state.queued = [];
          }
        } else output.deliver(pcm);
      },
      onFrame: data => {
        if (data.fields !== state.fields + 1) throw new Error("A field was skipped");
        state.fields = data.fields;
        const now = performance.now();
        if (state.previous !== null) state.gaps.push(now - state.previous);
        state.previous = now;
        if (canvas.width !== data.width || canvas.height !== data.height) { canvas.width = data.width; canvas.height = data.height; }
        context.putImageData(new ImageData(new Uint8ClampedArray(data.pixels), data.width, data.height), 0, 0);
      },
    });
    await client.ready;
    await client.reset({ mode: "bios", rom: Uint8Array.from(rom) });
    document.getElementById("audio").onclick = async () => {
      try {
        await output.enable();
        const original = output.node.port.onmessage;
        output.node.port.onmessage = event => { original(event); if (event.data.stats) state.reports++; };
        window.audioReady = true;
      } catch (error) { state.failure = String(error); }
    };
    window.audioTest = { client, output, state };
  }, { rom: [...rom], direct: values.direct });
  await page.click("#audio");
  await page.waitForFunction(() => window.audioReady || window.audioTest.state.failure, null, { timeout: 15000 });
  const result = await page.evaluate(async () => {
    const { client, output, state } = window.audioTest;
    const timer = setInterval(() => state.ticks++, 1);
    const before = { ...output.stats };
    const start = performance.now();
    try {
      if (state.failure) throw new Error(state.failure);
      await client.play();
      await new Promise(resolve => setTimeout(resolve, 5000));
      const snapshot = await client.pause();
      const elapsed = performance.now() - start;
      output.setPaused(true);
      const report = state.reports;
      for (let i = 0; i < 100 && (state.reports <= report || output.pending); i++) await new Promise(resolve => setTimeout(resolve, 5));
      if (state.failure) throw new Error(state.failure);
      const quantile = q => [...state.gaps].sort((a, b) => a - b)[Math.ceil(state.gaps.length * q) - 1];
      return { elapsedMs: elapsed, fields: state.fields, audioPackets: state.audioPackets,
        referenceClocks: snapshot.clocks, emulatedMs: snapshot.clocks * 1000 / 26874100,
        peakPendingFrames: snapshot.peakPendingFrames, pendingFrames: snapshot.pendingFrames,
        mainTimerTicks: state.ticks, reports: state.reports, pendingAudio: output.pending,
        p95PresentationGapMs: quantile(0.95), p99PresentationGapMs: quantile(0.99),
        contextState: output.context.state, sampleRate: output.context.sampleRate,
        received: output.stats.received - before.received, rendered: output.stats.rendered - before.rendered,
        underflow: output.stats.underflow - before.underflow, dropped: output.stats.dropped - before.dropped,
        peak: output.stats.peak };
    } finally { clearInterval(timer); client.close(); await output.context.close(); }
  });
  console.error("PLAYBACK_AUDIO_RESULT " + JSON.stringify(result));
  assert.equal(result.contextState, "running");
  assert.equal(result.audioPackets, result.fields);
  assert(result.fields >= 280 && result.fields <= 320, "normal field pacing");
  assert(result.rendered > 44100 * 4, "Worklet must actually render audio");
  assert(result.peak > 0.74 && result.peak <= 0.75, "original nonzero waveform");
  assert.equal(result.dropped, 0);
  assert.equal(result.underflow, 0);
  assert.equal(result.pendingAudio, 0);
  assert.equal(result.pendingFrames, 0);
  assert(result.peakPendingFrames <= 2);
  assert(result.mainTimerTicks > 100);
  assert(result.emulatedMs > result.elapsedMs * 0.9 && result.emulatedMs < result.elapsedMs * 1.05);
  console.log(JSON.stringify({ browser: browser.version(), cross_origin_isolated: await page.evaluate(() => crossOriginIsolated),
    contract: "original SCSP PCM ROM, normal paced Worker playback, Canvas and real AudioWorklet; no game-FPS claim", sha256, results: result }, null, 2));
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }
