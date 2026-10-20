// Runs the actual static UI and Wasm in Chromium. Synthetic vectors are original
// test data; no Sega firmware or game data is used.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { lstat, mkdtemp, readFile, readdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright";

// Fail before upload if web/ includes anything beyond the public runtime assets.
const assets = ["app.js", "index.html", "pkg/saturn_rust.js", "pkg/saturn_rust_bg.wasm", "style.css", "star-collector.iso", "star-collector-LICENSE.txt"];
assert.deepEqual((await readdir("web", { recursive: true })).sort(), [...assets, "pkg"].sort());
assert((await lstat("web/pkg")).isDirectory());
for (const asset of assets) {
  const stat = await lstat(join("web", asset));
  assert(stat.isFile() && stat.size > 0, `Expected a nonempty regular file: ${asset}`);
}

// Mount exactly web/ beneath the project-page prefix, with no repository fallback.
const mount = await mkdtemp(join(tmpdir(), "saturn-rust-browser-"));
const prefix = "/saturn-rust/";
let server;
let browser;
try {
  await symlink(resolve("web"), join(mount, "saturn-rust"), "dir");
  server = spawn("python3", ["-u", "-m", "http.server", "0", "--bind", "127.0.0.1", "--directory", mount], { stdio: ["ignore", "pipe", "pipe"] });
  const address = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("Static server did not start")), 10000);
    server.once("error", (error) => { clearTimeout(timer); reject(error); });
    server.once("exit", (code) => { clearTimeout(timer); reject(new Error(`Static server exited: ${code}`)); });
    server.stdout.on("data", (data) => {
      const match = String(data).match(/port (\d+)/);
      if (match) { clearTimeout(timer); resolve(`http://127.0.0.1:${match[1]}`); }
    });
  });
  browser = await chromium.launch({ headless: true, ...(process.env.BROWSER_CHANNEL ? { channel: process.env.BROWSER_CHANNEL } : {}) });
  const context = await browser.newContext();
  const page = await context.newPage();
  const pageErrors = [];
  const requests = [];
  const responses = new Map();
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("request", (request) => requests.push({ url: request.url(), method: request.method(), body: request.postData() }));
  page.on("response", (response) => responses.set(new URL(response.url()).pathname, response));
  await page.goto(address + prefix);
  await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  for (const [asset, mime] of [
    ["", "text/html"], ["style.css", "text/css"], ["app.js", "javascript"],
    ["pkg/saturn_rust.js", "javascript"], ["pkg/saturn_rust_bg.wasm", "application/wasm"],
  ]) {
    const response = responses.get(prefix + asset);
    assert(response, `Missing project-page asset: ${asset}`);
    assert.equal(response.status(), 200);
    assert(response.headers()["content-type"].includes(mime), `Wrong MIME type: ${asset}`);
    if (asset.endsWith(".wasm")) {
      assert.deepEqual((await response.body()).subarray(0, 4), Buffer.from([0, 97, 115, 109]));
    }
  }
  assert(await page.evaluate(() => [...document.styleSheets].some((sheet) => sheet.href?.endsWith("/saturn-rust/style.css") && sheet.cssRules.length > 0)));
  const value = (id) => page.locator(`#${id}`).textContent();
  const showView = (name) => page.locator(`#view-${name}`).click();
  const settings = page.locator("#settings > summary");
  const machine = () => page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    return { registers: Array.from(emulator.registers()), special: Array.from(emulator.special_registers()),
      retired: String(emulator.instruction_count()), frames: String(emulator.frames()),
      exceptions: emulator.exception_count(), lastException: emulator.last_exception(),
      delay: emulator.delay_target() ?? null, mode: emulator.mode(), status: emulator.status() };
  });
  // waitForFunction predicates must be synchronous in this Playwright version.
  await page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    window.emulatedFrames = () => Number(emulator.frames());
  });
  const responsive = async (isPlay) => {
    for (const [width, height] of [[1440, 900], [768, 1024], [390, 844]]) {
      await page.setViewportSize({ width, height });
      const layout = await page.evaluate(() => {
        const canvas = document.getElementById("screen");
        const rect = canvas.getBoundingClientRect();
        return { overflow: document.documentElement.scrollWidth > innerWidth,
          left: rect.left, right: rect.right, width: rect.width, height: rect.height,
          native: [canvas.width, canvas.height], pixels: getComputedStyle(canvas).imageRendering };
      });
      assert.equal(layout.overflow, false, `Horizontal overflow at ${width} in ${isPlay ? "Play" : "Debug"}`);
      assert(layout.width >= Math.min(280, width - 80), `Canvas is too small at ${width}`);
      assert(layout.left >= 0 && layout.right <= width);
      assert(Math.abs(layout.width / layout.height - 320 / 224) < 0.005);
      assert.deepEqual(layout.native, [320, 224]);
      assert.equal(layout.pixels, "pixelated");
      const controls = isPlay ? ["view-debug", "play-homebrew", "pause", "restart", "fullscreen"] : ["view-play", "step", "run", "reset", "frame", "count"];
      if (await page.locator("#mode").isVisible()) controls.push("mode");
      for (const id of ["disc", "bios"]) if (await page.locator(`#${id}`).isVisible()) controls.push(id);
      for (const id of controls) {
        const control = page.locator(`#${id}`);
        assert(await control.isVisible(), `Missing ${id} at ${width}`);
        // Chromium can round the intersection of a control clipped by <1 CSS px
        // to "fully visible". Center explicitly before checking exact bounds.
        await control.evaluate((element) => element.scrollIntoView({ block: "center", inline: "nearest", behavior: "instant" }));
        const rect = await control.boundingBox();
        assert(rect.x >= 0 && rect.x + rect.width <= width && rect.y >= 0 && rect.y + rect.height <= height, `Clipped ${id} at ${width}×${height}: ${JSON.stringify(rect)}`);
      }
      const boxes = await page.evaluate((ids) => ids.map((id) => {
        const {x, y, width, height} = document.getElementById(id).getBoundingClientRect();
        return {id, x, y, width, height};
      }), controls);
      for (let i = 0; i < boxes.length; i++) for (let j = i + 1; j < boxes.length; j++) {
        const a = boxes[i], b = boxes[j];
        assert(a.x + a.width <= b.x || b.x + b.width <= a.x || a.y + a.height <= b.y || b.y + b.height <= a.y,
          `Overlapping ${a.id}/${b.id} at ${width}`);
      }
      if (isPlay) {
        // The primary action and display must share the initial viewport.
        await page.evaluate(() => window.scrollTo(0, 0));
        const canvas = await page.locator("#screen").boundingBox();
        const cta = await page.locator("#play-homebrew").boundingBox();
        assert(canvas.y >= 0 && canvas.y + canvas.height <= height);
        assert(cta.y >= 0 && cta.y + cta.height <= height);
      }
      // Keyboard navigation works at every size, including with local media.
      const beforeSwitch = await machine();
      await page.locator(isPlay ? "#view-debug" : "#view-play").focus();
      await page.keyboard.press("Enter");
      await page.locator(isPlay ? "#view-play" : "#view-debug").focus();
      await page.keyboard.press("Space");
      assert.equal(await page.locator("#debug-view").isVisible(), !isPlay);
      assert.deepEqual(await machine(), beforeSwitch);
    }
    await page.setViewportSize({ width: 1440, height: 900 });
  };
  assert.equal(await page.locator("#view-play").getAttribute("aria-pressed"), "true");
  assert.equal(await page.locator("#debug-view").isVisible(), false);
  assert.equal(await page.locator("#register-panel").isVisible(), false);
  assert.equal(await page.locator("#settings").getAttribute("open"), null);
  assert.equal(await page.locator("#registers tr").count(), 0);
  assert.equal(await value("play-homebrew"), "Play Star Collector");
  await responsive(true);
  const initialMachine = await machine();
  // Native disclosure is keyboard operable and never boots or discards a machine.
  await settings.focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => document.getElementById("settings").open);
  await page.locator("#mode").focus();
  await page.keyboard.press("Tab");
  assert.deepEqual(await machine(), initialMachine);
  await settings.focus();
  await page.keyboard.press("Space");
  await page.waitForFunction(() => !document.getElementById("settings").open);
  assert.deepEqual(await machine(), initialMachine);
  await page.locator("#view-debug").focus();
  await page.keyboard.press("Enter");
  assert.equal(await page.locator("#debug-view").isVisible(), true);
  assert.deepEqual(await machine(), initialMachine);
  await settings.click();
  await responsive(false);

  assert.equal(await value("active-mode"), "HLE bootstrap prototype");
  assert.equal(await value("pc"), "0x06000000");
  await page.locator("#step").click();
  assert.equal(await value("r0"), "0x00000005");
  assert.equal(await value("instructions"), "1");
  await page.locator("#reset").click();
  await page.locator("#count").fill("6");
  await page.locator("#run").click();
  assert.equal(await value("pc"), "0x0600000E");
  assert.equal(await value("r0"), "0x00000004");
  assert.equal(await value("r1"), "0x00000007");
  assert.equal(await value("instructions"), "6");
  for (const count of ["0", "10001", "1.5", ""]) {
    await page.locator("#count").fill(count);
    await page.locator("#run").click();
    assert.match(await value("error"), /whole instruction count/);
    assert.equal(await value("instructions"), "6");
  }
  await page.locator("#step").click();
  assert.equal(await value("delay"), "0x0600000E");
  const pendingBranch = await machine();
  await showView("play");
  await showView("debug");
  assert.deepEqual(await machine(), pendingBranch);
  assert.equal(await value("delay"), "0x0600000E");
  await page.locator("#step").click();
  assert.equal(await value("delay"), "none");
  assert.equal(await value("pc"), "0x0600000E");
  await page.locator("#mode").selectOption("bios");
  assert.equal(await value("active-mode"), "User-provided BIOS");
  assert.match(await value("error"), /512 KiB/);
  assert.equal(await page.locator("#step").isDisabled(), true);
  await page.locator("#bios").setInputFiles({ name: "invalid.bin", mimeType: "application/octet-stream", buffer: Buffer.from([1]) });
  assert.match(await value("error"), /received 1 bytes/);
  const bios = Buffer.alloc(524288);
  bios.writeUInt32BE(0x100, 0);
  bios.writeUInt32BE(0x06100000, 4);
  bios.writeUInt16BE(0xE02A, 0x100); // MOV #42,R0
  bios.writeUInt16BE(0x6102, 0x102); // MOV.L @R0,R1: deterministic unaligned bus diagnostic
  await page.locator("#bios").setInputFiles({ name: "original-test-vectors.bin", mimeType: "application/octet-stream", buffer: bios });
  await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  assert.equal(await value("pc"), "0x00000100");
  assert.equal(await value("r15"), "0x06100000");
  assert.equal(await value("instructions"), "0");
  await page.locator("#step").click();
  assert.equal(await value("r0"), "0x0000002A");
  await page.locator("#step").click();
  assert.match(await value("error"), /Memory access at PC 0x00000102: Unaligned 4-byte access at 0x0000002A/);
  assert.equal(await page.locator("#run").isDisabled(), true);
  const faulted = await machine();
  const faultText = await value("error");
  await showView("play");
  assert.equal(await value("error"), faultText);
  await showView("debug");
  assert.deepEqual(await machine(), faulted);
  assert.equal(await value("error"), faultText);
  assert.equal(await page.locator("#bios").evaluate((input) => input.files[0].name), "original-test-vectors.bin");
  await settings.click();
  await settings.click();
  assert.deepEqual(await machine(), faulted);
  await page.locator("#reset").click();
  assert.equal(await value("pc"), "0x00000100");
  assert.equal(await value("r0"), "0x00000000");
  await responsive(false);
  // The exact same MIT-licensed synthetic ROM runs in tests/sh2_system.rs.
  // Guest handlers manipulate their own exception stack; host only steps/reads.
  const systemRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sh2-system.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => systemRom.writeUInt16BE(word, offset + i * 2));
  }
  await page.locator("#bios").setInputFiles({ name: "original-sh2-system.bin", mimeType: "application/octet-stream", buffer: systemRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000100");
  await page.locator("#count").fill("10000");
  await page.locator("#run").click();
  assert.equal(await value("error"), "");
  assert.equal(await value("pc"), "0x0000011A");
  assert.equal(await value("r0"), "0x00000023");
  assert.equal(await value("r15"), "0x06100000");
  assert.equal(await value("instructions"), "19");
  assert.equal(await value("exceptions"), "3");
  assert.equal(await value("last-exception"), "Vector 6 · saved PC 0x00000114");
  assert.match(await value("status"), /Sleeping/);
  assert.equal(await page.locator("#run").isDisabled(), true);
  const sleeping = await machine();
  await page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    if (emulator.read_long(0x06001000) !== 35) throw new Error("Guest result was not stored");
    emulator.run_steps(10000);
    emulator.advance_frame();
  });
  const afterSleep = await machine();
  assert.deepEqual({ ...afterSleep, frames: sleeping.frames }, sleeping);
  await page.locator("#reset").click();
  assert.equal(await value("exceptions"), "0");
  assert.equal(await value("last-exception"), "none");
  await page.locator("#count").fill("6");
  // A malformed replacement must clear the previously running external ROM.
  await page.locator("#bios").setInputFiles({ name: "wrong.bin", mimeType: "application/octet-stream", buffer: Buffer.from([0, 0]) });
  assert.equal(await value("pc"), "0x00000000");
  assert.equal(await page.locator("#step").isDisabled(), true);
  await page.locator("#mode").selectOption("hle");
  assert.equal(await value("error"), "");
  assert.equal(await value("pc"), "0x06000000");
  // An asynchronous file read finishing after a mode switch must not revive ROM.
  await page.locator("#mode").selectOption("bios");
  await page.evaluate(() => {
    const original = File.prototype.arrayBuffer;
    File.prototype.arrayBuffer = function () {
      const file = this;
      return new Promise((resolve, reject) => {
        window.finishBiosRead = async () => {
          try { resolve(await original.call(file)); } catch (error) { reject(error); }
        };
      });
    };
    window.restoreFileRead = () => { File.prototype.arrayBuffer = original; };
  });
  await page.locator("#bios").setInputFiles({ name: "delayed-test-vectors.bin", mimeType: "application/octet-stream", buffer: bios });
  await page.waitForFunction(() => typeof window.finishBiosRead === "function");
  assert.equal(await page.locator("#step").isDisabled(), true);
  await page.locator("#bios").setInputFiles({ name: "invalid-during-read.bin", mimeType: "application/octet-stream", buffer: Buffer.from([0]) });
  assert.equal(await page.locator("#reset").isDisabled(), false);
  assert.match(await value("error"), /received 1 bytes/);
  await page.evaluate(async () => { await window.finishBiosRead(); });
  assert.equal(await value("pc"), "0x00000000");
  assert.equal(await page.locator("#step").isDisabled(), true);
  await page.locator("#bios").setInputFiles({ name: "delayed-again.bin", mimeType: "application/octet-stream", buffer: bios });
  await page.locator("#mode").selectOption("hle");
  await page.evaluate(async () => { await window.finishBiosRead(); window.restoreFileRead(); });
  assert.equal(await value("active-mode"), "HLE bootstrap prototype");
  assert.equal(await value("pc"), "0x06000000");
  // Once assets have loaded, the bootstrap runs with the network disabled.
  await context.setOffline(true);
  await page.locator("#count").fill("6");
  await page.locator("#run").click();
  assert.equal(await value("r1"), "0x00000007");
  assert.equal(await value("pc"), "0x0600000E");
  await page.locator("#count").fill("10000");
  await page.locator("#run").click();
  assert.equal(await value("instructions"), "10006");
  assert.equal(await value("r1"), "0x00000007");
  // Presentation changes during a sample download preserve the load but cancel
  // automatic playback, even after returning to Play and closing settings.
  await context.setOffline(false);
  await settings.click();
  await showView("play");
  let finishSample;
  const sampleRequested = new Promise((resolve) => { finishSample = resolve; });
  const sampleDeadline = setTimeout(() => finishSample(null), 10000).unref();
  await page.route("**/star-collector.iso", (route) => finishSample(route));
  await page.locator("#play-homebrew").click();
  const sampleRoute = await sampleRequested;
  clearTimeout(sampleDeadline);
  assert(sampleRoute, "Sample ISO request did not start");
  assert.match(await value("player-status"), /Loading image/);
  await settings.click();
  await page.locator("#mode").focus();
  await settings.click();
  await showView("debug");
  await showView("play");
  await sampleRoute.fulfill({status: 200, contentType: "application/octet-stream", body: await readFile("web/star-collector.iso")});
  await page.waitForFunction(() => document.getElementById("pause").disabled === false);
  assert.equal(await value("pause"), "Resume");
  assert.match(await value("player-status"), /Paused/);
  assert.equal(Number((await machine()).frames), 0);
  await page.unroute("**/star-collector.iso");
  // Same actual ISO + Wasm + input bridge + Canvas as the UI. Stop wall-clock
  // playback and advance deterministic frames, with no game-specific host writes.
  await page.locator("#play-homebrew").click();
  await page.waitForFunction(() => window.emulatedFrames() > 0);
  assert.equal(await value("active-mode"), "BIOS-less HLE disc");
  await page.locator("#pause").click();
  assert.match(await value("player-status"), /Paused/);
  // Hidden register tables are not rewritten during real playback.
  await page.evaluate(() => {
    window.registerChanges = 0;
    new MutationObserver(() => window.registerChanges++).observe(document.getElementById("registers"), {childList: true, subtree: true});
  });
  const observedFrames = Number((await machine()).frames);
  await page.locator("#pause").click();
  await page.waitForFunction((frames) => window.emulatedFrames() >= frames + 5, observedFrames);
  assert.equal(await page.evaluate(() => window.registerChanges), 0);
  // Switch while running/holding input in one event turn, so no timer races
  // intervene between the machine snapshots. Other navigation uses real clicks.
  const switchState = await page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    const before = String(emulator.instruction_count());
    document.getElementById("screen").focus();
    window.dispatchEvent(new KeyboardEvent("keydown", {key: "ArrowRight"}));
    document.getElementById("view-debug").click();
    return [before, String(emulator.instruction_count())];
  });
  assert.equal(switchState[0], switchState[1]);
  assert.equal(await value("pause"), "Resume");
  const pausedMachine = await machine();
  await showView("play");
  await showView("debug");
  assert.deepEqual(await machine(), pausedMachine);
  await page.locator("#reset").click();
  const gameState = () => page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    const pixels = document.getElementById("screen").getContext("2d").getImageData(0,0,320,224).data;
    let hash = 2166136261;
    for (const byte of pixels) hash = Math.imul(hash ^ byte, 16777619) >>> 0;
    return { state: Array.from({length: 5}, (_, i) => emulator.read_long(0x06010000 + i*4)),
      instructions: Number(emulator.instruction_count()), hash,
      progress: Array.from(pixels.slice((27*320+45)*4, (27*320+45)*4+4)),
      banner: Array.from(pixels.slice((150*320+100)*4, (150*320+100)*4+4)) };
  });
  const stepFrames = (n) => page.evaluate(async (n) => {
    const { advanceFrame } = await import("./app.js");
    for (let i = 0; i < n; i++) advanceFrame();
  }, n);
  const hold = async (key, n) => {
    await page.locator("#screen").focus();
    await page.keyboard.down(key);
    await stepFrames(n);
    await page.keyboard.up(key);
  };
  await stepFrames(20);
  const initial = await gameState();
  assert.deepEqual(initial.state.slice(0,4), [32,112,0,0]);
  assert(initial.instructions > 100000);
  await hold("ArrowRight", 8);
  const collected = await gameState();
  assert.deepEqual(collected.state.slice(0,4), [64,112,1,0]);
  assert.notEqual(initial.hash, collected.hash);
  assert.deepEqual(collected.progress, [255,255,0,255]);
  assert.notDeepEqual(collected.progress, initial.progress);
  assert(collected.instructions > initial.instructions);
  assert(collected.state[4] > initial.state[4]);
  await hold("ArrowRight", 48);
  const won = await gameState();
  assert.deepEqual(won.state.slice(0,4), [256,112,5,1]);
  assert.deepEqual(won.banner, [0,255,0,255]);
  assert.notEqual(won.hash, collected.hash);
  await hold("Enter", 20);
  await stepFrames(2);
  assert.deepEqual((await gameState()).state.slice(0,4), [32,112,0,0]);
  assert.equal((await gameState()).hash, initial.hash);
  // Second sequence: bypass the star at a different Y, then come back for it.
  await hold("ArrowUp", 4);
  await hold("ArrowRight", 8);
  const missed = await gameState();
  assert.deepEqual(missed.state.slice(0,4), [64,96,0,0]);
  assert.notEqual(missed.hash, initial.hash);
  await hold("ArrowDown", 4);
  assert.deepEqual((await gameState()).state.slice(0,4), [64,112,1,0]);
  // Losing focus must release held input, including while paused.
  await page.keyboard.down("ArrowRight");
  await page.locator("#count").focus();
  await stepFrames(4);
  assert.deepEqual((await gameState()).state.slice(0,4), [64,112,1,0]);
  await page.keyboard.up("ArrowRight");
  const stationary = (await gameState()).state.slice(0,4);
  // Held buttons release on view changes, pause, settings and window blur.
  for (const action of ["view-play", "settings", "pause", "window-blur"]) {
    await page.locator("#screen").focus();
    await page.keyboard.down("ArrowRight");
    if (action === "window-blur") await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    else if (action === "pause") {
      // Click resume and pause in one task; no real-time frame can run between.
      await page.evaluate(() => {
        document.getElementById("pause").click();
        document.getElementById("pause").click();
      });
    } else if (action === "settings") {
      await settings.click();
      await page.locator("#disc").focus();
      await page.keyboard.down("ArrowRight");
      await page.keyboard.up("ArrowRight");
      await settings.click();
    } else await page.locator(`#${action}`).click();
    await stepFrames(4);
    assert.deepEqual((await gameState()).state.slice(0,4), stationary, `Stuck input after ${action}`);
    await page.keyboard.up("ArrowRight");
  }
  // Opening settings during playback pauses; dismissing it never resumes/reset.
  await page.locator("#pause").click();
  await settings.click();
  await page.waitForFunction(() => document.getElementById("pause").textContent === "Resume");
  const settingsMachine = await machine();
  await settings.click();
  assert.deepEqual(await machine(), settingsMachine);
  assert.equal(await value("pause"), "Resume");
  await showView("debug");
  const beforeFrame = Number((await machine()).frames);
  await page.locator("#frame").click();
  assert.equal(Number((await machine()).frames), beforeFrame + 1);
  await showView("play");
  // Exercise actual Fullscreen API when allowed by the headless browser.
  let fullscreenResult = "unavailable";
  await page.locator("#fullscreen").click();
  await page.waitForFunction(() => document.fullscreenElement || document.getElementById("fullscreen-status").textContent);
  if (await page.evaluate(() => !!document.fullscreenElement)) {
    fullscreenResult = "entered/exited";
    assert.equal(await page.evaluate(() => document.fullscreenElement.id), "player-shell");
    await page.waitForFunction(() => document.activeElement === document.getElementById("screen"));
    const rect = await page.locator("#screen").boundingBox();
    assert(Math.abs(rect.width / rect.height - 10 / 7) < 0.005);
    await page.locator("#fullscreen").click();
    await page.waitForFunction(() => !document.fullscreenElement);
    await page.waitForFunction(() => document.activeElement === document.getElementById("fullscreen") && document.getElementById("fullscreen").textContent === "Fullscreen");
  } else assert.match(await value("fullscreen-status"), /unavailable|could not open/);
  // Deterministic denied/unavailable paths, even on browsers that permit it.
  await page.evaluate(() => {
    window.originalFullscreen = document.getElementById("player-shell").requestFullscreen;
    document.getElementById("player-shell").requestFullscreen = () => Promise.reject(new Error("test denial"));
  });
  if (await page.evaluate(() => document.fullscreenEnabled)) {
    await page.locator("#fullscreen").click();
    assert.match(await value("fullscreen-status"), /test denial/);
  }
  await page.evaluate(() => { document.getElementById("player-shell").requestFullscreen = undefined; });
  await page.locator("#fullscreen").click();
  assert.match(await value("fullscreen-status"), /unavailable/);
  await page.evaluate(() => { document.getElementById("player-shell").requestFullscreen = window.originalFullscreen; });
  await responsive(true);
  await page.locator("#restart").click();
  await page.waitForFunction(() => window.emulatedFrames() > 0);
  assert.equal(await value("pause"), "Pause");
  assert.match(await value("player-status"), /Running/);
  await page.locator("#pause").click();
  await showView("debug");
  await settings.click();
  // Local supported ISO works offline, and selecting bad media clears old CPU.
  const disc = await readFile("web/star-collector.iso");
  await context.setOffline(true);
  await page.locator("#disc").setInputFiles({name:"authorized-homebrew.iso", mimeType:"application/octet-stream", buffer:disc});
  await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  assert.equal(await value("pause"), "Resume");
  await settings.click();
  const localMachine = await machine();
  await showView("play");
  await showView("debug");
  assert.deepEqual(await machine(), localMachine);
  await settings.click();
  assert.equal(await page.locator("#disc").evaluate((input) => input.files[0].name), "authorized-homebrew.iso");
  await page.locator("#reset").click();
  await stepFrames(20);
  assert.deepEqual((await gameState()).state.slice(0,4), [32,112,0,0]);
  await page.locator("#disc").setInputFiles({name:"bad.iso", mimeType:"application/octet-stream", buffer:Buffer.alloc(disc.length)});
  // setInputFiles does not await the async file read; wait for the parser result.
  await page.waitForFunction(() => /missing Saturn hardware identifier/.test(document.getElementById("error").textContent));
  assert.match(await value("error"), /missing Saturn hardware identifier/);
  assert.equal(await page.locator("#frame").isDisabled(), true);
  assert.equal(await value("pc"), "0x00000000");
  await page.locator("#disc").setInputFiles({name:"raw.bin", mimeType:"application/octet-stream", buffer:Buffer.alloc(2352)});
  assert.match(await value("error"), /2048-byte-sector ISO/);
  // A disc read finishing after changing modes must not revive the old game.
  await page.evaluate(() => {
    const original = File.prototype.arrayBuffer;
    File.prototype.arrayBuffer = function () {
      const file = this;
      return new Promise((resolve) => { window.finishDiscRead = async () => resolve(await original.call(file)); });
    };
    window.restoreDiscRead = () => { File.prototype.arrayBuffer = original; };
  });
  await page.locator("#disc").setInputFiles({name:"delayed.iso", mimeType:"application/octet-stream", buffer:disc});
  await page.waitForFunction(() => typeof window.finishDiscRead === "function");
  await page.locator("#mode").selectOption("hle");
  await page.evaluate(async () => { await window.finishDiscRead(); window.restoreDiscRead(); });
  assert.equal(await value("active-mode"), "HLE bootstrap prototype");
  assert.equal(await value("pc"), "0x06000000");
  assert.deepEqual(pageErrors, []);
  assert(requests.length > 0);
  const staticPaths = new Set([prefix, ...assets.map((asset) => prefix + asset), prefix + "favicon.ico", "/favicon.ico"]);
  assert(requests.every((r) => {
    const url = new URL(r.url);
    return url.origin === address && staticPaths.has(url.pathname) && url.search === "" && r.method === "GET" && r.body === null;
  }));
  // A missing build must be actionable in the UI too, including import failure.
  await context.setOffline(false);
  const missingBuild = await context.newPage();
  await missingBuild.route("**/pkg/saturn_rust.js", (route) => route.abort());
  await missingBuild.goto(address + prefix);
  await missingBuild.waitForFunction(() => document.getElementById("status").textContent === "Initialization failed");
  assert.match(await missingBuild.locator("#error").textContent(), /Build web\/pkg and serve/);
  assert.equal(await missingBuild.locator("#reset").isDisabled(), true);
  console.log(`Browser smoke passed (${await browser.version()}): Play-first, responsive desktop/tablet/mobile, keyboard settings/navigation, state/diagnostic preservation, input release, fullscreen ${fullscreenResult} + denied/unavailable feedback; /saturn-rust/ assets/MIME/Wasm, fixture trace, Step/Reset/bounded Run, local BIOS validation/vectors/fault, mode switching, offline execution, no local-file transmission; actual ISO gameplay, keyboard input, guest state/pixels, scoring, completion, restart, local disc errors/races.`);
} finally {
  if (browser) await browser.close();
  if (server?.pid && server.exitCode === null) {
    const stopped = once(server, "exit");
    server.kill();
    await stopped;
  }
  await rm(mount, { recursive: true, force: true });
}
