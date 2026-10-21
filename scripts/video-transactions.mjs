// Original synthetic guest execution through the production Wasm and Canvas.
// Expected pixels/registers are authored from ST-013 §§2/4 and ST-058 §§2–4.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

async function rom(name) {
  const bytes = Buffer.alloc(524288);
  for (const line of (await readFile(`tests/fixtures/${name}.hex`, "utf8")).split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const [address, ...words] = line.split(/\s+/);
    let at = parseInt(address, 16);
    for (const word of words) { bytes.writeUInt16BE(parseInt(word, 16), at); at += 2; }
  }
  return bytes;
}
function patch(bytes, before, after, count = 1) {
  const changed = Buffer.from(bytes), locations = [];
  for (let i = 0; i <= changed.length - 4; i++) if (changed.readUInt32BE(i) === before) locations.push(i);
  assert.equal(locations.length, count);
  for (const i of locations) changed.writeUInt32BE(after, i);
  return changed;
}
async function run(page, bytes, base, count, pixels = false) {
  return page.evaluate(async ({ bytes, base, count, pixels }) => {
    const { emulator, advanceFrame } = await import("./app.js");
    emulator.set_pal(false);
    emulator.reset_bios(Uint8Array.from(bytes));
    for (let i = 0; i < 100 && emulator.read_long(base + (count - 1) * 4) !== 0x600D; i++) emulator.run_steps(5000);
    const records = Array.from({ length: count }, (_, i) => emulator.read_long(base + i * 4));
    if (!pixels) return { records };
    advanceFrame();
    const f = emulator.framebuffer(), width = emulator.video_width(), height = emulator.video_height();
    const canvas = document.getElementById("screen").getContext("2d");
    return { records, width, height, length: f.length,
      rows: [0, 479].map(y => Array.from(f.slice(y * width * 4, y * width * 4 + 16))),
      canvas: [0, 479].map(y => Array.from(canvas.getImageData(0, y, 4, 1).data)) };
  }, { bytes: [...bytes], base, count, pixels });
}
export async function videoTransactions(page) {
  const draw = await rom("vdp1-progress");
  const expected = (await readFile("tests/fixtures/vdp1-progress.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v,16));
  for (let repeat = 0; repeat < 2; repeat++) assert.deepEqual((await run(page, draw, 0x0600D800,10)).records, expected);
  for (const [before,after,indices] of [[0x801F,0x83E0,[0]],[0x1234,0x2468,[6,7]]]) {
    const e = [...expected]; for (const i of indices) e[i] = after;
    assert.deepEqual((await run(page, patch(draw,before,after),0x0600D800,10)).records,e);
  }
  const monitor = await rom("exclusive-monitor");
  const colors = [255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255];
  for (const [mode,width] of [[0x8004,320],[0x8005,352],[0x8006,640],[0x8007,704]]) {
    for (let repeat = 0; repeat < 2; repeat++) {
      const r = await run(page,patch(monitor,0x8006,mode),0x0600DC00,5,true);
      assert.deepEqual(r.records,[8,0,480,mode,0x600D]);
      assert.deepEqual([r.width,r.height,r.length],[width,480,width*480*4]);
      assert.deepEqual(r.rows,[colors,colors]); assert.deepEqual(r.canvas,r.rows);
    }
  }
  const changed = await run(page,patch(monitor,0x25F80080,0x25F80084,2),0x0600DC00,5,true);
  assert.deepEqual(changed.rows[0],[255,0,0,255,0,0,0,255,0,0,255,255,0,0,0,255]);
  assert.deepEqual(changed.canvas,changed.rows);
  const grants = await rom("vram-grants");
  for (let repeat = 0; repeat < 2; repeat++) {
    const { records: r } = await run(page,grants,0x0600DE00,7);
    assert.equal(r[6],0x600D);
    for (const i of [0,2,4]) assert(r[i] > 0 && r[i] < 60, `${r}`);
    for (const i of [1,3,5]) assert(r[i] >= 80 && r[i] < 220, `${r}`);
  }
  const { records: released } = await run(page,patch(grants,0x25F80020,0x25F80022),0x0600DE00,7);
  assert.equal(released[6],0x600D);
  for (const value of released.slice(0,6)) assert(value > 0 && value < 60, `${released}`);
  console.log("Video transactions passed: partial VDP1 dots/ENDR/status, bounded VBlank erase and operand changes; all four exclusive monitor modes, 480-line pixels/counters/Wasm/Canvas and NBG1 mutation; split VRAM grants, HBlank stalls, ignored high-resolution slots and disabled-screen release.");
}
