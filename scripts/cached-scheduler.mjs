// Original guest bytes, optimized fields versus ordinary one-step scheduling.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

export async function cachedScheduler(page) {
  const cases = [
    ["WRAM conflict data fills", "examples/runtime-loop.hex", "data-cache"],
    ["WRAM disabled data replacement", "examples/runtime-loop.hex", "data-disabled"],
    ["cache/RAM", "examples/runtime-loop.hex", false],
    ["cached registers", "examples/runtime-loop.hex", true],
    ["cached MAC.L operands", "examples/runtime-loop.hex", "mac-cache"],
    ["uncached MAC.L fallback", "examples/runtime-loop.hex", "mac-bypass"],
    ["idle VDP1 command RAM stores", "examples/runtime-loop.hex", "vdp1-idle"],
    ["active VDP1 command RAM stores", "examples/runtime-loop.hex", "vdp1-active"],
    ["DT/BF countdown", "examples/runtime-countdown.hex", false],
    ["countdown zero exit", "examples/runtime-countdown.hex", "dt"],
    ["cached countdown/fast refresh", "examples/runtime-countdown.hex", "refresh-fast"],
    ["cached countdown/slow refresh", "examples/runtime-countdown.hex", "refresh-slow"],
    ["cached countdown/refresh IRQ horizon", "examples/runtime-countdown.hex", "refresh-irq"],
    ["cached countdown/stopped refresh", "examples/runtime-countdown.hex", "refresh-disabled"],
    ["mixed cached RAM/refresh", "examples/runtime-loop.hex", "refresh-fast"],
    ["mixed cached RAM/refresh IRQ horizon", "examples/runtime-loop.hex", "refresh-irq"],
    ["unrolled register division", "examples/runtime-divide.hex", false],
    ["concurrent CPU/rotation/VDP1/effects", "examples/runtime-scene.hex", false],
    ["system/control stack and RTE", "examples/runtime-stack.hex", false],
    ["passive draw status and restart", "examples/runtime-poll.hex", false],
    ["passive CD HIRQ polling with A-bus precharge", "examples/runtime-poll.hex", "cd-hirq"],
    ["CD HIRQ byte fault fallback", "examples/runtime-poll.hex", "cd-byte"],
    ["CD HIRQ long fault fallback", "examples/runtime-poll.hex", "cd-long"],
    ["draw polling with rotation/VDP1/effects", "examples/runtime-poll-scene.hex", false],
    ["draw polling with continuous sound CPU and effects", "examples/runtime-poll-sound-busy-scene.hex", false],
    ["normal bitmap ports with effects", "examples/runtime-normal-scene.hex", false],
    ["GBR RAM wait", "examples/runtime-wait.hex", false],
    ["GBR changed data", "examples/runtime-wait.hex", "gbr"],
    ["uncached ROM division", "examples/runtime-divide.hex", "uncached-rom"],
    ["SH7604/cache/reset", "tests/fixtures/sh7604.hex", false],
    ["dual SH-2", "tests/fixtures/dual-sh2.hex", false],
    ["dual resident polling and buffered RAM writes", "tests/fixtures/dual-poll.hex", false],
    ["multiplier", "tests/fixtures/sh2-multiplier.hex", false],
    ["PLL", "tests/fixtures/clock.hex", false],
    ["VDP1 acquisition", "tests/fixtures/vdp1-command-fetch.hex", false],
    ["VDP1 ownership", "tests/fixtures/vdp1-memory-ownership.hex", false],
    ["cached CPU/VDP1 acquisition", "tests/fixtures/vdp1-command-fetch.hex", "sound-cache"],
    ["cached CPU/VDP1 ownership", "tests/fixtures/vdp1-memory-ownership.hex", "sound-cache"],
    ["cached CPU/rotation scanout", "tests/fixtures/sprite-rotation16.hex", "sound-cache"],
    ["sound CPU/RAM", "tests/fixtures/scsp-cpu-memory.hex", false],
    ["cached CPU/shared sound RAM", "tests/fixtures/scsp-cpu-memory.hex", "sound-cache"],
    ["cached CPU/live PCM", "tests/fixtures/scsp-pcm.hex", "sound-cache"],
    ["cached CPU/live effects DSP", "tests/fixtures/scsp-effects.hex", "sound-cache"],
    ["fault checkpoint", "tests/fixtures/smpc-fault-word-read.hex", false],
  ];
  for (const [name, path, mutate] of cases) {
    const bytes = Buffer.alloc(524288);
    for (const line of (await readFile(path, "utf8")).split(/\r?\n/)) {
      if (!line.trim() || line.startsWith("#")) continue;
      const [at, ...words] = line.trim().split(/\s+/).map(word => parseInt(word, 16));
      words.forEach((word, i) => bytes.writeUInt16BE(word, at + i * 2));
    }
    if (typeof mutate === "string" && mutate.startsWith("refresh-")) {
      assert(bytes.subarray(0x100, 0x180).every(byte => byte === 0));
      const entry = bytes.readUInt32BE(0); bytes.writeUInt32BE(0x100, 0);
      let pc = 0x100, literal = 0x140;
      const emit = word => { bytes.writeUInt16BE(word, pc); pc += 2; };
      const load = (register, value) => {
        const displacement = (literal - ((pc + 4) & ~3)) / 4;
        assert(Number.isInteger(displacement) && displacement >= 0 && displacement < 256);
        emit(0xD000 | register << 8 | displacement);
        bytes.writeUInt32BE(value, literal); literal += 4;
      };
      // Original keyed register writes, then the unchanged cached DT/BF guest.
      for (const [address, value] of [
        [0xFFFF_FFEC, 0xA55A_0048],
        [0xFFFF_FFF8, mutate === "refresh-slow" ? 0xA55A_007F : 0xA55A_0007],
        [0xFFFF_FFF4, 0xA55A_0000],
        [0xFFFF_FFF0, mutate === "refresh-disabled" ? 0xA55A_0000 :
          mutate === "refresh-irq" ? 0xA55A_0048 : 0xA55A_0008],
      ]) { load(1, address); load(0, value); emit(0x2102); }
      load(1, entry); emit(0x412B); emit(0x0009);
      assert(pc <= 0x140 && literal <= 0x180);
    }
    if (typeof mutate === "string" && mutate.startsWith("cd-")) {
      assert(bytes.subarray(0x100, 0x140).every(byte => byte === 0));
      const entry = bytes.readUInt32BE(0);
      bytes.writeUInt32BE(0x100, 0);
      // Original guest configures SCU CS2 precharge before polling HIRQ.
      [0xD107, 0xD008, 0x2102, 0xD108, 0x412B, 0x0009]
        .forEach((word, i) => bytes.writeUInt16BE(word, 0x100 + i * 2));
      [0x25FE00B4, 0x60000000, entry]
        .forEach((word, i) => bytes.writeUInt32BE(word, 0x120 + i * 4));
      bytes.writeUInt32BE(0x25890008, 0x224);
      const load = mutate === "cd-byte" ? 0x6040 : mutate === "cd-long" ? 0x6042 : 0x6041;
      [load, load, 0x7801, 0xAFFB, 0x0009]
        .forEach((word, i) => bytes.writeUInt16BE(word, 0x20C + i * 2));
    }
    if (mutate === "data-cache" || mutate === "data-disabled") {
      bytes.fill(0,0x200,0x400);
      let pc=0x200, literal=0x300;
      const emit=word=>{bytes.writeUInt16BE(word,pc);pc+=2;};
      const load=(register,value)=>{
        const displacement=(literal-((pc+4)&~3))/4;
        assert(Number.isInteger(displacement) && displacement>=0 && displacement<256);
        emit(0xD000 | register<<8 | displacement);
        bytes.writeUInt32BE(value,literal);literal+=4;
      };
      load(1,0xFFFFFE92);emit(mutate === "data-disabled" ? 0xE005 : 0xE001);emit(0x2100);
      for(let i=0;i<8;i++) {
        load(1,0x26020000+i*0x400);load(0,3+i*17);emit(0x2102);
      }
      const loop=pc;
      for(let i=0;i<8;i++) {load(1,0x06020000+i*0x400);emit(0x6012);emit(0x330C);}
      emit(0xA000 | (((loop-(pc+4))/2)&0xFFF));emit(0x0009);
      assert(pc<=0x300 && literal<=0x400);
    }
    if (mutate === "mac-cache" || mutate === "mac-bypass") {
      // Original nonzero operands, then reload pointers for every MAC.L.
      // The bypass variant must retain the ordinary external-read path.
      bytes.fill(0,0x200,0x300);
      let pc=0x200, literal=0x280;
      const emit=word=>{bytes.writeUInt16BE(word,pc);pc+=2;};
      const load=(register,value)=>{
        const displacement=(literal-((pc+4)&~3))/4;
        assert(Number.isInteger(displacement) && displacement>=0 && displacement<256);
        emit(0xD000 | register<<8 | displacement);
        bytes.writeUInt32BE(value,literal);literal+=4;
      };
      const address=mutate==='mac-bypass' ? 0x2602000C : 0x0602000C;
      load(1,0xFFFFFE92);emit(0xE001);emit(0x2100);
      for(const [at,value] of [[address,3],[address+4,7]]) {
        load(1,at);load(0,value);emit(0x2102);
      }
      const loop=pc;
      load(1,address);load(2,address+4);emit(0x012F);
      emit(0x001A);emit(0x040A);
      emit(0xA000 | (((loop-(pc+4))/2)&0xFFF));emit(0x0009);
      assert(pc<=0x280 && literal<=0x300);
    }
    if (mutate === "sound-cache") {
      assert(bytes.subarray(0x180, 0x198).every(byte => byte === 0));
      const entry = bytes.readUInt32BE(0); bytes.writeUInt32BE(0x180, 0);
      [0xD103, 0xE001, 0x2100, 0xD103, 0x412B, 0x0009].forEach((word, i) => bytes.writeUInt16BE(word, 0x180+i*2));
      bytes.writeUInt32BE(0xFFFFFE92, 0x190); bytes.writeUInt32BE(entry, 0x194);
    }
    if (mutate === "vdp1-idle" || mutate === "vdp1-active") {
      bytes.writeUInt32BE(0x05C02000, 0x224);
      if (mutate === "vdp1-active") {
        assert(bytes.subarray(0x180, 0x1B0).every(byte => byte === 0));
        const entry = bytes.readUInt32BE(0); bytes.writeUInt32BE(0x180, 0);
        // Original skip-and-jump command, activated before the cache loop.
        [0xD107, 0xD008, 0x2101, 0xD108, 0xE001, 0x2101, 0xD107, 0x412B, 0x0009]
          .forEach((word, i) => bytes.writeUInt16BE(word, 0x180 + i * 2));
        [0x05C00000, 0x00005000, 0x05D00004, entry]
          .forEach((word, i) => bytes.writeUInt32BE(word, 0x1A0 + i * 4));
      }
    }
    if (mutate === "dt") bytes.writeUInt32BE(3, 0x244);
    if (mutate === "gbr") bytes.writeUInt32BE(9, 0x248);
    if (mutate === "uncached-rom") bytes.writeUInt16BE(0x0009, 0x204);
    if (mutate === true) {
      bytes.writeUInt16BE(0x7007, 0x20A);
      bytes.writeUInt16BE(0x0009, 0x20C);
    }
    const result = await page.evaluate(async bytes => {
      const { Emulator } = await import("./pkg/saturn_rust.js");
      const fast = new Emulator(), reference = new Emulator();
      try {
        for (const m of [fast, reference]) m.reset_bios(Uint8Array.from(bytes));
        const outcomes = [];
        for (let field = 1; field <= 2; field++) {
          let fastError = null, referenceError = null;
          try { fast.advance_frame(); } catch (error) { fastError = String(error); }
          try {
            let steps = 0;
            while (Number(reference.frames()) < field && steps++ < 1000000) reference.run_steps(1);
            if (Number(reference.frames()) < field) throw new Error("reference frame exceeded original step bound");
          } catch (error) { referenceError = String(error); }
          const state = m => ({ frames: m.frames(), clocks: m.peripheral_clocks(), status: m.status(),
            registers: [...m.registers()], special: [...m.special_registers()], instructions: m.instruction_count(), exceptions: m.exception_count(),
            slave: [...m.slave_registers()], slavePc: m.slave_pc(), slaveInstructions: m.slave_instruction_count(),
            ram: Array.from({ length: 4096 }, (_, i) => m.read_long(0x06000000 + i * 4)), counter: m.read_long(0x06020000),
            commandRam: Array.from({ length: 16 }, (_, i) => m.read_word(0x05C02000 + i * 2)),
            soundPc: m.sound_pc(), soundInstructions: m.sound_instruction_count(), soundRegisters: [...m.sound_registers()],
            dsp: Array.from({ length: 1008 }, (_, i) => m.read_word(0x05B00700 + i * 2)),
            refresh: [0xFFFF_FFF0, 0xFFFF_FFF4, 0xFFFF_FFF8].map(at => m.read_long(at)),
            pixels: [...m.framebuffer()], audio: [...m.audio_samples(8192)] });
          outcomes.push({ fast: state(fast), reference: state(reference), fastError, referenceError });
          if (fastError || referenceError) break;
        }
        return outcomes;
      } finally { fast.free(); reference.free(); }
    }, [...bytes]);
    for (const [field, resultAtField] of result.entries()) {
      assert.equal(resultAtField.fastError, resultAtField.referenceError, `${name}, field ${field + 1}: fault parity`);
      assert.deepEqual(resultAtField.fast, resultAtField.reference, `${name}, field ${field + 1}: scheduler parity`);
    }
  }
  console.log(`Cached scheduler: ${cases.length} original CPU/RAM/loop, sound, video, fault and PLL cases match ordinary steps in production Wasm.`);
}
