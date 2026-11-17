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

const { values } = parseArgs({ options: { pkg: { type: "string", default: "web/pkg" }, direct: { type: "boolean", default: false }, "delay-audio": { type: "boolean", default: false }, "delay-receipt": { type: "boolean", default: false }, "early-credit": { type: "boolean", default: false }, "sound-error": { type: "boolean", default: false }, "controller-disabled": { type: "boolean", default: false } } });
const assets = new Map(), sha256 = {};
for (const [url, file] of [
  ["/pkg/saturn_rust.js", resolve(values.pkg, "saturn_rust.js")],
  ["/pkg/saturn_rust_bg.wasm", resolve(values.pkg, "saturn_rust_bg.wasm")],
  ...["video-worker.js", "video-render-client.js", "playback-worker.js", "playback-client.js", "sound-client.js", "sound-worker.js"].map(name => ["/" + name, resolve("web", name)]),
]) {
  let bytes = await readFile(file);
  if (url === "/playback-worker.js" && values["delay-audio"]) {
    const send = 'self.postMessage({ type: "audio", sequence: id, epoch: fieldEpoch, pcm }, [pcm.buffer]);';
    assert(bytes.toString().includes(send), "controlled PCM delay must alter the served producer");
    bytes = Buffer.from(bytes.toString().replace(send, `setTimeout(() => ${send.slice(0, -1)}, 100);`));
  }
  if (url === "/playback-worker.js" && values["early-credit"]) {
    const source = bytes.toString();
    const finish = "}).then(() => { pending.delete(id); });";
    const acknowledge = "    item.presented = true;";
    assert(source.includes(finish) && source.includes(acknowledge), "early-credit mutant must alter both completion sites");
    bytes = Buffer.from(source.replace(finish, "});").replace(acknowledge, "    pending.delete(data.sequence);\n" + acknowledge));
  }
  if (url === "/video-worker.js" && values["delay-receipt"]) {
    const source = bytes.toString();
    const start = source.indexOf('    self.postMessage({ type: "frame", sequence: data.sequence');
    const end = source.indexOf("\n", start);
    const close = source.indexOf("\n", end + 1);
    assert(start >= 0 && close > end, "controlled renderer delay must alter its receipt");
    const send = source.slice(start, close).trim();
    assert(send.endsWith(";"));
    bytes = Buffer.from(source.slice(0, start) + "    " + send.replace("self.postMessage(", "((...args) => setTimeout(() => self.postMessage(...args), 250))(") + source.slice(close));
  }
  if (url === "/sound-worker.js" && values["sound-error"]) {
    const finish = "const state=actor.snapshot_at(data.at); owned=false;";
    assert(bytes.toString().includes(finish), "controlled Sound failure must alter the served return");
    bytes = Buffer.from(bytes.toString().replace(finish, 'throw new Error("Controlled Sound return failure");'));
  }
  if (url === "/playback-worker.js" && values["controller-disabled"]) {
    const old = "  await sound.reset();"; assert(bytes.toString().includes(old));
    bytes = Buffer.from(bytes.toString().replace(old, old + `\n  machine.enable_sound_controller_independence(${!values["controller-disabled"]});`));
  }
  assets.set(url, bytes);
  sha256[url] = createHash("sha256").update(bytes).digest("hex");
}
const fixtures = [];
for (const [name, marker] of [["rotation-fetch-tile", 0x0600DC0C], ["rotation-fetch-rbg1", 0x0600DC0C],
  ["sprite-rotation16", 0x0600DE18], ["sprite-interlace", 0x0600DF34], ["scsp-pcm", 0x0600ED00], ["scsp-clock", 0x05A00804], ["scsp-clock-controller", 0x05A00804]]) {
  const bytes = Buffer.alloc(524288);
  for (const line of (await readFile(`tests/fixtures/${name === "scsp-clock-controller" ? "scsp-clock" : name}.hex`, "utf8")).split(/\r?\n/)) {
    if (!line.trim() || line.startsWith("#")) continue;
    const [at, ...words] = line.trim().split(/\s+/).map(word => parseInt(word, 16));
    words.forEach((word, i) => bytes.writeUInt16BE(word, at + i * 2));
  }
  if (name === "scsp-clock-controller") {
    assert.equal(bytes.readUInt16BE(0x294), 0x001B);
    [0xD101, 0x412B, 9, 9, 0, 0x8000].forEach((word, i) => bytes.writeUInt16BE(word, 0x294 + i*2));
    const words = [], fixes = [];
    for (const [offset, value] of [[1,1],[3,0],[5,0xF0],[0x63,1],[0x1F,0x10]]) {
      fixes.push([words.length, 0x20100000 + offset]); words.push(0, 0xE000 | value, 0x2100);
    }
    fixes.push([words.length,0x20100063]); words.push(0);
    const poll = words.length; words.push(0x6010,0xC801);
    words.push(0x8B00 | ((poll - words.length - 2) & 255));
    words.push(0xA000 | ((-words.length - 2) & 4095),9);
    if (words.length % 2) words.push(9);
    for (const [at, value] of fixes) {
      const target = 0x8000 + words.length*2, pc = (0x8000 + at*2 + 4) & ~3;
      assert((target-pc)%4===0 && target>=pc && target-pc<1024);
      words[at] = 0xD100 | ((target-pc)/4); words.push(value >>> 16,value & 65535);
    }
    words.forEach((word,i)=>bytes.writeUInt16BE(word,0x8000+i*2));
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
  const result = await page.evaluate(async ({ fixtures, direct, soundError }) => {
    const { default: init, Emulator } = await import("./pkg/saturn_rust.js");
    const { PlaybackClient } = await import("./playback-client.js");
    await init();
    const timed = async promise => {
      let timeout;
      try { return await Promise.race([promise, new Promise((_, reject) => { timeout = setTimeout(() => reject(new Error("Playback verification timed out")), 15000); })]); }
      finally { clearTimeout(timeout); }
    };
    const same = (a, b, label) => { if (a.length !== b.length || a.some((x, i) => x !== b[i])) throw new Error(label); };
    let ticks = 0;
    const timer = setInterval(() => ticks++, 1);
    const results = [];
    try {
      for (const { name, marker, rom } of fixtures) {
        const reference = new Emulator();
        const audio = new Map();
        let fields = 0, nonzero = 0, lastFrame = 0, failure, afterAudio = null;
        const client = new PlaybackClient({
          directPresentation: direct,
          onError: error => { failure = error; },
          onAudio: packet => { audio.set(packet.sequence, packet.pcm); afterAudio?.(); },
          onFrame: packet => {
            reference.advance_video_field();
            if (packet.fields !== Number(reference.frames()) || packet.clocks !== Number(reference.peripheral_clocks())) throw new Error("Field time changed");
            const pcm = reference.audio_samples(8192), received = audio.get(packet.sequence);
            if (!received) throw new Error("Frame arrived without its PCM");
            same(received, pcm, "PCM changed"); audio.delete(packet.sequence);
            nonzero += received.filter(value => value !== 0).length;
            if (name === "scsp-pcm" && received.some(value => ![0, 0x2000, 0x4000, 0x6000].includes(value))) throw new Error("Original PCM waveform changed");
            if (packet.width !== reference.video_width() || packet.height !== reference.video_height()) throw new Error("Geometry changed");
            same(packet.pixels, reference.framebuffer(), "Image changed");
            fields++; lastFrame = packet.fields;
          },
        });
        const compare = state => {
          if (state.coreHashes?.length !== 22 || reference.loop_state_hashes().length !== 22) throw new Error("Private CPU/cache hash coverage missing");
          same(state.coreHashes, [...reference.loop_state_hashes()], "Private CPU/cache state changed");
          same(state.registers, [...reference.registers()], "Master registers changed");
          same(state.special, [...reference.special_registers()], "Special registers changed");
          same(state.slaveRegisters, [...reference.slave_registers()], "Slave registers changed");
          same(state.soundRegisters, [...reference.sound_registers()], "Sound registers changed");
          for (const [key, method] of [["instructions", "instruction_count"], ["exceptions", "exception_count"],
            ["status", "status"], ["addressErrorPending", "address_error_pending"], ["slavePc", "slave_pc"],
            ["slaveInstructions", "slave_instruction_count"], ["slaveExceptions", "slave_exception_count"],
            ["soundPc", "sound_pc"], ["soundInstructions", "sound_instruction_count"], ["soundHalted", "sound_halted"],
            ["audioGenerated", "audio_generated"], ["audioDropped", "audio_dropped"]]) {
            if (state[key] !== reference[method]()) throw new Error("Scalar state changed: " + key);
          }
          if (state.frames !== lastFrame || state.pendingFrames || state.pendingRenderFrames || state.peakPendingFrames > 2 || audio.size) throw new Error("Playback did not drain within its bound");
        };
        try {
          await timed(client.ready);
          for (let boot = 0; boot < 2; boot++) {
            reference.reset_bios(Uint8Array.from(rom));
            await timed(client.reset({ mode: "bios", rom: Uint8Array.from(rom) }));
            reference.set_buttons(boot ? 0x80 : 0); reference.set_port2_buttons(0x10);
            client.setInput({ buttons: boot ? 0x80 : 0, port2Buttons: 0x10 });
            for (let i = 0; i < 4; i++) compare(await timed(client.advance(2)));
            if (reference.read_long(marker) !== 0x600D) throw new Error("Original guest did not finish");
          }
          if (name === "scsp-pcm" && nonzero === 0) throw new Error("PCM must include the original nonzero waveform");
          // Autoplay pause drains every already committed field before inspection.
          const from = fields;
          await timed(client.play({ paced: false }));
          while (fields < from + 3) { if (failure) throw failure; await new Promise(resolve => setTimeout(resolve, 1)); }
          compare(await timed(client.pause()));
          const stopped = fields;
          await new Promise(resolve => setTimeout(resolve, 20));
          if (fields !== stopped) throw new Error("Paused playback kept advancing");
          // Reset as soon as PCM arrives, while that field's image/renderer
          // receipt can still be outstanding on a different message channel.
          let resetting;
          const requested = new Promise(resolve => {
            afterAudio = () => {
              afterAudio = null;
              resetting = client.reset({ mode: "bios", rom: Uint8Array.from(rom) });
              reference.reset_bios(Uint8Array.from(rom));
              lastFrame = 0;
              audio.clear();
              resolve();
            };
          });
          await timed(client.play({ paced: false }));
          await timed(requested);
          compare(await timed(resetting));
          reference.set_buttons(0); reference.set_port2_buttons(0);
          client.setInput({ buttons: 0, port2Buttons: 0 });
          compare(await timed(client.advance(2)));
          const final = await timed(client.inspect());
          if (name === "scsp-clock" || name === "scsp-clock-controller") {
            if (!(final.soundTransport.jobs > 0)) throw new Error("Original sound CPU must exercise actual offload");
            if (final.soundTransport.disabled !== soundError) throw new Error("Sound failure/fallback policy was not exercised");
          }
          if (name === "scsp-clock-controller" && !soundError && final.soundTransport.jobs > fields * 2) throw new Error("Controller-only guest must avoid per-command Sound ownership returns");
          results.push({ name, fields, nonzero, stopped: fields, inFlightReset: true,
            soundTransport: final.soundTransport, peak: final.peakPendingFrames });
        } finally { client.close(); reference.free(); }
      }
      return { fixtures: results, mainTimerTicks: ticks };
    } finally { clearInterval(timer); }
  }, { fixtures, direct: values.direct, soundError: values["sound-error"] });
  assert.equal(result.fixtures.length, fixtures.length);
  assert(result.mainTimerTicks > 0, "main thread must service timers");
  console.log(JSON.stringify({ browser: browser.version(), cross_origin_isolated: await page.evaluate(() => crossOriginIsolated),
    directPresentation: values.direct, delayedPcmMs: values["delay-audio"] ? 100 : 0, delayedReceiptMs: values["delay-receipt"] ? 250 : 0, earlyCreditMutant: values["early-credit"],
    contract: "original RBG/VDP1/interlace/nonzero-PCM guests, two cold boots, input, autoplay/pause, full CPU/device/PCM and every-frame bytes; two-field presentation credits", sha256, results: result }, null, 2));
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }
