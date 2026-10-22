// Original MIT refresh guest, H §§7.5.7/7.10, using the production Wasm.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function refreshArbitration(page) {
  const original = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/refresh.hex", "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => original.writeUInt16BE(word, at + i * 2));
  }
  const replace = (image, before, after) => {
    const needle = Buffer.alloc(4); needle.writeUInt32BE(before);
    const at = image.indexOf(needle);
    assert(at >= 0 && image.indexOf(needle, at + 1) < 0);
    image.writeUInt32BE(after, at);
  };
  const traces = [];
  for (const variant of ["active", "disabled", "operand"]) {
    const image = Buffer.from(original);
    if (variant === "disabled") replace(image, 0xA55A0008, 0xA55A0000);
    if (variant === "operand") replace(image, 0x013579BD, 0x02468ACE);
    const sum = variant === "operand" ? 0x48D159C0 : 0x26AF37A0;
    await page.locator("#bios").setInputFiles({ name: `original-refresh-${variant}.bin`, mimeType: "application/octet-stream", buffer: image });
    await page.waitForFunction(() => document.getElementById("status").textContent.startsWith("Resetting"));
    const run = () => page.evaluate(async () => {
      const { emulator } = await import("./app.js");
      emulator.run_steps(6000);
      return { traces: [[0x0600E400, 7], [0x0600E500, 4]].map(([at, n]) =>
        Array.from({ length: n }, (_, i) => emulator.read_long(at + i * 4))),
      exceptions: [emulator.exception_count(), emulator.slave_exception_count()] };
    });
    const result = await run(), [master, slave] = result.traces;
    assert.deepEqual(result.exceptions, ["0", "0"]);
    assert.deepEqual([master[1], master[2], master[4], master[5], master[6]],
      [sum, 32, sum, variant === "disabled" ? 0 : 0x88, 0x600D]);
    assert.deepEqual(slave.slice(1), [sum, 0, 0x600D]);
    for (const ticks of [master[0], master[3], slave[0]]) assert(ticks > 0 && ticks < 128);
    if (variant !== "disabled") assert(master[3] > master[0] + 4);
    await page.locator("#reset").click();
    assert.deepEqual(await run(), result, `${variant} repeated cold reset`);
    traces.push(result.traces);
  }
  const [active, disabled, changed] = traces;
  assert.equal(active[0][0], disabled[0][0]);
  assert(active[0][3] > disabled[0][3] + 4);
  assert(active[1][0] > disabled[1][0] + 4);
  assert.deepEqual([changed[0][0], changed[0][3], changed[1][0]],
    [active[0][0], active[0][3], active[1][0]]);
  const held = Buffer.from(original);
  assert.equal(held.readUInt16BE(0x4242), 0x2A92);
  held.writeUInt16BE(9, 0x4242); // omit guest self-refresh exit
  const blocked = await page.evaluate(async image => {
    const { emulator } = await import("./app.js");
    emulator.reset_bios(Uint8Array.from(image));
    emulator.run_steps(6000);
    return [emulator.read_long(0x0600E408), emulator.read_long(0x0600E418), emulator.read_long(0x0600E50C)];
  }, [...held]);
  assert.deepEqual(blocked, [0, 0, 0]);

  const monitor = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/monitor.hex", "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at, ...words] = line.split(/\s+/).map(v => parseInt(v, 16));
    words.forEach((word, i) => monitor.writeUInt16BE(word, at + i * 2));
  }
  const intervals = [];
  for (const control of [0x3000, 0x1000]) {
    for (const value of [90, 60]) {
      const image = Buffer.from(monitor);
      replace(image, 0x3000, control);
      const at = image.indexOf(Buffer.from([0xE0, 90]));
      assert(at >= 0 && image.indexOf(Buffer.from([0xE0, 90]), at + 1) < 0);
      image[at + 1] = value;
      const execute = () => page.evaluate(async image => {
        const { emulator } = await import("./app.js");
        emulator.reset_bios(Uint8Array.from(image));
        emulator.run_steps(6000);
        return { trace: [0x0600E600, 0x0600E604, 0x0600E608, 0x0600E700].map(at => emulator.read_long(at)),
          exceptions: [emulator.exception_count(), emulator.slave_exception_count()] };
      }, [...image]);
      const result = await execute();
      assert.deepEqual(result.exceptions, ["0", "0"]);
      assert.deepEqual(result.trace.slice(1), [value * 32, 0x600D, 0x600D]);
      assert(result.trace[0] > 0 && result.trace[0] < 256);
      assert.deepEqual(await execute(), result, "monitor cold reset");
      intervals.push(result.trace[0]);
    }
  }
  assert.equal(intervals[0], intervals[1]);
  assert.equal(intervals[2], intervals[3]);
  assert(intervals[0] > intervals[2] + 4);
  console.log("Refresh/monitor guests passed: owned refresh delays both SH-2s; cached self-refresh exit and bounded missing-exit mutation; external UBC monitoring delays real slave SCI/FRT reads; changed operands and repeated cold resets.");
}
