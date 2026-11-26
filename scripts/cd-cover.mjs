// Original reset-vector firmware through the production player and Worker.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { waitForGuest } from "./browser-transport.mjs";

export async function cdCover(browser, url) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const requests = [];
  page.on("request", request => requests.push(request));
  try {
    await page.goto(url);
    const slot = id => page.locator("#" + id);
    const firmware = Buffer.alloc(524288);
    for (const line of (await readFile("tests/fixtures/cd-cover.hex", "utf8")).trim().split("\n")) {
      if (line.startsWith("#")) continue;
      const [address, ...words] = line.split(/\s+/).map(value => parseInt(value, 16));
      words.forEach((word, i) => firmware.writeUInt16BE(word, address + i * 2));
    }
    const file = (name, buffer) => ({ name, mimeType: "application/octet-stream", buffer });
    const first = file("disc-a.iso", Buffer.alloc(18 * 2048, 0xa1));
    const cue = file("disc-b.cue", Buffer.from('FILE "disc-b.bin" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\n'));
    const track = file("disc-b.bin", Buffer.alloc(32 * 2048, 0x52));

    const base = 0x06010000;
    const indices = (start, length) => Array.from({ length }, (_, i) => start + i);
    const read = indices => page.evaluate(async ({ base, indices }) => {
      const { emulator: m } = await import("./app.js");
      return Promise.all(indices.map(i => m.read_long(base + i * 4)));
    }, { base, indices });
    const untilWord = (index, value) => waitForGuest(page, async ({ address, value }) => {
      const { emulator: m } = await import("./app.js");
      return (await m.read_long(address)) === value;
    }, { address: base + index * 4, value });
    const waitText = (id, value) => page.waitForFunction(({ id, value }) =>
      document.getElementById(id).textContent === value, { id, value });
    const consoleState = () => page.evaluate(async () => {
      const { emulator: m } = await import("./app.js");
      const [frames, clocks, pc, backup, rtc] = await Promise.all([
        m.frames(), m.peripheral_clocks(), m.special_registers(), m.export_backup_ram(), m.export_smpc_battery(),
      ]);
      return { frames: Number(frames), clocks: Number(clocks), pc: pc[0], backup: [...backup], rtc: [...rtc] };
    });
    const select = async files => {
      const [picker] = await Promise.all([page.waitForEvent("filechooser"), slot("disc-choose").click()]);
      await picker.setFiles(files);
    };
    const running = async () => assert.equal(await slot("play").textContent(), "Pause");
    const open = async round => {
      assert.equal(await slot("disc-cover").textContent(), "Open cover");
      assert(await slot("disc-choose").isDisabled());
      await slot("disc-cover").click();
      await waitText("disc-cover", "Close cover");
      const start = 4 + 13 * round;
      await untilWord(start + 5, 0x86ff);
      assert.deepEqual(await read(indices(start, 6)), [0x26ff, 0x20, 0, 0x06ff, 0x86ff, 0x86ff]);
    };
    const readDisc = async (round, leadout, marker) => {
      await untilWord(3, round + 1);
      assert.deepEqual(await read(indices(10 + 13 * round, 7)),
        [0x20ff, 0x80ff, 0x80ff, 0x4100, 0x41000096, leadout, marker]);
    };

    await page.waitForFunction(() => !document.getElementById("controls").disabled);
    await slot("bios").setInputFiles(file("original-cover-bios.bin", firmware));
    await slot("play").click();
    await untilWord(2, 0x07ff);
    const before = await consoleState();
    assert.equal(before.backup[0], 0x5a);
    const discoveryRequests = requests.length;
    assert.equal(await slot("disc-state").textContent(), "Cover closed · Empty");

    await open(0);
    await select(first);
    await waitText("disc-selection", first.name);
    assert.equal(await slot("disc-state").textContent(), "Cover open · Disc inserted");
    // Decoder-level rejection must retain the accepted image.
    await select([cue, file("wrong-track.bin", track.buffer)]);
    await page.waitForFunction(() => document.getElementById("disc-error").textContent.includes("Missing CUE file reference"));
    assert.equal(await slot("disc-selection").textContent(), first.name);
    assert.equal(await slot("disc-choose").textContent(), "Change");
    await slot("disc").dispatchEvent("cancel");
    assert.match(await slot("disc-error").textContent(), /cancelled.*try again/i);
    assert.equal(await slot("disc-selection").textContent(), first.name);
    assert.deepEqual(await read([3, 10]), [0, 0], "opening/inserting never completes recognition");
    await running();

    await slot("disc-cover").click();
    await readDisc(0, 168, 0xa1a1a1a1);
    const [heartbeat] = await read([0]);
    await open(1);
    await slot("play").click();
    await waitText("play", "Resume");
    await page.waitForFunction(() => document.querySelector("main").getAttribute("aria-busy") === "false");
    const paused = await consoleState();
    await slot("disc-clear").click();
    await waitText("disc-selection", "No disc selected.");
    await slot("disc-cover").click();
    await waitText("disc-state", "Cover closed · Empty");
    assert.equal(await slot("play").textContent(), "Resume");
    assert.deepEqual(await consoleState(), paused, "paused exchange preserves PC, clocks, saves and RTC");

    await slot("play").click();
    await untilWord(3, 2);
    assert.deepEqual(await read(indices(23, 7)), [0x27ff, 0, 0, 0, 0, 0, 0]);
    await open(2);
    // Reuse the same drop and local multi-file loader used by cold selection.
    await slot("disc-drop").evaluate((zone, entries) => {
      const dataTransfer = new DataTransfer();
      for (const { name, bytes } of entries) dataTransfer.items.add(new File([Uint8Array.from(bytes)], name));
      zone.dispatchEvent(new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer }));
    }, [cue, track].map(({ name, buffer }) => ({ name, bytes: [...buffer] })));
    await waitText("disc-selection", `${cue.name}\n${track.name}`);
    await slot("disc-cover").click();
    await readDisc(2, 182, 0x52525252);
    await running();
    const after = await consoleState();
    const [counter, sentinel] = await read([0, 1]);
    assert(counter > heartbeat);
    assert.equal(sentinel, 0x434f5645);
    assert(after.frames > before.frames && after.clocks > before.clocks);
    assert.deepEqual(after.backup, before.backup);
    assert.deepEqual(after.rtc.slice(7, 12), before.rtc.slice(7, 12));
    assert.equal(requests.length, discoveryRequests, "disc exchange performs no requests");
    console.log("Live CD cover passed in Chromium: BIOS-only play, OPEN/DCHG, ISO picker, invalid/cancelled selection, BUSY/TOC, paused removal, CUE drop and distinct sector reads.");
  } finally {
    await context.close();
  }
}
