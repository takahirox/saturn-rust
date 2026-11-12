// Original MIT polling guest: production UI -> Wasm -> timed SMPC INTBACK.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

// Independent active-low packet literals, not computed from the adapter table.
const bindings = [
  ["ArrowUp", 12, 1, [0xef, 0xff]], ["ArrowDown", 13, 2, [0xdf, 0xff]],
  ["ArrowLeft", 14, 4, [0xbf, 0xff]], ["ArrowRight", 15, 8, [0x7f, 0xff]],
  ["KeyZ", 0, 64, [0xfb, 0xff]], ["KeyX", 1, 16, [0xfe, 0xff]],
  ["KeyC", 2, 32, [0xfd, 0xff]], ["KeyA", 3, 1024, [0xff, 0xbf]],
  ["KeyS", 4, 512, [0xff, 0xdf]], ["KeyD", 5, 256, [0xff, 0xef]],
  ["KeyQ", 7, 2048, [0xff, 0x7f]], ["KeyW", 6, 4096, [0xff, 0xf7]],
  ["Enter", 9, 128, [0xf7, 0xff]],
];
const neutral = [0xff, 0xff];

export async function digitalInput(browser, url) {
  const context = await browser.newContext();
  try {
    // Explicit guest stepping keeps input observations independent of RAF timing.
    await context.addInitScript(() => { window.requestAnimationFrame = () => 1; });
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", error => errors.push(error.message));
    await page.goto(url);
    const ready = () => page.waitForFunction(() => !document.getElementById("start").disabled, undefined, {polling:10});
    await page.waitForFunction(() => !document.getElementById("controls").disabled, undefined, {polling:10});
    const rom = Buffer.alloc(524288);
    for (const line of (await readFile("tests/fixtures/digital-input.hex", "utf8")).split(/\r?\n/)) {
      if (!line || line.startsWith("#")) continue;
      const [at, ...words] = line.trim().split(/\s+/).map(w => parseInt(w, 16));
      words.forEach((w, i) => rom.writeUInt16BE(w, at+i*2));
    }
    // Reuse the original licensed ISO container; only its executable is replaced.
    const iso = Buffer.from(await readFile("web/star-collector.iso"));
    const payload = rom.subarray(0x200, 0x300);
    payload.copy(iso, 21*2048);
    iso.writeUInt32BE(payload.length, 0xf4);
    iso.writeUInt32LE(payload.length, 20*2048+68+10);
    iso.writeUInt32BE(payload.length, 20*2048+68+14);
    await page.locator("#disc").setInputFiles({name:"original-digital-input.iso", mimeType:"application/octet-stream", buffer:iso});
    await ready();
    await page.locator("#bios").setInputFiles({name:"original-digital-input.bin", mimeType:"application/octet-stream", buffer:rom});
    await page.evaluate(async () => {
      const {emulator} = await import("./app.js");
      window.inputMasks = [0, 0];
      for (const [port, method] of ["set_buttons", "set_port2_buttons"].entries()) {
        const original = emulator[method].bind(emulator);
        emulator[method] = mask => { window.inputMasks[port] = mask; original(mask); };
      }
      window.inputPads = Array.from({length:2}, (_, index) => ({index, connected:true, mapping:"standard", axes:[],
        buttons:Array.from({length:16}, () => ({pressed:false, value:0}))}));
      Object.defineProperty(navigator, "getGamepads", {configurable:true, value:() => window.inputPads});
    });
    const focus = () => page.locator("#screen").focus();
    const key = (code, down = true, extra = {}) => page.evaluate(({code, down, extra}) => {
      const event = new KeyboardEvent(down ? "keydown" : "keyup", {code, key:code, bubbles:true, cancelable:true, ...extra});
      document.activeElement.dispatchEvent(event);
      return event.defaultPrevented;
    }, {code, down, extra});
    const gamepads = (pressed = [[], []]) => page.evaluate(async pressed => {
      window.inputPads.forEach((pad, port) => pad.buttons.forEach((button, i) => {
        button.pressed = pressed[port].includes(i); button.value = button.pressed ? 1 : 0;
      }));
      (await import("./app.js")).pollGamepads();
    }, pressed);
    const sample = async (masks, packets, label) => {
      const result = await page.evaluate(async () => {
        const {emulator:m} = await import("./app.js");
        const before = m.read_long(0x06008044);
        // Two completed reports ensure a command in flight at keyup is retired.
        for (let i=0; i<100 && m.read_long(0x06008044)<before+2; i++) m.run_steps(1000);
        const packet = Array.from({length:4}, (_, i) => m.read_long(0x06008020+i*4))
          .flatMap(w => [w>>>24, (w>>>16)&255, (w>>>8)&255, w&255]);
        return {masks:window.inputMasks, packet, fresh:m.read_long(0x06008044)>=before+2};
      });
      assert(result.fresh, `${label}: guest must complete fresh INTBACK reports`);
      assert.deepEqual(result.masks, masks, `${label}: frontend masks`);
      const expected = packets.flatMap(packet => [0xf1, packet.length === 2 ? 2 : 0x16, ...packet]);
      assert.deepEqual(result.packet, [...expected, ...Array(16-expected.length).fill(255)], `${label}: guest packet`);
    };
    // Identical position-independent code must work under external BIOS and HLE.
    for (const mode of ["bios", "disc"]) {
      await page.locator("#mode").selectOption(mode);
      await ready();
      await page.locator("#start").click();
      await focus();
      await sample([0, 0], [neutral, neutral], `${mode} initial`);
      for (const [code, index, mask, packet] of bindings) {
        const physical = code.startsWith("Key") ? code.slice(3).toLowerCase() : code;
        await page.keyboard.down(physical);
        await sample([mask, 0], [packet, neutral], `${mode} ${code}`);
        await page.keyboard.up(physical);
        await sample([0, 0], [neutral, neutral], `${code} release`);
        for (const port of [0, 1]) {
          const pressed = [[], []]; pressed[port] = [index];
          await gamepads(pressed);
          await sample(port ? [0, mask] : [mask, 0], port ? [neutral, packet] : [packet, neutral], `${mode} gamepad ${port} button ${index}`);
          await gamepads();
          await sample([0, 0], [neutral, neutral], "gamepad release");
        }
      }
      // Real Standard Gamepads normally have axes, selecting the ID16 path.
      // Verify every button there too, including keyboard shoulders translated
      // into full-travel triggers while retaining the ordinary trigger hysteresis.
      const pad3d = (packet = neutral, right = 0, left = 0) => [...packet, 128, 128, right, left];
      await page.evaluate(() => window.inputPads.forEach(pad => { pad.axes = [0, 0]; }));
      await gamepads();
      for (const [code, index, mask, packet] of bindings) {
        await key(code);
        await sample([mask, 0], [pad3d(packet, code === "KeyQ" ? 255 : 0, code === "KeyW" ? 255 : 0), pad3d()], `${mode} 3D keyboard ${code}`);
        await key(code, false);
        await sample([0, 0], [pad3d(), pad3d()], "3D keyboard release");
        for (const port of [0, 1]) {
          const pressed = [[], []]; pressed[port] = [index];
          await gamepads(pressed);
          const reports = [pad3d(), pad3d()];
          reports[port] = pad3d(packet, index === 7 ? 255 : 0, index === 6 ? 255 : 0);
          await sample(port ? [0, mask] : [mask, 0], reports, `${mode} 3D gamepad ${port} button ${index}`);
          await gamepads();
          await sample([0, 0], [pad3d(), pad3d()], "3D gamepad release");
        }
      }
      // Hysteresis is guest-visible, independently of the adapter's >0.5 bit.
      for (const [level, mask, packet] of [[140,2048,neutral], [146,2048,[255,0x7f]], [100,0,[255,0x7f]], [85,0,neutral]]) {
        await page.evaluate(async level => {
          window.inputPads[0].buttons[7] = {pressed:false, value:level/255};
          (await import("./app.js")).pollGamepads();
        }, level);
        await sample([mask, 0], [pad3d(packet, level), pad3d()], `physical trigger level ${level}`);
      }
      await gamepads();
      await key("KeyQ"); await key("KeyP");
      await gamepads([[0], [6]]);
      await sample([2048|64, 32|4096], [pad3d([0xfb,0x7f],255), pad3d([0xfd,0xf7],0,255)], "3D keyboard/gamepad union on both ports");
      await key("KeyQ", false); await key("KeyP", false);
      await sample([64, 4096], [pad3d([0xfb,255]), pad3d([255,0xf7],0,255)], "3D keyboard release retains physical input");
      await gamepads();
      await page.evaluate(() => window.inputPads.forEach(pad => { pad.axes = []; }));
      await gamepads();
      // Every keyboard bit can be held together; P2's bindings are disjoint.
      for (const [code] of bindings) await key(code);
      for (const code of ["KeyI", "KeyK", "KeyJ", "KeyL", "KeyU", "KeyO", "KeyP", "ShiftRight"]) await key(code);
      await sample([0x1fff, 0xff], [[0, 7], [0, 255]], "simultaneous keyboards");
      for (const [code] of bindings) await key(code, false);
      await sample([0, 0xff], [neutral, [0, 255]], "P1 release leaves P2");
      const p2 = [["KeyI",1,[0xef,255]], ["KeyK",2,[0xdf,255]], ["KeyJ",4,[0xbf,255]], ["KeyL",8,[0x7f,255]],
        ["KeyU",64,[0xfb,255]], ["KeyO",16,[0xfe,255]], ["KeyP",32,[0xfd,255]], ["ShiftRight",128,[0xf7,255]]];
      for (const [code] of p2) await key(code, false);
      await sample([0, 0], [neutral, neutral], "both keyboards released");
      for (const [code, mask, packet] of p2) {
        await key(code);
        await sample([0, mask], [neutral, packet], `P2 ${code}`);
        await key(code, false);
        await sample([0, 0], [neutral, neutral], `P2 ${code} release`);
      }
      // Changed characters/layout/case still use the physical code. Left Shift
      // never controls P2; shortcut modifiers do not acquire emulated buttons.
      await page.keyboard.down("Shift");
      await page.keyboard.down("z");
      await key("KeyZ", true, {key:"Y", repeat:true});
      await sample([64, 0], [[0xfb,255], neutral], "Shift and repeat");
      await key("KeyZ", false, {key:"y", ctrlKey:true});
      await page.keyboard.up("z"); await page.keyboard.up("Shift");
      for (const modifier of ["ctrlKey", "altKey", "metaKey"]) assert.equal(await key("KeyZ", true, {[modifier]:true}), false);
      await sample([0, 0], [neutral, neutral], "modifier keyup and shortcuts");
      await key("KeyX", true, {key:"Ж"});
      await key("KeyZ", true, {key:"Z"});
      await gamepads([[0,3], [7,15]]);
      await sample([64|16|1024, 2048|8], [[0xfa,0xbf], [0x7f,0x7f]], "keyboard/gamepad union");
      await key("KeyZ", false);
      await sample([64|16|1024, 2048|8], [[0xfa,0xbf], [0x7f,0x7f]], "shared A stays held by gamepad");
      await gamepads();
      await sample([16, 0], [[0xfe,255], neutral], "keyboard survives gamepad release");
      await key("KeyX", false);
      // Sparse/disconnected API entries must release only their own port.
      await gamepads([[4], [5]]);
      await page.evaluate(async () => {
        window.inputPads[0].connected = false;
        (await import("./app.js")).pollGamepads();
      });
      await sample([0, 256], [neutral, [255,0xef]], "disconnect preserves P2");
      await page.evaluate(async () => {
        window.disconnectedPad = window.inputPads[0]; window.inputPads[0] = null;
        (await import("./app.js")).pollGamepads();
        window.inputPads[0] = window.disconnectedPad; window.inputPads[0].connected = true;
        window.inputPads[0].buttons.forEach(b => { b.pressed=false; b.value=0; });
        (await import("./app.js")).pollGamepads();
      });
      await sample([0, 256], [neutral, [255,0xef]], "neutral reconnect");
      await gamepads();
      // Release paths run with both keyboard and gamepad state asserted. No
      // adapter repoll is made before observing the actual released guest state.
      for (const action of ["canvas-blur", "window-blur", "hidden", "pause", "settings", "view", "restart", "mode"]) {
        await focus(); await key("KeyW"); await key("ShiftRight");
        await gamepads([[0], [1]]);
        await sample([4096|64, 128|16], [[0xfb,0xf7], [0xf6,255]], `${action} held`);
        await page.evaluate(action => {
          const $ = id => document.getElementById(id);
          if (action === "canvas-blur") $("mode").focus();
          if (action === "window-blur") window.dispatchEvent(new Event("blur"));
          if (action === "hidden") {
            Object.defineProperty(document,"hidden",{configurable:true,value:true});
            document.dispatchEvent(new Event("visibilitychange"));
            delete document.hidden;
          }
          if (action === "pause") { if ($("pause").textContent === "Resume") $("pause").click(); $("pause").click(); }
          if (action === "settings") $("settings").open = true;
          if (action === "view") $("view-debug").click();
          if (action === "restart") $("restart").click();
          if (action === "mode") { $("mode").value = $("mode").value === "bios" ? "disc" : "bios"; $("mode").dispatchEvent(new Event("change")); }
        }, action);
        if (action === "settings") await page.waitForFunction(() => document.getElementById("pause").textContent === "Resume", undefined, {polling:10});
        // A held key's autorepeat after returning focus cannot reacquire it.
        await focus(); await key("KeyW", true, {repeat:true});
        await sample([0, 0], [neutral, neutral], `${action} released`);
        await key("KeyW", false); await key("ShiftRight", false);
        await gamepads();
        await page.evaluate(() => { document.getElementById("settings").open=false; document.getElementById("view-play").click(); });
      }
      await page.locator("#mode").selectOption(mode);
      await ready();
      // Input outside the display remains available to normal UI controls.
      await page.locator("#mode").focus();
      assert.equal(await key("KeyZ"), false);
      await sample([0, 0], [neutral, neutral], "unfocused keyboard");
    }
    assert.deepEqual(errors, []);
  } finally { await context.close(); }
}
