import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { runInNewContext } from "node:vm";

import { AudioOutput } from "../web/audio.js";
import { PlaybackClient } from "../web/playback-client.js";

const worklet = readFileSync(new URL("../web/audio-worklet.js", import.meta.url), "utf8");

async function audioHarness() {
  const incoming = [], outgoing = [];
  let Processor;
  class AudioWorkletProcessor {
    constructor() { this.port = { postMessage: message => outgoing.push(message) }; }
  }
  runInNewContext(worklet, { AudioWorkletProcessor, sampleRate: 44100,
    registerProcessor: (_, ctor) => { Processor = ctor; } });
  const processor = new Processor();
  globalThis.AudioContext = class {
    constructor() { this.state = "running"; this.audioWorklet = { addModule: async () => {} }; }
    async resume() { this.state = "running"; this.onstatechange?.(); }
  };
  globalThis.AudioWorkletNode = class {
    constructor() { this.port = { postMessage: message => incoming.push(message) }; }
    connect() {}
  };
  const output = new AudioOutput();
  await output.enable();
  function flush() {
    while (incoming.length) processor.port.onmessage({ data: incoming.shift() });
    while (outgoing.length) output.node.port.onmessage({ data: outgoing.shift() });
  }
  function render(frames) {
    const target = [[new Float32Array(frames), new Float32Array(frames)]];
    assert(processor.process([], target));
    flush();
    return target[0];
  }
  flush();
  return { output, processor, flush, render };
}

function stereo(frames, sample) {
  const pcm = new Int16Array(frames * 2);
  for (let index = 0; index < pcm.length; index += 2) {
    pcm[index] = sample;
    pcm[index + 1] = -sample;
  }
  return pcm;
}

function assertStereo([left, right], expected) {
  assert(left.every(sample => sample === expected));
  assert(right.every(sample => sample === -expected));
}

test("the actual Worklet waits for enough complete PCM frames and keeps FIFO order", async () => {
  const { output, processor, flush, render } = await audioHarness();
  assert(output.start(stereo(8192, 8192)));
  // The first write is still in flight when the player asks for more room.
  let arrived = false;
  const next = Promise.resolve(output.deliverPaced(stereo(512, 16384)))
    .then(result => { arrived = true; return result; });
  flush();
  assert.equal(processor.length, 8192);
  assert.equal(output.capacityWaits, 1);
  for (let i = 0; i < 3; i++) assertStereo(render(128), 0.25);
  await Promise.resolve();
  assert.equal(arrived, false);
  assert.equal(processor.length, 7808);
  assertStereo(render(128), 0.25);
  assert.equal(await next, true);
  flush();
  assert.equal(processor.length, 8192);
  assertStereo(render(7680), 0.25);
  assertStereo(render(512), 0.5);
  assert.equal(processor.stats.dropped, 0);
  assert.equal(processor.stats.underflow, 0);
});

test("pausing or suspending releases a held field before a new playback epoch", async () => {
  for (const block of [false, true]) {
    const { output, processor, flush, render } = await audioHarness();
    assert(output.start(new Int16Array(8192 * 2).fill(8192)));
    flush();
    const next = output.deliverPaced(new Int16Array(64 * 2).fill(16384));
    assert.equal(typeof next.then, "function");
    flush();
    if (block) {
      output.context.state = "suspended";
      output.context.onstatechange();
      output.context.state = "running";
      output.context.onstatechange();
    }
    if (!block) output.setPaused(true);
    assert.equal(await next, false);
    output.setPaused(true);
    flush();
    assert.equal(processor.length, 0);
    assert(output.start(new Int16Array(128 * 2).fill(4096)));
    flush();
    assert(render(128)[0].every(sample => sample === 0.125));
    assert.equal(processor.stats.dropped, 0);
  }
});

async function assertStretchPitch(frequency, slowFieldsPerSecond) {
  const harness = await audioHarness();
  const { output, processor, flush, render } = harness;
  let source = 0;
  function wave(frames) {
    const samples = new Int16Array(frames * 2);
    for (let i = 0; i < frames; i++, source++) {
      samples[i * 2] = Math.round(12000 * Math.sin(2 * Math.PI * frequency * source / 44100));
      samples[i * 2 + 1] = -samples[i * 2];
    }
    return samples;
  }
  assert(output.start(wave(2948), true));
  flush();
  const crossings = [];
  let previous = 0;
  let nextArrival = 737;
  for (let time = 0; time < 44100 * 5; time += 128) {
    while (nextArrival <= time) {
      assert(output.deliver(wave(737)));
      flush();
      const rate = time < 44100 || time >= 4 * 44100 ? 59.8265 : slowFieldsPerSecond;
      nextArrival += 44100 / rate;
    }
    const [left, right] = render(128);
    assert(left.every((value, i) => value === -right[i]));
    for (let i = 0; i < left.length; i++) {
      if (time >= 2 * 44100 && time < 4 * 44100 && previous < 0 && left[i] >= 0) {
        crossings.push(time + i);
      }
      previous = left[i];
    }
  }
  assert(crossings.length > frequency * 1.9);
  const pitch = (crossings.length - 1) * 44100 / (crossings.at(-1) - crossings[0]);
  assert(Math.abs(pitch - frequency) < 1, `${frequency} Hz input produced ${pitch} Hz`);
  assert.equal(processor.stats.underflow, 0);
  assert.equal(processor.stats.dropped, 0);
  return harness;
}

test("BIOS audio follows a slow producer and preserves stereo pitch", async () => {
  const { output, processor, flush, render } = await assertStretchPitch(110, 32.5);
  output.setMuted(true);
  flush();
  assertStereo(render(128), 0);
  output.setMuted(false);
  flush();
  for (let i = 0; i < 170; i++) render(128);
  assert(processor.stats.underflow > 0, "a stopped producer must report underflow");

  output.setPaused(true);
  flush();
  assert.equal(processor.length, 0);
  assert(output.start(stereo(128, 4096)));
  flush();
  assertStereo(render(128), 0.125);
});

test("BIOS audio preserves bass pitch with no PCM shortages during slow playback", async () => {
  await assertStretchPitch(40, 24);
  await assertStretchPitch(55, 42);
});

test("BIOS audio stays continuous across several delayed PCM packets", async () => {
  const { output, processor, flush, render } = await audioHarness();
  const packetFrames = 737;
  assert(output.start(stereo(4 * packetFrames, 8192), true));
  flush();
  let due = packetFrames;
  let packets = 0;
  for (let time = 0; time < 44100 * 1.5; time += 128) {
    while (due <= time && output.hasCapacity(packetFrames)) {
      assert(output.deliver(stereo(packetFrames, 8192)));
      flush();
      packets++;
      // Three consecutive slow fields arrive about 52 ms apart.
      due += packets > 10 && packets <= 13 ? 44100 * 0.052 : packetFrames;
    }
    assertStereo(render(128), 0.25);
  }
  assert.equal(processor.stats.underflow, 0);
  assert.equal(processor.stats.dropped, 0);
});

test("presentation receipts remain ordered across asynchronous PCM admission", async () => {
  globalThis.Worker = class {
    constructor() { this.messages = []; }
    postMessage(message) { this.messages.push(message); }
    terminate() {}
  };
  const events = [];
  let resume;
  const client = new PlaybackClient({ onFrame: row => { events.push("frame " + row.sequence); },
    onAudio: row => {
      events.push("audio " + row.sequence);
      if (row.sequence === 0) return new Promise(resolve => { resume = resolve; });
    },
  });
  const worker = client.worker;
  worker.onmessage({ data: { type: "ready", methods: [] } });
  await client.ready;
  for (const sequence of [0, 1]) {
    worker.onmessage({ data: { type: "frame", sequence, epoch: 0, pixels: new Uint8Array(4), width: 1, height: 1 } });
    worker.onmessage({ data: { type: "audio", sequence, epoch: 0, pcm: new Int16Array(2) } });
  }
  assert.deepEqual(events, ["audio 0"]);
  assert.equal(worker.messages.filter(message => message.type === "presented").length, 0);
  resume();
  await Promise.resolve();
  assert.deepEqual(events, ["audio 0", "frame 0", "audio 1", "frame 1"]);
  assert.deepEqual(worker.messages.filter(message => message.type === "presented").map(row => row.sequence), [0, 1]);
  client.close();
});
