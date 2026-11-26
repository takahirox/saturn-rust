import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { parseArgs } from "node:util";
import { createServer } from "node:http";
import { once } from "node:events";
import { createHash } from "node:crypto";
import { resolve } from "node:path";
import { chromium } from "playwright";

// Measures the actual static player, including Canvas and a real AudioWorklet.
// Original firmware by default; optional authorized local inputs are never served.
const {values} = parseArgs({options:{
  web:{type:"string",default:"web"},pkg:{type:"string"},bios:{type:"string"},disc:{type:"string"},
  track:{type:"string",multiple:true},buttons:{type:"string"},warmup:{type:"string",default:"120"},
  seconds:{type:"string",default:"5"},runs:{type:"string",default:"6"},capture:{type:"string"},
  fields:{type:"string"},"display-filter":{type:"string"},
  "min-colors":{type:"string",default:"1"},label:{type:"string",default:"player"},
  unpaced:{type:"boolean",default:false},gpu:{type:"boolean",default:false},"expect-sound-worker":{type:"boolean",default:false},
}});
const web = resolve(values.web), pkg = resolve(values.pkg ?? web + "/pkg"), label = values.label;
const warmup = Number(values.warmup), seconds = Number(values.seconds), runs = Number(values.runs), minColors = Number(values["min-colors"]);
const fixedFields = values.fields === undefined ? null : Number(values.fields);
assert(fixedFields === null || (Number.isSafeInteger(fixedFields) && fixedFields > 0), "fields must be a positive integer");
assert(values["display-filter"] === undefined || ["clean", "composite", "s-video", "rgb"].includes(values["display-filter"]));
assert(Number.isSafeInteger(warmup) && warmup >= 0, "warmup must be a nonnegative field count");
assert(Number.isFinite(seconds) && seconds > 0 && seconds <= 60, "seconds must be in (0,60]");
assert(Number.isSafeInteger(runs) && runs > 0 && runs <= 100, "runs must be in [1,100]");
assert(Number.isSafeInteger(minColors) && minColors > 0, "min-colors must be positive");
assert(!values.track?.length || values.disc?.toLowerCase().endsWith(".cue"), "tracks require a CUE disc");
let bios = values.bios ? resolve(values.bios) : null;
if (!bios && !values.disc) {
  const buffer = Buffer.alloc(524288);
  for (const line of (await readFile("examples/runtime-loop.hex", "utf8")).split(/\r?\n/)) {
    if (!line.trim() || line.startsWith("#")) continue;
    const [at,...words] = line.trim().split(/\s+/).map(value=>parseInt(value,16));
    words.forEach((word,i)=>buffer.writeUInt16BE(word,at+i*2));
  }
  bios = {name:"original-runtime-loop.bin",mimeType:"application/octet-stream",buffer};
}
const events = values.buttons ? (await readFile(values.buttons,"utf8")).split(/\r?\n/).filter(line=>line.trim()&&!line.startsWith("#")).map(line=>{
  const [field,mask,extra] = line.trim().split(/\s+/), event = [Number(field),parseInt(mask,16)];
  assert(extra===undefined && Number.isSafeInteger(event[0]) && event[0]>=0 && Number.isInteger(event[1]) && event[1]>=0 && event[1]<=0x1fff,"invalid controller event");
  return event;
}) : [];
assert(events.every((event,i)=>!i||event[0]>events[i-1][0]),"controller fields must increase");
const assets = new Map(), hashes = {};
for (const file of ["index.html", "style.css", "app.js", "crt-display.js", "audio.js", "audio-worklet.js", "video-render-client.js", "video-worker.js", "worker-emulator.js", "playback-client.js", "playback-worker.js", "sound-client.js", "sound-worker.js"]) {
  try { const bytes = await readFile(web + "/" + file); assets.set("/" + file, bytes); hashes[file] = createHash("sha256").update(bytes).digest("hex"); }
  catch (error) { if (error.code !== "ENOENT" || !["crt-display.js", "worker-emulator.js", "playback-client.js", "playback-worker.js", "sound-client.js", "sound-worker.js"].includes(file)) throw error; }
}
for (const file of ["saturn_rust.js", "saturn_rust_bg.wasm", "saturn_rust_compatible_bg.wasm"]) {
  try {
    const bytes = await readFile(pkg + "/" + file); assets.set("/pkg/" + file, bytes); hashes[file] = createHash("sha256").update(bytes).digest("hex");
  } catch (error) {
    if (file !== "saturn_rust_compatible_bg.wasm" || error.code !== "ENOENT") throw error;
  }
}
const server = createServer((req, res) => {
  const name = new URL(req.url, "http://localhost").pathname;
  const file = name === "/" ? "/index.html" : name;
  if (req.method !== "GET" || !assets.has(file)) { res.writeHead(404).end(); return; }
  res.setHeader("Content-Type", file.endsWith(".wasm") ? "application/wasm" : file.endsWith(".css") ? "text/css" : file.endsWith(".html") ? "text/html" : "text/javascript");
  res.end(assets.get(file));
});
let browser, progress;
try {
  server.listen(0, "127.0.0.1"); await once(server, "listening");
  browser = await chromium.launch({ headless:true, channel:process.env.BROWSER_CHANNEL, args:values.gpu ? ["--enable-gpu"] : [] });
  const page = await browser.newPage(), errors = [], requests = [];
  page.on("pageerror", error => errors.push(error.message));
  const origin = `http://127.0.0.1:${server.address().port}`;
  await page.route("**/*", route => {
    const req = route.request();
    if (req.url().startsWith(origin + "/") && req.method() === "GET" && !req.postData()) return route.continue();
    requests.push({ externalOrWrite:true }); return route.abort();
  });
  await page.goto(origin);
  const playback = page.locator("#play, #start:not([hidden]), #pause:not([hidden])");
  await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  if (bios) {
    await page.locator("#bios").setInputFiles(bios);
    await page.waitForFunction(() => !(document.getElementById("play") ?? document.getElementById("start")).disabled);
  }
  if (values.disc) {
    await page.locator("#disc").setInputFiles([values.disc,...(values.track ?? [])].map(file=>resolve(file)));
    await page.waitForFunction(() => !(document.getElementById("play") ?? document.getElementById("start")).disabled, undefined, {timeout:120000});
  }
  assert.equal(await page.locator("#error").textContent(), "");
  await page.evaluate(async events => {
    const app = await import("./app.js"), m = app.emulator;
    const ctx = document.getElementById("screen").getContext("2d"), draw = ctx.putImageData.bind(ctx);
    const state = window.playbackMeasurement = { app, fields:() => m.state?.frames ?? Number(m.frames()),
      draws:0, events:[], ticks:0, gaps:[], submissions:[], lastDraw:null,
      gpuDraws:0, uploads:0, uploadBytes:0, allocations:0, frameBytes:0 };
    const count = (prototype, name, callback) => {
      const original = prototype[name];
      prototype[name] = function (...args) {
        callback(args);
        return original.apply(this, args);
      };
    };
    const gl = WebGL2RenderingContext.prototype;
    count(gl, "drawArrays", () => state.gpuDraws++);
    count(gl, "texImage2D", args => { state.allocations++; state.uploads++; state.uploadBytes += args.at(-1).byteLength; });
    count(gl, "texSubImage2D", args => { state.uploads++; state.uploadBytes += args.at(-1).byteLength; });
    const keys = [[1,"ArrowUp"],[2,"ArrowDown"],[4,"ArrowLeft"],[8,"ArrowRight"],[16,"KeyX"],[32,"KeyC"],[64,"KeyZ"],[128,"Enter"],[256,"KeyD"],[512,"KeyS"],[1024,"KeyA"],[2048,"KeyQ"],[4096,"KeyW"]];
    let next = 0, mask = 0;
    const presented = () => {
      const now = performance.now();
      if (state.lastDraw !== null) state.gaps.push(now - state.lastDraw);
      state.lastDraw = now;
      state.onPresentation?.(now);
    };
    ctx.putImageData = (...args) => {
      draw(...args); state.draws++;
      if (!m.client) presented();
      const field = state.fields();
      while (next < events.length && events[next][0] <= field) {
        const value = events[next++][1];
        for (const [bit, code] of keys) if (Boolean(mask & bit) !== Boolean(value & bit)) {
          document.getElementById("screen").dispatchEvent(new KeyboardEvent(value & bit ? "keydown" : "keyup", {code,bubbles:true,cancelable:true}));
        }
        mask = value; state.events.push({requested:events[next-1][0],presented:field,mask});
      }
    };
    if (m.client) {
      const present = m.client.onFrame;
      m.client.onFrame = packet => {
        const started = performance.now();
        present(packet);
        state.submissions.push(performance.now() - started);
        state.frameBytes += packet.pixels.byteLength;
        presented();
      };
    }
    state.timer = setInterval(() => state.ticks++, 1);
  }, events);
  if (values["display-filter"]) await page.getByLabel("Display filter", { exact: true }).selectOption(values["display-filter"]);
  if (await page.locator("#audio-enable").isVisible()) await page.locator("#audio-enable").click();
  await playback.click();
  await page.waitForFunction(() => playbackMeasurement.app.audio.context?.state === "running");
  if (values.unpaced) await page.evaluate(async () => {
    const client = playbackMeasurement.app.emulator.client;
    if (!client) throw new Error("Unpaced capacity requires Worker playback");
    await client.play({paced:false});
  });
  progress = setInterval(() => page.evaluate(() => playbackMeasurement.fields()).then(fields => console.error(JSON.stringify({warmupFields:fields,target:warmup}))).catch(()=>{}), 10000);
  await page.waitForFunction(warmup => playbackMeasurement.fields() >= warmup || document.getElementById("error").textContent, warmup, {timeout:Math.max(240000,warmup*1000/15)});
  clearInterval(progress); progress = null;
  assert.equal(await page.locator("#error").textContent(), "");
  const windows = [];
  for (let i = 0; i < runs; i++) windows.push(await page.evaluate(async ({ seconds, fixedFields }) => {
    const s = playbackMeasurement, audio = {...s.app.audio.stats};
    const before = {time:fixedFields === null ? performance.now() : s.lastDraw ?? performance.now(),
      fields:s.fields(),draws:s.draws,ticks:s.ticks,gaps:s.gaps.length,submissions:s.submissions.length,
      gpuDraws:s.gpuDraws, uploads:s.uploads, uploadBytes:s.uploadBytes, allocations:s.allocations, frameBytes:s.frameBytes};
    const end = await new Promise((resolve, reject) => {
      if (fixedFields === null) setTimeout(() => resolve(performance.now()), seconds * 1000);
      else {
        const timeout = setTimeout(() => reject(new Error("Timed out waiting for presented fields")), 120000);
        s.onPresentation = now => {
          if (s.fields() - before.fields < fixedFields) return;
          s.onPresentation = null;
          clearTimeout(timeout);
          resolve(now);
        };
      }
    });
    const elapsed = end - before.time;
    const gaps = s.gaps.slice(before.gaps).sort((a,b)=>a-b);
    const submissions = s.submissions.slice(before.submissions).sort((a,b)=>a-b);
    const p95 = samples => samples[Math.ceil(samples.length * .95) - 1];
    return { elapsed,fields:s.fields()-before.fields,draws:s.draws-before.draws,ticks:s.ticks-before.ticks,
      p95:p95(gaps),p99:gaps[Math.ceil(gaps.length*.99)-1],submissionP95:p95(submissions),
      gpuDraws:s.gpuDraws-before.gpuDraws, uploads:s.uploads-before.uploads, uploadBytes:s.uploadBytes-before.uploadBytes,
      allocations:s.allocations-before.allocations, workerFrameBytes:s.frameBytes-before.frameBytes,
      audioBefore:audio,audioAfter:{...s.app.audio.stats},fps:(s.fields()-before.fields)*1000/elapsed };
  }, { seconds, fixedFields }));
  await playback.click();
  await page.waitForFunction(() => document.getElementById("main").getAttribute("aria-busy") !== "true");
  const final = await page.evaluate(async () => {
    const s = playbackMeasurement; clearInterval(s.timer);
    const canvas = document.getElementById("screen"), data = canvas.getContext("2d").getImageData(0,0,canvas.width,canvas.height).data;
    const colors = new Set(); for(let i=0;i<data.length;i+=4)colors.add(`${data[i]},${data[i+1]},${data[i+2]}`);
    const producer = s.app.emulator.client ? await s.app.emulator.client.inspect() : null;
    const crt = document.getElementById("crt-screen");
    const gl = crt && !crt.hidden ? crt.getContext("webgl2") : null;
    const info = gl?.getExtension("WEBGL_debug_renderer_info");
    return {fields:s.fields(),draws:s.draws,inputEvents:s.events,colors:colors.size,audio:{...s.app.audio.stats},
      display: { status: document.getElementById("display-filter-status")?.textContent,
        renderer: info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : null,
        size: gl ? [gl.drawingBufferWidth, gl.drawingBufferHeight] : [canvas.width, canvas.height],
        dpr: devicePixelRatio, peakPendingFrames: producer?.peakPendingFrames },
      soundTransport:producer?.soundTransport ?? null};
  });
  if (values.capture) await page.locator("#screen").screenshot({path:resolve(values.capture)});
  assert.equal(errors.length, 0, JSON.stringify(errors)); assert.equal(requests.length, 0);
  assert.equal(final.inputEvents.length, events.filter(event=>event[0]<=final.fields).length);
  assert(final.colors >= minColors, "image has fewer colors than requested");
  if (values["expect-sound-worker"]) assert(final.soundTransport?.jobs > 0 && !final.soundTransport.disabled, "actual Sound Worker must remain active");
  assert(windows.every(window=>window.fields>0&&window.fields===window.draws), "every measured field must be presented");
  if (fixedFields !== null) assert(windows.every(window => window.fields === fixedFields));
  if (values["display-filter"] && values["display-filter"] !== "clean") {
    assert.equal(final.display.status, "", "CRT must render instead of falling back");
    assert(windows.every(window => window.gpuDraws >= window.fields && window.uploads === window.fields), "every field must be uploaded and drawn");
  }
  const result = { label,normalProductionUi:true,paced:!values.unpaced,localBiosProvided:Boolean(values.bios),localMediaProvided:Boolean(values.disc),AudioWorklet:true,notStrictPrivateStateDifferential:true,
    browser:browser.version(),displayFilter:values["display-filter"] ?? "default",hashes,warmup,fixedFields,inputEvents:events.length,windows,final,errors,
    combinedFps:windows.reduce((sum,w)=>sum+w.fields,0)*1000/windows.reduce((sum,w)=>sum+w.elapsed,0) };
  console.log(JSON.stringify(result,null,2));
} finally { clearInterval(progress); await browser?.close(); await new Promise(resolve => { server.close(resolve); server.closeAllConnections(); }); }
