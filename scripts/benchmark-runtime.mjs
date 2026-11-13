// Optional production-Wasm throughput comparison. Original guest by default;
// authorized private firmware stays in local memory and is never served.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { once } from "node:events";
import { readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { resolve } from "node:path";
import { parseArgs } from "node:util";
import { chromium } from "playwright";

const { values } = parseArgs({ options: {
  frames: { type: "string", default: "60" }, runs: { type: "string", default: "3" },
  bios: { type: "string" }, baseline: { type: "string" }, candidate: { type: "string" },
  disc: { type: "string" }, track: { type: "string", multiple: true },
  skip: { type: "string", default: "0" }, buttons: { type: "string" },
  draw: { type: "boolean", default: false }, profile: { type: "string" },
  capture: { type: "string" },
  continuous: { type: "boolean", default: false },
  "vdp1-stores": { type: "boolean", default: false },
} });
const frames = Number(values.frames), runs = Number(values.runs);
const skip = Number(values.skip);
assert(Number.isSafeInteger(skip) && skip >= 0, "skip must be a nonnegative field number");
assert(Number.isSafeInteger(frames) && frames > 0, "frames must be positive");
assert(Number.isSafeInteger(runs) && runs > 0, "runs must be positive");
assert(!values.bios || !values["vdp1-stores"], "--vdp1-stores uses the original guest; omit --bios");
assert(!values.disc || !values["vdp1-stores"], "--vdp1-stores cannot mount a disc");
assert(!values.track?.length || values.disc?.toLowerCase().endsWith(".cue"), "--track requires --disc CUE");
assert(!values.profile || !values.baseline, "profile one runtime at a time; omit --baseline");
assert(!values.capture || values.draw, "--capture requires --draw");
let rom;
if (values.bios) rom = await readFile(values.bios);
else {
  rom = Buffer.alloc(524288);
  for (const line of (await readFile("examples/runtime-loop.hex", "utf8")).split(/\r?\n/)) {
    if (!line.trim() || line.startsWith("#")) continue;
    const [at, ...words] = line.trim().split(/\s+/).map(word => parseInt(word, 16));
    words.forEach((word, i) => rom.writeUInt16BE(word, at + i * 2));
  }
}
if (values["vdp1-stores"]) {
  assert.equal(rom.readUInt32BE(0x224), 0x06020000, "original loop target changed");
  rom.writeUInt32BE(0x05C02000, 0x224);
}
assert.equal(rom.length, 524288, "expected a raw 512 KiB ROM");
const buttons = values.buttons ? (await readFile(values.buttons, "utf8")).split(/\r?\n/)
  .filter(line => line.trim() && !line.startsWith("#")).map(line => {
    const [field, mask, extra] = line.trim().split(/\s+/);
    const event = [Number(field), parseInt(mask, 16)];
    assert(extra === undefined && Number.isSafeInteger(event[0]) && event[0] >= 0
      && Number.isInteger(event[1]) && event[1] >= 0 && event[1] <= 65535, "invalid button event");
    return event;
  }) : [];
for (let i = 1; i < buttons.length; i++) {
  assert(buttons[i - 1][0] < buttons[i][0], "button fields must be strictly increasing");
}
const assets = new Map();
const artifacts = {};
for (const [route, directory] of [["pkg", values.candidate ?? "web/pkg"], ...(values.baseline ? [["baseline", values.baseline]] : [])]) {
  artifacts[route] = {};
  for (const file of ["saturn_rust.js", "saturn_rust_bg.wasm"]) {
    const bytes = await readFile(resolve(directory, file));
    assets.set(`/${route}/${file}`, bytes);
    artifacts[route][file] = createHash("sha256").update(bytes).digest("hex");
  }
}
const server = createServer((req, res) => {
  const path = new URL(req.url, "http://localhost").pathname;
  if (req.method !== "GET") { res.writeHead(405).end(); return; }
  if (path === "/") { res.setHeader("Content-Type", "text/html"); res.end('<!doctype html><title>Local runtime benchmark</title><input id="media" type="file" multiple>'); return; }
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
  if (values.disc) await page.locator("#media").setInputFiles([values.disc, ...(values.track ?? [])]);
  await page.evaluate(async ({ disc, bios }) => {
    if (!disc) return;
    const [image, ...tracks] = [...document.getElementById("media").files];
    const cue = image.name.toLowerCase().endsWith(".cue");
    const files = cue ? tracks : [image];
    const lengths = files.map(file => file.size);
    const bytes = new Uint8Array(lengths.reduce((sum, n) => sum + n, 0));
    let offset = 0;
    for (const file of files) {
      bytes.set(new Uint8Array(await file.arrayBuffer()), offset);
      offset += file.size;
    }
    // Browser-local File reads keep private media out of HTTP and RPC payloads.
    window.runtimeMedia = { cue: cue ? await image.text() : null,
      names: files.map(file => file.name).join("\0"), lengths: Uint32Array.from(lengths), bytes, bios };
  }, { disc: Boolean(values.disc), bios: Boolean(values.bios) });
  const variants = values.baseline ? ["baseline", "pkg"] : ["pkg"];
  for (const variant of variants) await page.evaluate(async ({ variant, rom }) => {
    const { default: init, Emulator } = await import(`./${variant}/saturn_rust.js`);
    await init();
    window.runtimeBench ??= {};
    runtimeBench[variant] = { Emulator, rom: Uint8Array.from(rom) };
    const m = new Emulator();
    runtimeBench[variant].reset = m => {
      const media = window.runtimeMedia;
      if (!media || media.bios) m.reset_bios(runtimeBench[variant].rom);
      if (media?.cue != null) {
        const method = media.bios ? "mount_cue" : "reset_cue";
        m[method](media.cue, media.names, media.lengths, media.bytes);
      } else if (media) m[media.bios ? "mount_disc" : "reset_disc"](media.bytes);
    };
    try { runtimeBench[variant].reset(m); for (let i = 0; i < 3; i++) { m.advance_frame(); m.audio_samples(8192); } }
    finally { m.free(); }
  }, { variant, rom: [...rom] });
  const samples = Object.fromEntries(variants.map(variant => [variant, []]));
  const references = new Map();
  const cdp = values.profile ? await page.context().newCDPSession(page) : null;
  if (cdp) await cdp.send("Profiler.enable");
  for (let run = 0; run < runs; run++) for (const variant of run % 2 ? [...variants].reverse() : variants) {
    // Boot/skip outside the profile as well as the throughput interval.
    await page.evaluate(({ variant, skip, buttons, continuous }) => {
      const runtime = runtimeBench[variant];
      if (!continuous || !runtime.machine) {
        runtime.machine = new runtime.Emulator();
        runtime.reset(runtime.machine);
      }
      const m = runtime.machine, events = new Map(buttons);
      while (Number(m.frames()) < skip) {
        const field = Number(m.frames());
        if (events.has(field)) m.set_buttons(events.get(field));
        m.advance_frame();
        m.audio_samples(8192);
      }
    }, { variant, skip, buttons, continuous: values.continuous });
    if (cdp && run === 0) await cdp.send("Profiler.start");
    const result = await page.evaluate(({ variant, frames, draw, buttons, continuous }) => {
      const runtime = runtimeBench[variant];
      const m = runtime.machine;
      try {
        const events = new Map(buttons);
        const advance = () => {
          const field = Number(m.frames());
          if (events.has(field)) m.set_buttons(events.get(field));
          m.advance_frame();
        };
        const canvas = draw ? document.createElement("canvas") : null;
        const context = canvas?.getContext("2d");
        const timings = [];
        let pcm = 2166136261, audioSamples = 0;
        const start = performance.now();
        for (let i = 0; i < frames; i++) {
          const before = performance.now();
          advance();
          const audio = m.audio_samples(8192);
          audioSamples += audio.length;
          for (const value of audio) pcm = Math.imul(pcm ^ (value & 65535), 16777619) >>> 0;
          if (draw) {
            const width = m.video_width(), height = m.video_height();
            if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
            context.putImageData(new ImageData(new Uint8ClampedArray(m.framebuffer()), width, height), 0, 0);
          }
          timings.push(performance.now() - before);
        }
        const ms = performance.now() - start;
        if (canvas) document.body.replaceChildren(canvas);
        const pixels = m.framebuffer().reduce((hash, byte) => Math.imul(hash ^ byte, 16777619) >>> 0, 2166136261);
        timings.sort((a, b) => a - b);
        return { ms, p50FrameMs: timings[Math.ceil(timings.length * 0.50) - 1],
          p95FrameMs: timings[Math.ceil(timings.length * 0.95) - 1],
          p99FrameMs: timings[Math.ceil(timings.length * 0.99) - 1], maxFrameMs: timings.at(-1),
          state: { frames: Number(m.frames()), clocks: Number(m.peripheral_clocks()), instructions: Number(m.instruction_count()),
          exceptions: Number(m.exception_count()), registers: [...m.registers()], special: [...m.special_registers()],
          status: m.status(), addressErrorPending: m.address_error_pending(),
          slaveRegisters: [...m.slave_registers()], slaveInstructions: m.slave_instruction_count(),
          slavePc: m.slave_pc(), slaveExceptions: m.slave_exception_count(),
          soundPc: m.sound_pc(), soundInstructions: m.sound_instruction_count(), soundRegisters: [...m.sound_registers()],
          soundHalted: m.sound_halted(),
          pcm, audioSamples, audioGenerated: m.audio_generated(), audioDropped: m.audio_dropped(),
          mailbox: m.read_long(0x06020000), commandWord: m.read_long(0x05C02000), pixels } };
      } finally { if (!continuous) { m.free(); delete runtime.machine; } }
    }, { variant, frames, draw: values.draw, buttons, continuous: values.continuous });
    const key = values.continuous ? run : 0;
    if (!references.has(key)) references.set(key, result.state);
    assert.deepEqual(result.state, references.get(key), `guest state changed: ${variant}, run ${run}`);
    samples[variant].push({ ms: result.ms, p50FrameMs: result.p50FrameMs,
      p95FrameMs: result.p95FrameMs, p99FrameMs: result.p99FrameMs, maxFrameMs: result.maxFrameMs });
    if (cdp && run === 0) { const { profile } = await cdp.send("Profiler.stop"); await writeFile(values.profile, JSON.stringify(profile)); }
    console.error(JSON.stringify({ variant, run, endField: result.state.frames, ms: result.ms, fps: frames * 1000 / result.ms }));
  }
  if (values.capture) await page.screenshot({ path: values.capture });
  if (values.continuous) await page.evaluate(() => {
    for (const runtime of Object.values(runtimeBench)) {
      runtime.machine?.free();
      delete runtime.machine;
    }
  });
  const median = times => [...times].sort((a, b) => a - b)[Math.floor(times.length / 2)];
  console.log(JSON.stringify({ guest: values.disc ? "user-provided-disc" : values.bios ? "user-provided" : values["vdp1-stores"] ? "original-vdp1-stores" : "original-runtime-loop", browser: browser.version(), frames, runs,
    skip, continuous: values.continuous, draw: values.draw, artifacts, samples, medianMs: Object.fromEntries(variants.map(v => [v, median(samples[v].map(s => s.ms))])),
    fps: Object.fromEntries(variants.map(v => [v, frames * 1000 / median(samples[v].map(s => s.ms))])),
    combinedFps: Object.fromEntries(variants.map(v => [v, frames * runs * 1000 / samples[v].reduce((sum, s) => sum + s.ms, 0)])),
    combinedSpeedup: values.baseline ? samples.baseline.reduce((sum, s) => sum + s.ms, 0) / samples.pkg.reduce((sum, s) => sum + s.ms, 0) : null,
    speedup: values.baseline ? median(samples.baseline.map(s => s.ms)) / median(samples.pkg.map(s => s.ms)) : null,
    state: references.get(values.continuous ? runs - 1 : 0) }, null, 2));
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }
