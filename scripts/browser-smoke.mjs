// Runs the actual static UI and Wasm in Chromium. Synthetic vectors are original
// test data; no Sega firmware or game data is used.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { lstat, mkdtemp, readFile, readdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "playwright";
import { rawMode1 } from "./raw-media-fixture.mjs";
import { videoTransactions } from "./video-transactions.mjs";
import { busTransactions } from "./bus-transactions.mjs";
import { dmacBus } from "./dmac-bus.mjs";
import { abusTiming } from "./abus.mjs";
import { hardwareBoundaries } from "./hardware-boundaries.mjs";
import { refreshArbitration } from "./refresh.mjs";
import { hlePlayer } from "./hle-player.mjs";
import { scspMonitor } from "./scsp-monitor.mjs";
import { m68kReset } from "./m68k-reset.mjs";

// Fail before upload if web/ includes anything beyond the public runtime assets.
const assets = ["audio.js", "audio-worklet.js", "app.js", "index.html", "pkg/saturn_rust.js", "pkg/saturn_rust_bg.wasm", "style.css", "star-collector.iso", "star-collector-LICENSE.txt", "prism-courier.iso", "prism-courier-LICENSE.txt", "hardware-lab.iso", "hardware-lab-audio.bin", "hardware-lab.cue", "hardware-lab-LICENSE.txt"];
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
  bios.writeUInt16BE(0x2000, 0x102); // ROM has no write-enable: completes without changing bytes
  bios.writeUInt16BE(0xD101, 0x104); // MOV.L @(1,PC),R1 -> unsupported real SCU port
  bios.writeUInt16BE(0x2100, 0x106); // MOV.B R0,@R1
  bios.writeUInt32BE(0x05FE00CC, 0x10C);
  await page.locator("#bios").setInputFiles({ name: "original-test-vectors.bin", mimeType: "application/octet-stream", buffer: bios });
  await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  assert.equal(await value("pc"), "0x00000100");
  assert.equal(await value("r15"), "0x06100000");
  assert.equal(await value("instructions"), "0");
  await page.locator("#step").click();
  assert.equal(await value("r0"), "0x0000002A");
  await page.locator("#step").click();
  assert.equal(await value("error"), "");
  assert.equal(await value("instructions"), "2");
  assert.equal(await page.evaluate(async()=>{
    const {emulator}=await import("./app.js");return emulator.read_long(0x28);
  }),0);
  await page.locator("#step").click();
  await page.locator("#step").click();
  assert.match(await value("error"), /Memory access at PC 0x00000106: Unsupported Saturn device\/register: 1-byte access at 0x05FE00CC/);
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
  [0xD101, 0x6F13, 0x002B, 0x001B, 0x0800, 0x0011].forEach((word, i) => sleepingErrorRom.writeUInt16BE(word, 0x100 + i * 2));
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
      emulator.step(); // Start timer after the ordinary ROM fetch completes.
      const deadline = Number(emulator.peripheral_clocks()) + 128 * (compare + 1);
      emulator.step(); // SLEEP fetch includes its waits; sleeping turns do not.
      while (Number(emulator.peripheral_clocks()) + 8 < deadline) emulator.step();
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
  // Runtime filesystem failures must be CD responses rather than Wasm traps.
  // This original BIOS executes the register writes/polls and records responses.
  const directoryRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/cd-directory.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [address, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => directoryRom.writeUInt16BE(word, address + i * 2));
  }
  const directoryIso = await readFile("web/star-collector.iso");
  const cacheRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/cache.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>cacheRom.writeUInt16BE(w,at+i*2));
  }
  for (const value of [0x12345678,0x76543210]) {
    const image=Buffer.from(cacheRom),literal=Buffer.from([0x12,0x34,0x56,0x78]);
    const at=image.indexOf(literal);
    assert(at>=0 && image.indexOf(literal,at+1)===-1);image.writeUInt32BE(value,at);
    const result=await page.evaluate(async rom=>{
      const {Emulator}=await import("./pkg/saturn_rust.js");
      const m=new Emulator(),trace=[];
      for(let repetition=0;repetition<2;repetition++) {
        m.reset_bios(new Uint8Array(rom));m.run_steps(1000);
        trace.push(Array.from({length:6},(_,i)=>m.read_long(0x0600A500+i*4)));
      }
      m.free();return trace;
    },[...image]);
    const expected=[value,0xABCDEF01,0xBAD0BAD0,0xC0DEC0DE,0xABCDEF01,0x600D];
    assert.deepEqual(result,[expected,expected]);
  }
  const dackRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/dack.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>dackRom.writeUInt16BE(w,at+i*2));
  }
  const dack=await page.evaluate(async rom=>{
    const {Emulator}=await import("./pkg/saturn_rust.js");
    const guest=new Emulator();
    try {
      const trace=[];
      for(let i=0;i<2;i++) {
        guest.reset_bios(Uint8Array.from(rom));
        trace.push(guest.read_long(0xFFFFFF8C));
        guest.run_steps(2000);
        trace.push(...[0,4,8,12].map(at=>guest.read_long(0x06003000+at)),guest.read_long(0x06002000));
      }
      return trace;
    } finally {guest.free();}
  },[...dackRom]);
  assert.deepEqual(dack,[0,3,3,1,0x600D,0,0,3,3,1,0x600D,0]);
  const descriptorIso=Buffer.from(directoryIso);
  directoryIso.copy(descriptorIso,17*2048,16*2048,17*2048);
  for (const [sector,type] of [[16,0],[18,2],[19,255]]) {
    descriptorIso.fill(0,sector*2048,(sector+1)*2048);
    Buffer.from([type,67,68,48,48,49,1]).copy(descriptorIso,sector*2048);
  }
  descriptorIso[20*2048+68+25]=1; // ISO hidden file, still file identifier2
  for (const offset of [null, "descriptors", 16 * 2048 + 158, 20 * 2048 + 70]) {
    const image = Buffer.from(offset === "descriptors" ? descriptorIso : directoryIso);
    if (typeof offset === "number") image.fill(255, offset, offset + 8);
    const trace = await page.evaluate(async ({ rom, image }) => {
      const { Emulator } = await import("./pkg/saturn_rust.js");
      const emulator = new Emulator();
      try {
        emulator.reset_bios(Uint8Array.from(rom));
        emulator.mount_disc(Uint8Array.from(image));
        emulator.run_steps(3000);
        return [0, 0x10, 0x20, 0x24, 0x30].map((offset) => emulator.read_long(0x06009000 + offset));
      } finally { emulator.free(); }
    }, { rom: [...directoryRom], image: [...image] });
    if (typeof offset !== "number") {
      assert.deepEqual(trace, [0x0180, 0x0180, 0x0180, 2, 0x600D]);
    } else {
      assert.deepEqual([trace[0], trace[1], trace[2], trace[4]], [0xFF00, 0xFF00, 0xFF00, 0x600D]);
    }
  }
  // Original interleaved XA disc: actual SH-2 commands/FIFO reads, independent
  // metadata/checksum goldens, logical-offset mutation and repeated cold reset.
  const filesystemRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/cd-filesystem.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.trim().split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((v, i) => filesystemRom.writeUInt16BE(v, at+i*2));
  }
  const filesystemRaw = await readFile("tests/fixtures/cd-filesystem.bin");
  const filesystemCue = await readFile("tests/fixtures/cd-filesystem.cue", "utf8");
  const filesystemGolden = (await readFile("tests/fixtures/cd-filesystem.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v,16));
  for (const offset of [0, 1]) {
    const rom = Buffer.from(filesystemRom), at = rom.indexOf(Buffer.from([0xE6,0]));
    assert(at >= 0 && rom.indexOf(Buffer.from([0xE6,0]), at+1) < 0);
    rom[at+1] = offset;
    const results = await page.evaluate(async ({rom, raw, cue}) => {
      const {Emulator} = await import("./pkg/saturn_rust.js");
      const emulator = new Emulator(), results = [];
      try {
        for (let repetition = 0; repetition < 2; repetition++) {
          emulator.reset_bios(Uint8Array.from(rom));
          emulator.mount_cue(cue, "cd-filesystem.bin", Uint32Array.from([raw.length]), Uint8Array.from(raw));
          for (let block = 0; block < 100 && emulator.read_long(0x0600B01C) !== 0x600D; block++) emulator.run_steps(10000);
          results.push({trace: Array.from({length:8}, (_,i) => emulator.read_long(0x0600B000+i*4)),
            mode: emulator.read_long(0x0600A020) & 0x5F, connectors: emulator.read_long(0x0600A034),
            device: emulator.read_long(0x0600A048), exceptions: emulator.exception_count()});
        }
        return results;
      } finally {emulator.free();}
    }, {rom:[...rom], raw:[...filesystemRaw], cue:filesystemCue});
    const trace = [...filesystemGolden];
    if (offset) {trace[3] = 2; trace[4] -= 0x1111*1024;}
    const expected = {trace, mode:0x41, connectors:0x03FF, device:0x0300, exceptions:"0"};
    assert.deepEqual(results, [expected, expected]);
  }
  // Both startup paths below execute original AIP instructions in Wasm.
  const startup=await page.evaluate(async images=>{
    const {Emulator}=await import("./pkg/saturn_rust.js");
    return images.map(image=>{
      const emulator=new Emulator();
      try {
        emulator.reset_disc(Uint8Array.from(image));
        emulator.run_steps(100);
        return [emulator.read_long(0x06008000),emulator.read_long(0x06004000),Number(emulator.instruction_count())];
      } finally {emulator.free();}
    });
  },[descriptorIso,(()=>{
    const image=Buffer.from(directoryIso);
    image.fill(0,0xF0,0xF4); image.fill(0,20*2048+68,21*2048);
    [0xE02A,0xD101,0x2102,0x001B,0x0600,0x8000].forEach((w,i)=>image.writeUInt16BE(w,0xE20+i*2));
    return image;
  })()].map(b=>[...b]));
  assert.equal(startup[0][1],directoryIso.readUInt32BE(21*2048));
  assert.deepEqual(startup[1],[42,0,27]); // 23 original reset + four AIP instructions
  const layoutIso = await readFile("tests/fixtures/boot-layout.iso");
  const layoutGolden = (await readFile("tests/fixtures/boot-layout.tsv","utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
  for (const [areas, stack, value] of [["U",0,0x87654321],["JT",0x060FD000,0x87654321],["EUJT",0x060FD000,0x76543210]]) {
    const image = Buffer.from(layoutIso), entry = 0xE00+areas.length*32;
    image.fill(32,0x40,0x50); image.write(areas,0x40,"ascii");
    image.fill(0,0xE00,0xF00); layoutIso.copy(image,entry,0xE80,0xE8C);
    image.writeUInt32BE(stack,0xEC); image.writeUInt32BE(value,36*2048);
    const result = await page.evaluate(async ({image,entry})=>{
      const {Emulator} = await import("./pkg/saturn_rust.js");
      const m = new Emulator(), results=[];
      try {
        for (let repeat=0;repeat<2;repeat++) {
          m.reset_disc(Uint8Array.from(image));
          for (let i=0;i<32 && m.special_registers()[0] !== 0x06002000+entry;i++) m.step();
          const aip = m.special_registers()[0]; m.run_steps(1000);
          results.push({aip,trace:Array.from({length:6},(_,i)=>m.read_long(0x0600A000+i*4)),
            exceptions:[m.exception_count(),m.slave_exception_count()]});
        }
        return results;
      } finally {m.free();}
    },{image:[...image],entry});
    const trace=[...layoutGolden]; trace[1]=(0x12345678+value)>>>0; trace[4]=stack||0x06001000;
    const expected={aip:0x06002000+entry,trace,exceptions:["0","0"]};
    assert.deepEqual(result,[expected,expected]);
  }
  // Original HLE-service disc: actual SH-2 calls through BIOS entry pointers,
  // both-CPU TAS/VBR, nested SCU callbacks, copied priorities and reset domains.
  const hleSystemIso=await readFile("tests/fixtures/hle-system.iso");
  const hleSystemExpected=(await readFile("tests/fixtures/hle-system.tsv","utf8")).trim()
    .split(/\r?\n/).map(line=>parseInt(line.split("\t")[1],16));
  const hleSystemRun=async image=>page.evaluate(async ({image,count})=>{
    const {Emulator}=await import("./pkg/saturn_rust.js");
    const m=new Emulator(),traces=[];
    try {
      for(let repeat=0;repeat<2;repeat++) {
        m.reset_disc(Uint8Array.from(image));m.run_steps(10000);
        const words=base=>Array.from({length:7},(_,i)=>m.read_long(base+i*4));
        traces.push({main:Array.from({length:count},(_,i)=>m.read_long(0x0600E000+i*4)),
          slave:words(0x0600E500),outer:words(0x0600E600).slice(0,5),
          inner:words(0x0600E700).slice(0,5),exceptions:m.exception_count(),
          copied:m.read_long(0x06000BAC),source:m.read_long(0x0600F02C)});
      }
      return traces;
    } finally {m.free();}
  },{image:[...image],count:hleSystemExpected.length});
  const hleSystemTrace={main:hleSystemExpected,
    slave:[0,1,1,1,0x06009000,0x4B00,0x600D],outer:[0x20,0xFBFF,0,0,1],
    inner:[0xF0,0xFFFF,0,0,1],exceptions:"3",copied:0x0020F800,source:0x00F0FFFF};
  assert.deepEqual(await hleSystemRun(hleSystemIso),[hleSystemTrace,hleSystemTrace]);
  const hleUncached=Buffer.from(hleSystemIso),cacheEnable=Buffer.from([0xE0,19,0x21,0]);
  const hleCacheAt=hleUncached.indexOf(cacheEnable);
  assert(hleCacheAt>=0 && hleUncached.indexOf(cacheEnable,hleCacheAt+1)===-1);
  hleUncached[hleCacheAt+1]=0;
  const hleUncachedExpected=structuredClone(hleSystemTrace);hleUncachedExpected.main[2]=0;
  assert.deepEqual(await hleSystemRun(hleUncached),[hleUncachedExpected,hleUncachedExpected]);
  for(const [before,after] of [[0x13579BDF,0x2468ACE0],[0x0020F800,0x0030F800]]) {
    const image=Buffer.from(hleSystemIso),literal=Buffer.alloc(4);literal.writeUInt32BE(before);
    const at=image.indexOf(literal,21*2048);
    assert(at>=21*2048 && image.indexOf(literal,at+1)===-1);
    image.writeUInt32BE(after,at);
    const expected=structuredClone(hleSystemTrace);
    if(before===0x13579BDF) {
      expected.main[expected.main.length-3]=after;
      expected.main[expected.main.length-2]=after;
    } else {expected.outer[0]=0x30;expected.copied=after;}
    assert.deepEqual(await hleSystemRun(image),[expected,expected]);
  }
  const hleResetImage=Buffer.from(hleSystemIso);
  hleResetImage.writeUInt32BE(0x06009600,0xE28);
  const hleResetTrace=await page.evaluate(async image=>{
    const {Emulator}=await import("./pkg/saturn_rust.js");
    const m=new Emulator(),traces=[];
    try {
      for(let repeat=0;repeat<2;repeat++) {
        m.reset_disc(Uint8Array.from(image));
        const cold=[m.read_long(0x06000C00),m.read_long(0x06000C04)];
        for(let i=0;i<35;i++)m.run_steps(10000);
        traces.push({cold,words:Array.from({length:6},(_,i)=>m.read_long(0x0600E800+i*4)),
          exceptions:m.exception_count()});
      }
      return traces;
    } finally {m.free();}
  },[...hleResetImage]);
  const hleResetExpected={cold:[0,0],words:[0x12345678,0x12345679,0xBFFF,0x4B10,1,0x600D],exceptions:"0"};
  assert.deepEqual(hleResetTrace,[hleResetExpected,hleResetExpected]);
  // BUP_Init expands original SH-2 code and publishes the documented vector
  // table. All allocation/calendar/copy/checksum work executes in Wasm's CPU.
  const backupIso=await readFile("tests/fixtures/hle-backup.iso");
  const backupExpected=(await readFile("tests/fixtures/hle-backup.tsv","utf8")).trim()
    .split(/\r?\n/).map(line=>parseInt(line.split("\t")[1],16));
  const backupRun=async image=>page.evaluate(async ({image,count})=>{
    const {Emulator}=await import("./pkg/saturn_rust.js");
    const m=new Emulator();
    const finish=(machine,last)=>{
      for(let i=0;i<300;i++) {
        machine.run_steps(10000);
        if(machine.read_long(last)===0x600D) {
          if(machine.exception_count()!=="0")throw new Error("Backup guest took an exception");
          return;
        }
      }
      throw new Error("Backup guest exceeded its bounded instruction budget");
    };
    const probeImage=Uint8Array.from(image);
    new DataView(probeImage.buffer).setUint32(0xE28,0x0600E000);
    const probe=machine=>{
      machine.reset_disc(probeImage);finish(machine,0x0602400C);
      return Array.from({length:4},(_,i)=>machine.read_long(0x06024000+i*4));
    };
    try {
      m.reset_disc(Uint8Array.from(image));finish(m,0x06024000+(count-1)*4);
      const main=Array.from({length:count},(_,i)=>m.read_long(0x06024000+i*4));
      const vector=m.read_long(0x06000354),entry=m.read_long(vector+16);
      const packed=m.export_backup_ram();
      const persisted=[probe(m),probe(m)];
      assertBytes(m.export_backup_ram(),packed);
      const fresh=new Emulator();
      let imported;
      try {
        fresh.import_backup_ram(packed);imported=probe(fresh);
        assertBytes(fresh.export_backup_ram(),packed);
      } finally {fresh.free();}
      const broken=[];
      for(const [offset,bytes] of [[129,[0,0]],[129,[2,0]],[160,[0,0,1,25]],
                                  [128,[0x7F]],[168,[0x33]],[143,[1]],
                                  [154,[1]],[155,[6]],[164,[0,4]]]) {
        const damaged=Uint8Array.from(packed);damaged.set(bytes,offset);
        m.import_backup_ram(damaged);broken.push(probe(m));
        assertBytes(m.export_backup_ram(),damaged);
      }
      const unknown=new Uint8Array(32768).fill(0xA5);
      m.import_backup_ram(unknown);const unformatted=probe(m);
      assertBytes(m.export_backup_ram(),unknown);
      return {main,vector,entry,persisted,imported,broken,unformatted,
        header:Array.from(packed.slice(0,8)),data:[packed[168],packed[169],packed[424]]};
    } finally {m.free();}
    function assertBytes(actual,expected) {
      if(actual.length!==expected.length || actual.some((v,i)=>v!==expected[i]))
        throw new Error("Read-only backup probe changed persistent RAM");
    }
  },{image:[...image],count:backupExpected.length});
  const backupTrace={main:backupExpected,vector:0x06030000,entry:0x06030600,
    persisted:[[0,0,7,0x600D],[0,0,7,0x600D]],imported:[0,0,7,0x600D],
    broken:[[8,8,8,0x600D],[8,8,8,0x600D],[8,8,8,0x600D],[8,8,8,0x600D],[0,8,8,0x600D],
      [8,8,8,0x600D],[8,8,8,0x600D],[8,8,8,0x600D],[8,8,8,0x600D]],
    unformatted:[2,2,2,0x600D],header:[83,82,66,85,80,48,48,49],data:[90,91,90]};
  assert.deepEqual(await backupRun(backupIso),backupTrace);
  const relocatedBackup=Buffer.from(backupIso);let backupRelocations=0;
  for(let at=21*2048;at+4<=relocatedBackup.length;at+=4) {
    if(relocatedBackup.readUInt32BE(at)===0x06030000) {
      relocatedBackup.writeUInt32BE(0x06050000,at);backupRelocations++;
    }
  }
  assert.equal(backupRelocations,2);
  assert.deepEqual(await backupRun(relocatedBackup),{...backupTrace,vector:0x06050000,entry:0x06050600});
  const changedBackup=Buffer.from(backupIso),backupXor=Buffer.from([0xE4,90]);
  const backupXorAt=changedBackup.indexOf(backupXor,21*2048);
  assert(backupXorAt>=21*2048 && changedBackup.indexOf(backupXor,backupXorAt+1)===-1);
  changedBackup[backupXorAt+1]=91;
  const changedBackupExpected=structuredClone(backupTrace);
  (await readFile("tests/fixtures/hle-backup.tsv","utf8")).trim().split(/\r?\n/)
    .forEach((line,i)=>{if(line.startsWith("A byte "))changedBackupExpected.main[i]^=1;});
  changedBackupExpected.data=[90,90,91];
  assert.deepEqual(await backupRun(changedBackup),changedBackupExpected);
  // Real PLL command deadlines/reset domains, through original BIOS and HLE.
  const clockIso=await readFile("tests/fixtures/clock.iso");
  const clockRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/clock.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [address,...words]=line.split(/\s+/);let at=parseInt(address,16);
    for (const word of words) {clockRom.writeUInt16BE(parseInt(word,16),at);at+=2;}
  }
  const clockExpected=(await readFile("tests/fixtures/clock.tsv","utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
  const clockTraces=await page.evaluate(async ({iso,rom,count})=>{
    const {Emulator}=await import("./pkg/saturn_rust.js");
    const m=new Emulator(),traces=[];
    try {
      for(const hle of [false,true]) for(let repeat=0;repeat<2;repeat++) {
        if(hle)m.reset_disc(Uint8Array.from(iso));else m.reset_bios(Uint8Array.from(rom));
        while(Number(m.peripheral_clocks())<6000000)m.run_steps(10000);
        const slaveResult=m.read_long(0x0600D100);
        if(slaveResult===0 || m.slave_instruction_count()!=="0")throw new Error("Clock transition must stop and reset the executing slave");
        m.run_steps(1000);
        if(m.read_long(0x0600D100)!==slaveResult)throw new Error("Stopped slave must retain its last shared computation");
        m.advance_frame();m.advance_frame();
        const pixels=m.framebuffer();
        traces.push({words:Array.from({length:count},(_,i)=>m.read_long(0x0600E000+i*4)),
          dimensions:[m.video_width(),m.video_height()],length:pixels.length,
          green:pixels.every((v,i)=>v===[0,255,0,255][i%4]),exceptions:m.exception_count(),
          r8:m.registers()[8]});
      }
      const changed=Uint8Array.from(iso),positions=[];
      for(let i=21*2048;i<changed.length-1;i++)if(changed[i]===0xE4 && changed[i+1]===0)positions.push(i);
      if(positions.length!==1)throw new Error("Expected a unique original clock-call argument");
      changed[positions[0]+1]=1;m.reset_disc(changed);
      while(Number(m.peripheral_clocks())<6000000)m.run_steps(10000);
      const changedWords=Array.from({length:count},(_,i)=>m.read_long(0x0600E000+i*4));
      if(changedWords[0]!==1 || changedWords[9]!==0x75 || changedWords[count-1]!==0x600D) {
        throw new Error("Changed guest clock argument must change the clock ABI and DOTSEL");
      }
      // Production draw path must use the emulated dimensions on Canvas.
      const {emulator,advanceFrame}=await import("./app.js");
      emulator.reset_disc(Uint8Array.from(iso));
      while(Number(emulator.peripheral_clocks())<6000000)emulator.run_steps(10000);
      advanceFrame();
      const canvas=document.getElementById("screen");
      traces.push({canvas:[canvas.width,canvas.height],
        edge:Array.from(canvas.getContext("2d").getImageData(351,223,1,1).data)});
      return traces;
    } finally {m.free();}
  },{iso:[...clockIso],rom:[...clockRom],count:clockExpected.length});
  const clockTrace={words:clockExpected,dimensions:[352,224],length:352*224*4,
    green:true,exceptions:"0",r8:0x0600E054};
  assert.deepEqual(clockTraces,[clockTrace,clockTrace,clockTrace,clockTrace,
    {canvas:[352,224],edge:[0,255,0,255]}]);
  // Original TV-mode BIOS, same guest bytes as tests/display_modes.rs. The
  // host changes only its ROM operand and physical TV/controller input pins.
  const displayRom=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/display-modes.hex","utf8")).split(/\r?\n/)) {
    if(!line||line.startsWith("#"))continue;
    const [address,...words]=line.split(/\s+/);let at=parseInt(address,16);
    for(const word of words){displayRom.writeUInt16BE(parseInt(word,16),at);at+=2;}
  }
  const displayTraces=await page.evaluate(async rom=>{
    const {Emulator}=await import("./pkg/saturn_rust.js");
    const {emulator,advanceFrame}=await import("./app.js");
    const m=new Emulator(),traces=[];
    try {
      for(const pal of [false,true])for(const [vbits,lines] of [[0,224],[16,240],[32,256]]) {
        if(!pal&&lines===256)continue;
        for(const [hbits,width] of [[0,320],[1,352],[2,640],[3,704]])for(const interlace of [0,0x80,0xC0]) {
          const mode=0x8000|vbits|hbits|interlace;
          const bytes=Uint8Array.from(rom);bytes[0x100]=mode>>8;bytes[0x101]=mode&255;
          m.set_pal(pal);m.reset_bios(bytes);
          m.set_buttons((1<<0)|(1<<4)|(1<<12));m.set_port2_buttons((1<<3)|(1<<7)|(1<<11));
          m.set_analog(1,true,0,128,255);
          for(let n=0;n<60 && m.read_long(0x0600E088)!==0x600D;n++)m.run_steps(10000);
          const r=Array.from({length:35},(_,i)=>m.read_long(0x0600E000+i*4));
          const f=m.framebuffer(),height=lines*(interlace===0xC0?2:1);
          const points=[];
          for(const y of [0,height-1])for(const x of [0,1,2,3])points.push(Array.from(f.slice((y*width+x)*4,(y*width+x)*4+4)));
          if(width>=640)for(const x of [511,512])points.push(Array.from(f.slice(x*4,x*4+4)));
          traces.push({mode,pal,lines,width,height,r,points,dimensions:[m.video_width(),m.video_height()],length:f.length});
        }
      }
      // Production Canvas follows guest-selected high-resolution dimensions.
      const bytes=Uint8Array.from(rom);bytes[0x100]=0x80;bytes[0x101]=0xD3;
      emulator.set_pal(false);emulator.reset_bios(bytes);
      for(let n=0;n<60 && emulator.read_long(0x0600E088)!==0x600D;n++)emulator.run_steps(10000);
      advanceFrame();
      const canvas=document.getElementById("screen");
      traces.push({canvas:[canvas.width,canvas.height],edge:Array.from(canvas.getContext("2d").getImageData(1,479,1,1).data)});
      emulator.set_pal(false);
      return traces;
    }finally{m.free();}
  },[...displayRom]);
  assert.equal(displayTraces.length,61);
  for(const {mode,pal,lines,width,height,r,points,dimensions,length} of displayTraces.slice(0,-1)) {
    assert.equal(r[34],0x600D,`TV mode ${mode.toString(16)}, PAL=${pal}`);
    assert(r[1]<24);
    assert(Math.floor(r[15]/2)>=lines-18 && Math.floor(r[15]/2)<lines-12);
    for(const base of [0,14]) {
      assert.equal(r[base+2],0xC0);
      assert.deepEqual(r.slice(base+3,base+14),[0xF1,2,0xEE,0xF7,0xF1,0x15,0x77,0x7F,0,128,255]);
    }
    for(const base of [28,31]){assert.equal(r[base]&9,8|Number(pal));assert.equal(Math.floor(r[base+2]/2),lines);}
    assert.equal((r[28]&2)===(r[31]&2),(mode&0xC0)===0);
    assert.equal((r[30]&1)===(r[33]&1),(mode&0xC0)!==0xC0);
    assert.deepEqual(dimensions,[width,height]);assert.equal(length,width*height*4);
    const row=[[255,0,0,255],[0,255,0,255],[0,0,255,255],[255,255,255,255]];
    assert.deepEqual(points,[...row,...row,...(width>=640?[[255,0,255,255],[255,0,0,255]]:[])]);
  }
  assert.deepEqual(displayTraces.at(-1),{canvas:[704,480],edge:[0,255,0,255]});
  await videoTransactions(page);
  await abusTiming(page);
  await busTransactions(page);
  await dmacBus(page);
  const shrinkRom=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/sprite-shrink.hex","utf8")).split(/\r?\n/)) {
    if(!line||line.startsWith("#"))continue;
    const [address,...words]=line.split(/\s+/);let at=parseInt(address,16);
    for(const word of words){shrinkRom.writeUInt16BE(parseInt(word,16),at);at+=2;}
  }
  const shrinkExpected=(await readFile("tests/fixtures/sprite-shrink.tsv","utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
  const shrinkTraces=await page.evaluate(async ({rom,count})=>{
    const {emulator,advanceFrame}=await import("./app.js");
    const traces=[];
    for(let repeat=0;repeat<2;repeat++) {
      emulator.reset_bios(Uint8Array.from(rom));
      for(let i=0;i<300 && emulator.read_long(0x0600D074)!==0x600D;i++)emulator.run_steps(5000);
      emulator.advance_frame();
      advanceFrame();
      const f=emulator.framebuffer(),canvas=document.getElementById("screen").getContext("2d");
      traces.push({words:Array.from({length:count},(_,i)=>emulator.read_long(0x0600D000+i*4)),
        pixels:Array.from(f.slice((20*320+10)*4,(20*320+15)*4)),
        canvas:Array.from(canvas.getImageData(10,20,5,1).data)});
    }
    // Only the six guest HSS mode-word operands change. The same guest
    // commands still draw; EOS alone must have no effect when HSS is clear.
    const changed=Uint8Array.from(rom);let patches=0;
    for(let i=0x200;i<changed.length-4;i+=4) {
      if(changed[i]===0&&changed[i+1]===0&&changed[i+2]===0x10&&changed[i+3]===0x68){changed[i+2]=0;patches++;}
    }
    if(patches!==6)throw new Error(`HSS operand count ${patches}`);
    emulator.reset_bios(changed);
    for(let i=0;i<300 && emulator.read_long(0x0600D074)!==0x600D;i++)emulator.run_steps(5000);
    traces.push({changed:Array.from({length:count},(_,i)=>emulator.read_long(0x0600D000+i*4))});
    return traces;
  },{rom:[...shrinkRom],count:shrinkExpected.length});
  const shrinkPixels=[57,0,0,255,41,0,0,255,24,0,0,255,8,0,0,255,0,0,0,255];
  const shrinkTrace={words:shrinkExpected,pixels:shrinkPixels,canvas:shrinkPixels};
  const ordinaryShrink=[0,2,4,7].map(x=>0x8000+x),flippedShrink=[7,5,3,0].map(x=>0x8000+x);
  assert.deepEqual(shrinkTraces,[shrinkTrace,shrinkTrace,{changed:[...ordinaryShrink,...ordinaryShrink,
    ...ordinaryShrink,...ordinaryShrink,...ordinaryShrink,...flippedShrink,...flippedShrink,0x1090,0x600D]}]);
  const digitalRom=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/digital-timing.hex","utf8")).split(/\r?\n/)) {
    if(!line || line.startsWith("#"))continue;
    const [address,...words]=line.split(/\s+/);let at=parseInt(address,16);
    for(const word of words){digitalRom.writeUInt16BE(parseInt(word,16),at);at+=2;}
  }
  const digitalExpected=(await readFile("tests/fixtures/digital-timing.tsv","utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
  const digitalTrace=await page.evaluate(async ({rom,count})=>{
    const {Emulator}=await import("./pkg/saturn_rust.js");
    const m=new Emulator(),traces=[];
    try {
      for(const buttons of [[0x842,0x184],[0x842,0x184],[0,0]]) {
        m.reset_bios(Uint8Array.from(rom));
        m.set_buttons(buttons[0]);m.set_port2_buttons(buttons[1]);m.run_steps(1000);
        traces.push(Array.from({length:count},(_,i)=>m.read_long(0x0600E000+i*4)));
      }
      return traces;
    } finally {m.free();}
  },{rom:[...digitalRom],count:digitalExpected.length});
  assert.deepEqual(digitalTrace.slice(0,2),[digitalExpected,digitalExpected]);
  for(const [bank,select] of [0x20,0x40,0,0x60].entries())for(let port=0;port<2;port++) {
    assert.equal(digitalTrace[2][bank*4+2+port],select|0x10|(select===0x60?0xC:15));
  }
  const rawDirectory = rawMode1(directoryIso);
  for (const damage of ["none", "correctable", "uncorrectable"]) {
    const raw = Buffer.from(rawDirectory);
    if (damage === "correctable") raw[20 * 2352 + 16 + 70] ^= 0x5A;
    if (damage === "uncorrectable") raw.fill(0xB7, 20 * 2352 + 16, 20 * 2352 + 2064);
    const trace = await page.evaluate(async ({ rom, raw }) => {
      const { Emulator } = await import("./pkg/saturn_rust.js");
      const emulator = new Emulator();
      try {
        emulator.reset_bios(Uint8Array.from(rom));
        const before = [emulator.mode(), emulator.special_registers()[0], emulator.instruction_count()];
        let rejected = false;
        try {
          emulator.mount_cue('FILE "d" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00', "d", new Uint32Array([0xFFFFFFFF]), Uint8Array.from(raw));
        } catch { rejected = true; }
        if (!rejected || JSON.stringify(before) !== JSON.stringify([emulator.mode(), emulator.special_registers()[0], emulator.instruction_count()])) {
          throw new Error("Malformed CUE mount must reject without resetting the BIOS");
        }
        emulator.mount_cue('FILE "d" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00', "d", new Uint32Array([raw.length]), Uint8Array.from(raw));
        emulator.run_steps(3000);
        return [0, 0x10, 0x20, 0x24, 0x30].map((offset) => emulator.read_long(0x06009000 + offset));
      } finally { emulator.free(); }
    }, { rom: [...directoryRom], raw: [...raw] });
    if (damage === "uncorrectable") {
      assert.deepEqual([trace[0], trace[1], trace[2], trace[4]], [0xFF00, 0xFF00, 0xFF00, 0x600D]);
    } else {
      assert.deepEqual(trace, [0x0180, 0x0180, 0x0180, 2, 0x600D]);
    }
  }
  // The original guest configures runtime correction/retries, observes RETRY
  // and ERROR, reads received/corrected bytes and cancels infinite retry. Host
  // code supplies only original media/ROM bytes, never MMIO or result writes.
  const decodeRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/cd-decode.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [address, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => decodeRom.writeUInt16BE(word, address + i * 2));
  }
  const decodeMedia = rawMode1(Buffer.alloc(4096, 0x69));
  for (const [delta, offsets] of [[0x5A, [16]], [0xA6, [16]], [0x5A, [16, 102, 190, 18]]]) {
    const raw = Buffer.from(decodeMedia);
    for (const offset of offsets) raw[offset] ^= delta;
    const traces = await page.evaluate(async ({ rom, raw }) => {
      const { Emulator } = await import("./pkg/saturn_rust.js");
      const emulator = new Emulator();
      const traces = [];
      try {
        for (let reset = 0; reset < 2; reset++) {
          emulator.reset_bios(Uint8Array.from(rom));
          emulator.mount_cue('FILE "d" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00', "d", new Uint32Array([raw.length]), Uint8Array.from(raw));
          for (let i = 0; i < 100; i++) emulator.run_steps(10000);
          traces.push(Array.from({ length: 0x190 / 4 + 1 }, (_, i) => emulator.read_long(0x0600A000 + i * 4)));
        }
        return traces;
      } finally { emulator.free(); }
    }, { rom: [...decodeRom], raw: [...raw] });
    assert.deepEqual(traces[0], traces[1]);
    const read = (offset) => traces[0][offset / 4];
    assert.equal(read(0x190), 0x600D);
    assert.equal(read(0x30), 0x0980);
    assert.equal(read(0x3C), 150);
    assert.equal(read(0x4C), 0);
    for (const offset of [0x7C, 0xCC, 0x13C]) assert.equal(read(offset), 1);
    assert.equal(read(0x180), raw.readUInt32BE(16));
    assert.equal(read(0x184), 0x69696969);
    assert.equal(read(0x188), 0x69696969);
    assert(read(0x18C) > 0);
    assert.equal(read(0x110), 0x0880);
    assert.equal(read(0x11C), 150);
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
  // H §§10.2–10.4/table10.2: independent overflow/normal DIVU vectors,
  // guest IRQ acknowledgement and register/stack independence on both SH-2s.
  const divuRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/divu.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map(word => parseInt(word,16));
    words.forEach((word,i) => divuRom.writeUInt16BE(word,offset+i*2));
  }
  const divuExpected = (await readFile("tests/fixtures/divu.tsv","utf8")).trim()
    .split(/\r?\n/).map(word => parseInt(word,16));
  const runDivu = async rom => page.evaluate(async ({rom, count}) => {
    const { Emulator } = await import("./pkg/saturn_rust.js");
    const guest = new Emulator();
    const runs = [];
    try {
      for (let boot = 0; boot < 2; boot++) {
        guest.reset_bios(new Uint8Array(rom));
        guest.run_steps(10000);
        runs.push({mailboxes:[0x06012000,0x06013000].map(base =>
          Array.from({length:count},(_,i) => guest.read_long(base+i*4))),
          exceptions:[guest.exception_count(),guest.slave_exception_count()],
          stacks:[guest.registers()[15],guest.slave_registers()[15]]});
      }
      return runs;
    } finally { guest.free(); }
  }, {rom:[...rom],count:divuExpected.length});
  for (const changed of [false,true]) {
    const bios = Buffer.from(divuRom);
    const expected = [...divuExpected];
    if (changed) {
      let mutations=0;
      for(let at=0;at+4<=bios.length;at+=4) {
        if(bios.readUInt32BE(at)===0x9ABCDEF0) {
          bios.writeUInt32BE(0x9ABCDEF1,at); mutations++;
        }
      }
      assert(mutations>0);
      for(const [i,value] of expected.entries()) if(value===0xD5E6F787) expected[i]+=8;
    }
    const runs=await runDivu(bios);
    assert.deepEqual(runs[0],runs[1]);
    assert.deepEqual(runs[0].mailboxes,[expected,expected]);
    assert.deepEqual(runs[0].exceptions,["2","2"]);
    assert.deepEqual(runs[0].stacks,[0x060FF000,0x060FE000]);
  }
  // ST-169 §2.4.2: board area straps and independent CD/sound reset
  // reports. Every command, poll and mailbox write executes in the guest.
  const statusRom = Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/smpc-status.hex","utf8")).split(/\r?\n/)) {
    if(!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>statusRom.writeUInt16BE(w,at+i*2));
  }
  const statuses = await page.evaluate(async rom => {
    const { Emulator } = await import("./pkg/saturn_rust.js");
    const guest = new Emulator();
    const results=[];
    try {
      for(const code of [1,2,4,5,6,10,12,13]) {
        guest.set_region(code);
        for(const invalid of [0,3,7,8,9,11,14,15,255]) {
          let rejected=false;
          try { guest.set_region(invalid); } catch { rejected=true; }
          if(!rejected) throw new Error("Reserved SMPC area must reject");
        }
        const runs=[];
        for(let boot=0;boot<2;boot++) {
          guest.reset_bios(new Uint8Array(rom)); guest.run_steps(10000);
          runs.push(Array.from({length:21},(_,i)=>guest.read_long(0x06017000+i*4)));
          if(guest.exception_count()!=="0") throw new Error("Unexpected SMPC guest exception");
        }
        results.push(runs);
      }
      return results;
    } finally { guest.free(); }
  },[...statusRom]);
  for(const [i,code] of [1,2,4,5,6,10,12,13].entries()) {
    const expected=[...[[0x35,0x40],[0x34,0x40],[0x34,0],[0x35,0]]
      .flatMap(([sound,cd])=>[code,sound,cd,0,0]),0x600D];
    assert.deepEqual(statuses[i],[expected,expected]);
  }
  // Original BSC guest identifies both CPUs through BCR1.MASTER and executes
  // independent refresh handlers and module-stop/resume through real Wasm MMIO.
  const bscRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/bsc.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => bscRom.writeUInt16BE(word, offset + i * 2));
  }
  await page.locator("#bios").setInputFiles({ name: "original-bsc.bin", mimeType: "application/octet-stream", buffer: bscRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
  const bscTrace = () => page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    emulator.run_steps(3000);
    return { mailboxes: [0x06008000, 0x06008040].map((base) =>
      Array.from({length: 8}, (_, i) => emulator.read_long(base + i * 4))),
      exceptions: [emulator.exception_count(), emulator.slave_exception_count()],
      stacks: [emulator.registers()[15], emulator.slave_registers()[15]],
      clocks: emulator.peripheral_clocks() };
  });
  const bscFirst = await bscTrace();
  for (const [i, id] of [0x3F0, 0x83F0].entries()) {
    const box = bscFirst.mailboxes[i];
    assert.deepEqual(box.slice(0, 6), [id, id, 0xD0, 1, 0, 0]);
    assert(box[6] > 0);
    assert.equal(box[7], 0x600D);
  }
  assert.deepEqual(bscFirst.exceptions, ["1", "1"]);
  assert.deepEqual(bscFirst.stacks, [0x06100000, 0x060FE000]);
  await page.locator("#reset").click();
  assert.deepEqual(await bscTrace(), bscFirst);
  // The same independent both-CPU wait diagnostic as native tests. FRT measures
  // real external read delays. Changing only WCR or the source data must change
  // the corresponding computed guest records, excluding canned timing/results.
  await hardwareBoundaries(page);
  await refreshArbitration(page);
  const waitsRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/bsc-waits.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => waitsRom.writeUInt16BE(word, offset + i * 2));
  }
  const waitVectors = (await readFile("tests/fixtures/bsc-waits.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  const endianVectors = (await readFile("tests/fixtures/bsc-endian.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  for (const [before, after, width, little] of [[0, 0, 4, true], [0xA55AAA40, 0xA55AAA80, 4, true],
    [0x13579BDF, 0x2468ACE0, 4, true], [0xA55A00FC, 0xA55A00AC, 2, true],
    [0xA55A00FC, 0xA55A005C, 1, false], [0, 0, 4, false]]) {
    const changed = Buffer.from(waitsRom);
    if (before) {
      const needle = Buffer.alloc(4); needle.writeUInt32BE(before);
      const at = changed.indexOf(needle);
      assert(at >= 0 && changed.indexOf(needle, at + 1) < 0);
      changed.writeUInt32BE(after, at);
    }
    if (!little) {
      const needle = Buffer.from([0, 0, 0x10, 0]);
      const at = changed.indexOf(needle);
      assert(at >= 0 && changed.indexOf(needle, at + 1) < 0);
      changed.writeUInt32BE(0, at);
    }
    await page.locator("#bios").setInputFiles({ name: "original-bsc-waits.bin", mimeType: "application/octet-stream", buffer: changed });
    await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
    const run = () => page.evaluate(async () => {
      const { emulator } = await import("./app.js");
      emulator.run_steps(3000);
      return { traces: [0x0600E000, 0x0600E100].map(base => Array.from({length: 24}, (_, i) => emulator.read_long(base + i * 4))),
        exceptions: [emulator.exception_count(), emulator.slave_exception_count()] };
    });
    const actual = await run();
    assert.deepEqual(actual.exceptions, ["0", "0"]);
    for (const [cpu, trace] of actual.traces.entries()) {
      const expected = [cpu ? 0x83F0 : 0x3F0, 0xAAFF];
      waitVectors.forEach((ticks, i) => expected.push(before === 0xA55AAA40 && i === 1 ? 12 : 10 + 2 * (4 / width - 1) + (ticks - 10) * 4 / width,
        before === 0x13579BDF ? after : 0x13579BDF));
      expected.push(...(little ? endianVectors : [0x11223344,0x1122,0x3344,0x11,0xA1B2C3D4,0x1234C3D4,0x5634C3D4]), 0x600D);
      assert.deepEqual(trace, expected);
    }
    await page.locator("#reset").click();
    assert.deepEqual(await run(), actual);
  }
  const shareRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/bsc-share.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.trim().split(/\s+/).map(v => parseInt(v,16));
    words.forEach((v,i) => shareRom.writeUInt16BE(v, at+i*2));
  }
  const shareGolden = (await readFile("tests/fixtures/bsc-share.tsv", "utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
  for (const bits of [0xFF0, 0xBF0]) {
    const rom = Buffer.from(shareRom), needle = Buffer.from([0xA5,0x5A,0x0F,0xF0]), at = rom.indexOf(needle);
    assert(at >= 0 && rom.indexOf(needle, at+1) < 0);
    rom.writeUInt32BE(0xA55A0000 + bits, at);
    const results = await page.evaluate(async rom => {
      const {Emulator} = await import("./pkg/saturn_rust.js");
      const emulator = new Emulator(), results = [];
      try {
        for (let repetition=0; repetition<2; repetition++) {
          emulator.reset_bios(Uint8Array.from(rom)); emulator.run_steps(3000);
          results.push({traces:[0x0600E600,0x0600E700].map(base => Array.from({length:5},(_,i)=>emulator.read_long(base+i*4))),
            exceptions:[emulator.exception_count(),emulator.slave_exception_count()]});
        }
        return results;
      } finally {emulator.free();}
    }, [...rom]);
    const expected = [...shareGolden]; if (!(bits & 0x400)) expected[0] = 0x7000;
    const trace = {traces:[[bits,...expected],[bits|0x8000,...expected]],exceptions:["0","0"]};
    assert.deepEqual(results,[trace,trace]);
  }
  console.log("Original BSC-sharing, XA/interleaved-filesystem and multi-area/bootstrap guests passed, including operand mutations and repeated cold resets.");
  // Both SH-2s warm their instruction cache and execute real TAS read/write
  // pairs. FRT must reflect IW3=0/1's mandatory one cycle and IW3=2's two.
  const idleRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/bsc-idle.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map(word => parseInt(word, 16));
    words.forEach((word, i) => idleRom.writeUInt16BE(word, offset + i * 2));
  }
  const idleVectors = (await readFile("tests/fixtures/bsc-idle.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  for (const [before, after, index, value] of [[0, 0, 0, 0xAAFF],
    [0xA55A8000, 0xA55A4000, 3, 11], [0x25, 0x3C, 4, 0xBC]]) {
    const changed = Buffer.from(idleRom);
    if (before) {
      const needle = Buffer.alloc(4); needle.writeUInt32BE(before);
      const at = changed.indexOf(needle);
      assert(at >= 0 && changed.indexOf(needle, at + 1) < 0);
      changed.writeUInt32BE(after, at);
    }
    await page.locator("#bios").setInputFiles({ name: "original-bsc-idle.bin", mimeType: "application/octet-stream", buffer: changed });
    await page.waitForFunction(() => document.getElementById("pc").textContent === "0x20000200");
    const runIdle = () => page.evaluate(async () => {
      const { emulator } = await import("./app.js");
      emulator.run_steps(3000);
      return { traces: [0x0600E200, 0x0600E300].map(base => Array.from({length: 6}, (_, i) => emulator.read_long(base + i * 4))),
        exceptions: [emulator.exception_count(), emulator.slave_exception_count()] };
    });
    const actual = await runIdle();
    assert.deepEqual(actual.exceptions, ["0", "0"]);
    const expected = [...idleVectors]; expected[index] = value;
    for (const trace of actual.traces) assert.deepEqual(trace, expected);
    await page.locator("#reset").click();
    assert.deepEqual(await runIdle(), actual);
  }
  // Original cached instruction-state kernels run independently on both
  // SH-2s. Each resets its FRT divider and measures table7.2 states. Changing
  // only guest ADDs to real BRA/NOP pairs changes time and computed results.
  const timingRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sh2-timing.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.trim().split(/\s+/).map(v => parseInt(v,16));
    words.forEach((word,i) => timingRom.writeUInt16BE(word,at+i*2));
  }
  const timingVectors = (await readFile("tests/fixtures/sh2-timing.tsv","utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
  for (const mutation of [false,true,"peripheral"]) {
    const image=Buffer.from(timingRom),expected=[...timingVectors];
    if(mutation === "peripheral") {
      let changes=0;
      for(let at=0;at<image.length-4;at+=2) {
        if(image.readUInt32BE(at)===0xE0FF2A00) {image[at+1]=0x5A;changes++;}
      }
      assert.equal(changes,24);
      expected[21]=0x5A;
    } else if(mutation) {
      const at=image.indexOf(Buffer.from([0x76,1,0x76,1]));
      assert(at>=0);
      for(let i=0;i<8;i++)image.set([0xA0,0,0,9],at+i*4);
      expected[0]=5;expected[1]=16;
    }
    await page.locator("#bios").setInputFiles({name:"original-sh2-timing.bin",mimeType:"application/octet-stream",buffer:image});
    await page.waitForFunction(()=>document.getElementById("pc").textContent==="0x00000200");
    const runTiming=()=>page.evaluate(async(count)=>{
      const {emulator}=await import("./app.js");
      emulator.run_steps(10000);
      return {words:[0x0600E600,0x0600E700].map(base=>Array.from({length:count},(_,i)=>emulator.read_long(base+i*4))),
        exceptions:[emulator.exception_count(),emulator.slave_exception_count()]};
    },timingVectors.length);
    const actual=await runTiming();
    assert.deepEqual(actual,{words:[expected,expected],exceptions:["0","0"]});
    await page.locator("#reset").click();
    assert.deepEqual(await runTiming(),actual);
  }
  // Original register-count/fixed-output DSP DMA and actual execution of a
  // second DSP program transferred from guest work RAM into program RAM.
  const dspDmaRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/dsp-dma.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map(word => parseInt(word, 16));
    words.forEach((word, i) => dspDmaRom.writeUInt16BE(word, offset + i * 2));
  }
  const dspDmaVectors = (await readFile("tests/fixtures/dsp-dma.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  for (const [before, after, index, value] of [[0, 0, 0, 17],
    [0x11, 0x35, 0, 0x35], [3, 2, 0, 0], [0x8400002A, 0x84000063, 5, 99],
    [0x8C000039, 0x8C00004D, 8, 77], [0x00001207, 0x0000120B, 7, 11],
    [0x8C00006B, 0x8C00007D, 11, 125], [0xC000C400, 0xC000C401, 11, 0]]) {
    const changed = Buffer.from(dspDmaRom);
    if (before) {
      const needle = Buffer.alloc(4); needle.writeUInt32BE(before);
      const at = changed.indexOf(needle);
      assert(at >= 0 && changed.indexOf(needle, at + 1) < 0);
      changed.writeUInt32BE(after, at);
    }
    await page.locator("#bios").setInputFiles({ name: "original-dsp-dma.bin", mimeType: "application/octet-stream", buffer: changed });
    await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
    const runDspDma = () => page.evaluate(async () => {
      const { emulator } = await import("./app.js");
      emulator.run_steps(10000);
      return { trace: Array.from({length: 13}, (_, i) => emulator.read_long(0x0600E400 + i * 4)),
        exceptions: emulator.exception_count() };
    });
    const actual = await runDspDma();
    const expected = [...dspDmaVectors]; expected[index] = value;
    assert.deepEqual(actual, { trace: expected, exceptions: "0" });
    await page.locator("#reset").click();
    assert.deepEqual(await runDspDma(), actual);
  }
  // SOA #8 §4.1: touching the DMA bank is prohibited. Mutate the legal
  // concurrent MC0 -> MC2 instruction into MC1 -> MC2 during M1 DMA.
  const conflictedDsp = Buffer.from(dspDmaRom);
  const conflictAt = conflictedDsp.indexOf(Buffer.from([0, 0, 0x32, 4]));
  assert(conflictAt >= 0 && conflictedDsp.indexOf(Buffer.from([0, 0, 0x32, 4]), conflictAt + 1) < 0);
  conflictedDsp.writeUInt32BE(0x00003205, conflictAt);
  await page.locator("#bios").setInputFiles({name:"original-dsp-bank-conflict.bin", mimeType:"application/octet-stream", buffer:conflictedDsp});
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
  await page.locator("#count").fill("4000");
  await page.locator("#run").click();
  assert.match(await value("error"), /SCU DSP at 02: data bank 1 is owned by DMA/);
  assert.equal(await page.locator("#run").isDisabled(), true);
  const conflictStopped = await machine();
  await page.evaluate(async () => {
    const {emulator} = await import("./app.js");
    try { emulator.run_steps(4000); } catch { /* Sticky diagnostic. */ }
    if (emulator.read_long(0x0600E400) !== 0) throw Error("Conflicting DSP fabricated a result");
  });
  assert.deepEqual(await machine(), conflictStopped);
  await page.locator("#reset").click();
  await page.locator("#run").click();
  assert.match(await value("error"), /SCU DSP at 02: data bank 1 is owned by DMA/);
  // Original SH-2/DSP B-bus guest: all eight output strides, counter wrap,
  // sound RAM input, independent authored words and changed guest operands.
  const dspBusRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/dsp-bus.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map(word => parseInt(word, 16));
    words.forEach((word, i) => dspBusRom.writeUInt16BE(word, offset + i * 2));
  }
  const dspBusVectors = (await readFile("tests/fixtures/dsp-bus.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  for (const mutation of ["baseline", "data", "stride"]) {
    const changed = Buffer.from(dspBusRom);
    const expected = [...dspBusVectors];
    if (mutation === "data") {
      const needle = Buffer.from([0x11, 0x22, 0x33, 0x44]);
      let count = 0;
      for (let at = changed.indexOf(needle); at >= 0; at = changed.indexOf(needle, at + 4)) {
        changed.writeUInt32BE(0x55667788, at); count++;
      }
      assert.equal(count, 8);
      expected[1] = 0x55667788;
      for (const i of [3, 7, 11, 15, 19, 23]) {
        expected[i] = 0x5566EEEE; expected[i + 1] = 0x7788EEEE;
      }
    } else if (mutation === "stride") {
      const needle = Buffer.from([0xC0, 0, 0x90, 2]);
      const at = changed.indexOf(needle);
      assert(at >= 0 && changed.indexOf(needle, at + 1) < 0);
      changed.writeUInt32BE(0xC0001002, at);
      expected[1] = 0xCCDDEEEE; expected[2] = 0xEEEEEEEE;
    }
    await page.locator("#bios").setInputFiles({name:"original-dsp-bus.bin", mimeType:"application/octet-stream", buffer:changed});
    await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
    const runDspBus = () => page.evaluate(async count => {
      const {emulator} = await import("./app.js");
      emulator.run_steps(10000);
      return {trace:Array.from({length:count}, (_, i) => emulator.read_long(0x0600E800 + i * 4)),
        exceptions:emulator.exception_count()};
    }, dspBusVectors.length);
    assert.deepEqual(await runDspBus(), {trace:expected, exceptions:"0"});
    await page.locator("#reset").click();
    assert.deepEqual(await runDspBus(), {trace:expected, exceptions:"0"});
  }
  // Original UBC fixture: real SH-2 and DMA accesses, saved exception frames,
  // before/after fetches and delay-slot deferral, repeated after a cold reset.
  const ubcRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/ubc.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => ubcRom.writeUInt16BE(word, offset + i * 2));
  }
  await page.locator("#bios").setInputFiles({ name: "original-ubc.bin", mimeType: "application/octet-stream", buffer: ubcRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
  const ubcTrace = () => page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    emulator.run_steps(2000);
    return { mailbox: Array.from({length: 21}, (_, i) => emulator.read_long(0x0600A000 + i * 4)),
      exceptions: emulator.exception_count(), sp: emulator.registers()[15] };
  });
  const ubcFirst = await ubcTrace();
  assert.deepEqual(ubcFirst.mailbox, [0x1000, 0, 0x9000, 0, 0x1102, 22, 0x9400, 0,
    0x1206, 33, 0x9000, 0, 0x1288, 33, 0x1088, 0, 0x133C, 33, 0x1048, 0, 0x600D]);
  assert.equal(ubcFirst.exceptions, "5");
  assert.equal(ubcFirst.sp, 0x06100000);
  await page.locator("#reset").click();
  assert.deepEqual(await ubcTrace(), ubcFirst);
  // SYSRES and software standby use original ROMs and the production reset/NMI
  // paths. No browser code writes a mailbox or supplies device results.
  for (const [fixture, base, count, steps, expected] of [
    ["system-reset", 0x0600C000, 12, 350000, [13, 0, 0, 0, 0, 0, 0, 0x12, 0x34, 0x56, 0x78, 0x600D]],
    ["standby", 0x0600D000, 5, 2000, [0x1002, 0xF0, 0xFFFFFFC8, 0xFFFFFFFF, 0x600D]],
  ]) {
    const rom = Buffer.alloc(524288);
    for (const line of (await readFile(`tests/fixtures/${fixture}.hex`, "utf8")).split(/\r?\n/)) {
      if (!line || line.startsWith("#")) continue;
      const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
      words.forEach((word, i) => rom.writeUInt16BE(word, offset + i * 2));
    }
    if (fixture === "system-reset") {
      // Explicit local import establishes a new battery image; the guest changes
      // only its own reset marker. Rejected imports remain covered separately.
      await page.locator("#backup-import").setInputFiles({ name: "system-reset.bup", mimeType: "application/octet-stream", buffer: Buffer.alloc(32768, 0xA5) });
      await page.waitForFunction(() => document.getElementById("backup-status").textContent.startsWith("Imported"));
    }
    await page.locator("#bios").setInputFiles({ name: `original-${fixture}.bin`, mimeType: "application/octet-stream", buffer: rom });
    await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
    const result = await page.evaluate(async ({base, count, steps}) => {
      const { emulator } = await import("./app.js");
      for (let done = 0; done < steps; done += 10000) emulator.run_steps(Math.min(10000, steps - done));
      return { mailbox: Array.from({length: count}, (_, i) => emulator.read_long(base + i * 4)),
        sp: emulator.registers()[15], exceptions: emulator.exception_count() };
    }, {base, count, steps});
    assert.deepEqual(result.mailbox, expected);
    assert.equal(result.sp, 0x06100000);
    assert.equal(result.exceptions, fixture === "standby" ? "1" : "0");
    if (fixture === "system-reset") {
      const disc = Buffer.from(await readFile("web/star-collector.iso"));
      rom.copy(disc, 21 * 2048, 0x200, 0x39C);
      const hleResult = await page.evaluate(async (bytes) => {
        const { Emulator } = await import("./pkg/saturn_rust.js");
        const guest = new Emulator();
        guest.reset_disc(new Uint8Array(bytes));
        for (let i = 0; i < 35; i++) guest.run_steps(10000);
        const result = { mode: guest.mode(), done: guest.read_long(0x0600C02C),
          command: guest.read_long(0x0600C000), sp: guest.registers()[15] };
        guest.free();
        return result;
      }, Array.from(disc));
      assert.deepEqual(hleResult, { mode: "BIOS-less HLE disc", done: 0x600D, command: 13, sp: 0x060FF000 });
    }
  }
  // Original ECMA Mode2 media: no XA subheaders/EDC/ECC are fabricated.
  // Guest reads and sums every 2336-byte payload via HIRQ and the real FIFO.
  const mode2Rom=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/mode2.hex","utf8")).split(/\r?\n/)) {
    if(!line || line.startsWith("#"))continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>mode2Rom.writeUInt16BE(w,at+i*2));
  }
  const mode2Cue=await readFile("tests/fixtures/mode2.cue","utf8");
  const mode2Audio=await readFile("tests/fixtures/mode2-audio.bin");
  for(const mutated of [false,true]) {
    const raw=Buffer.from(await readFile("tests/fixtures/mode2.bin"));
    if(mutated){raw[16]=0xAA;raw[2351]=0x55;}
    const result=await page.evaluate(async ({rom,raw,audio,cue})=>{
      const {Emulator}=await import("./pkg/saturn_rust.js");
      const guest=new Emulator();
      try {
        const traces=[];
        for(let reset=0;reset<2;reset++) {
          guest.reset_bios(Uint8Array.from(rom));
          guest.mount_cue(cue,"mode2.bin\0mode2-audio.bin",Uint32Array.from([raw.length,audio.length]),Uint8Array.from([...raw,...audio]));
          for(let i=0;i<10;i++)guest.run_steps(10000);
          traces.push({done:guest.read_long(0x0600D084),words:guest.read_long(0x0600D064),
            count:guest.read_long(0x0600D07C),sum:guest.read_long(0x0600D080),
            data:Array.from({length:584},(_,i)=>guest.read_long(0x06010000+i*4))});
        }
        return traces;
      } finally {guest.free();}
    },{rom:[...mode2Rom],raw:[...raw],audio:[...mode2Audio],cue:mode2Cue});
    let sum=0;for(let at=16;at<raw.length;at+=2)sum+=raw.readUInt16BE(at);
    const data=Array.from({length:584},(_,i)=>raw.readUInt32BE(16+i*4));
    for(const trace of result)assert.deepEqual(trace,{done:0x600D,words:1168,count:0,sum,data});
  }
  // Original CD guest issues every command and polls real HIRQ/CR registers.
  // ISO bytes are original zero-filled sectors; no browser writes to mailboxes.
  const driveRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/cd-drive.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>driveRom.writeUInt16BE(w,at+i*2));
  }
  let previousDrive;
  for (let reset=0;reset<2;reset++) {
    await page.locator("#bios").setInputFiles({name:"original-cd-drive.bin",mimeType:"application/octet-stream",buffer:driveRom});
    await page.waitForFunction(()=>document.getElementById("pc").textContent==="0x00000200");
    const result=await page.evaluate(async()=>{
      const {emulator}=await import("./app.js");
      emulator.mount_disc(new Uint8Array(2048*100));
      for(let i=0;i<80;i++) emulator.run_steps(10000);
      return Array.from({length:100},(_,i)=>emulator.read_long(0x06009000+i*4));
    });
    assert.equal(result[0xF0/4],0x600D);
    for (const [offset,status] of [[0,0x0180],[0x10,0x0500],[0x30,0x0500],[0x50,0x0400],
      [0x60,0x0180],[0x70,0x8180],[0x80,0x0600],[0x90,0x0180],[0xA0,0x4180]]) {
      assert.equal(result[offset/4],status,`CD guest offset ${offset.toString(16)}`);
    }
    assert(result[0x2C/4]>=170 && result[0x2C/4]<250);
    assert(result[0x4C/4]>=150 && result[0x4C/4]<160);
    assert.equal(result[0xA4/4],204);
    assert.equal(result[0xB4/4],2);
    assert.equal(result[0xCC/4],0);
    assert.equal(result[0xE0/4],0x41000096);
    for (const offset of [0x110,0x120,0x140,0x150]) assert.equal(result[offset/4],0xFF00);
    assert.equal(result[0x134/4],0x0304);
    assert.equal(result[0x188/4],0x0400);
    if (previousDrive) assert.deepEqual(result,previousDrive);
    previousDrive=result;
  }
  // Beam-polled guest changes back color on lines64/128/192, then changes
  // VRAM/offset during blanking. Both Wasm and production Canvas must retain
  // the historical bands rather than repainting them using the final state.
  const rasterRom=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/raster-history.hex","utf8")).split(/\r?\n/)) {
    if(!line || line.startsWith("#"))continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>rasterRom.writeUInt16BE(w,at+i*2));
  }
  const rasterYs=[0,63,64,127,128,191,192,223];
  for(const mutated of [false,true]) {
    const image=Buffer.from(rasterRom);
    if(mutated) {
      const at=image.indexOf(Buffer.from([0,0,0x83,0xE0]));
      assert(at>=0 && image.indexOf(Buffer.from([0,0,0x83,0xE0]),at+1)===-1);
      image.writeUInt32BE(0x801F,at);
    }
    await page.locator("#bios").setInputFiles({name:"original-raster-history.bin",mimeType:"application/octet-stream",buffer:image});
    await page.waitForFunction(()=>document.getElementById("pc").textContent==="0x00000200");
    const result=await page.evaluate(async ys=>{
      const {emulator,advanceFrame}=await import("./app.js");
      for(let i=0;i<400 && emulator.read_long(0x0600D900)!==0x600D;i++)emulator.run_steps(500);
      if(emulator.read_long(0x0600D900)!==0x600D)throw new Error("Raster guest did not complete");
      advanceFrame();
      const frame=emulator.framebuffer(),canvas=document.getElementById("screen").getContext("2d");
      const pixels=ys.map(y=>Array.from(frame.slice((y*320+159)*4,(y*320+159)*4+4)));
      const presented=ys.map(y=>Array.from(canvas.getImageData(159,y,1,1).data));
      advanceFrame();
      return {pixels,presented,rows: [64,128,192].map(y => Array.from(frame.slice(y*320*4,(y+1)*320*4))),
        next:Array.from(canvas.getImageData(0,100,1,1).data),
        finalVram:emulator.read_long(0x25E60000)>>>16};
    },rasterYs);
    const expected=rasterYs.map(y=>y<=64?[255,0,0,255]:y<=128?(mutated?[255,0,0,255]:[0,255,0,255]):y<=192?[0,0,255,255]:[255,255,255,255]);
    assert.deepEqual(result.pixels,expected);assert.deepEqual(result.presented,expected);
    for (const [index,y] of [64,128,192].entries()) {
      const old = y===64 ? [255,0,0,255] : y===128 ? (mutated ? [255,0,0,255] : [0,255,0,255]) : [0,0,255,255];
      const row = result.rows[index];
      assert.deepEqual(row,Array.from({length:320},()=>old).flat());
    }
    assert.deepEqual(result.next,[31,0,0,255]);assert.equal(result.finalVram,0x8000);
  }
  // Original VDP1 polygons, rotated through A's Q9 sprite link. The guest
  // rewrites starts at line64 without reload, then enables X/Y reload on128/192.
  // Compare historical Wasm and production Canvas, then the next black field.
  for(const depth of [16,8]) {
    const image=Buffer.alloc(524288);
    for(const line of (await readFile(`tests/fixtures/sprite-rotation${depth}.hex`,"utf8")).split(/\r?\n/)) {
      if(!line||line.startsWith("#"))continue;
      const [at,...words]=line.split(/\s+/).map(w=>parseInt(w,16));
      words.forEach((w,i)=>image.writeUInt16BE(w,at+i*2));
    }
    const records=(await readFile(`tests/fixtures/sprite-rotation${depth}.tsv`,"utf8")).trim().split(/\r?\n/).map(w=>parseInt(w,16));
    for(const changed of [false,false,true]) {
      if(changed)image.writeUInt32BE(0,0x104);
      const result=await page.evaluate(async rom=>{
        const {emulator,advanceFrame}=await import("./app.js");
        emulator.reset_bios(Uint8Array.from(rom));
        for(let i=0;i<500 && emulator.read_long(0x0600DE18)!==0x600D;i++)emulator.run_steps(500);
        if(emulator.read_long(0x0600DE18)!==0x600D)throw new Error("VDP1 rotation guest did not complete");
        advanceFrame();
        const frame=emulator.framebuffer(),canvas=document.getElementById("screen").getContext("2d");
        const points=[0,63,64,127,128,191,192,223].flatMap(y=>[0,127,128,255,256,319].map(x=>[x,y]));
        const pixels=points.map(([x,y])=>Array.from(frame.slice((y*320+x)*4,(y*320+x)*4+4)));
        const presented=points.map(([x,y])=>Array.from(canvas.getImageData(x,y,1,1).data));
        const records=Array.from({length:7},(_,i)=>emulator.read_long(0x0600DE00+i*4));
        advanceFrame();
        return {records,pixels,presented,next:Array.from(canvas.getImageData(0,0,320,224).data)};
      },[...image]);
      const expected=[0,63,64,127,128,191,192,223].flatMap(y=>[0,127,128,255,256,319].map(x=>{
        if(changed)x=0;
        if(x>=256)return [0,0,0,255];
        if(y<128)return x<128?[0,255,0,255]:[255,255,255,255];
        if(y<192)return x<128?[255,0,0,255]:[0,0,255,255];
        return x<128?[0,0,255,255]:[0,0,0,255];
      }));
      assert.deepEqual(result.records,records);
      assert.deepEqual(result.pixels,expected);assert.deepEqual(result.presented,expected);
      assert(result.next.every((v,i)=>v===(i%4===3?255:0)));
    }
  }
  const interlaceImage=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/sprite-interlace.hex","utf8")).split(/\r?\n/)) {
    if(!line||line.startsWith("#"))continue;
    const [at,...words]=line.split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>interlaceImage.writeUInt16BE(w,at+i*2));
  }
  const interlaceRecords=(await readFile("tests/fixtures/sprite-interlace.tsv","utf8")).trim().split(/\r?\n/).map(w=>parseInt(w,16));
  for(const changed of [false,false,true]) {
    if(changed)interlaceImage.writeUInt32BE(0xF,0x100);
    const result=await page.evaluate(async rom=>{
      const {emulator,advanceFrame}=await import("./app.js");
      emulator.reset_bios(Uint8Array.from(rom));
      for(let i=0;i<1000 && emulator.read_long(0x0600DF34)!==0x600D;i++)emulator.run_steps(500);
      if(emulator.read_long(0x0600DF34)!==0x600D)throw new Error("VDP1 interlace guest did not complete");
      advanceFrame();
      const frame=emulator.framebuffer(),canvas=document.getElementById("screen").getContext("2d");
      const pixels=Array.from({length:10},(_,i)=>Array.from(frame.slice(((429+i)*320+10)*4,((429+i)*320+10)*4+4)));
      const presented=Array.from({length:10},(_,i)=>Array.from(canvas.getImageData(10,429+i,1,1).data));
      const records=Array.from({length:14},(_,i)=>emulator.read_long(0x0600DF00+i*4));
      const dimensions=[canvas.canvas.width,canvas.canvas.height];
      advanceFrame();
      return {records,pixels,presented,dimensions,next:Array.from(canvas.getImageData(10,430,1,8).data)};
    },[...interlaceImage]);
    const records=[...interlaceRecords];if(changed)records.fill(0x83E0,1,5);
    const expected=Array.from({length:10},(_,i)=>i===0||i===9?[0,0,0,255]:changed||i%2===0?[0,255,0,255]:[255,0,0,255]);
    assert.deepEqual(result.records,records);assert.deepEqual(result.dimensions,[320,448]);
    assert.deepEqual(result.pixels,expected);assert.deepEqual(result.presented,expected);
    assert.deepEqual(result.next,Array.from({length:8},()=>[0,255,0,255]).flat());
  }
  // Four normal tile backgrounds come exclusively from guest instruction/MMIO
  // writes. Mutate the guest palette literal and assert Wasm plus real Canvas.
  const tileRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/tiles.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => tileRom.writeUInt16BE(word, offset + i * 2));
  }
  for (const mutated of [false, true]) {
    const image = Buffer.from(tileRom);
    if (mutated) {
      const at = image.indexOf(Buffer.from([0, 0, 0, 31]));
      assert(at >= 0);
      image.writeUInt32BE(0x3FF, at);
    }
    await page.locator("#bios").setInputFiles({ name: "original-tiles.bin", mimeType: "application/octet-stream", buffer: image });
    await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
    await page.locator("#count").fill("3000");
    await page.locator("#run").click();
    assert.equal(await value("error"), "");
    const pixels = await page.evaluate(async () => {
      const { emulator, advanceFrame } = await import("./app.js");
      advanceFrame(); advanceFrame(); // Present through the same production path used by Play.
      const frame = emulator.framebuffer();
      const canvas = document.getElementById("screen").getContext("2d");
      return { done: emulator.read_long(0x06009000),
        wasm: [0,8,16,24,32,36,40].map((x) => Array.from(frame.slice(x*4,x*4+4))),
        canvas: [0,8,16,24,32,36,40].map((x) => Array.from(canvas.getImageData(x,0,1,1).data)) };
    });
    assert.equal(pixels.done, 0x600D);
    const expected = [mutated ? [255,255,0,255] : [255,0,0,255], [0,255,0,255],
      [0,0,255,255], [0,0,255,255], [255,255,255,255], [0,0,0,255], [0,0,0,255]];
    assert.deepEqual(pixels.wasm, expected);
    assert.deepEqual(pixels.canvas, expected);
  }
  // The same original guest's framebuffer writes drive sprite windows,
  // palette-MSB calculation and shadows through Wasm and production Canvas.
  const spriteEffectsRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sprite-effects.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>spriteEffectsRom.writeUInt16BE(w,at+i*2));
  }
  const spriteEffectPixels=async(rom)=>{
    await page.locator("#bios").setInputFiles({name:"original-sprite-effects.bin",mimeType:"application/octet-stream",buffer:rom});
    await page.waitForFunction(()=>document.getElementById("pc").textContent==="0x00000200");
    return page.evaluate(async()=>{
      const {emulator,advanceFrame}=await import("./app.js");
      emulator.run_steps(3000); advanceFrame(); advanceFrame();
      const wasm=Array.from(emulator.framebuffer().slice(0,32));
      const canvas=Array.from(document.getElementById("screen").getContext("2d").getImageData(0,0,8,1).data);
      return {done:emulator.read_long(0x0600B000),wasm,canvas};
    });
  };
  const spriteGolden=[255,0,0,255, 0,127,127,255, 0,0,127,255, 127,127,0,255,
    0,0,255,255, 255,0,0,255, 127,0,0,255, 0,0,255,255];
  for (let reset=0;reset<2;reset++) {
    const result=await spriteEffectPixels(spriteEffectsRom);
    assert.equal(result.done,0x600D);
    assert.deepEqual(result.wasm,spriteGolden);
    assert.deepEqual(result.canvas,spriteGolden);
  }
  const changeSpriteLiteral=(rom,old,value)=>{
    const instruction=Buffer.alloc(4); instruction.writeUInt32BE(old);
    const at=rom.indexOf(instruction);
    assert(at>=0 && rom.indexOf(instruction,at+1)===-1);
    rom.writeUInt32BE(value,at);
  };
  const noSpriteWindow=Buffer.from(spriteEffectsRom);
  changeSpriteLiteral(noSpriteWindow,0x3013,0x3003);
  const shadowGolden=[0,0,255,255, 0,127,127,255, 0,0,127,255, 0,63,63,255,
    0,0,255,255, 0,0,255,255, 0,0,127,255, 0,0,255,255];
  const shadows=await spriteEffectPixels(noSpriteWindow);
  assert.deepEqual(shadows.wasm,shadowGolden);
  assert.deepEqual(shadows.canvas,shadowGolden);
  const noSpriteCalculation=Buffer.from(spriteEffectsRom);
  changeSpriteLiteral(noSpriteCalculation,0x83E0,0x03E0);
  const unblended=await spriteEffectPixels(noSpriteCalculation);
  assert.deepEqual(unblended.wasm.slice(4,8),[0,255,0,255]);
  assert.deepEqual(unblended.wasm.slice(12,16),[0,255,0,255]);
  assert.deepEqual(unblended.canvas,unblended.wasm);
  // Original SH-2 instructions create advanced VDP2 tables. Compare actual
  // Wasm pixels with production Canvas presentation, then alter guest matrix data.
  const effectsRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/effects.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word,16));
    words.forEach((word,i) => effectsRom.writeUInt16BE(word,offset+i*2));
  }
  const effectsPoints = [[0,0],[1,0],[2,0],[3,0],[0,1],[1,1],[2,1],
    [8,0],[9,0],[8,1],[10,1],[13,1],[14,1],[0,8]];
  for (const mutate of [false,true]) {
    const image = Buffer.from(effectsRom);
    if (mutate) {
      const literal = Buffer.from([0,3,0,0]);
      const at = image.indexOf(literal);
      assert(at >= 0 && image.indexOf(literal,at+1) === -1);
      image.writeUInt32BE(2<<16,at);
    }
    await page.locator("#bios").setInputFiles({name:"original-effects.bin",mimeType:"application/octet-stream",buffer:image});
    await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
    const result = await page.evaluate(async (points) => {
      const { emulator, advanceFrame } = await import("./app.js");
      emulator.run_steps(10000);
      advanceFrame(); advanceFrame();
      const frame = emulator.framebuffer();
      const canvas = document.getElementById("screen").getContext("2d");
      return {done:emulator.read_long(0x0600A000),matrix:emulator.read_long(0x25E70048),
        wasm:points.map(([x,y]) => Array.from(frame.slice((y*320+x)*4,(y*320+x)*4+4))),
        canvas:points.map(([x,y]) => Array.from(canvas.getImageData(x,y,1,1).data))};
    },effectsPoints);
    const R=[255,0,0,255],G=[0,255,0,255],B=[0,0,255,255],W=[255,255,255,255],K=[0,0,0,255];
    const expected=mutate ? [B,G,W,R,W,B,R,R,G,K,G,R,K,K] : [W,B,W,R,R,W,R,R,G,K,G,R,K,K];
    assert.equal(result.done,0x600D);
    assert.equal(result.matrix,(mutate ? 2 : 3)<<16);
    assert.deepEqual(result.wasm,expected);
    assert.deepEqual(result.canvas,expected);
  }
  // Original SH-2 dual-rotation guest: RBG1 must read B0 characters/B1 names,
  // use parameter B and NBG0 windows/priority, and compose over RBG0.
  const rbg1Rom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/rbg1.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>rbg1Rom.writeUInt16BE(w,at+i*2));
  }
  for (const my of [15,7]) {
    const image=Buffer.from(rbg1Rom), literal=Buffer.from([0,15,0,0]);
    const at=image.indexOf(literal);
    assert(at>=0 && image.indexOf(literal,at+1)===-1);
    image.writeUInt32BE(my<<16,at);
    await page.locator("#bios").setInputFiles({name:"original-rbg1.bin",mimeType:"application/octet-stream",buffer:image});
    await page.waitForFunction(()=>document.getElementById("pc").textContent==="0x00000200");
    const result=await page.evaluate(async()=>{
      const {emulator,advanceFrame}=await import("./app.js");
      emulator.run_steps(10000); advanceFrame(); advanceFrame();
      const frame=emulator.framebuffer(),canvas=document.getElementById("screen").getContext("2d");
      const points=[[0,0],[8,0],[0,8],[8,8],[16,0]];
      return {done:emulator.read_long(0x0600A100),
        wasm:points.map(([x,y])=>Array.from(frame.slice((y*320+x)*4,(y*320+x)*4+4))),
        canvas:points.map(([x,y])=>Array.from(canvas.getImageData(x,y,1,1).data))};
    });
    const R=[255,0,0,255],G=[0,255,0,255],B=[0,0,255,255];
    const expected=my===15?[G,B,B,G,R]:[B,R,G,R,R];
    assert.equal(result.done,0x600D);
    assert.deepEqual(result.wasm,expected);
    assert.deepEqual(result.canvas,expected);
  }
  // Original SH-2 guest writes CRAM coefficients, line colors and all matrices.
  // Mutating only guest literals must change Wasm pixels and actual Canvas.
  const lineColorRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/line-color.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>lineColorRom.writeUInt16BE(w,at+i*2));
  }
  const M=[127,0,127,255],Y=[127,127,0,255],C=[0,127,127,255],K=[0,0,0,255];
  for (const [old,value,expected] of [
    [0,0,[M,Y,C,K,Y,C,M,K]],
    [0x02010000,0x02020000,[M,M,C,K,Y,[0,0,255,255],M,K]],
    [0x01010000,0x03010000,[Y,Y,C,K,[255,0,0,255],C,M,K]],
    [0x00000210,0x00000630,[[127,0,63,255],[63,127,0,255],[0,63,127,255],K,
      [127,63,0,255],[0,127,63,255],[63,0,127,255],K]],
  ]) {
    const image=Buffer.from(lineColorRom);
    if(old!==0) {
      const literal=Buffer.alloc(4); literal.writeUInt32BE(old);
      const at=image.indexOf(literal);
      assert(at>=0 && image.indexOf(literal,at+1)===-1);
      image.writeUInt32BE(value,at);
    }
    let previous;
    for(let repetition=0;repetition<2;repetition++) {
      await page.locator("#bios").setInputFiles({name:"original-line-color.bin",mimeType:"application/octet-stream",buffer:image});
      await page.waitForFunction(()=>document.getElementById("pc").textContent==="0x00000200");
      const result=await page.evaluate(async()=>{
        const {emulator,advanceFrame}=await import("./app.js");
        emulator.run_steps(10000); advanceFrame(); advanceFrame();
        const frame=emulator.framebuffer(),canvas=document.getElementById("screen").getContext("2d");
        const points=Array.from({length:8},(_,i)=>[i%4,Math.floor(i/4)]);
        return {done:emulator.read_long(0x0600A200),
          wasm:points.map(([x,y])=>Array.from(frame.slice((y*320+x)*4,(y*320+x)*4+4))),
          canvas:points.map(([x,y])=>Array.from(canvas.getImageData(x,y,1,1).data))};
      });
      assert.equal(result.done,0x600D);
      assert.deepEqual(result.wasm,expected);
      assert.deepEqual(result.canvas,expected);
      if(previous) assert.deepEqual(result,previous);
      previous=result;
    }
  }
  // Original SH-2 gradation guest: actual MMIO, filtered Wasm and Canvas,
  // reset determinism and changed guest color controls; no host pixel writes.
  const gradationRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/gradation.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>gradationRom.writeUInt16BE(w,at+i*2));
  }
  const gradationPixels=[[31,31,191,255],[191,223,223,255],[223,159,191,255],
    [191,223,159,255],[159,159,191,255],[191,223,223,255]];
  for (const [control,expected] of [
    [0xA003,gradationPixels],[0xA403,gradationPixels],
    [3,[[0,127,127,255],[127,255,127,255],[127,255,127,255],
      [127,255,127,255],[127,255,127,255],[127,255,127,255]]],
  ]) {
    const image=Buffer.from(gradationRom),literal=Buffer.from([0,0,0xA0,3]);
    const at=image.indexOf(literal);
    assert(at>=0 && image.indexOf(literal,at+1)===-1);
    image.writeUInt32BE(control,at);
    let previous;
    for(let repetition=0;repetition<2;repetition++) {
      await page.locator("#bios").setInputFiles({name:"original-gradation.bin",mimeType:"application/octet-stream",buffer:image});
      await page.waitForFunction(()=>document.getElementById("pc").textContent==="0x00000200");
      const result=await page.evaluate(async()=>{
        const {emulator,advanceFrame}=await import("./app.js");
        emulator.run_steps(10000); advanceFrame(); advanceFrame();
        const frame=emulator.framebuffer(),canvas=document.getElementById("screen").getContext("2d");
        return {done:emulator.read_long(0x0600A400),
          wasm:[0,1].map(y=>Array.from(frame.slice((y*320+2)*4,(y*320+8)*4))),
          canvas:[0,1].map(y=>Array.from(canvas.getImageData(2,y,6,1).data))};
      });
      assert.equal(result.done,0x600D);
      assert.deepEqual(result.wasm,[expected.flat(),expected.flat()]);
      assert.deepEqual(result.canvas,result.wasm);
      if(previous) assert.deepEqual(result,previous);
      previous=result;
    }
  }
  const specialVideoRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/special-video.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>specialVideoRom.writeUInt16BE(w,at+i*2));
  }
  const specialBlue=[0,0,255,255],specialRed=[255,0,0,255];
  for(const [address,old,value,expected] of [
    [0,0,0,[specialBlue,M,M,specialBlue]],
    [0x25F80024,0,1,[M,specialBlue,specialBlue,specialBlue]],
    [0x25F8002C,0x30,0x20,[specialBlue,specialRed,specialRed,specialBlue]],
    [0x25F8002C,0x30,0x10,[specialBlue,specialBlue,specialBlue,specialBlue]],
  ]) {
    const image=Buffer.from(specialVideoRom);
    if(address!==0) {
      const hits=[];
      for(let at=0x200;at<0x10000;at+=2) {
        const op=image.readUInt16BE(at+4);
        if(image.readUInt32BE(at)!==address || (op&0xFF00)!==0xD000) continue;
        const literal=((at+4)&~3)+4+(op&255)*4;
        if(image.readUInt32BE(literal)===old && image.readUInt16BE(literal+4)===0x2101) hits.push(literal);
      }
      assert.equal(hits.length,1); image.writeUInt32BE(value,hits[0]);
    }
    let previous;
    for(let repetition=0;repetition<2;repetition++) {
      await page.locator("#bios").setInputFiles({name:"original-special-video.bin",mimeType:"application/octet-stream",buffer:image});
      await page.waitForFunction(()=>document.getElementById("pc").textContent==="0x00000200");
      const result=await page.evaluate(async()=>{
        const {emulator,advanceFrame}=await import("./app.js");
        emulator.run_steps(10000); advanceFrame(); advanceFrame();
        const frame=emulator.framebuffer(),canvas=document.getElementById("screen").getContext("2d");
        return {done:emulator.read_long(0x0600A300),wasm:Array.from(frame.slice(0,32)),
          canvas:Array.from(canvas.getImageData(0,0,8,1).data)};
      });
      assert.equal(result.done,0x600D);
      assert.deepEqual(result.wasm,[...expected,...expected].flat());
      assert.deepEqual(result.canvas,result.wasm);
      if(previous) assert.deepEqual(result,previous);
      previous=result;
    }
  }
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
      dropped: emulator.audio_dropped(), count: emulator.sound_instruction_count(), irqs:emulator.read_long(0x05A00804) };
  });
  const soundFinished = await soundResult();
  assert.deepEqual(soundFinished.mailbox, [1, 0x600D]);
  assert.deepEqual(soundFinished.waveform, [0x1000F000, 0xE0002000]);
  assert.deepEqual(soundFinished.samples, [256,4096,-256,-4096,-512,-8192,512,8192,256,4096,-256,-4096,-512,-8192,512,8192]);
  assert.equal(soundFinished.irqs,2);
  assert.equal(BigInt(soundFinished.count),30n); // setup23 + two IRQ3 handlers + STOP1; IRQ2 precedes STOP
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
  const fractionalSoundRom = Buffer.from(soundRom);
  const pitchInstruction = Buffer.from([0x33,0xFC,0,0,0,0x10,0,0x10]);
  const pitchAt = fractionalSoundRom.indexOf(pitchInstruction);
  assert(pitchAt >= 0 && fractionalSoundRom.indexOf(pitchInstruction,pitchAt+1) === -1);
  fractionalSoundRom.writeUInt16BE(0x7800,pitchAt+2);
  await page.locator("#bios").setInputFiles({ name:"original-fractional-sound.bin",mimeType:"application/octet-stream",buffer:fractionalSoundRom });
  await page.waitForFunction(() => document.getElementById("pc").textContent === "0x00000200");
  const fractionalSound = await soundResult();
  assert.deepEqual(fractionalSound.waveform, [0x1000F000,0xE0002000]);
  assert.deepEqual(fractionalSound.samples,[256,4096,0,0,-256,-4096,-384,-6144,-512,-8192,0,0,512,8192,384,6144]);
  const reverseSoundRom=Buffer.from(fractionalSoundRom);
  const keyInstruction=Buffer.from([0x33,0xFC,0x18,0x20,0,0x10,0,0]);
  const keyAt=reverseSoundRom.indexOf(keyInstruction);
  assert(keyAt>=0 && reverseSoundRom.indexOf(keyInstruction,keyAt+1)===-1);
  reverseSoundRom.writeUInt16BE(0x1840,keyAt+2);
  await page.locator("#bios").setInputFiles({name:"original-reverse-fractional-sound.bin",mimeType:"application/octet-stream",buffer:reverseSoundRom});
  await page.waitForFunction(()=>document.getElementById("pc").textContent==="0x00000200");
  assert.deepEqual((await soundResult()).samples,[512,8192,0,0,-512,-8192,-384,-6144,-256,-4096,0,0,256,4096,384,6144]);
  // Original CPU-board aliases and SCSP DRAM-geometry guest. The source ROM
  // drives all writes, including DMA and effects-DSP read instructions.
  const memoryRom=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/memory-contracts.hex","utf8")).split(/\r?\n/)) {
    if(!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>memoryRom.writeUInt16BE(w,at+i*2));
  }
  for(const mutated of [false,true]) {
    const image=Buffer.from(memoryRom);
    if(mutated) {
      let count=0;
      for(let at=0;at<image.length-4;at+=2) if(image.readUInt32BE(at)===0x2345DCBB) {
        image.writeUInt32BE(0x1234EDCC,at);count++;
      }
      assert.equal(count,2);
    }
    const result=await page.evaluate(async image=>{
      const {Emulator}=await import("./pkg/saturn_rust.js");
      const {AudioOutput}=await import("./audio.js");
      const guest=new Emulator(),output=new AudioOutput();
      try {
        const traces=[];
        for(let boot=0;boot<2;boot++) {
          guest.reset_bios(Uint8Array.from(image));
          for(let i=0;i<10;i++)guest.run_steps(10000);
          traces.push(Array.from({length:16},(_,i)=>guest.read_long(0x0600F000+i*4)));
        }
        const pcm=guest.audio_samples(8192);
        const samples=Array.from(pcm);
        await output.enable();output.setPaused(false);output.deliver(pcm);
        for(let i=0;i<200;i++) {
          if(output.stats.rendered>128 && output.stats.peak>0 && output.stats.state==="underflow")break;
          await new Promise(resolve=>setTimeout(resolve,25));
        }
        return {traces,positive:Math.max(...samples),negative:Math.min(...samples),
          dsp:guest.read_long(0x05B00E00)&0xFFFF,stats:{...output.stats}};
      } finally {guest.free();await output.context?.close();}
    },[...image]);
    const word=mutated?0x1234:0x2345;
    result.traces.forEach((trace,i)=>assert.deepEqual(trace,
      [...Array(8).fill(0xFF01+i),word,0xFFFF,word,word&0xFFF8,0x600D,0xFFFFFFFF,0x200,0x600D]));
    assert.equal(result.dsp,word);
    assert.equal(result.positive,word);assert.equal(result.negative,-word);
    assert.equal(result.stats.state,"underflow");assert(result.stats.rendered>128);
    assert(Math.abs(result.stats.peak-word/32768)<0.00001,JSON.stringify(result.stats));
  }
  // Independent 68k family/fault vectors plus SCSP DMA and timed MIDI.
  const intcRom=Buffer.alloc(524288);
  for(const line of (await readFile("tests/fixtures/intc.hex","utf8")).split(/\r?\n/)) {
    if(!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>intcRom.writeUInt16BE(w,at+i*2));
  }
  const divisions=(await readFile("tests/fixtures/signed-divisions.tsv","utf8")).trim().split(/\r?\n/)
    .flatMap(line=>line.split("\t").slice(1).map(w=>parseInt(w,16)));
  const intcResult=async rom=>page.evaluate(async ({rom,divisionWords})=>{
    const {Emulator}=await import("./pkg/saturn_rust.js");
    const guest=new Emulator();
    try {
      guest.reset_bios(Uint8Array.from(rom));
      guest.run_steps(10000);
      const start=Array.from({length:4},(_,i)=>guest.read_long(0x06009000+i*4));
      const quotients=Array.from({length:divisionWords},(_,i)=>guest.read_long(0x06009100+i*4));
      const traces=[];
      if(start[3]===0x600D) for(const port of [0,1,0]) {
        guest.set_peripheral_th(port,true);
        const time=Number(guest.peripheral_clocks());
        const expected=((Math.floor((time%1708)/4)*2)<<16)|(Math.floor(time/1708)%263)*2;
        guest.set_peripheral_th(port,false);
        guest.run_steps(500);
        const words=Array.from({length:4},(_,i)=>guest.read_long(0x06009010+i*4));
        guest.set_peripheral_th(port,false);
        guest.run_steps(500);
        traces.push({words,expected,count:guest.read_long(0x06009010)});
      }
      return {start,quotients,traces,exceptions:guest.exception_count()};
    } finally {guest.free();}
  },{rom:[...rom],divisionWords:divisions.length});
  const intc=await intcResult(intcRom);
  assert.deepEqual(intc.start,[0xFFFF8000,0x42,0x4B,0x600D]);
  assert.deepEqual(intc.quotients,divisions);
  intc.traces.forEach(({words,expected,count},i)=>{
    assert.equal(words[0],i+1);
    assert.equal(words[1],expected);
    assert.equal(words[2]&0x202,0x202);
    assert.equal(words[3]&0x202,2);
    assert.equal(count,i+1);
  });
  assert.equal(intc.exceptions,"5");
  assert.deepEqual(await intcResult(intcRom),intc);
  const changedIntc=Buffer.from(intcRom);
  const vectorMode=Buffer.from([0xE0,1,0x21,1]);
  const vectorModeAt=changedIntc.indexOf(vectorMode);
  assert(vectorModeAt>=0 && changedIntc.indexOf(vectorMode,vectorModeAt+1)===-1);
  changedIntc[vectorModeAt+1]=0;
  assert.deepEqual((await intcResult(changedIntc)).start,[0xFFFF8000,0x42,0,0]);
  const soundIoRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/sound-io.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>soundIoRom.writeUInt16BE(w,at+i*2));
  }
  const soundIoResults=async(rom)=>{
    await page.locator("#bios").setInputFiles({name:"original-sound-io.bin",mimeType:"application/octet-stream",buffer:rom});
    await page.waitForFunction(()=>document.getElementById("pc").textContent==="0x00000200");
    return page.evaluate(async()=>{
      const {emulator}=await import("./app.js");
      for(let i=0;i<10;i++) emulator.run_steps(10000);
      const samples=Array.from(emulator.audio_samples(8192));
      return {done:emulator.read_long(0x06007004),
        words:Array.from({length:41},(_,i)=>emulator.read_long(0x05A00800+i*4)),
        halted:emulator.sound_halted(),count:emulator.sound_instruction_count(),
        peak:Math.max(...samples.map(Math.abs))};
    });
  };
  let previousSoundIo;
  for(let reset=0;reset<2;reset++) {
    const result=await soundIoResults(soundIoRom);
    assert.equal(result.done,0x600D);
    for(const [offset,value] of [[0x804,1],[0x810,0xA55A1234],[0x818,1],[0x81C,1],
      [0x830,0x99],[0x834,1],[0x838,0x99],[0x83C,0x12345678],[0x840,0x11223344],
      [0x844,0x55667788],[0x848,0x4500],[0x850,0xCAFEBABE],[0x854,0xDEADBEEF],
      [0x858,0x80000],[0x860,0x00150000],[0x864,0x80000],[0x868,0x4CD80000],
      [0x884,1],[0x88C,0xFFFFFFFE],[0x890,0xFFFFFFFD],[0x894,0],[0x898,0xFF],
      [0x89C,0x9D0],[0x8A0,0x27180000]]) {
      assert.equal(result.words[(offset-0x800)/4],value,`68k guest ${offset.toString(16)}`);
    }
    assert.equal((result.words[5]>>>16)&0x210,0x210);
    assert.equal(result.halted,false);
    assert(BigInt(result.count)>100n);
    assert(result.peak>0);
    if(previousSoundIo) assert.deepEqual(result,previousSoundIo);
    previousSoundIo=result;
  }
  const changedSoundIo=Buffer.from(soundIoRom);
  const dmaOperand=Buffer.from([0x33,0xFC,0xA5,0x5A,0,0,0x43,0]);
  const dmaOperandAt=changedSoundIo.indexOf(dmaOperand);
  assert(dmaOperandAt>=0 && changedSoundIo.indexOf(dmaOperand,dmaOperandAt+1)===-1);
  changedSoundIo.writeUInt16BE(0x5AA5,dmaOperandAt+2);
  assert.equal((await soundIoResults(changedSoundIo)).words[4],0x5AA51234);
  await scspMonitor(page);
  await m68kReset(page);
  // Source-authored MC68000 family/EA/exception vectors, executed from sound
  // RAM after SH-2 upload. The host reads results; it never writes CPU state.
  const familyRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/m68k-families.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>familyRom.writeUInt16BE(w,at+i*2));
  }
  const familyVectors=(await readFile("tests/fixtures/m68k-families.tsv","utf8")).trim().split(/\r?\n/).map(line=>{
    const [name,value,flags]=line.split("\t");
    return {name,value:parseInt(value,16),flags:flags==="-"?null:parseInt(flags,16)};
  });
  const faultVectors=(await readFile("tests/fixtures/m68k-faults.tsv","utf8")).trim().split(/\r?\n/).map(line=>{
    const [name,...values]=line.split("\t");
    return {name,words:values.map(v=>parseInt(v,16))};
  });
  const familyResult=async rom=>page.evaluate(async ({rom,count,faultCount})=>{
    const {Emulator}=await import("./pkg/saturn_rust.js");
    const guest=new Emulator();
    try {
      guest.reset_bios(Uint8Array.from(rom));
      for(let i=0;i<10;i++) guest.run_steps(10000);
      return {done:guest.read_long(0x06007004),faults:[guest.read_long(0x05A05800),guest.read_long(0x05A05804)],
        words:Array.from({length:count*2},(_,i)=>guest.read_long(0x05A05000+i*4)),
        accessFrames:Array.from({length:faultCount},(_,i)=>Array.from({length:8},(_,j)=>guest.read_long(0x05A05900+i*32+j*4))),
        audible:Array.from(guest.audio_samples(8192)).some(s=>s!==0)};
    } finally {guest.free();}
  },{rom:[...rom],count:familyVectors.length,faultCount:faultVectors.length});
  const family=await familyResult(familyRom);
  assert.equal(family.done,0x600D);
  assert.deepEqual(family.faults,[3,2]);
  assert(family.audible);
  faultVectors.forEach(({name,words},i)=>assert.deepEqual(family.accessFrames[i],words,`68k fault frame ${name}`));
  familyVectors.forEach(({name,value,flags},i)=>{
    assert.equal(family.words[i*2],value,`${name} result`);
    if(flags!==null) assert.equal(family.words[i*2+1],flags,`${name} CCR`);
  });
  assert.deepEqual(await familyResult(familyRom),family);
  const changedFamily=Buffer.from(familyRom);
  const addVector=Buffer.from([0x70,0x7F,0x72,1,0xD0,1]);
  const addAt=changedFamily.indexOf(addVector);
  assert(addAt>=0 && changedFamily.indexOf(addVector,addAt+1)===-1);
  changedFamily.writeUInt16BE(0x9001,addAt+4); // actual SUB.B replaces ADD.B
  const changedFamilyResult=await familyResult(changedFamily);
  assert.deepEqual(changedFamilyResult.words.slice(0,2),[0x7E,0]);
  assert.deepEqual(changedFamilyResult.words.slice(2),family.words.slice(2));
  assert.deepEqual(changedFamilyResult.accessFrames,family.accessFrames);
  // Plain writes omit the specified destination read: the guest must now
  // stack a write error and the changed IR, with every other record retained.
  for(const [opcode,replacement,index] of [[0x50D0,0x1080,10],[0x40D0,0x3080,11]]) {
    const image=Buffer.from(familyRom),needle=Buffer.alloc(6);
    needle.writeUInt16BE(opcode);needle.writeUInt32BE(0x33FC1234,2);
    const at=image.indexOf(needle);
    assert(at>=0&&image.indexOf(needle,at+1)<0);
    image.writeUInt16BE(replacement,at);
    const result=await familyResult(image),expected=family.accessFrames.map(words=>[...words]);
    expected[index][0]=0x00050008;expected[index][1]=replacement;
    assert.deepEqual(result.accessFrames,expected);
    assert.equal(result.done,0x600D);
  }
  // Changing the actual MOVEM width moves the final read from disconnected
  // 80000 to real RAM at 7FFFE. Only this exception record must disappear.
  const changedMovem=Buffer.from(familyRom);
  const movemNeedle=Buffer.from([0x4C,0xD8,0,1,0x33,0xFC,0x12,0x34]);
  const movemAt=changedMovem.indexOf(movemNeedle);
  assert(movemAt>=0 && changedMovem.indexOf(movemNeedle,movemAt+1)<0);
  changedMovem[movemAt+1]=0x98;
  const changedMovemResult=await familyResult(changedMovem);
  const changedMovemFrames=faultVectors.filter(v=>v.name!=="movem long terminal").map(v=>v.words);
  changedMovemFrames.push(Array(8).fill(0));
  assert.equal(changedMovemResult.done,0x600D);
  assert.deepEqual(changedMovemResult.accessFrames,changedMovemFrames);
  assert.deepEqual(await familyResult(changedMovem),changedMovemResult);
  // Original slot-synthesis ROM: real SH-2 uploads and starts the 68k, which
  // writes guard waveforms and configures a silent modulator, FM carrier and ALFO.
  const synthesisRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/synthesis.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>synthesisRom.writeUInt16BE(w,at+i*2));
  }
  const patchSynthesis=(rom,address,old,value)=>{
    const instruction=Buffer.alloc(8);
    instruction.writeUInt16BE(0x33FC,0); instruction.writeUInt16BE(old,2); instruction.writeUInt32BE(address,4);
    const at=rom.indexOf(instruction);
    assert(at>=0 && rom.indexOf(instruction,at+1)===-1);
    rom.writeUInt16BE(value,at+2);
  };
  const synthesisResult=async(rom)=>{
    await page.locator("#bios").setInputFiles({name:"original-slot-synthesis.bin",mimeType:"application/octet-stream",buffer:rom});
    await page.waitForFunction(()=>document.getElementById("pc").textContent==="0x00000200");
    return page.evaluate(async()=>{
      const {emulator}=await import("./app.js");
      for(let i=0;i<20;i++) emulator.run_steps(10000);
      const audio=Array.from(emulator.audio_samples(8192));
      const frames=Array.from({length:audio.length/2},(_,i)=>audio.slice(i*2,i*2+2));
      const first=frames.findIndex(s=>s[1]!==0);
      return {mailbox:emulator.read_long(0x06007004),waveform:emulator.read_long(0x05A04300),
        first:frames.slice(first,first+16),right:[...new Set(frames.slice(first).map(s=>s[1]))].sort((a,b)=>a-b),
        tail:frames.slice(-100),count:frames.length-first,dropped:emulator.audio_dropped()};
    });
  };
  const synthesis=await synthesisResult(synthesisRom);
  assert.equal(synthesis.mailbox,0x600D);
  assert.equal(synthesis.waveform,0x20002000);
  assert.equal(synthesis.dropped,"0");
  assert(synthesis.count>1000);
  assert.deepEqual(synthesis.first.slice(0,8),[[1024,1024],[1152,1024],[1280,1024],[1408,1024],
    [1536,1024],[1664,1024],[1792,1024],[1920,1024]]);
  assert.deepEqual(synthesis.right,[64,1024]);
  assert.deepEqual(await synthesisResult(synthesisRom),synthesis);
  const quietSynthesis=Buffer.from(synthesisRom);
  patchSynthesis(quietSynthesis,0x1000AC,16,32);
  assert.deepEqual((await synthesisResult(quietSynthesis)).first.slice(0,4),[[512,1024],[576,1024],[640,1024],[704,1024]]);
  const drySynthesis=Buffer.from(synthesisRom);
  patchSynthesis(drySynthesis,0x1000AE,0x5EFB,0);
  assert.deepEqual((await synthesisResult(drySynthesis)).first.slice(0,4),[[0,1024],[128,1024],[256,1024],[384,1024]]);
  const pitchedSynthesis=Buffer.from(synthesisRom);
  patchSynthesis(pitchedSynthesis,0x1000B2,0x8000,0x81E0);
  const pitched=(await synthesisResult(pitchedSynthesis)).first;
  pitched.forEach((s,i)=>{
    const expected=Math.floor((8+i*2**(494*127/128/1200))*128);
    assert(Math.abs(s[0]-expected)<=1,`PLFO frame${i}: ${s[0]} vs ${expected}`);
    assert.equal(s[1],1024);
  });
  const heldSynthesis=Buffer.from(synthesisRom);
  patchSynthesis(heldSynthesis,0x1000D2,0x7C0F,0x800F);
  assert.deepEqual((await synthesisResult(heldSynthesis)).right,[1024]);
  const noiseSynthesis=Buffer.from(synthesisRom);
  patchSynthesis(noiseSynthesis,0x1000C0,0x0820,0x08A0);
  patchSynthesis(noiseSynthesis,0x1000CC,32,0x100);
  const noise=await synthesisResult(noiseSynthesis);
  assert(noise.right.some(v=>v>16000) && noise.right.some(v=>v< -16000));
  assert.deepEqual(await synthesisResult(noiseSynthesis),noise);
  patchSynthesis(noiseSynthesis,0x1000D2,0x7C0F,0xFC0F);
  assert.deepEqual(await synthesisResult(noiseSynthesis),noise); // LFORE cannot reset noise
  // Original timed-envelope guest: actual 68k timer IRQ keys off slot6. No
  // host sound-register writes; mutated guest operands must change the signal.
  const envelopeRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/envelope.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at,...words]=line.trim().split(/\s+/).map(w=>parseInt(w,16));
    words.forEach((w,i)=>envelopeRom.writeUInt16BE(w,at+i*2));
  }
  const envelope=await synthesisResult(envelopeRom);
  assert.equal(envelope.mailbox,0x600D);
  envelope.first.forEach((s,i)=>{
    const expected=Math.floor(1024*2**(-i*0.75/64));
    assert(Math.abs(s[1]-expected)<=1,`EG decay ${i}: ${s[1]} vs ${expected}`);
  });
  assert(envelope.right.includes(255));
  assert(envelope.tail.every(s=>s[1]===0));
  assert.deepEqual(await synthesisResult(envelopeRom),envelope);
  const heldEnvelope=Buffer.from(envelopeRom);
  patchSynthesis(heldEnvelope,0x1000CA,0x3C9F,0x3C80);
  assert((await synthesisResult(heldEnvelope)).tail.every(s=>s[1]===255));
  const zeroAttack=Buffer.from(envelopeRom);
  patchSynthesis(zeroAttack,0x1000C8,0x06DF,0x06C0);
  assert.deepEqual((await synthesisResult(zeroAttack)).right,[0]);
  const linkedEnvelope=Buffer.from(envelopeRom);
  patchSynthesis(linkedEnvelope,0x1000CA,0x3C9F,0x7C9F);
  assert.deepEqual((await synthesisResult(linkedEnvelope)).right,[0]);
  // Reload held PCM so the production worklet check below validates FM/ALFO.
  await synthesisResult(heldEnvelope);
  // Exercise this synthesized output through the production AudioWorklet too.
  await page.evaluate(async()=>{
    const {emulator,audio,pumpAudio}=await import("./app.js");
    audio.setMuted(false); audio.setPaused(false);
    for(let i=0;i<4;i++) emulator.run_steps(10000);
    const before=audio.stats.received;
    pumpAudio();
    for(let i=0;i<100;i++) {
      if(audio.stats.received>before && audio.stats.peak>0.1) {audio.setPaused(true); return;}
      await new Promise(resolve=>setTimeout(resolve,25));
    }
    throw new Error(`Slot synthesis did not reach AudioWorklet: ${JSON.stringify(audio.stats)}`);
  });
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
  // Standard Gamepads map to the four-channel ID16 3D pad, with independent
  // analog R/L values; retain the original ID15 adapter for other mappings.
  await page.locator("#reset").click();
  await page.locator("#screen").focus();
  await page.evaluate(async () => {
    navigator.getGamepads()[1].mapping="standard";
    (await import("./app.js")).pollGamepads();
  });
  const pad3dSmpc=await smpcResult();
  assert.deepEqual(pad3dSmpc.packet,[0xF1,2,0xEE,0xF7,0xF1,0x16,0x77,0x7F,0,128,255,0,255,255,255,255]);
  await page.locator("#reset").click();
  await page.locator("#screen").focus();
  await page.evaluate(async () => (await import("./app.js")).pollGamepads());
  await page.locator("#count").focus();
  assert.deepEqual((await smpcResult()).packet,[0xF1,2,255,255,0xF1,0x16,255,255,128,128,0,0,255,255,255,255]);
  await page.evaluate(() => { delete navigator.getGamepads; });
  const controllerRom=Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/controller.hex","utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [offset,...words]=line.trim().split(/\s+/).map((word)=>parseInt(word,16));
    words.forEach((word,i)=>controllerRom.writeUInt16BE(word,offset+i*2));
  }
  await page.locator("#bios").setInputFiles({name:"original-controller.bin",mimeType:"application/octet-stream",buffer:controllerRom});
  await page.waitForFunction(()=>document.getElementById("pc").textContent==="0x00000200");
  for (const x of [0x12,0x56]) {
    if (x===0x56) await page.locator("#reset").click();
    const result=await page.evaluate(async (x)=>{
      const {emulator}=await import("./app.js");
      emulator.set_buttons(1|16|4096);emulator.set_port2_buttons(8|128|2048);
      emulator.set_3d_pad(0,true,x,0x34,0xAB,0xCD);emulator.set_analog(1,true,0xFE,0xDC,0xBA);
      emulator.run_steps(10000);
      const bytes=(address,count)=>Array.from({length:Math.ceil(count/4)},(_,i)=>emulator.read_long(address+i*4))
        .flatMap((n)=>[n>>>24,(n>>>16)&255,(n>>>8)&255,n&255]).slice(0,count);
      return {done:emulator.read_long(0x0600B040),first:bytes(0x0600B000,16),second:bytes(0x0600B020,14)};
    },x);
    assert.equal(result.done,0x600D);
    assert.deepEqual(result.first,[1,0x36,14,0x3E,7,0x37,x>>4,0x30|(x&15),3,0x34,10,0x3B,12,0x3D,0,0x31]);
    assert.deepEqual(result.second,[1,0x35,7,0x37,7,0x3F,15,0x3E,13,0x3C,11,0x3A,0,0x31]);
  }
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
  badSlaveRom.writeUInt32BE(0x08000010, badStack);
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
  const readyCollector = async () => {
    for (let i = 0; i < 80 && (await gameState()).state[4] < 2; i++) await stepFrames(1);
    assert((await gameState()).state[4] >= 2, "Guest must finish its own bitmap initialization");
    await stepFrames(2); // complete scanout after guest VRAM initialization
  };
  await readyCollector();
  const initial = await gameState();
  assert.deepEqual(initial.state.slice(0,4), [32,112,0,0]);
  assert(initial.instructions > 100000);
  await hold("ArrowRight", 8);
  await stepFrames(2); // the beam must reach the guest-updated scoreboard
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
  await stepFrames(2); // display the completed win banner field
  const won = await gameState();
  assert.deepEqual(won.state.slice(0,4), [256,112,5,1]);
  assert.deepEqual(won.banner, [0,255,0,255]);
  assert.notEqual(won.hash, collected.hash);
  await hold("Enter", 40); // Guest VRAM clearing includes ordinary bus waits.
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
  // ST-040 §4.3 permits no first-read file. Exercise the actual picker with
  // compact original images: IP, PVD, terminator and a self/parent-only root.
  // The guest AIP computes R0=37 and loops; the host supplies no result state.
  for (const sectors of [19, 21]) {
    const compact = Buffer.alloc(sectors * 2048);
    disc.copy(compact, 0, 0, 18 * 2048);
    const dual32 = (at, value) => { compact.writeUInt32LE(value, at); compact.writeUInt32BE(value, at + 4); };
    compact.writeUInt32BE(0, 0xF0);
    compact.writeUInt16BE(0xE025, 0xE20); // MOV #37,R0
    compact.writeUInt16BE(0xAFFE, 0xE22); // BRA to itself
    compact.writeUInt16BE(0x0009, 0xE24);
    dual32(16 * 2048 + 80, sectors);
    dual32(16 * 2048 + 156 + 2, sectors - 1);
    const root = compact.subarray(16 * 2048 + 156, 16 * 2048 + 190);
    root.copy(compact, (sectors - 1) * 2048);
    root.copy(compact, (sectors - 1) * 2048 + 34);
    compact[(sectors - 1) * 2048 + 34 + 33] = 1;
    await page.locator("#disc").setInputFiles({ name:`compact-${sectors}.iso`, mimeType:"application/octet-stream", buffer:compact });
    await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
    assert.equal(await value("error"), "");
    await page.locator("#count").fill("100");
    await page.locator("#run").click();
    const compactState = await machine();
    assert.equal(compactState.registers[0], 37);
    assert.equal(compactState.retired, "100");
    assert.equal(compactState.exceptions, "0");
  }
  // Size-valid malformed media reaches the core parser; incomplete sectors
  // and oversized images remain picker errors and cannot keep an old CPU alive.
  for (const size of [17 * 2048, 18 * 2048 + 1, 16777216 + 2048]) {
    await page.locator("#disc").setInputFiles({ name:"invalid-size.iso", mimeType:"application/octet-stream", buffer:Buffer.alloc(size) });
    assert.match(await value("error"), /18 sectors to 16 MiB/);
    assert.equal(await page.locator("#run").isDisabled(), true);
  }
  await page.locator("#disc").setInputFiles({ name:"invalid-structure.iso", mimeType:"application/octet-stream", buffer:Buffer.alloc(18 * 2048) });
  await page.waitForFunction(() => /missing Saturn hardware identifier/.test(document.getElementById("error").textContent));
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
  await readyCollector();
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
    emulator.run_steps(123); // Include the original 23-instruction HLE reset.
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
  let mutatedCourier;
  for (let i = 0; i < 100; i++) {
    mutatedCourier = await courierFrames(1);
    if (mutatedCourier.state[4] > 2) break;
  }
  assert(mutatedCourier.state[4] > 2, "Changed guest must finish initialization");
  mutatedCourier = await courierFrames(2); // publish the initialized texture's field
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
  // Mixed-track diagnostic boots through the real local-file UI and Wasm. All
  // result writes are guest instructions. Offline CUE references resolve only to
  // the selected bytes, and every CD sample travels through the SCSP route.
  const lab = await context.newPage();
  await lab.goto(address + prefix);
  await lab.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  await lab.locator("#view-debug").click();
  await lab.locator("#settings > summary").click();
  await lab.selectOption("#mode", "disc");
  const labIso = await readFile("web/hardware-lab.iso");
  const labAudio = await readFile("web/hardware-lab-audio.bin");
  const labCue = await readFile("web/hardware-lab.cue");
  const labFiles = (iso = labIso, audio = labAudio) => [
    {name:"hardware-lab.cue",mimeType:"text/plain",buffer:labCue},
    {name:"hardware-lab.iso",mimeType:"application/octet-stream",buffer:iso},
    {name:"hardware-lab-audio.bin",mimeType:"application/octet-stream",buffer:audio},
  ];
  const labRequests = [];
  lab.on("request", request => labRequests.push({url:request.url(),method:request.method(),body:request.postData()}));
  // ST-058-R2 §§6.1/6.4/8.1, independently authored rotation/window pixels.
  const labPoints = [[0,0],[1,0],[2,0],[3,0],[0,1],[1,1],[2,1],
    [8,0],[9,0],[8,1],[10,1],[13,1],[14,1],[0,8]];
  const labPixels = [
    [255,255,255,255],[0,0,255,255],[255,255,255,255],[255,0,0,255],
    [255,0,0,255],[255,255,255,255],[255,0,0,255],[255,0,0,255],
    [0,255,0,255],[0,0,0,255],[0,255,0,255],[255,0,0,255],
    [0,0,0,255],[0,0,0,255],
  ];
  const labTrace = (points = labPoints) => lab.evaluate(async points => {
    const {emulator,advanceFrame} = await import("./app.js");
    for(let i=0;i<40 && emulator.read_long(0x06010020)!==0x600D;i++) emulator.run_steps(10000);
    emulator.advance_frame();
    const samples=Array.from(emulator.audio_samples(8192));
    advanceFrame();
    const frame=emulator.framebuffer();
    const canvas=document.getElementById("screen").getContext("2d");
    return {results:Array.from({length:11},(_,i)=>emulator.read_long(0x06010000+i*4)),
      slave:Number(emulator.slave_instruction_count()),sound:Number(emulator.sound_instruction_count()),
      pcmNonzero:samples.some(v=>v!==0),stereo:samples.some((v,i)=>i%2===0&&v!==samples[i+1]),
      wasm:points.map(([x,y])=>Array.from(frame.slice((y*320+x)*4,(y*320+x)*4+4))),
      canvas:points.map(([x,y])=>Array.from(canvas.getImageData(x,y,1,1).data)),
      backup:emulator.export_backup_ram()[0]};
  }, points);
  const assertLab = result => {
    assert.deepEqual(result.results.slice(0,6),[1,1,84,35,42,393984]);
    assert(result.results[6]>=1);
    assert.deepEqual(result.results.slice(7,9),[1,0x600D]);
    assert.equal(result.results[9]&255,result.backup);
    assert.equal(result.results[10],0x900);
    assert(result.slave>5 && result.sound>20 && result.pcmNonzero && result.stereo, JSON.stringify(result));
    assert.deepEqual(result.canvas,result.wasm);
  };
  await lab.locator("#disc").setInputFiles(labFiles());
  await lab.waitForFunction(()=>document.getElementById("status").textContent==="Ready");
  const labResult=await labTrace();
  assertLab(labResult);
  assert.equal(labResult.backup,1);
  assert.deepEqual(labResult.wasm,labPixels);
  // Restart uses controller pins, not a host write to the result table.
  await lab.evaluate(async()=>{const {emulator}=await import("./app.js");emulator.set_buttons(1<<7);emulator.run_steps(50);emulator.set_buttons(0);});
  const labRestart=await labTrace();
  assertLab(labRestart);
  assert.equal(labRestart.backup,2);
  assert.deepEqual(labRestart.wasm,labPixels);
  // Mutate only guest LOAD literals in the original disc's video routine.
  for (const [address,before,after,points,expected] of [
    [0x25E70048,3<<16,2<<16,[[0,0],[1,0],[2,0]],[[0,0,255,255],[0,255,0,255],[255,255,255,255]]],
    [0x25E69000,(16<<16)|30,(20<<16)|30,[[8,0],[9,0],[10,0],[15,0]],[[0,0,0,255],[0,0,0,255],[255,0,0,255],[0,255,0,255]]],
  ]) {
    const changed=Buffer.from(labIso);
    const addresses=[];
    for(let i=21*2048+0x2200;i<21*2048+0x6000;i++) if(changed.readUInt32BE(i)===address) addresses.push(i);
    assert.equal(addresses.length,1);
    const at=addresses[0]+12;
    assert.equal(changed.readUInt32BE(at),before);
    changed.writeUInt32BE(after,at);
    await lab.locator("#disc").setInputFiles(labFiles(changed));
    await lab.waitForFunction(()=>document.getElementById("status").textContent==="Ready");
    const result=await labTrace(points);
    assertLab(result);
    assert.deepEqual(result.wasm,expected);
  }
  await lab.locator("#audio-enable").click();
  await lab.waitForFunction(()=>document.getElementById("audio-status").textContent.includes("paused"));
  await lab.evaluate(async()=>{
    const {emulator,audio}=await import("./app.js");audio.setPaused(false);
    for(let i=0;i<4;i++)emulator.advance_frame();
    audio.deliver(emulator.audio_samples(8192));
    window.labAudioReady=()=>audio.stats.rendered>128&&audio.stats.peak>0.05;
  });
  await lab.waitForFunction(()=>window.labAudioReady());
  // A zeroed CD-DA track changes the actual mixed buffer. This rejects an audio
  // route which is merely nonzero because the independent 68k voice is running.
  const labBuffers=[];
  for(const zeroAudio of [false,true]) {
    await lab.locator("#disc").setInputFiles(labFiles(labIso,zeroAudio?Buffer.alloc(labAudio.length):labAudio));
    await lab.waitForFunction(()=>document.getElementById("status").textContent==="Ready");
    labBuffers.push(await lab.evaluate(async()=>{
      const {emulator}=await import("./app.js");
      for(let i=0;i<40&&emulator.read_long(0x06010020)!==0x600D;i++)emulator.run_steps(10000);
      emulator.audio_samples(8192);emulator.advance_frame();
      return Array.from(emulator.audio_samples(8192));
    }));
  }
  assert.equal(labBuffers[0].length,labBuffers[1].length);
  assert.notDeepEqual(labBuffers[0],labBuffers[1]);
  // Original 68k instructions upload a real effects program. Change only its
  // coefficient immediate in the disc, then assert the DSP's guest-read result
  // and exact stereo samples. Dry PCM alone cannot satisfy the wet left vector.
  const effectMove=Buffer.from([0x33,0xFC,0x40,0,0,0x10,7,0]);
  const effectAt=labIso.indexOf(effectMove);
  assert(effectAt>=0);assert.equal(labIso.indexOf(effectMove,effectAt+1),-1);
  for(const [coef,result,left] of [[0x4000,0x900,2304],[0x2000,0x480,1280],[0,0,256]]) {
    const effectIso=Buffer.from(labIso);effectIso.writeUInt16BE(coef,effectAt+2);
    await lab.locator("#disc").setInputFiles(labFiles(effectIso,Buffer.alloc(labAudio.length)));
    await lab.waitForFunction(()=>document.getElementById("status").textContent==="Ready");
    const effect=await lab.evaluate(async()=>{
      const {emulator}=await import("./app.js");
      for(let i=0;i<40&&emulator.read_long(0x06010020)!==0x600D;i++)emulator.run_steps(10000);
      emulator.audio_samples(8192);emulator.advance_frame();
      return {result:emulator.read_long(0x06010028),samples:Array.from(emulator.audio_samples(8192))};
    });
    assert.equal(effect.result,result);
    const expected=[left,4096,-left,-4096,-left*2,-8192,left*2,8192];
    assert(effect.samples.some((_,i)=>i%2===0&&expected.every((v,j)=>effect.samples[i+j]===v)));
  }
  const changedLab=Buffer.from(labIso);changedLab[60*2048+3]=9;
  await lab.locator("#disc").setInputFiles(labFiles(changedLab));
  await lab.waitForFunction(()=>document.getElementById("status").textContent==="Ready");
  assert.equal((await labTrace()).results[5],393990);
  // Extra CUE indices retain sector layout and guest results through Wasm.
  // The native command vectors separately assert seek/status/Q index numbers.
  const indexedCue=Buffer.from(labCue.toString()+"    INDEX 02 00:00:02\n    INDEX 99 00:00:07\n");
  const beforeIndexedBackup=await lab.evaluate(async()=>
    (await import("./app.js")).emulator.export_backup_ram()[0]);
  await lab.locator("#disc").setInputFiles([
    {name:"hardware-lab.cue",mimeType:"text/plain",buffer:indexedCue},...labFiles().slice(1)]);
  await lab.waitForFunction(()=>document.getElementById("status").textContent==="Ready");
  const indexedResult=await labTrace();
  assert.deepEqual(indexedResult.results.slice(0,9),labResult.results.slice(0,9));
  assert.equal(indexedResult.results[10],labResult.results[10]);
  // This mailbox reports the guest's incremented persistent backup word.
  assert.equal(indexedResult.backup,(beforeIndexedBackup+1)&255);
  assert.equal(indexedResult.results[9],(0xFFFFFF00|indexedResult.backup)>>>0);
  const invalidIndices=Buffer.from(labCue.toString()+"    INDEX 03 00:00:03\n    INDEX 02 00:00:04\n");
  await lab.locator("#disc").setInputFiles([
    {name:"hardware-lab.cue",mimeType:"text/plain",buffer:invalidIndices},...labFiles().slice(1)]);
  await lab.waitForFunction(()=>document.getElementById("error").textContent.includes("increasing index numbers"));
  await lab.locator("#disc").setInputFiles(labFiles().slice(0,2));
  await lab.waitForFunction(()=>document.getElementById("error").textContent.includes("Missing or ambiguous CUE file reference"));
  const rejected=await lab.evaluate(async()=>{const {emulator}=await import("./app.js");return {state:emulator.status(),retired:emulator.instruction_count()};});
  assert.match(rejected.state,/Reset/);assert.equal(rejected.retired,"0");
  assert(labRequests.every(r=>r.method==="GET"&&!r.body&&r.url.startsWith(address+prefix)));
  await lab.close();
  // IEC 60908 PRE changes actual CD PCM, driven only by an original SH-2
  // routing/play guest. Compare the real Wasm buffer to the transfer function,
  // including impulses on both sides of a sector boundary, then play that PCM.
  const cddaRom = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/cdda.hex", "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [address, ...words] = line.trim().split(/\s+/).map((word) => parseInt(word, 16));
    words.forEach((word, i) => cddaRom.writeUInt16BE(word, address + i * 2));
  }
  const cddaInput = Buffer.alloc(3 * 2352);
  for (let i = 0; i < cddaInput.length / 4; i++) {
    cddaInput.writeInt16LE([0, 587, 588, 1176].includes(i) ? 10000 : 0, i * 4);
    cddaInput.writeInt16LE(-20000, i * 4 + 2);
  }
  const cddaPage = await context.newPage();
  await cddaPage.goto(address + prefix);
  await cddaPage.waitForFunction(() => document.getElementById("status").textContent === "Ready");
  await cddaPage.locator("#audio-enable").click();
  for (const pre of [false, true]) {
    const samples = await cddaPage.evaluate(async ({ rom, track, pre }) => {
      const { Emulator } = await import("./pkg/saturn_rust.js");
      const { audio } = await import("./app.js");
      const emulator = new Emulator();
      try {
        emulator.reset_bios(Uint8Array.from(rom));
        const cue = 'FILE "a" BINARY\nTRACK 01 AUDIO\n' + (pre ? 'FLAGS PRE\n' : '') + 'INDEX 01 00:00:00';
        emulator.mount_cue(cue, "a", new Uint32Array([track.length]), Uint8Array.from(track));
        for (let i = 0; i < 12; i++) emulator.run_steps(10000);
        if (emulator.read_long(0x06009010) !== 0x600D) throw new Error("CD-DA guest did not complete");
        const pcm = emulator.audio_samples(8192);
        if (pre) {
          audio.setPaused(false);
          audio.deliver(pcm);
          window.cddaAudio = audio;
        }
        return [...pcm];
      } finally { emulator.free(); }
    }, { rom: [...cddaRom], track: [...cddaInput], pre });
    const first = samples.findIndex((value, i) => i % 2 === 1 && value !== 0) - 1;
    assert(first >= 0 && samples.length - first > 1178 * 2);
    const previousInput = [0, 0], previousOutput = [0, 0];
    for (let i = 0; i < 1178; i++) {
      for (let channel = 0; channel < 2; channel++) {
        const input = cddaInput.readInt16LE(i * 4 + channel * 2);
        const expected = pre ? ((1 + 88200 * 15e-6) * input + (1 - 88200 * 15e-6) * previousInput[channel]
          - (1 - 88200 * 50e-6) * previousOutput[channel]) / (1 + 88200 * 50e-6) : input;
        assert(Math.abs(samples[first + i * 2 + channel] - expected) < 0.51, `PRE=${pre}, frame=${i}, channel=${channel}`);
        previousInput[channel] = input;
        previousOutput[channel] = expected;
      }
    }
  }
  await cddaPage.waitForFunction(() => window.cddaAudio.stats.rendered > 128 && window.cddaAudio.stats.peak > 0.1);
  await cddaPage.close();
  const playerPage=await context.newPage();
  const playerRequests=[];
  playerPage.on("pageerror",error=>pageErrors.push(error.message));
  playerPage.on("request",request=>playerRequests.push({url:request.url(),method:request.method(),body:request.postData()}));
  await hlePlayer(playerPage,address+prefix);
  assert(playerRequests.length>0);
  assert(playerRequests.every(r=>{
    const url=new URL(r.url);
    return url.origin===address&&staticPaths.has(url.pathname)&&url.search===""&&r.method==="GET"&&r.body===null;
  }));
  await playerPage.close();
  assert.deepEqual(pageErrors,[]);
  console.log(`Browser smoke passed (${await browser.version()}): Play-first, responsive desktop/tablet/mobile, keyboard settings/navigation, state/diagnostic preservation, input release, fullscreen ${fullscreenResult} + denied/unavailable feedback; /saturn-rust/ assets/MIME/Wasm, fixture trace, Step/Reset/bounded Run, local BIOS validation/vectors/fault, guest address-error PC/SR traces and RTE recovery, pending-error reset, SH7604 DIVU/DMA/IRQ/SCI/watchdog reset/cache coherence and bounded stacking errors, dual-CPU DIVU overflow intermediates/UBC acknowledgement and dividend mutations, all eight SMPC area codes and independent CD/sound reset reports, dual-SH-2 computation/capture IRQs/slave restart/master NMI and sticky slave faults, BCR1 processor identity/refresh IRQ/module stops/reset repeatability, UBC before/after/slot/data/DMAC saved-PC vectors, software standby/timed NMI wake, SYSRES volatile/battery retention, four-layer VDP2 tile guest/palette mutation/Wasm and production Canvas presentation, original rotation/coefficient/line-scroll/line-window guest and mutated matrix pixels, original RBG0/RBG1 guest with B0/B1 data and changed parameter-B pixels/Canvas, SCU DSP product/sum/external DMA/direct+indirect DMA/raster timer IRQs/reset/illegal-DSP faults, original CD scan/reverse/pause/tray/TOC guest, malformed runtime ISO root/child rejection and bounded raw-sector correction/failure responses, CD-DA PRE reference stereo/sector continuity and AudioWorklet, sprite-window/shadow/palette-MSB guest and Canvas mutations, 68k BCD/MOVEM/MOVEP/arithmetic/partial-fault/trap guest plus timed MIDI/SCSP DMA and three interrupt levels; SH-2-uploaded MC68EC000 PCM computation/SCSP timer IRQ/stereo samples and WebAudio worklet delivery/underflow/mute/blocked states, SCSP FM recent/past stack pipeline, TL gain, ALFO/PLFO and timed-envelope guest mutations and AudioWorklet; 73 original 68k family/EA/CCR/exception vectors, twenty-four guest access-frame/partial-transfer/prefetch records, MOVEM terminal reads and operand-width mutation, ADD-to-SUB and RMW-to-write mutations; ICR auto/external vectors, signed DIV1 sequences and both-port SMPC-to-SCU/VDP2 latching with control mutation, DACK armed waiting/reset, two-way cache RAM isolation and operand mutation, gradation top/second filtering and changed controls/Wasm/Canvas, descriptor-set runtime/startup and zero-first-read AIP; guest-executed HLE backup library expansion/relocation, 158 ABI records, full-capacity allocation, prefixes/compaction, Gregorian dates, corruption and persistent raw import/export, changed guest XOR operand; BIOS/HLE PLL transitions and reset domains, rational FRT/raster clocks, 352-dot Canvas output, 60 standard TV-mode/PAL/interlace guests with INTBACK/field/counter/pixel vectors and 704x480 Canvas, VDP1 Rotation16/Rotation8 Q9 readout, line-start X/Y reloads, guest delta mutations, double-interlace/DIL buffer traces and 448-line Wasm/Canvas fields, latched HSS/EOS scaled/distorted/flipped guest pixels and changed HSS operands, two-port digital settling/operand mutations; SMPC RTC/SMEM/INTBACK IRQs and synthetic digital+ID15/ID16 analog Gamepad/focus release, timed both-port direct analog handshake guest, mode switching, offline execution, no local-file transmission; actual ISO gameplay, keyboard input, guest state/pixels, scoring, completion, restart, local disc errors/races; guest backup/second-pad ROM and ISO, offline backup import/export/download bytes, reset retention, invalid imports/read races, second-pad keyboard/release, fresh-tab isolation; Prism Courier original disc, VDP1 sprite/polygons + VDP2 composition/Canvas, draw-end IRQ, 68k stereo + actual AudioWorklet, keyboard collision/win/restart, changed-disc texture mutation; Hardware Lab local CUE/mixed tracks, dual CPU, executing SCU DSP, CD FIFO-to-SCU DMA/IRQs/checksum, independent 68k-uploaded SCSP effects DSP, exact wet stereo/coefficient mutations, CD-DA through DSP and AudioWorklet, fourteen rotation/window Canvas pixels, disc matrix/window mutations, full diagnostic/pixel restart checks, backup, sector/audio mutations, CUE subindices and malformed ordering, missing local file rejection.`);
} finally {
  if (browser) await browser.close();
  if (server?.pid && server.exitCode === null) {
    const stopped = once(server, "exit");
    server.kill();
    await stopped;
  }
  await rm(mount, { recursive: true, force: true });
}
