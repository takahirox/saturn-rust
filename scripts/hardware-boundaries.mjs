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
  // Both backends assert this same checked-in record. Separate first-run
  // references cannot detect a consistent native/Wasm timing disagreement.
  const progress = new Map((await readFile("tests/fixtures/smpc-progress.tsv", "utf8"))
    .trim().split(/\r?\n/).filter(line => !line.startsWith("#"))
    .map(line => { const [name, ...counts] = line.split("\t"); return [name, counts]; }));
  function assertProgress(name, turns, result) {
    assert.deepEqual([String(turns), result.clocks, result.instructions, result.exceptions],
      progress.get(name), `${name}: Wasm progress must match the shared native oracle`);
  }
  // Real SH-2 reset-vector guests configure DMAC/cache with instructions.
  // DMA errors enter vector10; the handler records AE, remaining count,
  // saved PC/SR and untouched destination. A deliberate COMREG byte read
  // after completed probes exposes hidden SMPC state through the existing
  // diagnostic, distinct from the DMA error or the cache refill rejection.
  for (const name of ["dma", "dma-ram", "cache", "cache-through", "cache-disabled"]) {
    const image = await rom(`smpc-read-${name}`);
    let reference;
    for (const chunk of [1, 3, 127, 10000]) {
      for (const observe of [false, true]) {
        const result = await page.evaluate(async ({image, chunk, observe}) => {
          const {Emulator} = await import("./pkg/saturn_rust.js");
          const emulator = new Emulator();
          try {
            emulator.reset_bios(Uint8Array.from(atob(image), c => c.charCodeAt(0)));
            let message;
            for (let left = 10000; left > 0;) {
              const n = Math.min(left, chunk);
              try { emulator.run_steps(n); } catch (e) { message = String(e); }
              left -= n;
              if (observe) {
                emulator.registers(); emulator.smpc_fault_diagnostics();
                emulator.read_long(0x0601f000);
                // Debug must not acknowledge DMAOR.AE or CHCR.TE.
                emulator.read_long(0xffffffb0); emulator.read_long(0xffffff8c);
              }
              if (message) break;
            }
            const clocks = emulator.peripheral_clocks();
            const details = emulator.smpc_fault_diagnostics();
            let repeated;
            try { emulator.run_steps(1); } catch (e) { repeated = String(e); }
            return {message, repeated, details,
              unchanged: details === emulator.smpc_fault_diagnostics(),
              clocks, stationary: clocks === emulator.peripheral_clocks(),
              instructions: emulator.instruction_count(), exceptions: emulator.exception_count(),
              lastException: emulator.last_exception(),
              registers: Array.from(emulator.registers()), special: Array.from(emulator.special_registers()),
              trace: Array.from({length: 20}, (_, i) => emulator.read_long(0x0601f000 + i*4)),
              sentinel: emulator.read_long(0x0601f080)};
          } finally { emulator.free(); }
        }, {image: image.toString("base64"), chunk, observe});
        assertProgress(`smpc-read-${name}`, 0, result);
        assert.equal(result.repeated, result.message);
        assert.equal(result.stationary, true); assert.equal(result.unchanged, true);
        assert.equal(result.sentinel, 0);
        assert.match(result.details, /sf: true, busy: false, command: 0, pending: None, remaining: None/);
        assert.match(result.details, /await_continue: false, await_vblank: false, peripheral_phase: false/);
        assert.match(result.details, /input: \[18, 52, 86, 120, 154, 188, 222\], status: 0, command_report: 0/);
        assert.match(result.details, new RegExp(`main_clock: Some\\(${result.clocks}\\), instruction: Some\\(24592\\)`));
        const expected = [1, 0, 0];
        if (name === "cache") {
          assert.match(result.message, /PC 0x20001000.*1-byte read at 0x00100021.*cache refill: 4-byte read at 0x00100024.*ByteOnly/);
          assert.match(result.details, /Master SH-2 PC=0x20001000.*address: 1048609, width: 1, write: None, reason: ByteOnly/);
          assert.match(result.details, /refill: Some\(ReadUnit \{ address: 1048612, width: 4 \}\)/);
          assert.equal(result.special[0], 0x20001000);
        } else {
          assert.match(result.message, /1-byte read at 0x2010001F.*WriteOnly/);
          const pc = name.startsWith("dma") ? "200018E8" : "20001078";
          assert.match(result.details, new RegExp(`Master SH-2 PC=0x${pc}.*address: 537919519, width: 1, write: None, reason: WriteOnly`));
          assert.match(result.details, /refill: None/);
          assert.equal(result.special[0], parseInt(pc, 16));
          if (name === "dma") {
            expected.push(10, 0x20000444, 0xf0, 5, 0x00100020, 0x2601f100, 4, 0x4e11,
              1, 0, 0, ...Array(4).fill(0xa5a5a5a5), 0x600d);
            assert.equal(result.lastException, "Vector 10 · saved PC 0x20000444");
          } else if (name === "dma-ram") {
            expected.push(0, 1, 0x2601f210, 0x2601f110, 0, 0x4e13, 1, 0, 0,
              0x12345678, 0x89abcdef, 0x13579bdf, 0x2468ace0, 0x600d);
          } else {
            expected.push(0, 1, 0, 0, 0x600d);
          }
        }
        if (name !== "dma") assert.equal(result.lastException, "none");
        assert.deepEqual(result.trace, expected.concat(Array(20 - expected.length).fill(0)));
        reference ??= result;
        assert.deepEqual(result, reference, `${name}, chunk ${chunk}, observe ${observe}`);
      }
    }
  }
  // H §§4.8.2–3 exception cycles retain their full operand width even for
  // unaligned SP/VBR. Byte splitting must not mutate IREGs or read SR/SF.
  for (const name of ["stack", "vector"]) {
    const image = await rom(`smpc-exception-${name}`);
    let reference;
    for (const chunk of [1, 3, 127, 10000]) {
      for (const observe of [false, true]) {
        const result = await page.evaluate(async ({image, chunk, observe}) => {
          const {Emulator} = await import("./pkg/saturn_rust.js");
          const emulator = new Emulator();
          try {
            emulator.reset_bios(Uint8Array.from(atob(image), c => c.charCodeAt(0)));
            let message;
            for (let i = 0; i < 1000; i++) {
              try { emulator.run_steps(chunk); } catch (e) { message = String(e); }
              if (observe) { emulator.registers(); emulator.read_long(0x0601a200); emulator.smpc_fault_diagnostics(); }
              if (message) break;
            }
            const clocks = emulator.peripheral_clocks();
            let repeated;
            try { emulator.run_steps(1); } catch (e) { repeated = String(e); }
            return {message, repeated, details: emulator.smpc_fault_diagnostics(),
              clocks, stationary: clocks === emulator.peripheral_clocks(),
              instructions: emulator.instruction_count(), exceptions: emulator.exception_count(),
              sentinel: emulator.read_long(0x0601a200),
              frame: [emulator.read_long(0x060feff8), emulator.read_long(0x060feffc)]};
          } finally { emulator.free(); }
        }, {image: image.toString("base64"), chunk, observe});
        assert.match(result.message, /Exception vector 32.*4-byte (read|write).*ByteOnly/);
        assert.match(result.message, name === "stack" ? /write at 0x20100005.*value=0x000000F0/i : /read at 0x20100061/);
        assert.match(result.details, /Master SH-2 PC=0x.*width: 4.*ByteOnly/);
        assert.match(result.details, /instruction: Some\(49952\)/); // TRAPA #32 (C320)
        assert.match(result.details, /input: \[0, 0, 0, 0, 0, 0, 0\]/);
        assert.equal(result.repeated, result.message);
        assert.equal(result.stationary, true);
        assert.equal(result.exceptions, "0");
        assertProgress(`smpc-exception-${name}`, 0, result);
        assert.equal(result.sentinel, 0);
        assert.deepEqual(result.frame, name === "vector" ? [0x210, 0xf0] : [0, 0]);
        reference ??= result;
        assert.deepEqual(result, reference, `${name}, chunk ${chunk}, observe ${observe}`);
      }
    }
  }
  // Table2.5: these are real dual-SH-2 guests. Master releases slave with
  // SSHON; slave issues COMREG via the production scheduler/write buffer.
  const issuerProgress = new Map((await readFile("tests/fixtures/smpc-issuer-progress.tsv", "utf8"))
    .trim().split(/\r?\n/).filter(line => !line.startsWith("#"))
    .map(line => { const [name, ...counts] = line.split("\t"); return [name, counts]; }));
  const issuerExpected = (await readFile("tests/fixtures/smpc-issuer-allowed.tsv", "utf8"))
    .trim().split(/\r?\n/).map(v => parseInt(v, 16));
  for (const name of ["allowed", "02", "03", "0e", "0f", "10"]) {
    const image = await rom(`smpc-issuer-${name}`);
    let reference;
    for (const chunk of [1, 3, 127, 10000]) {
      for (const observe of [false, true]) {
        const result = await page.evaluate(async ({image, chunk, observe, count}) => {
          const {Emulator} = await import("./pkg/saturn_rust.js");
          const emulator = new Emulator();
          try {
            emulator.reset_bios(Uint8Array.from(atob(image), c => c.charCodeAt(0)));
            let message;
            for (let left = 250000; left > 0;) {
              const n = Math.min(left, chunk);
              try { emulator.run_steps(n); } catch (e) { message = String(e); break; }
              left -= n;
              if (observe) { emulator.registers(); emulator.slave_registers(); emulator.read_long(0x0601e100); emulator.smpc_fault_diagnostics(); }
            }
            const details = emulator.smpc_fault_diagnostics();
            const clocks = emulator.peripheral_clocks();
            let repeated;
            if (message) { try { emulator.run_steps(1); } catch (e) { repeated = String(e); } }
            return {message, details, repeated, stationary: clocks === emulator.peripheral_clocks(),
              clocks, instructions: emulator.instruction_count(), slave: emulator.slave_instruction_count(),
              exceptions: emulator.exception_count(), slaveExceptions: emulator.slave_exception_count(),
              masterTrace: Array.from({length: count}, (_, i) => emulator.read_long(0x0601e000 + i*4)),
              slaveTrace: Array.from({length: 11}, (_, i) => emulator.read_long(0x0601e100 + i*4)),
              done: emulator.read_long(0x0601e204)};
          } finally { emulator.free(); }
        }, {image: image.toString("base64"), chunk, observe, count: issuerExpected.length});
        assert.deepEqual([result.clocks, result.instructions, result.slave], issuerProgress.get(name), `${name}: shared native/Wasm issuer progress`);
        assert.equal(result.exceptions, "0"); assert.equal(result.slaveExceptions, "0");
        assert.equal(result.stationary, true);
        if (name === "allowed") {
          assert.equal(result.message, undefined);
          assert.equal(result.details, undefined);
          assert.deepEqual(result.masterTrace, issuerExpected);
          assert.deepEqual(result.slaveTrace, [1, 1, 8, 0, 0x17, 1, 1, 8, 0, 0x16, 0x600d]);
          assert.equal(result.done, 1);
        } else {
          assert.match(result.message, /Slave SH-2:.*1-byte write.*ProhibitedIssuer/);
          assert.match(result.message, new RegExp(`value=0x${parseInt(name, 16).toString(16).padStart(8, "0")}`, "i"));
          assert.equal(result.repeated, result.message);
          assert.match(result.details, /Slave SH-2 PC=0x.*ProhibitedIssuer/);
          assert.match(result.details, /sf: true, busy: false, command: 23, pending: None, remaining: None/);
          assert.match(result.details, /input: \[1, 0, 240/);
          assert.deepEqual(result.slaveTrace, [1, 1, 8, 0, 0x17, 1, 1, 8, 0, 0, 0]);
          assert.equal(result.done, 0);
        }
        reference ??= result;
        assert.deepEqual(result, reference, `${name}, chunk ${chunk}, observe ${observe}`);
      }
    }
  }
  const observerRom = await rom("smpc-reset-observer");
  const resetExpected = (await readFile("tests/fixtures/smpc-reset-observer.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  let resetReference;
  for (const command of ["17", "16", "10"]) {
    const producer = await rom(`smpc-reset-pending-${command}`);
    for (const extra of [0, 1, 7]) {
      for (const chunk of [1, 3, 127, 10000]) {
        for (const observe of [false, true]) {
          const result = await page.evaluate(async ({chunk, extra, observer, producer, count, command, observe}) => {
            // Each case owns a fresh production machine, as in native tests.
            // Cold resets retain battery state; separate cases must not share RTC time.
            const {Emulator} = await import("./pkg/saturn_rust.js");
            const emulator = new Emulator();
            try {
              emulator.reset_bios(Uint8Array.from(atob(producer), c => c.charCodeAt(0)));
              for (let i = 0; i < 100000 && emulator.read_long(0x0601a200) !== 0x55000000; i++) emulator.run_steps(1);
              if (emulator.read_long(0x0601a200) !== 0x55000000) throw Error("pending command mailbox absent");
              if (emulator.read_long(0x0601a00c) !== 1 || emulator.read_long(0x0601a010) !== parseInt(command, 16)) throw Error("guest did not observe the pending command");
              if (extra) emulator.run_steps(extra);
              // Debug status is read by the guest; reset uses only production API.
              // No fault stops execution before or after reset.
              emulator.reset_bios(Uint8Array.from(atob(observer), c => c.charCodeAt(0)));
              for (let left = 100000; left > 0;) {
                const n = Math.min(left, chunk); emulator.run_steps(n); left -= n;
                if (observe) {
                  emulator.registers(); emulator.read_long(0x0601c000);
                  if (emulator.smpc_fault_diagnostics() !== undefined) throw Error("reset observer fault");
                }
              }
              return {trace: Array.from({length: count}, (_, i) => emulator.read_long(0x0601c000 + i*4)),
                exceptions: emulator.exception_count(), clocks: emulator.peripheral_clocks(), instructions: emulator.instruction_count()};
            } finally { emulator.free(); }
          }, {chunk, extra, command, observe, producer: producer.toString("base64"), observer: observerRom.toString("base64"), count: resetExpected.length});
          assert.deepEqual(result.trace, resetExpected, `cancel ${command}, offset ${extra}, chunk ${chunk}`);
          assert.equal(result.exceptions, "0");
          assert(BigInt(result.clocks) > 108600n);
          assertProgress("smpc-reset-observer", 100000, result);
          resetReference ??= result;
          assert.deepEqual(result, resetReference);
        }
      }
    }
  }
  const byteRom = await rom("smpc-byte-audit");
  const byteExpected = (await readFile("tests/fixtures/smpc-byte-audit.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  let byteReference;
  for (const chunk of [1, 3, 127, 10000]) {
    for (const observe of [false, true]) {
      for (let boot = 0; boot < 2; boot++) {
        await load(page, "smpc-byte-audit", byteRom);
        const result = await page.evaluate(async ({chunk, count, observe}) => {
          const {emulator} = await import("./app.js");
          for (let left = 2000000; left > 0;) {
            const n = Math.min(left, chunk); emulator.run_steps(n); left -= n;
            if (observe) {
              emulator.registers(); emulator.read_long(0x0601b000);
              if (emulator.smpc_fault_diagnostics() !== undefined) throw Error("byte audit fault");
            }
          }
          return {trace: Array.from({length: count}, (_, i) => emulator.read_long(0x0601b000 + i*4)),
            exceptions: emulator.exception_count(), clocks: emulator.peripheral_clocks(), instructions: emulator.instruction_count()};
        }, {chunk, observe, count: byteExpected.length});
        assert.deepEqual(result.trace, byteExpected);
        assertProgress("smpc-byte-audit", 2000000, result);
        assert.equal(result.exceptions, "0");
        byteReference ??= result;
        assert.deepEqual(result, byteReference, `byte audit chunk ${chunk}, boot ${boot}, observe ${observe}`);
      }
    }
  }
  const timedRom = await rom("smpc-timed-peripherals");
  const timedExpected = (await readFile("tests/fixtures/smpc-timed-peripherals.tsv", "utf8")).trim().split(/\r?\n/).map(v => parseInt(v, 16));
  let timedReference;
  for (const chunk of [1, 3, 127, 10000]) {
    for (const observe of [false, true]) {
      await load(page, "smpc-timed-peripherals", timedRom);
      const result = await page.evaluate(async ({chunk, observe, count}) => {
        const {emulator} = await import("./app.js");
        for (let left = 500000; left > 0;) {
          const n = Math.min(left, chunk); emulator.run_steps(n); left -= n;
          if (observe) { emulator.registers(); emulator.read_long(0x0601d000); emulator.smpc_fault_diagnostics(); }
        }
        return {trace: Array.from({length: count}, (_, i) => emulator.read_long(0x0601d000 + i*4)),
          exceptions: emulator.exception_count(), clocks: emulator.peripheral_clocks(), instructions: emulator.instruction_count()};
      }, {chunk, observe, count: timedExpected.length});
      assert.deepEqual(result.trace, timedExpected);
      assertProgress("smpc-timed-peripherals", 500000, result);
      assert.equal(result.exceptions, "6");
      timedReference ??= result;
      assert.deepEqual(result, timedReference, `raster-gated INTBACK chunk ${chunk}, observe ${observe}`);
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
    ["comreg-word-read", "ByteOnly", "read", 2],
  ]) {
    const image = await rom(`smpc-fault-${name}`);
    let reference;
    for (const chunk of [1, 127, 10000]) {
      for (const observe of (name === "read" || name === "comreg-word-read" ? [false, true] : [true])) {
        await load(page, `smpc-fault-${name}`, image);
        const result = await page.evaluate(async ({chunk, observe}) => {
          const {emulator} = await import("./app.js");
          let message;
          for (let i = 0; i < 10000; i++) {
            try { emulator.run_steps(chunk); } catch (error) { message = String(error); }
            if (observe) { emulator.registers(); emulator.read_long(0x0601a200); emulator.smpc_fault_diagnostics(); }
            if (message) break;
          }
          const clocks = emulator.peripheral_clocks();
          const details = emulator.smpc_fault_diagnostics();
          const before = emulator.read_long(0x0601a200);
          const detailsAgain = emulator.smpc_fault_diagnostics();
          let repeated;
          try { emulator.run_steps(1); } catch (error) { repeated = String(error); }
          return {message, repeated, details, clocks, stationary: clocks === emulator.peripheral_clocks() && details === detailsAgain, before,
            operand: emulator.registers()[1], exceptions: emulator.exception_count()};
        }, {chunk, observe});
        assert.match(result.message, new RegExp(`${width}-byte ${direction}.*${reason}`));
        assert.match(result.details, /Master SH-2 PC=0x.*main_clock: Some/);
        assert.match(result.details, /remaining:.*input:/);
        if (name === "busy") assert.match(result.details, /pending: Some\(9\).*remaining: Some/);
        if (name === "read" || name === "comreg-word-read") {
          const word = name === "comreg-word-read";
          const address = word ? 0x0010001e : 0x0010001f;
          assert.match(result.message, new RegExp(`PC 0x0000020C.*${width}-byte read at 0x${address.toString(16).toUpperCase().padStart(8, "0")}: ${reason}`));
          assert.match(result.details, new RegExp(`address: ${address}, width: ${width}, write: None, reason: ${reason}`));
          assert.match(result.details, new RegExp(`instruction: Some\\(${word ? 0x6011 : 0x6010}\\)`));
          assert.match(result.details, /sf: false, busy: false, command: 0, pending: None, remaining: None/);
          assert.equal(result.operand, address); assert.equal(result.exceptions, "0");
        }
        assert.equal(result.repeated, result.message);
        assert.equal(result.stationary, true); assert.equal(result.before, 0);
        reference ??= result; assert.deepEqual(result, reference, `${name}, chunk ${chunk}, observe ${observe}`);
      }
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
