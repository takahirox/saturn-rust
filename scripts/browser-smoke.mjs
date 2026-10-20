// Runs the actual static UI and Wasm in Chromium. Synthetic vectors are original
// test data; no Sega firmware or game data is used.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { chromium } from "playwright";

const server = spawn("python3", ["-u", "-m", "http.server", "0", "--bind", "127.0.0.1", "--directory", "web"], { stdio: ["ignore", "pipe", "pipe"] });
let browser;
try {
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
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("request", (request) => requests.push({ url: request.url(), method: request.method(), body: request.postData() }));
  await page.goto(address);
  await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  const value = (id) => page.locator(`#${id}`).textContent();
  assert.equal(await value("active-mode"), "HLE bootstrap prototype");
  assert.equal(await value("pc"), "0x06000000");
  await page.locator("#step").click();
  assert.equal(await value("r0"), "0x00000005");
  assert.equal(await value("instructions"), "1");
  await page.locator("#reset").click();
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
  assert.deepEqual(pageErrors, []);
  assert(requests.length > 0);
  const staticPaths = new Set(["/", "/style.css", "/app.js", "/pkg/saturn_rust.js", "/pkg/saturn_rust_bg.wasm", "/favicon.ico"]);
  assert(requests.every((r) => {
    const url = new URL(r.url);
    return url.origin === address && staticPaths.has(url.pathname) && url.search === "" && r.method === "GET" && r.body === null;
  }));
  // A missing build must be actionable in the UI too, including import failure.
  await context.setOffline(false);
  const missingBuild = await context.newPage();
  await missingBuild.route("**/pkg/saturn_rust.js", (route) => route.abort());
  await missingBuild.goto(address);
  await missingBuild.waitForFunction(() => document.getElementById("status").textContent === "Initialization failed");
  assert.match(await missingBuild.locator("#error").textContent(), /Build web\/pkg and serve/);
  assert.equal(await missingBuild.locator("#reset").isDisabled(), true);
  console.log(`Browser smoke passed (${await browser.version()}): fixture trace, Step/Reset/bounded Run, local BIOS validation/vectors/fault, mode switching, offline execution, no BIOS transmission.`);
} finally {
  if (browser) await browser.close();
  if (server.pid && server.exitCode === null) {
    const stopped = once(server, "exit");
    server.kill();
    await stopped;
  }
}
