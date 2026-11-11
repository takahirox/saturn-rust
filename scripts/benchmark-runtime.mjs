// Optional production-Wasm throughput comparison. Original guest by default;
// authorized private firmware stays in local memory and is never served.
import assert from "node:assert/strict";
import { once } from "node:events";
import { readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { resolve } from "node:path";
import { parseArgs } from "node:util";
import { chromium } from "playwright";

const { values } = parseArgs({ options: {
  frames: { type: "string", default: "60" }, runs: { type: "string", default: "3" },
  bios: { type: "string" }, baseline: { type: "string" },
  skip: { type: "string", default: "0" }, buttons: { type: "string" },
  draw: { type: "boolean", default: false }, profile: { type: "string" },
  continuous: { type: "boolean", default: false },
  "vdp1-stores": { type: "boolean", default: false },
} });
const frames = Number(values.frames), runs = Number(values.runs);
const skip = Number(values.skip);
assert(Number.isSafeInteger(skip) && skip >= 0, "skip must be a nonnegative field number");
assert(Number.isSafeInteger(frames) && frames > 0, "frames must be positive");
assert(Number.isSafeInteger(runs) && runs > 0, "runs must be positive");
assert(!values.bios || !values["vdp1-stores"], "--vdp1-stores uses the original guest; omit --bios");
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
for (const [route, directory] of [["pkg", "web/pkg"], ...(values.baseline ? [["baseline", values.baseline]] : [])]) {
  for (const file of ["saturn_rust.js", "saturn_rust_bg.wasm"]) assets.set(`/${route}/${file}`, resolve(directory, file));
}
const server = createServer(async (req, res) => {
  const path = new URL(req.url, "http://localhost").pathname;
  if (req.method !== "GET") { res.writeHead(405).end(); return; }
  if (path === "/") { res.setHeader("Content-Type", "text/html"); res.end("<!doctype html><title>Local runtime benchmark</title>"); return; }
  const file = assets.get(path);
  if (!file) { res.writeHead(404).end(); return; }
  try {
    res.setHeader("Content-Type", path.endsWith(".wasm") ? "application/wasm" : "text/javascript");
    res.end(await readFile(file));
  } catch { res.writeHead(404).end(); }
});
let browser;
try {
  server.listen(0, "127.0.0.1"); await once(server, "listening");
  browser = await chromium.launch({ headless: true, channel: process.env.BROWSER_CHANNEL });
  const page = await browser.newPage();
  const origin = `http://127.0.0.1:${server.address().port}`;
  await page.route("**/*", route => route.request().url().startsWith(origin + "/") && route.request().method() === "GET" ? route.continue() : route.abort());
  await page.goto(origin + "/");
  const variants = values.baseline ? ["baseline", "pkg"] : ["pkg"];
  for (const variant of variants) await page.evaluate(async ({ variant, rom }) => {
    const { default: init, Emulator } = await import(`./${variant}/saturn_rust.js`);
    await init();
    window.runtimeBench ??= {};
    runtimeBench[variant] = { Emulator, rom: Uint8Array.from(rom) };
    const m = new Emulator();
    try { m.reset_bios(runtimeBench[variant].rom); for (let i = 0; i < 3; i++) m.advance_frame(); }
    finally { m.free(); }
  }, { variant, rom: [...rom] });
  const samples = Object.fromEntries(variants.map(variant => [variant, []]));
  const references = new Map();
  const cdp = values.profile ? await page.context().newCDPSession(page) : null;
  if (cdp) { await cdp.send("Profiler.enable"); await cdp.send("Profiler.start"); }
  for (let run = 0; run < runs; run++) for (const variant of run % 2 ? [...variants].reverse() : variants) {
    const result = await page.evaluate(({ variant, frames, draw, skip, buttons, continuous }) => {
      const runtime = runtimeBench[variant];
      const { Emulator, rom } = runtime;
      const fresh = !continuous || !runtime.machine;
      const m = continuous ? (runtime.machine ??= new Emulator()) : new Emulator();
      try {
        if (fresh) m.reset_bios(rom);
        const events = new Map(buttons);
        const advance = () => {
          const field = Number(m.frames());
          if (events.has(field)) m.set_buttons(events.get(field));
          m.advance_frame();
        };
        while (Number(m.frames()) < skip) advance();
        const canvas = draw ? document.createElement("canvas") : null;
        const context = canvas?.getContext("2d");
        const timings = [];
        const start = performance.now();
        for (let i = 0; i < frames; i++) {
          const before = performance.now();
          advance();
          if (draw) {
            const width = m.video_width(), height = m.video_height();
            if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
            context.putImageData(new ImageData(new Uint8ClampedArray(m.framebuffer()), width, height), 0, 0);
          }
          timings.push(performance.now() - before);
        }
        const ms = performance.now() - start;
        const pixels = m.framebuffer().reduce((hash, byte) => Math.imul(hash ^ byte, 16777619) >>> 0, 2166136261);
        timings.sort((a, b) => a - b);
        return { ms, p95FrameMs: timings[Math.ceil(timings.length * 0.95) - 1], maxFrameMs: timings.at(-1),
          state: { frames: Number(m.frames()), clocks: Number(m.peripheral_clocks()), instructions: Number(m.instruction_count()),
          exceptions: Number(m.exception_count()), registers: [...m.registers()], special: [...m.special_registers()],
          soundPc: m.sound_pc(), soundInstructions: m.sound_instruction_count(), soundRegisters: [...m.sound_registers()],
          mailbox: m.read_long(0x06020000), commandWord: m.read_long(0x05C02000), pixels } };
      } finally { if (!continuous) m.free(); }
    }, { variant, frames, draw: values.draw, skip, buttons, continuous: values.continuous });
    const key = values.continuous ? run : 0;
    if (!references.has(key)) references.set(key, result.state);
    assert.deepEqual(result.state, references.get(key), `guest state changed: ${variant}, run ${run}`);
    samples[variant].push({ ms: result.ms, p95FrameMs: result.p95FrameMs, maxFrameMs: result.maxFrameMs });
    console.error(JSON.stringify({ variant, run, endField: result.state.frames, ms: result.ms, fps: frames * 1000 / result.ms }));
  }
  if (cdp) { const { profile } = await cdp.send("Profiler.stop"); await writeFile(values.profile, JSON.stringify(profile)); }
  if (values.continuous) await page.evaluate(() => {
    for (const runtime of Object.values(runtimeBench)) {
      runtime.machine?.free();
      delete runtime.machine;
    }
  });
  const median = times => [...times].sort((a, b) => a - b)[Math.floor(times.length / 2)];
  console.log(JSON.stringify({ guest: values.bios ? "user-provided" : values["vdp1-stores"] ? "original-vdp1-stores" : "original-runtime-loop", browser: browser.version(), frames, runs,
    skip, continuous: values.continuous, draw: values.draw, samples, medianMs: Object.fromEntries(variants.map(v => [v, median(samples[v].map(s => s.ms))])),
    fps: Object.fromEntries(variants.map(v => [v, frames * 1000 / median(samples[v].map(s => s.ms))])),
    speedup: values.baseline ? median(samples.baseline.map(s => s.ms)) / median(samples.pkg.map(s => s.ms)) : null,
    state: references.get(values.continuous ? runs - 1 : 0) }, null, 2));
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }
