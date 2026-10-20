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
  bios.writeUInt16BE(0xFFFF, 0x102); // deterministic unsupported opcode
  await page.locator("#bios").setInputFiles({ name: "original-test-vectors.bin", mimeType: "application/octet-stream", buffer: bios });
  await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  assert.equal(await value("pc"), "0x00000100");
  assert.equal(await value("r15"), "0x06100000");
  assert.equal(await value("instructions"), "0");
  await page.locator("#step").click();
  assert.equal(await value("r0"), "0x0000002A");
  await page.locator("#step").click();
  assert.match(await value("error"), /Unsupported opcode 0xFFFF at PC 0x00000102/);
  assert.equal(await page.locator("#run").isDisabled(), true);
  await page.locator("#reset").click();
  assert.equal(await value("pc"), "0x00000100");
  assert.equal(await value("r0"), "0x00000000");
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
  // Same actual ISO + Wasm + input bridge + Canvas as the UI. Stop wall-clock
  // playback and advance deterministic frames, with no game-specific host writes.
  await context.setOffline(false);
  await page.locator("#play-homebrew").click();
  await page.waitForFunction(() => Number(document.getElementById("frames").textContent) > 0);
  assert.equal(await value("active-mode"), "BIOS-less HLE disc");
  await page.locator("#pause").click();
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
  // Local supported ISO works offline, and selecting bad media clears old CPU.
  const disc = await readFile("web/star-collector.iso");
  await context.setOffline(true);
  await page.locator("#disc").setInputFiles({name:"authorized-homebrew.iso", mimeType:"application/octet-stream", buffer:disc});
  await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  await page.locator("#pause").click();
  await page.locator("#reset").click();
  await stepFrames(20);
  assert.deepEqual((await gameState()).state.slice(0,4), [32,112,0,0]);
  await page.locator("#disc").setInputFiles({name:"bad.iso", mimeType:"application/octet-stream", buffer:Buffer.alloc(disc.length)});
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
  console.log(`Browser smoke passed (${await browser.version()}): /saturn-rust/ assets/MIME/Wasm, fixture trace, Step/Reset/bounded Run, local BIOS validation/vectors/fault, mode switching, offline execution, no local-file transmission; actual ISO gameplay, keyboard input, guest state/pixels, scoring, completion, restart, local disc errors/races.`);
} finally {
  if (browser) await browser.close();
  if (server?.pid && server.exitCode === null) {
    const stopped = once(server, "exit");
    server.kill();
    await stopped;
  }
  await rm(mount, { recursive: true, force: true });
}
