// SPDX-License-Identifier: MIT
// Original patterns and guests. Readbacks are confined to this browser test.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const presets = ["clean", "composite", "s-video", "rgb"];
const filter = page => page.getByLabel("Display filter", { exact: true });
const ready = page => page.waitForFunction(() => {
  return !document.getElementById("controls").disabled && document.getElementById("main").getAttribute("aria-busy") !== "true";
});
const visible = (page, expected) => page.waitForFunction(expected => document.getElementById("crt-screen").hidden === !expected, expected);

async function rom(name) {
  const bytes = Buffer.alloc(524288);
  for (const line of (await readFile(`tests/fixtures/${name}.hex`, "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(word => parseInt(word, 16));
    words.forEach((word, i) => bytes.writeUInt16BE(word, at + i * 2));
  }
  return bytes;
}

async function checkPatterns(page) {
  const result = await page.evaluate(async () => {
    const { CrtDisplay } = await import("./crt-display.js");
    const screen = document.createElement("canvas"), canvas = document.createElement("canvas");
    const selector = document.createElement("select"), status = document.createElement("p");
    for (const preset of ["clean", "composite", "s-video", "rgb"]) selector.add(new Option(preset, preset));
    document.body.append(screen, canvas, selector, status);
    const display = new CrtDisplay(screen, canvas, selector, status);
    const pattern = (width, height) => {
      const rgba = new Uint8Array(width * height * 4);
      const bars = [[224, 32, 32], [32, 224, 32], [32, 32, 224]];
      const glyphs = ["111100100100111", "111010010010010", "110101110101101"];
      for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
        let color;
        if (y < 40) color = Array(3).fill(x < width / 2 ? 48 : 208);
        else if (y < 80) color = bars[Math.floor(x * 3 / width)];
        else if (y < 120) color = Array(3).fill((x + y) % 2 ? 224 : 32);
        else if (y < 160) color = Array(3).fill(32 + 48 * Math.floor(5 * x / width));
        else if (y < 200) {
          const at = x - 100, glyph = glyphs[Math.floor(at / 4)];
          const ink = y >= 175 && y < 180 && glyph?.[(y - 175) * 3 + at % 4] === "1" && at % 4 < 3;
          color = Array(3).fill(ink ? 224 : 32);
        } else color = [160, 160, 160];
        rgba.set([...color, 255], (y * width + x) * 4);
      }
      return rgba;
    };
    const dimensions = [
      [320, 224, 1], [320, 224, 4], [352, 240, 2.5], [320, 448, 4],
      [640, 224, 1], [640, 224, 2], [704, 480, 1], [704, 480, 2],
    ];
    const results = [];
    let pixels, gl;
    const original = WebGL2RenderingContext.prototype.drawArrays;
    WebGL2RenderingContext.prototype.drawArrays = function (...args) {
      original.apply(this, args);
      gl = this;
      pixels = new Uint8Array(canvas.width * canvas.height * 4);
      gl.readPixels(0, 0, canvas.width, canvas.height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    };
    try {
      for (const [width, height, scale] of dimensions) {
        screen.style.width = `${width * scale / devicePixelRatio}px`;
        screen.style.height = `${height * scale / devicePixelRatio}px`;
        if (display.resources) display.measure();
        const input = pattern(width, height);
        display.present(input, width, height);
        for (const preset of ["composite", "s-video", "rgb"]) {
          selector.value = preset;
          selector.dispatchEvent(new Event("change"));
          if (canvas.hidden || !gl || status.textContent || gl.getError()) throw new Error(status.textContent || "Missing WebGL draw");
          const sample = (x, y, channel = 0) => pixels[((canvas.height - 1 - y) * canvas.width + x) * 4 + channel];
          const dot = source => Math.floor((source + 0.5) * scale);
          const at = (x, y, channel) => sample(dot(x), dot(y), channel);
          const flat = [];
          for (let y = 210 * scale; y < 215 * scale; y++) flat.push(sample(dot(16), y));
          const mask = [0, 1, 2].map(channel => at(16, 215, channel));
          const gray = [32, 96, 160, 224, 288].map(x => at(Math.floor(x * width / 320), 140));
          const corners = [at(0, 0), at(width - 1, 0), at(0, height - 1), at(width - 1, height - 1)];
          const before = pixels;
          selector.dispatchEvent(new Event("change"));
          let repeatDelta = 0, min = 255, max = 0, opaque = true;
          for (let i = 0; i < pixels.length; i += 4) {
            opaque &&= pixels[i + 3] === 255;
            for (let j = i; j < i + 3; j++) {
              min = Math.min(min, pixels[j]);
              max = Math.max(max, pixels[j]);
              repeatDelta = Math.max(repeatDelta, Math.abs(pixels[j] - before[j]));
            }
          }
          results.push({
            width, height, scale, preset, output: [canvas.width, canvas.height],
            edge: at(width / 2 - 1, 20) - at(20, 20),
            bleed: at(Math.ceil(width / 3) - 1, 60, 1) - at(20, 60, 1),
            checker: Math.abs(at(100, 100) - at(101, 100)),
            text: at(100, 177) - at(96, 177),
            solid: [at(20, 60, 0), at(Math.floor(width / 2), 60, 1), at(width - 20, 60, 2)],
            beam: Math.max(...flat) - Math.min(...flat), mask: Math.max(...mask) - Math.min(...mask),
            gray, corners, repeatDelta, min, max, opaque,
          });
        }
      }
      const renderer = gl.getExtension("WEBGL_debug_renderer_info");
      return { results, renderer: renderer ? gl.getParameter(renderer.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER) };
    } finally {
      WebGL2RenderingContext.prototype.drawArrays = original;
      display.clear();
      canvas.getContext("webgl2")?.getExtension("WEBGL_lose_context")?.loseContext();
      for (const node of [screen, canvas, selector, status]) node.remove();
    }
  });
  for (const entry of result.results) {
    const { width, height, scale, output, repeatDelta, min, max, opaque, beam, mask, gray, solid, corners } = entry;
    assert.deepEqual(output, [width * scale, height * scale]);
    assert(opaque && repeatDelta <= 1 && min >= 0 && max <= 250, JSON.stringify(entry));
    assert(beam >= 0 && beam <= 20 && mask <= 9, JSON.stringify(entry));
    if (height >= 448 || scale === 1) assert.equal(beam, 0);
    if (height < 448 && scale === 4) assert(beam >= 8 && mask > 0);
    if (scale <= 3) assert.equal(mask, 0);
    assert(gray.every((value, i) => Math.abs(value - (32 + 48 * i)) < 28), JSON.stringify(entry));
    assert(solid.every(value => value >= 195 && value <= 225), JSON.stringify(entry));
    assert(corners.every((value, i) => Math.abs(value - [48, 208, 160, 160][i]) < 31), JSON.stringify(entry));
  }
  for (let i = 0; i < result.results.length; i += 3) {
    const [composite, svideo, rgb] = result.results.slice(i, i + 3);
    const metrics = JSON.stringify([composite, svideo, rgb]);
    if (Number.isInteger(composite.scale)) {
      assert(composite.checker + 8 < svideo.checker && svideo.checker + 5 < rgb.checker, `Checkerboard contrast: ${metrics}`);
    }
    if (composite.width !== 320 || composite.height !== 224) continue;
    assert(composite.edge > svideo.edge + 8 && svideo.edge > rgb.edge + 7, metrics);
    assert(composite.bleed > svideo.bleed + 8 && svideo.bleed > rgb.bleed + 5, metrics);
    assert(composite.text > 65 && rgb.text > composite.text, metrics);
  }
  console.log("CRT WebGL output", JSON.stringify(result));
}

async function checkGuest(page) {
  const modes = await rom("display-modes");
  const iso = await readFile("web/hardware-lab.iso");
  const result = await page.evaluate(async ({ modes, iso, presets }) => {
    const { emulator: machine } = await import("./app.js");
    const { client } = machine;
    await machine.set_fast_mode(false);
    const digest = async bytes => Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)))
      .map(byte => byte.toString(16).padStart(2, "0")).join("");
    const selector = document.getElementById("display-filter");
    const raw = document.getElementById("screen");
    const ctx = raw.getContext("2d");
    const select = value => {
      selector.value = value;
      selector.dispatchEvent(new Event("change"));
    };
    const traces = [];
    let video, pcm, audible;
    const present = client.onFrame, audio = client.onAudio;
    client.onFrame = frame => {
      present(frame);
      const pixels = ctx.getImageData(0, 0, raw.width, raw.height).data;
      video.push(Promise.all([frame.fields, frame.clocks, frame.width, frame.height, digest(frame.pixels), digest(pixels)]));
    };
    client.onAudio = packet => {
      pcm.push(digest(packet.pcm));
      audible ||= packet.pcm.some(sample => sample !== 0);
      return audio(packet);
    };
    for (const preset of [...presets, "switch"]) {
      select(preset === "switch" ? "composite" : preset);
      await machine.reset_disc(Uint8Array.from(iso));
      video = []; pcm = []; audible = false;
      for (let i = 0; i < 4; i++) {
        if (preset === "switch") select(presets[i]);
        await client.advance(8);
      }
      const lab = await machine.read_long(0x06010020);
      const sounds = await Promise.all(pcm), heard = audible;
      const displayModes = [];
      for (const tvmd of [0x8000, 0x8081, 0x80d3]) {
        const bytes = new Uint8Array(524288);
        bytes.set(modes);
        bytes[0x100] = tvmd >> 8; bytes[0x101] = tvmd & 255;
        await machine.reset_bios(bytes);
        client.setInput({ buttons: 1 | 16 | 4096, port2Buttons: 8 | 128 | 2048,
          ports: [null, { analog: { connected: true, x: 0, y: 128, throttle: 255 } }] });
        for (let i = 0; i < 60 && await machine.read_long(0x0600e088) !== 0x600d; i++) await machine.run_steps(10000);
        if (await machine.read_long(0x0600e088) !== 0x600d) throw new Error(`TV mode guest did not finish for ${tvmd.toString(16)}`);
        await client.advance(4);
        const input = await Promise.all(Array.from({ length: 14 }, (_, i) => machine.read_long(0x0600e008 + i * 4)));
        const { soundTransport, ...state } = await client.inspect();
        displayModes.push({ state, input });
      }
      traces.push({ lab, sounds, heard, displayModes, video: await Promise.all(video) });
    }
    client.onFrame = present;
    client.onAudio = audio;
    return traces;
  }, { modes: [...modes], iso: [...iso], presets });
  const [baseline, ...filtered] = result;
  assert.equal(baseline.lab, 0x600d);
  assert(baseline.heard);
  assert.equal(baseline.sounds.length, 32);
  assert.equal(baseline.video.length, 44);
  for (const [i, { state, input }] of baseline.displayModes.entries()) {
    assert.deepEqual([state.width, state.height], [[320, 224], [352, 224], [704, 480]][i]);
    assert.deepEqual(input.slice(0, 12), [0xc0, 0xf1, 2, 0xee, 0xf7, 0xf1, 0x15, 0x77, 0x7f, 0, 128, 255]);
    assert.equal(state.pendingFrames, 0);
    assert(state.peakPendingFrames <= 2);
  }
  assert(baseline.video.every(([, , , , raw, canvas]) => raw === canvas));
  for (const [i, trace] of filtered.entries()) assert.deepEqual(trace, baseline, [...presets.slice(1), "switch"][i]);
  console.log("CRT differential passed: guest input packets, exact clocks/state, 44 raw/Canvas frames and 32 nonzero-capable PCM packets per trace.");
}

async function checkPlayer(page) {
  assert.equal(await filter(page).inputValue(), "clean");
  assert.deepEqual(await filter(page).locator("option").allTextContents(), ["Clean", "CRT Composite", "CRT S-Video", "CRT RGB"]);
  await page.locator("#disc").setInputFiles("web/star-collector.iso");
  await ready(page);
  await page.locator("#audio-enable").click();
  await page.locator("#start").click();
  await page.waitForFunction(() => document.getElementById("empty-state").hidden);
  for (const preset of presets.slice(1)) {
    await filter(page).selectOption(preset);
    await visible(page, true);
    assert.match(await page.locator("#player-status").textContent(), /Running/);
  }
  await page.waitForFunction(async () => (await import("./app.js")).audio.stats.rendered > 0);
  const settings = page.locator("#settings > summary");
  await settings.click();
  await page.waitForFunction(() => document.getElementById("pause").textContent === "Resume");
  await ready(page);
  await settings.click();
  await ready(page);
  const state = () => page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    const backup = await emulator.export_backup_ram();
    return [await emulator.frames(), await emulator.instruction_count(), await emulator.peripheral_clocks(),
      Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", backup)))];
  });
  const paused = await state();
  const matchSurface = () => page.waitForFunction(() => {
    const raw = document.getElementById("screen"), crt = document.getElementById("crt-screen");
    const rect = raw.getBoundingClientRect(), overlay = crt.getBoundingClientRect();
    return Math.abs(rect.width / rect.height - 4 / 3) < 0.001 &&
      rect.x === overlay.x && rect.y === overlay.y && rect.width === overlay.width && rect.height === overlay.height &&
      Math.abs(crt.width - rect.width * devicePixelRatio) < 1 && Math.abs(crt.height - rect.height * devicePixelRatio) < 1;
  });
  await matchSurface();
  await page.setViewportSize({ width: 820, height: 750 });
  await matchSurface();
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: 820, height: 750, deviceScaleFactor: 1.5, mobile: false });
  assert.equal(await page.evaluate(() => devicePixelRatio), 1.5);
  await matchSurface();
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: 820, height: 750, deviceScaleFactor: 2, mobile: false });
  await matchSurface();
  await cdp.detach();
  await filter(page).selectOption("clean");
  await visible(page, false);
  const picture = () => page.locator("#screen").screenshot();
  const cleanPicture = await picture();
  for (const preset of presets.slice(1)) {
    await filter(page).selectOption(preset);
    await visible(page, true);
    assert.deepEqual(await state(), paused);
  }
  assert.notDeepEqual(await picture(), cleanPicture, "The composed display must show the CRT surface");
  await filter(page).selectOption("clean");
  assert.deepEqual(await picture(), cleanPicture, "Returning to Clean must restore the same picture");
  await filter(page).selectOption("rgb");

  const fullscreen = await page.evaluate(() => document.fullscreenEnabled);
  if (fullscreen) {
    await page.locator("#fullscreen").click();
    await page.waitForFunction(() => document.fullscreenElement?.id === "player-shell" && document.activeElement.id === "screen");
    await matchSurface();
    await page.locator("#fullscreen").click();
    await page.waitForFunction(() => !document.fullscreenElement && document.activeElement.id === "fullscreen");
  }
  const lose = await page.evaluate(() => {
    const gl = document.getElementById("crt-screen").getContext("webgl2");
    window.crtLoss = gl.getExtension("WEBGL_lose_context");
    window.crtLoss?.loseContext();
    return Boolean(window.crtLoss);
  });
  if (lose) {
    await page.waitForFunction(() => document.getElementById("display-filter-status").textContent.includes("interrupted"));
    await visible(page, false);
    await page.evaluate(() => window.crtLoss.restoreContext());
    await visible(page, true);
    assert.equal(await page.locator("#display-filter-status").textContent(), "");
  }
  await page.locator("#view-debug").click();
  await ready(page);
  await visible(page, true);
  assert.deepEqual(await state(), paused);
  await page.locator("#reset").click();
  await ready(page);
  await visible(page, false);
  assert.equal(await filter(page).inputValue(), "rgb");
  await page.locator("#frame").click();
  await visible(page, true);
  await page.locator("#view-play").click();
  await ready(page);
  assert.equal(await page.locator("#pause").textContent(), "Resume");
  assert.equal(await page.locator("#error").textContent(), "");
  console.log(`CRT player passed at DPR 2, fullscreen ${fullscreen}, context restoration ${lose}.`);
}

async function checkFallback(page, url, failure) {
  await page.addInitScript(failure => {
    if (failure === "creation") {
      const getContext = HTMLCanvasElement.prototype.getContext;
      HTMLCanvasElement.prototype.getContext = function (type, ...args) {
        return type === "webgl2" ? null : getContext.call(this, type, ...args);
      };
    } else if (failure === "compile") {
      const source = WebGL2RenderingContext.prototype.shaderSource;
      WebGL2RenderingContext.prototype.shaderSource = function (shader, code) {
        return source.call(this, shader, shader && code.includes("out vec4 color") ? "invalid shader" : code);
      };
    } else {
      WebGL2RenderingContext.prototype.attachShader = () => {};
    }
  }, failure);
  await page.goto(url);
  await ready(page);
  await filter(page).selectOption("composite");
  assert.match(await page.locator("#display-filter-status").textContent(), /Showing Clean/);
  await page.locator("#disc").setInputFiles("web/star-collector.iso");
  await ready(page);
  await page.locator("#start").click();
  await page.waitForFunction(() => {
    const canvas = document.getElementById("screen");
    return canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data.some((channel, i) => i % 4 !== 3 && channel !== 0);
  });
  await visible(page, false);
  assert.equal(await page.locator("#error").textContent(), "");
  await filter(page).selectOption("clean");
  assert.equal(await page.locator("#display-filter-status").textContent(), "");
}

export async function crtDisplay(browser, url) {
  const run = async (test, start = true) => {
    const context = await browser.newContext({ deviceScaleFactor: 2 });
    try {
      const page = await context.newPage(), errors = [], requests = [];
      page.on("pageerror", error => errors.push(error.message));
      page.on("request", request => requests.push([request.url(), request.method(), request.postData()]));
      if (start) { await page.goto(url); await ready(page); }
      await test(page);
      assert.deepEqual(errors, []);
      assert(requests.every(([target, method, body]) => target.startsWith(url) && method === "GET" && !body));
      assert(!requests.some(([target]) => /\.(iso|cue|bin)$/.test(target)), "Local selections must stay local");
    } finally {
      await context.close();
    }
  };
  await run(checkPatterns);
  await run(checkGuest);
  await run(checkPlayer);
  for (const failure of ["creation", "compile", "link"]) await run(page => checkFallback(page, url, failure), false);
}
