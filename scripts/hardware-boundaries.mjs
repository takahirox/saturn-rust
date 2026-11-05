// Independent original guest evidence for SH7604 memory and SMPC boundaries.
// The browser uploads local synthetic BIOSes and observes actual Wasm execution.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

async function rom(name) {
  const image = Buffer.alloc(524288);
  for (const line of (await readFile(`tests/fixtures/${name}.hex`, "utf8")).trim().split(/\r?\n/)) {
    if (line.startsWith("#")) continue;
    const [at,...words] = line.split(/\s+/).map(v=>parseInt(v,16));
    words.forEach((word,i)=>image.writeUInt16BE(word,at+i*2));
  }
  return image;
}
async function load(page,name,image) {
  await page.locator("#bios").setInputFiles({name:`original-${name}.bin`,mimeType:"application/octet-stream",buffer:image});
  await page.waitForFunction(() => document.getElementById("status").textContent.startsWith("Resetting"));
}

export async function hardwareBoundaries(page) {
  const byteRom = await rom("smpc-byte-audit");
  const byteExpected = (await readFile("tests/fixtures/smpc-byte-audit.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  let byteReference;
  for (const chunk of [1, 127, 10000]) {
    for (let boot = 0; boot < 2; boot++) {
      await load(page, "smpc-byte-audit", byteRom);
      const result = await page.evaluate(async ({chunk, count}) => {
        const {emulator} = await import("./app.js");
        for (let left = 100000; left > 0;) {
          const n = Math.min(left, chunk); emulator.run_steps(n); left -= n;
          emulator.registers(); emulator.read_long(0x0601b000);
        }
        return {trace: Array.from({length: count}, (_, i) => emulator.read_long(0x0601b000 + i*4)),
          exceptions: emulator.exception_count(), clocks: emulator.peripheral_clocks(), instructions: emulator.instruction_count()};
      }, {chunk, count: byteExpected.length});
      assert.deepEqual(result.trace, byteExpected);
      assert.equal(result.exceptions, "0");
      byteReference ??= result;
      assert.deepEqual(result, byteReference, `byte audit chunk ${chunk}, boot ${boot}`);
    }
  }
  const transactionRom = await rom("smpc-transactions");
  const transactionExpected = (await readFile("tests/fixtures/smpc-transactions.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  let transactionReference;
  for (const chunk of [1, 127, 10000]) {
    await load(page, "smpc-transactions", transactionRom);
    const result = await page.evaluate(async ({chunk, count}) => {
      const {emulator} = await import("./app.js");
      for (let left = 10000; left > 0;) {
        const n = Math.min(left, chunk); emulator.run_steps(n); left -= n;
        emulator.registers(); emulator.read_long(0x0601a000);
        if (emulator.smpc_fault_diagnostics() !== undefined) throw Error("unexpected SMPC fault");
      }
      return {trace: Array.from({length: count}, (_, i) => emulator.read_long(0x0601a000 + i*4)),
        exceptions: emulator.exception_count(), clocks: emulator.peripheral_clocks(), instructions: emulator.instruction_count()};
    }, {chunk, count: transactionExpected.length});
    assert.deepEqual(result.trace, transactionExpected);
    assert.equal(result.exceptions, "1");
    transactionReference ??= result;
    assert.deepEqual(result, transactionReference, `SMPC chunk ${chunk} with Debug observation`);
  }
  for (const [name, reason, direction, width] of [
    ["read", "WriteOnly", "read", 1], ["missing-sf", "MissingSf", "write", 1],
    ["invalid-sf", "InvalidSf", "write", 1], ["busy", "Busy", "write", 1],
    ["command", "UnsupportedCommand", "write", 1], ["parameters", "InvalidParameters", "write", 1],
    ["word", "ByteOnly", "write", 2], ["long", "ByteOnly", "write", 4],
    ["word-read", "ByteOnly", "read", 2], ["long-read", "ByteOnly", "read", 4],
  ]) {
    const image = await rom(`smpc-fault-${name}`);
    let reference;
    for (const chunk of [1, 127, 10000]) {
      await load(page, `smpc-fault-${name}`, image);
      const result = await page.evaluate(async chunk => {
        const {emulator} = await import("./app.js");
        let message;
        for (let i = 0; i < 10000; i++) {
          try { emulator.run_steps(chunk); } catch (error) { message = String(error); break; }
        }
        const clocks = emulator.peripheral_clocks();
        const details = emulator.smpc_fault_diagnostics();
        const before = emulator.read_long(0x0601a200);
        const detailsAgain = emulator.smpc_fault_diagnostics();
        return {message, details, clocks, stationary: clocks === emulator.peripheral_clocks() && details === detailsAgain, before};
      }, chunk);
      assert.match(result.message, new RegExp(`${width}-byte ${direction}.*${reason}`));
      assert.match(result.details, /Master SH-2 PC=0x.*main_clock: Some/);
      assert.match(result.details, /remaining:.*input:/);
      if (name === "busy") assert.match(result.details, /pending: Some\(9\).*remaining: Some/);
      assert.equal(result.stationary, true); assert.equal(result.before, 0);
      reference ??= result; assert.deepEqual(result, reference, `${name}, chunk ${chunk}`);
    }
    await page.locator("#reset").click();
    assert.equal(await page.evaluate(async () => (await import("./app.js")).emulator.smpc_fault_diagnostics()), undefined);
  }
  const commandReport=await rom("smpc-command-report");
  const commandExpected=(await readFile("tests/fixtures/smpc-command-report.tsv","utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
  await load(page,"smpc-command-report",commandReport);
  const runReport=()=>page.evaluate(async count=>{
    const {emulator}=await import("./app.js");emulator.run_steps(10000);
    return {trace:Array.from({length:count},(_,i)=>emulator.read_long(0x0600ED00+i*4)),exceptions:emulator.exception_count()};
  },commandExpected.length);
  const reportResult=await runReport();
  assert.deepEqual(reportResult,{trace:commandExpected,exceptions:"0"});
  await page.locator("#reset").click();assert.deepEqual(await runReport(),reportResult);
  const original = await rom("bsc-memory");
  const vectors = (await readFile("tests/fixtures/bsc-memory.tsv","utf8")).trim().split(/\r?\n/).filter(l=>!l.startsWith("#"));
  for (const line of vectors) {
    const [name,...words] = line.split(/\s+/), values=words.map(v=>parseInt(v,16));
    const image=Buffer.from(original);
    [0xA55A0401,0xA55A0040,0xA55A0000].forEach((before,i)=>{
      const needle=Buffer.alloc(4);needle.writeUInt32BE(before);
      const at=original.indexOf(needle);
      assert(at>=0 && original.indexOf(needle,at+1)<0);
      image.writeUInt32BE(0xA55A0000+values[i],at);
    });
    await load(page,name,image);
    const run=()=>page.evaluate(async()=>{
      const {emulator}=await import("./app.js");
      emulator.run_steps(4000);
      return {traces:[0x0600E800,0x0600E900].map(base=>Array.from({length:6},(_,i)=>emulator.read_long(base+i*4))),
        exceptions:[emulator.exception_count(),emulator.slave_exception_count()]};
    });
    const result=await run();
    assert.deepEqual(result.exceptions,["0","0"],name);
    for (const trace of result.traces) assert.deepEqual(trace,values.slice(3),name);
    await page.locator("#reset").click();
    assert.deepEqual(await run(),result,`${name} cold reset`);
  }
  const mutated=Buffer.from(original), needle=Buffer.from([0x12,0x34,0x56,0]);
  let mutations=0;
  for (let at=original.indexOf(needle);at>=0;at=original.indexOf(needle,at+4)) {
    mutated.writeUInt32BE(0x87654300,at);mutations++;
  }
  assert.equal(mutations,4);
  await load(page,"bsc-memory-mutation",mutated);
  assert.deepEqual(await page.evaluate(async()=>{
    const {emulator}=await import("./app.js");emulator.run_steps(4000);
    return [0x0600E800,0x0600E900].map(base=>[emulator.read_long(base+12),emulator.read_long(base+16),emulator.read_long(base+20)]);
  }),[[0x87654308,0x87654308,0x600D],[0x87654308,0x87654308,0x600D]]);

  const smpc=await rom("smpc-boundaries");
  const expected=(await readFile("tests/fixtures/smpc-boundaries.tsv","utf8")).trim().split(/\r?\n/).map(v=>parseInt(v,16));
  await load(page,"smpc-boundaries",smpc);
  const runSmpc=()=>page.evaluate(async count=>{
    const {emulator}=await import("./app.js");
    emulator.set_buttons(0x11);emulator.set_port2_buttons(0x88);
    emulator.set_3d_pad(1,true,0x12,0x34,0x56,0x78);
    const marker=0x0600EC00+(count-1)*4;
    for (let i=0;i<100;i++) {
      emulator.run_steps(10000);
      if (emulator.read_long(marker)===0x600D) break;
    }
    return {trace:Array.from({length:count},(_,i)=>emulator.read_long(0x0600EC00+i*4)),exceptions:emulator.exception_count()};
  },expected.length);
  const result=await runSmpc();
  assert.equal(result.exceptions,"0");assert.deepEqual(result.trace,expected);
  await page.locator("#reset").click();assert.deepEqual(await runSmpc(),result);
  // Leave the existing synthetic browser gamepad/focus checks a released pad.
  await page.evaluate(async()=>{const {emulator}=await import("./app.js");emulator.set_buttons(0);emulator.set_port2_buttons(0);emulator.set_3d_pad(1,false,128,128,0,0);});
}
