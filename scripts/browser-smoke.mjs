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
const assets = ["audio.js", "audio-worklet.js", "app.js", "index.html", "pkg/saturn_rust.js", "pkg/saturn_rust_bg.wasm", "style.css", "star-collector.iso", "star-collector-LICENSE.txt", "prism-courier.iso", "prism-courier-LICENSE.txt"];
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
      addressErrorPending: emulator.address_error_pending(),
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
      const controls = isPlay ? ["view-debug", "play-homebrew", "play-prism", "pause", "restart", "fullscreen"] : ["view-play", "step", "run", "reset", "frame", "count"];
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
  bios.writeUInt16BE(0x2000, 0x102); // MOV.B R0,@R0: explicit read-only ROM diagnostic
  await page.locator("#bios").setInputFiles({ name: "original-test-vectors.bin", mimeType: "application/octet-stream", buffer: bios });
  await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  assert.equal(await value("pc"), "0x00000100");
  assert.equal(await value("r15"), "0x06100000");
  assert.equal(await value("instructions"), "0");
  await page.locator("#step").click();
  assert.equal(await value("r0"), "0x0000002A");
  await page.locator("#step").click();
  assert.match(await value("error"), /Memory access at PC 0x00000102: Read-only BIOS: 1-byte write at 0x0000002A/);
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
  // Same original BIOS and golden exception trace as tests/sh2_address_errors.rs.
  // The guest logs its own PC/SR frames and fixes an odd fetch PC with RTE.
  const addressRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sh2-address-errors.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => addressRom.writeUInt16BE(word, offset + i * 2));
  }
  await page.locator("#bios").setInputFiles({ name: "original-sh2-address-errors.bin", mimeType: "application/octet-stream", buffer: addressRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000100");
  await page.locator("#count").fill("4");
  await page.locator("#run").click();
  assert.equal(await value("error"), "");
  assert.equal(await value("pc"), "0x00000108");
  assert.equal(await value("r1"), "0x06001005");
  assert.equal(await value("exceptions"), "0");
  assert.equal((await machine()).addressErrorPending, true);
  await page.locator("#step").click();
  assert.equal(await value("pc"), "0x00000200");
  assert.equal(await value("instructions"), "4");
  assert.equal(await value("last-exception"), "Vector 9 · saved PC 0x00000108");
  assert.equal((await machine()).addressErrorPending, false);
  await page.locator("#count").fill("1000");
  await page.locator("#run").click();
  assert.equal(await value("error"), "");
  assert.equal(await value("pc"), "0x00000120");
  assert.equal(await value("r1"), "0x06001003");
  assert.equal(await value("r2"), "0x00000055");
  assert.equal(await value("r6"), "0x00000000");
  assert.equal(await value("r4"), "0x00000001");
  assert.equal(await value("r5"), "0x00000007");
  assert.equal(await value("r9"), "0x00000003");
  assert.equal(await value("r15"), "0x06100000");
  assert.equal(await value("instructions"), "55");
  assert.equal(await value("exceptions"), "3");
  assert.equal(await value("last-exception"), "Vector 9 · saved PC 0x0000011B");
  assert.match(await value("status"), /Sleeping/);
  const addressTrace = await page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    return { frames: Array.from({length: 6}, (_, i) => emulator.read_long(0x06002000 + i * 4)),
      neighbors: [emulator.read_long(0x06001000), emulator.read_long(0x06001004)] };
  });
  assert.deepEqual(addressTrace, { frames: [0x108, 0xF0, 0x112, 0xF0, 0x11B, 0xF0], neighbors: [0, 0] });
  const addressSleeping = await machine();
  await page.locator("#reset").click();
  await page.locator("#run").click();
  assert.deepEqual(await machine(), addressSleeping);
  // Reset must also discard an error that is still waiting for acceptance.
  await page.locator("#reset").click();
  await page.locator("#count").fill("4");
  await page.locator("#run").click();
  assert.equal((await machine()).addressErrorPending, true);
  await page.locator("#reset").click();
  assert.equal((await machine()).addressErrorPending, false);
  assert.equal(await value("exceptions"), "0");
  // RTE with an erroneous stack read defers through its SLEEP successor.
  // The pending synchronous event must keep Debug Step/Run enabled.
  const sleepingErrorRom = Buffer.alloc(524288);
  sleepingErrorRom.writeUInt32BE(0x100, 0);
  sleepingErrorRom.writeUInt32BE(0x06100000, 4);
  [0xD101, 0x6F13, 0x002B, 0x001B, 0x0400, 0x0001].forEach((word, i) => sleepingErrorRom.writeUInt16BE(word, 0x100 + i * 2));
  await page.locator("#bios").setInputFiles({ name: "original-sleeping-address-error.bin", mimeType: "application/octet-stream", buffer: sleepingErrorRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000100");
  await page.locator("#count").fill("4");
  await page.locator("#run").click();
  assert.equal((await machine()).addressErrorPending, true);
  assert.match(await value("status"), /Sleeping/);
  assert.equal(await page.locator("#step").isEnabled(), true);
  assert.equal(await page.locator("#run").isEnabled(), true);
  await page.locator("#step").click();
  assert.match(await value("error"), /Exception vector 9 entry.*Unmapped/);
  assert.equal(await page.locator("#step").isDisabled(), true);
  await page.locator("#count").fill("6");
  // SH7604 figs. 11.7/11.11: the shared native guest must wake only after
  // the edge leaving OCRA, including OCRA=0. All timer setup is guest MMIO.
  const compareRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/frt-compare.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => compareRom.writeUInt16BE(word, offset + i * 2));
  }
  for (const compare of [0, 3]) {
    compareRom.writeUInt16BE(0xE000 | compare, 0x114);
    await page.locator("#bios").setInputFiles({ name: "original-frt-compare.bin", mimeType: "application/octet-stream", buffer: compareRom });
    await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000100");
    const trace = await page.evaluate(async (compare) => {
      const { emulator } = await import("./app.js");
      const pc = () => emulator.special_registers()[0];
      for (let i = 0; i < 100 && pc() !== 0x13C; i++) emulator.step();
      if (pc() !== 0x13C) throw new Error("Guest did not reach timer start");
      const state = () => [pc(), emulator.exception_count()];
      emulator.run_steps(16 * (compare + 1) - 1);
      const beforeEdge = state();
      emulator.step();
      const onEdge = state();
      emulator.step();
      const accepted = [...state(), emulator.read_long(0x060FFFF8), emulator.read_long(0x060FFFFC)];
      emulator.run_steps(100);
      return { beforeEdge, onEdge, accepted,
        done: [...state(), emulator.read_long(0x06002000), emulator.registers()[15]] };
    }, compare);
    assert.deepEqual(trace, { beforeEdge: [0x140, "0"], onEdge: [0x140, "0"],
      accepted: [0x200, "1", 0x140, 0], done: [0x142, "1", 9, 0x06100000] });
  }
  // Original SH7604 guest: signed division, actual DMA copy and IRQ, watchdog
  // IRQ/manual reset, SCI transmission, and bounded misaligned exception entry.
  const peripheralRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sh7604.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => peripheralRom.writeUInt16BE(word, offset + i * 2));
  }
  await page.locator("#bios").setInputFiles({ name: "original-sh7604.bin", mimeType: "application/octet-stream", buffer: peripheralRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
  const resetSteps = await page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    for (let i = 1; i <= 10000; i++) {
      emulator.step();
      if (emulator.status().startsWith("Resetting")) return i;
    }
    throw new Error("SH7604 guest did not assert the watchdog reset");
  });
  await page.locator("#reset").click();
  await page.locator("#count").fill(String(resetSteps));
  await page.locator("#run").click();
  assert.match(await value("status"), /Resetting/);
  assert.equal(await page.locator("#step").isEnabled(), true);
  assert.equal(await page.locator("#run").isEnabled(), true);
  const peripherals = () => page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    emulator.run_steps(10000);
    return { mailbox: Array.from({length: 16}, (_, i) => emulator.read_long(0x06003000 + i*4)),
      copy: Array.from({length: 4}, (_, i) => emulator.read_long(0x06005000 + i*4)),
      exceptions: emulator.exception_count(), last: emulator.last_exception(),
      sp: emulator.registers()[15], pending: emulator.address_error_pending() };
  });
  const peripheralResult = await peripherals();
  assert.deepEqual(peripheralResult.mailbox, [0xFFFFFFF2, 0xFFFFFFFE, 0x55555555, 1, 1, 1, 0xFFFFFF84, 1, 0xFFFFFFFF, 1, 0x600D, 0, 0, 0x12345678, 0, 1]);
  assert.deepEqual(peripheralResult.copy, [0x12345678, 0x12345679, 0x1234567A, 0x1234567B]);
  assert.equal(peripheralResult.exceptions, "2");
  assert.match(peripheralResult.last, /Vector 9.*0x00001C00/);
  assert.equal(peripheralResult.sp, 0x060FF000);
  assert.equal(peripheralResult.pending, false);
  await page.locator("#reset").click();
  assert.deepEqual(await peripherals(), peripheralResult);
  // SCU guest executes DSP arithmetic + external DMA, direct/indirect DMA,
  // and raster timer handlers through the actual Wasm machine and UI BIOS path.
  const scuRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/scu.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => scuRom.writeUInt16BE(word, offset + i * 2));
  }
  await page.locator("#bios").setInputFiles({ name: "original-scu.bin", mimeType: "application/octet-stream", buffer: scuRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
  const scuResult = () => page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    emulator.run_steps(10000);
    return { mailbox: Array.from({length: 11}, (_, i) => emulator.read_long(0x06004000 + i * 4)),
      pixels: Array.from({length: 4}, (_, i) => emulator.read_long(0x05E00090 + i * 4)),
      dspSum: emulator.read_long(0x06006000), exceptions: emulator.exception_count(),
      sp: emulator.registers()[15], status: emulator.status() };
  });
  const scuFinished = await scuResult();
  assert.deepEqual(scuFinished.mailbox, [2, 2, 1, 84, 7, 48, 0x600D, 1, 0x40015, 7, 11]);
  assert.deepEqual(scuFinished.pixels, [7, 11, 13, 17]);
  assert.equal(scuFinished.dspSum, 48);
  assert.equal(scuFinished.exceptions, "6");
  assert.equal(scuFinished.sp, 0x060FF000);
  assert.match(scuFinished.status, /Sleeping/);
  await page.locator("#reset").click();
  assert.deepEqual(await scuResult(), scuFinished);
  const badScuRom = Buffer.from(scuRom);
  const dspLiteral = badScuRom.indexOf(Buffer.from([0x02, 0x08, 0x40, 0x00]));
  assert(dspLiteral > 0);
  badScuRom.writeUInt32BE(0x40000000, dspLiteral);
  await page.locator("#bios").setInputFiles({ name: "original-illegal-scu.bin", mimeType: "application/octet-stream", buffer: badScuRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
  await page.locator("#count").fill("1000");
  await page.locator("#run").click();
  assert.match(await value("error"), /SCU DSP illegal instruction 40000000 at 00/);
  assert.equal(await page.locator("#run").isDisabled(), true);
  const scuStopped = await machine();
  await page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    try { emulator.run_steps(10); } catch { /* Sticky hardware fault. */ }
  });
  assert.deepEqual(await machine(), scuStopped);
  // Original SH-2 uploads a separately executing 68k sound program. Expected
  // PCM derives from the program's shifts/negation and SCSP pan, never canned audio.
  const soundRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sound.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => soundRom.writeUInt16BE(word, offset + i * 2));
  }
  await page.locator("#bios").setInputFiles({ name: "original-sound.bin", mimeType: "application/octet-stream", buffer: soundRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
  const soundResult = () => page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    for (let i = 0; i < 10; i++) emulator.run_steps(10000);
    const audio = Array.from(emulator.audio_samples(8192));
    const first = audio.findIndex((v) => v !== 0);
    return { mailbox: [emulator.read_long(0x06007000), emulator.read_long(0x06007004)],
      waveform: [emulator.read_long(0x05A04000), emulator.read_long(0x05A04004)],
      samples: audio.slice(first, first + 16), generated: emulator.audio_generated(),
      dropped: emulator.audio_dropped(), count: emulator.sound_instruction_count() };
  });
  const soundFinished = await soundResult();
  assert.deepEqual(soundFinished.mailbox, [1, 0x600D]);
  assert.deepEqual(soundFinished.waveform, [0x1000F000, 0xE0002000]);
  assert.deepEqual(soundFinished.samples, [256,4096,-256,-4096,-512,-8192,512,8192,256,4096,-256,-4096,-512,-8192,512,8192]);
  assert(BigInt(soundFinished.count) > 30n);
  assert.equal(soundFinished.dropped, "0");
  await page.locator("#reset").click();
  assert.deepEqual(await soundResult(), soundFinished);
  // A real user gesture enables AudioWorklet. Instrument the same producer path
  // used by frame playback, and assert the consumer actually renders nonzero PCM.
  await page.locator("#audio-enable").click();
  await page.waitForFunction(() => document.getElementById("audio-status").textContent.includes("paused"));
  await page.evaluate(async () => {
    const { emulator, audio, pumpAudio } = await import("./app.js");
    audio.setPaused(false);
    for (let i = 0; i < 4; i++) emulator.run_steps(10000);
    pumpAudio();
  });
  await page.evaluate(async () => {
    const { audio } = await import("./app.js");
    for (let i = 0; i < 200; i++) {
      if (audio.stats.rendered > 128 && audio.stats.peak > 0.1 && audio.stats.state === "underflow") return;
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error(`Audio consumer did not render and drain guest PCM: ${JSON.stringify(audio.stats)}`);
  });
  const audioPath = await page.evaluate(async () => {
    const { audio } = await import("./app.js");
    const before = { ...audio.stats };
    audio.setMuted(true);
    const muted = audio.stats.state;
    audio.setPaused(true);
    const paused = audio.stats.state;
    await audio.context.suspend();
    audio.setPaused(false);
    const blocked = audio.stats.state;
    await audio.context.resume();
    audio.setPaused(true);
    return { before, muted, paused, blocked, sampleRate: audio.context.sampleRate };
  });
  assert(audioPath.before.received > 0 && audioPath.before.underflow > 0, JSON.stringify(audioPath));
  assert(audioPath.before.queued <= 8192);
  assert.equal(audioPath.muted, "muted");
  assert.equal(audioPath.paused, "paused");
  assert.equal(audioPath.blocked, "blocked");
  // Mutating original guest arithmetic changes both memory and audio in Wasm.
  const changedSoundRom = Buffer.from(soundRom);
  changedSoundRom[0x2107] = 8;
  await page.locator("#bios").setInputFiles({ name: "original-changed-sound.bin", mimeType: "application/octet-stream", buffer: changedSoundRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
  const changedSound = await soundResult();
  assert.deepEqual(changedSound.waveform, [0x0800F800, 0xF0001000]);
  assert.deepEqual(changedSound.samples.slice(0,4), [128,2048,-128,-2048]);
  // Real Gamepad API input adaptation feeds only emulated SMPC levels. The
  // original SH-2 guest sets RTC/SMEM, handles IRQs and copies INTBACK packets.
  const smpcRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/smpc.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => smpcRom.writeUInt16BE(word, offset + i * 2));
  }
  await page.locator("#bios").setInputFiles({ name: "original-smpc.bin", mimeType: "application/octet-stream", buffer: smpcRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
  await page.locator("#screen").focus();
  const smpcResult = () => page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    for (let i = 0; i < 10; i++) emulator.run_steps(10000);
    // Read-only debugger longwords; mailbox contents came entirely from guest.
    const bytes = (address) => Array.from({length:4}, (_,i) => emulator.read_long(address+i*4))
      .flatMap((word) => [word>>>24,(word>>>16)&255,(word>>>8)&255,word&255]);
    return { status: bytes(0x06008000), packet: bytes(0x06008020), irq: emulator.read_long(0x06008040), done: emulator.read_long(0x06008044) };
  });
  await page.evaluate(async () => {
    const pads = [
      { connected:true, axes:[], buttons:Array.from({length:16}, (_,i) => ({pressed:[1,6,12].includes(i),value:[1,6,12].includes(i)?1:0})) },
      { connected:true, axes:[-1,0,1], buttons:Array.from({length:16}, (_,i) => ({pressed:[7,9,15].includes(i),value:[7,9,15].includes(i)?1:0})) },
    ];
    Object.defineProperty(navigator,"getGamepads",{configurable:true,value:()=>pads});
    const { pollGamepads } = await import("./app.js"); pollGamepads();
  });
  const smpcFinished = await smpcResult();
  assert.deepEqual(smpcFinished.status,[0xC0,0x20,0x24,0x32,0x29,0x12,0x34,0x56,0,1,0x35,0,0x12,0x34,0x56,0x78]);
  assert.deepEqual(smpcFinished.packet,[0xF1,2,0xEE,0xF7,0xF1,0x15,0x77,0x7F,0,128,255,255,255,255,255,255]);
  assert.equal(smpcFinished.irq,2); assert.equal(smpcFinished.done,0x600D);
  // Blur neutralizes axes/buttons without pretending a connected pad vanished.
  await page.locator("#reset").click();
  await page.locator("#screen").focus();
  await page.evaluate(async () => (await import("./app.js")).pollGamepads());
  await page.locator("#count").focus();
  const neutralSmpc = await smpcResult();
  assert.deepEqual(neutralSmpc.packet,[0xF1,2,255,255,0xF1,0x15,255,255,128,128,0,255,255,255,255,255]);
  await page.evaluate(() => { delete navigator.getGamepads; });
  // Independent processors execute this same original licensed guest natively
  // in tests/dual_sh2.rs. All shared RAM/IRQ/SMPC changes are guest MMIO writes.
  const dualRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/dual-sh2.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => dualRom.writeUInt16BE(word, offset + i * 2));
  }
  await page.locator("#bios").setInputFiles({ name: "original-dual-sh2.bin", mimeType: "application/octet-stream", buffer: dualRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
  const dualStarted = await page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    for (let i = 0; i < 2000 && emulator.read_long(0x06002004) !== 1; i++) emulator.step();
    return { boots: emulator.read_long(0x06002004), master: Array.from(emulator.registers()),
      slave: Array.from(emulator.slave_registers()), retired: Number(emulator.slave_instruction_count()) };
  });
  assert.equal(dualStarted.boots, 1);
  assert(dualStarted.retired > 0);
  assert.equal(dualStarted.master[14], 42);
  assert.equal(dualStarted.slave[14], 17);
  assert.equal(dualStarted.master[15], 0x06100000);
  assert.equal(dualStarted.slave[15], 0x060FE000);
  await page.locator("#count").fill("4000");
  await page.locator("#run").click();
  assert.equal(await value("error"), "");
  assert.match(await value("status"), /Sleeping/);
  const dualResult = () => page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    return { mailbox: Array.from({length: 14}, (_, i) => emulator.read_long(0x06002000 + i * 4)),
      slaveRetired: emulator.slave_instruction_count(), slavePc: emulator.slave_pc(),
      slaveExceptions: emulator.slave_exception_count(), masterExceptions: emulator.exception_count(),
      masterSp: emulator.registers()[15], clocks: Number(emulator.peripheral_clocks()) };
  });
  const dualFinished = await dualResult();
  assert.deepEqual(dualFinished.mailbox.slice(0, 11), [1, 2, 35, 42, 2, 0x1234, 2, 0x5678, 2, 42, 0x600D]);
  assert.deepEqual(dualFinished.mailbox.slice(11), [0x34E, 0xF0, 1]);
  assert.equal(dualFinished.masterExceptions, "3");
  assert.equal(dualFinished.masterSp, 0x06100000);
  assert.equal(dualFinished.slaveRetired, "0");
  assert.equal(dualFinished.slavePc, 0);
  assert.equal(dualFinished.slaveExceptions, "0");
  assert(dualFinished.clocks > 0);
  await page.locator("#reset").click();
  assert.equal((await dualResult()).clocks, 0);
  await page.locator("#run").click();
  const dualRepeat = await dualResult();
  assert.deepEqual({ ...dualRepeat, clocks: dualFinished.clocks }, dualFinished);
  // A slave exception-entry failure must be visible in the app, and bounded
  // retries must not let the master continue changing RAM or CPU registers.
  const badSlaveRom = Buffer.from(dualRom);
  const badStack = badSlaveRom.indexOf(Buffer.from([0x06, 0x0F, 0xE0, 0x00]));
  assert(badStack > 0);
  badSlaveRom.writeUInt32BE(0x04000000, badStack);
  await page.locator("#bios").setInputFiles({ name: "original-faulting-slave.bin", mimeType: "application/octet-stream", buffer: badSlaveRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
  await page.locator("#run").click();
  assert.match(await value("error"), /Slave SH-2: Exception vector 66 entry/);
  assert.match(await value("status"), /Faulted: Slave SH-2/);
  assert.equal(await page.locator("#run").isDisabled(), true);
  const stoppedAfterSlaveFault = await machine();
  await page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    try { emulator.run_steps(10); } catch { /* Sticky error expected. */ }
  });
  assert.deepEqual(await machine(), stoppedAfterSlaveFault);
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
  await page.locator("#screen").focus();
  await page.keyboard.down("ArrowRight");
  for (let i = 0; i < 80 && (await gameState()).state[3] !== 1; i++) await stepFrames(1);
  await page.keyboard.up("ArrowRight");
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

  // Explicit backup import/export stays local and preserves machine state.
  const backupBytes = Buffer.from(Array.from({length: 32768}, (_, i) => (i ^ (i >> 8)) & 255));
  const backup = () => page.evaluate(async () => Array.from((await import("./app.js")).emulator.export_backup_ram()));
  const beforeImport = await machine();
  await context.setOffline(true);
  await page.locator("#backup-import").setInputFiles({name:"original-backup.bin", mimeType:"application/octet-stream", buffer:backupBytes});
  await page.waitForFunction(() => document.getElementById("backup-status").textContent.startsWith("Imported"));
  assert.deepEqual(await machine(), beforeImport);
  assert.deepEqual(Buffer.from(await backup()), backupBytes);
  for (const size of [0, 1, 32767, 32769, 65536]) {
    await page.locator("#backup-import").setInputFiles({name:"invalid-backup.bin", mimeType:"application/octet-stream", buffer:Buffer.alloc(size)});
    assert.match(await value("backup-status"), /Backup unchanged/);
    assert.deepEqual(Buffer.from(await backup()), backupBytes);
    assert.deepEqual(await machine(), beforeImport);
  }
  const downloadReady = page.waitForEvent("download");
  await page.locator("#backup-export").click();
  const download = await downloadReady;
  assert.equal(download.suggestedFilename(), "saturn-backup.bin");
  assert.deepEqual(await readFile(await download.path()), backupBytes);
  await page.evaluate(() => {
    const original = File.prototype.arrayBuffer;
    File.prototype.arrayBuffer = () => Promise.reject(new Error("simulated local read failure"));
    window.restoreFailedBackupRead = () => { File.prototype.arrayBuffer = original; };
  });
  await page.locator("#backup-import").setInputFiles({name:"unreadable.bin", mimeType:"application/octet-stream", buffer:Buffer.alloc(32768)});
  await page.waitForFunction(() => document.getElementById("backup-status").textContent.includes("simulated local read failure"));
  await page.evaluate(() => window.restoreFailedBackupRead());
  assert.deepEqual(Buffer.from(await backup()), backupBytes);
  assert.deepEqual(await machine(), beforeImport);
  // A late read cannot replace bytes after a rejected replacement or mode switch.
  await page.evaluate(() => {
    const original = File.prototype.arrayBuffer;
    File.prototype.arrayBuffer = function () {
      const file = this;
      return new Promise((resolve) => { window.finishBackupRead = async () => resolve(await original.call(file)); });
    };
    window.restoreBackupRead = () => { File.prototype.arrayBuffer = original; };
  });
  const replacement = {name:"replacement.bin", mimeType:"application/octet-stream", buffer:Buffer.alloc(32768, 0x55)};
  await page.locator("#backup-import").setInputFiles(replacement);
  assert.equal(await page.locator("#run").isDisabled(), true);
  assert.equal(await page.locator("#backup-export").isDisabled(), true);
  await page.locator("#backup-import").setInputFiles({name:"invalid.bin", mimeType:"application/octet-stream", buffer:Buffer.alloc(1)});
  await page.evaluate(async () => { await window.finishBackupRead(); });
  assert.deepEqual(Buffer.from(await backup()), backupBytes);
  await page.locator("#backup-import").setInputFiles(replacement);
  await page.locator("#mode").selectOption("bios");
  await page.evaluate(async () => { await window.finishBackupRead(); window.restoreBackupRead(); });
  assert.match(await value("backup-status"), /cancelled by boot change/);
  assert.deepEqual(Buffer.from(await backup()), backupBytes);
  assert.match(await value("error"), /512 KiB/);
  // The shared original guest executes real MOV.B instructions on Wasm too.
  const backupRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/backup-pad.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => backupRom.writeUInt16BE(word, offset + i * 2));
  }
  await page.locator("#bios").setInputFiles({name:"original-backup-pad.bin", mimeType:"application/octet-stream", buffer:backupRom});
  await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  const romResults = await page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    emulator.set_port2_buttons(1);
    emulator.run_steps(83);
    return [emulator.export_backup_ram()[0], emulator.read_long(0x06001004)];
  });
  assert.deepEqual(romResults, [1, 0x3E]);
  await page.locator("#reset").click();
  assert.equal((await backup())[0], 1);
  // The same diagnostic also boots via the existing original ISO container.
  // This temporary local test image is not an additional completed demo game.
  const backupDisc = Buffer.from(await readFile("web/star-collector.iso"));
  backupRom.copy(backupDisc, 21 * 2048, 0x100, 0x150);
  backupDisc.writeUInt32BE(0x50, 0xF4);
  backupDisc.writeUInt32LE(0x50, 20 * 2048 + 68 + 10);
  backupDisc.writeUInt32BE(0x50, 20 * 2048 + 68 + 14);
  await page.locator("#mode").selectOption("disc");
  await page.locator("#disc").setInputFiles({name:"original-backup-pad-test.iso", mimeType:"application/octet-stream", buffer:backupDisc});
  await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  const samplePad = () => page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    emulator.run_steps(100);
    return emulator.read_long(0x06001004);
  });
  await settings.click(); // Close settings; local discs remain paused.
  await showView("play");
  await page.locator("#screen").focus();
  await page.keyboard.down("i");
  assert.equal(await samplePad(), 0x3E);
  await page.keyboard.up("i");
  assert.equal(await samplePad(), 0x3F);
  await page.keyboard.down("l");
  assert.equal(await samplePad(), 0x37);
  // View switching, window blur, settings and pause must release the second pad.
  await showView("debug");
  assert.equal(await samplePad(), 0x3F);
  await page.keyboard.up("l");
  await showView("play");
  await page.locator("#screen").focus();
  await page.keyboard.down("i");
  assert.equal(await samplePad(), 0x3E);
  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  assert.equal(await samplePad(), 0x3F);
  await page.keyboard.up("i");
  await page.keyboard.down("i");
  await settings.click();
  assert.equal(await samplePad(), 0x3F);
  await page.keyboard.up("i");
  await settings.click();
  await page.locator("#pause").click();
  await page.locator("#screen").focus();
  await page.keyboard.down("i");
  await page.locator("#pause").click();
  assert.equal(await samplePad(), 0x3F);
  await page.keyboard.up("i");
  assert.equal((await backup())[0], 2);
  const anotherTab = await context.newPage();
  await context.setOffline(false);
  await anotherTab.goto(address + prefix);
  await anotherTab.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  assert(await anotherTab.evaluate(async () => (await import("./app.js")).emulator.export_backup_ram().every((byte) => byte === 0)));
  await anotherTab.close();
  // Original playable VDP1+VDP2+SCSP disc. UI loads the packaged image; only
  // controller input enters the guest. Graphics, collision and state are SH-2
  // computations and real command/texture writes; the 68k computes PCM.
  const courier = await context.newPage();
  courier.on("pageerror", (error) => pageErrors.push(error.message));
  courier.on("request", (request) => requests.push({ url: request.url(), method: request.method(), body: request.postData() }));
  await courier.goto(address + prefix);
  await courier.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  await courier.evaluate(async () => {
    const { emulator } = await import("./app.js");
    window.courierState = () => [0,4,8,12,16].map((offset) => emulator.read_long(0x06018000+offset));
  });
  await courier.locator("#play-prism").click();
  await courier.waitForFunction(() => window.courierState()[4] > 2, null, { timeout: 30000 });
  await courier.locator("#pause").click();
  const courierState = () => courier.evaluate(() => window.courierState());
  assert.deepEqual((await courierState()).slice(0,4), [24,112,0,0]);
  const courierFrames = async (n) => courier.evaluate(async (n) => {
    const { emulator } = await import("./app.js");
    let stereo = false;
    for (let i=0; i<n; i++) {
      emulator.advance_frame();
      const pcm = emulator.audio_samples(8192);
      for (let p=0; p<pcm.length; p+=2) stereo ||= pcm[p] !== 0 && pcm[p+1] !== 0 && pcm[p] !== pcm[p+1];
    }
    const pixels = emulator.framebuffer();
    return { state: window.courierState(), stereo, pixels: [[27,115],[150,50],[285,110],[100,20]].map(([x,y]) => Array.from(pixels.slice((y*320+x)*4,(y*320+x)*4+4))),
      count: emulator.sound_instruction_count(), waveform: emulator.read_long(0x05A04000), drawIrqs: emulator.read_long(0x06018014) };
  },n);
  const courierInitial = await courierFrames(3);
  assert.deepEqual(courierInitial.pixels.slice(0,3), [[0,255,255,255],[255,0,0,255],[0,255,0,255]]);
  assert(courierInitial.stereo);
  assert(BigInt(courierInitial.count) >= 27n);
  assert.equal(courierInitial.waveform, 0x1000F000);
  assert(courierInitial.drawIrqs > 2);
  await courier.locator("#screen").focus();
  await courier.keyboard.down("ArrowUp");
  await courierFrames(12);
  await courier.keyboard.up("ArrowUp");
  await courier.keyboard.down("ArrowRight");
  const lostCourier = await courierFrames(32);
  await courier.keyboard.up("ArrowRight");
  assert.deepEqual(lostCourier.state.slice(0,4), [140,64,3,2]);
  assert.deepEqual(lostCourier.pixels[3], [255,0,0,255]);
  await courier.keyboard.down("Enter");
  await courierFrames(2);
  await courier.keyboard.up("Enter");
  assert.deepEqual((await courierFrames(3)).state.slice(0,4), [24,112,0,0]);
  await courier.keyboard.down("ArrowRight");
  await courierFrames(64);
  await courier.keyboard.up("ArrowRight");
  const wonCourier = await courierFrames(2);
  assert.deepEqual(wonCourier.state.slice(0,4), [280,112,7,1]);
  assert.deepEqual(wonCourier.pixels[3], [0,255,0,255]);
  // The Debug frame control uses the same Canvas presentation as Play.
  await courier.locator("#view-debug").click();
  await courier.locator("#frame").click();
  assert.deepEqual(await courier.evaluate(() => Array.from(document.getElementById("screen").getContext("2d").getImageData(100,20,1,1).data)), [0,255,0,255]);
  await courier.locator("#view-play").click();
  await courier.locator("#screen").focus();
  await courier.keyboard.down("Enter");
  await courierFrames(2);
  await courier.keyboard.up("Enter");
  assert.deepEqual((await courierFrames(3)).pixels, courierInitial.pixels);
  // The new disc's computed PCM also traverses the actual AudioWorklet.
  await courier.locator("#audio-enable").click();
  await courier.waitForFunction(() => document.getElementById("audio-status").textContent.includes("paused"));
  await courier.evaluate(async () => {
    const { emulator, audio } = await import("./app.js");
    audio.setPaused(false);
    emulator.advance_frame();
    audio.deliver(emulator.audio_samples(8192));
    window.courierAudioReady = () => audio.stats.rendered > 128 && audio.stats.peak > 0.1;
  });
  await courier.waitForFunction(() => window.courierAudioReady());
  // Replacing the original texture in a local disc must change Wasm pixels.
  const changedCourier = Buffer.from(await readFile("web/prism-courier.iso"));
  for (let i=21*2048+0x2400; i<21*2048+0x2480; i+=2) {
    if (changedCourier.readUInt16BE(i)) changedCourier.writeUInt16BE(0xFC00,i);
  }
  await courier.locator("#settings > summary").click();
  await courier.locator("#disc").setInputFiles({ name:"original-changed-prism.iso",mimeType:"application/octet-stream",buffer:changedCourier });
  await courier.waitForFunction(() => window.courierState()[4] === 0);
  const mutatedCourier = await courierFrames(40);
  assert.deepEqual(mutatedCourier.state.slice(0,4), [24,112,0,0]);
  assert.deepEqual(mutatedCourier.pixels[0], [0,0,255,255]);
  await courier.close();
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
  console.log(`Browser smoke passed (${await browser.version()}): Play-first, responsive desktop/tablet/mobile, keyboard settings/navigation, state/diagnostic preservation, input release, fullscreen ${fullscreenResult} + denied/unavailable feedback; /saturn-rust/ assets/MIME/Wasm, fixture trace, Step/Reset/bounded Run, local BIOS validation/vectors/fault, guest address-error PC/SR traces and RTE recovery, pending-error reset, SH7604 DIVU/DMA/IRQ/SCI/watchdog reset/cache coherence and bounded stacking errors, dual-SH-2 computation/capture IRQs/slave restart/master NMI and sticky slave faults, SCU DSP product/sum/external DMA/direct+indirect DMA/raster timer IRQs/reset/illegal-DSP faults, SH-2-uploaded MC68EC000 PCM computation/SCSP timer IRQ/stereo samples and WebAudio worklet delivery/underflow/mute/blocked states, SMPC RTC/SMEM/INTBACK IRQs and synthetic digital+analog Gamepad/focus release, mode switching, offline execution, no local-file transmission; actual ISO gameplay, keyboard input, guest state/pixels, scoring, completion, restart, local disc errors/races; guest backup/second-pad ROM and ISO, offline backup import/export/download bytes, reset retention, invalid imports/read races, second-pad keyboard/release, fresh-tab isolation; Prism Courier original disc, VDP1 sprite/polygons + VDP2 composition/Canvas, draw-end IRQ, 68k stereo + actual AudioWorklet, keyboard collision/win/restart, changed-disc texture mutation.`);
} finally {
  if (browser) await browser.close();
  if (server?.pid && server.exitCode === null) {
    const stopped = once(server, "exit");
    server.kill();
    await stopped;
  }
  await rm(mount, { recursive: true, force: true });
}
