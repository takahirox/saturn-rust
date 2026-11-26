// Original Rust Machine owner. The nested renderer consumes owned video data;
// this Worker sends every completed field and PCM packet to its owner.
import init, { Emulator } from "./pkg/saturn_rust.js";
import { VideoRenderClient } from "./video-render-client.js";
import { SoundClient, SOUND_TRANSPORT_METHODS } from "./sound-client.js";
import { ConsoleSaves } from "./console-saves.js";

const REFERENCE_HZ = 26874100;
const MAX_PACING_LAG_FIELDS = 16;
const RESET_PACING_GAP_FIELDS = 16;
let machine, video, sound, epoch = 0, sequence = 0, playing = false, input = {};
let lastSize = [320, 224], peakPendingFrames = 0;
let run = Promise.resolve(), failed = false;
let resolvePresentation;
const presentation = new Promise(resolve => { resolvePresentation = resolve; });
let directPresentation = false;
let methods, saves;
const mediaChange = method => method?.startsWith("reset_") || method?.startsWith("mount_")
  || method === "clear_media" || method === "set_cartridge";
const backupReplacement = method => method === "import_backup_ram" || method === "import_cartridge_backup_ram";
const saveReplacement = method => backupReplacement(method) || method === "import_battery_recovery";
let playWanted = false, playOptions = { paced: true };
const pending = new Map();
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

function fatal(error) {
  if (failed) return;
  failed = true;
  playing = false;
  self.postMessage({ type: "fatal", message: String(error?.message ?? error) });
  sound?.close();
  video?.close();
  self.close();
}
const ready = init().then(async () => {
  machine = new Emulator();
  if (!machine.enable_video_transport) throw new Error("Use the renderer-enabled Wasm build");
  const { port, persistSaves } = await presentation;
  directPresentation = Boolean(port);
  video = new VideoRenderClient({ presentationPort: port });
  sound = new SoundClient(machine);
  await Promise.all([video.ready, sound.ready]);
  video.activate();
  if (!machine.enable_video_transport(true)) throw new Error("Renderer requires a pristine Machine");
  if (persistSaves) {
    saves = new ConsoleSaves(machine, status => self.postMessage({ type: "save-status", status }));
    await saves.restore();
    setInterval(() => saves.poll(), 1000);
  }
  // Playback uses relaxed CPU/device interleaving for real-time speed.
  machine.set_fast_mode(true);
  methods = new Set(Object.getOwnPropertyNames(Emulator.prototype).filter(name =>
    !name.startsWith("_") && !SOUND_TRANSPORT_METHODS.has(name) && !["constructor", "free", "enable_video_transport", "enable_video_journal", "stop_execution"].includes(name)
    && typeof machine[name] === "function"));
  if (saves) {
    methods.add("export_battery_recovery");
    methods.add("import_battery_recovery");
  }
  self.postMessage({ type: "ready", methods: [...methods], view: view() });
}).catch(fatal);
let control = ready;

function applyInput() {
  machine.set_buttons(input.buttons ?? 0);
  machine.set_port2_buttons(input.port2Buttons ?? 0);
  for (const [port, state] of (input.ports ?? []).entries()) {
    if (port > 1) break;
    if (!state) continue;
    if (state.pad3d) {
      const p = state.pad3d;
      machine.set_3d_pad(port, Boolean(p.connected), p.x ?? 128, p.y ?? 128, p.right ?? 0, p.left ?? 0);
      machine.set_3d_pad_digital(port, state.digital ?? 0);
    } else if (state.analog) {
      const p = state.analog;
      machine.set_analog(port, Boolean(p.connected), p.x ?? 128, p.y ?? 128, p.throttle ?? 0);
    }
  }
}
async function drain() {
  await Promise.all([...pending.values()].map(item => item.done));
}
async function produce(automatic = false) {
  while (pending.size >= 2) await pending.values().next().value.done;
  await video.waitForCapacity();
  if (automatic && !playing) return null;
  video.activate();
  applyInput();
  const beforeField = Number(machine.frames());
  const beforeClock = Number(machine.peripheral_clocks());
  await sound.advanceField();
  const fields = Number(machine.frames());
  if (fields !== beforeField + 1) throw new Error("Producer did not complete exactly one field");
  const clocks = Number(machine.peripheral_clocks());
  const pcm = machine.audio_samples(8192);
  const id = sequence++, fieldEpoch = epoch;
  let acknowledge;
  const presented = new Promise(resolve => { acknowledge = resolve; });
  const identity = { sequence: id, epoch: fieldEpoch, fields, clocks, view: liveView() };
  const done = video.submit(fields, { presentation: identity }).then(receipt => {
    lastSize = [receipt.width, receipt.height];
    if (!directPresentation) self.postMessage({ type: "frame", sequence: id, epoch: fieldEpoch, fields, clocks,
      width: receipt.width, height: receipt.height, pixels: receipt.pixels, view: identity.view }, [receipt.pixels.buffer]);
    return presented;
  }).then(() => { pending.delete(id); });
  done.catch(fatal);
  pending.set(id, { epoch: fieldEpoch, acknowledge, done, presented: false });
  peakPendingFrames = Math.max(peakPendingFrames, pending.size);
  // Both arrays are owned, not borrowed Wasm views. Main-thread presentation
  // acknowledges the frame; the two-field bound also bounds pending PCM.
  self.postMessage({ type: "audio", sequence: id, epoch: fieldEpoch, pcm }, [pcm.buffer]);
  return (clocks - beforeClock) * 1000 / REFERENCE_HZ;
}
// Only scalar player status travels during continuous playback. Register
// arrays and diagnostic hashes remain paused-query work.
function liveView() {
  return { status: machine.status(), mode: machine.mode(),
    addressErrorPending: machine.address_error_pending(), hardwareWakePossible: machine.hardware_wake_possible() };
}
function view() {
  return { ...liveView(), clocks: Number(machine.peripheral_clocks()), frames: Number(machine.frames()), width: lastSize[0], height: lastSize[1],
    registers: [...machine.registers()], special: [...machine.special_registers()],
    instructions: machine.instruction_count(), exceptions: machine.exception_count(),
    lastException: machine.last_exception(), delayTarget: machine.delay_target() };
}
async function capture(capturePixels) {
  video.activate();
  machine.flush_video_output();
  const receipt = await video.submit(Number(machine.frames()), { capture: capturePixels });
  lastSize = [receipt.width, receipt.height];
  return receipt;
}
async function invoke(data) {
  await stop();
  if (!Array.isArray(data.operations) || data.operations.length < 1 || data.operations.length > 65536)
    throw new Error("Invalid debugger operation batch");
  if (data.epoch !== undefined) epoch = data.epoch;
  const values = [];
  let resetSound = false;
  for (const { method, args } of data.operations) {
    try {
      if (!methods.has(method) || !Array.isArray(args)) throw new Error("Unknown emulator method");
      if (saves && mediaChange(method)) await saves.flush();
      if (method.startsWith("reset_") || method === "clear_media" || method === "set_cartridge") resetSound = true;
      video.activate();
      if (method === "set_cartridge") input = {};
      applyInput();
      let value;
      if (method === "framebuffer" || method === "video_width" || method === "video_height") {
        const receipt = await capture(method === "framebuffer");
        value = method === "framebuffer" ? receipt.pixels : method === "video_width" ? receipt.width : receipt.height;
      } else if (method === "export_battery_recovery") {
        value = saves.exportBatteryRecovery();
      } else if (method === "import_battery_recovery") {
        await saves.importBatteryRecovery(args[0], { sessionOnly: args[1] === true });
      } else if (saves && backupReplacement(method)) {
        await saves.replace(method === "import_backup_ram" ? 0 : 1, args[0], { sessionOnly: args[1] === true });
        // The paused guest may be in the middle of writing or formatting this device.
        machine.stop_execution();
        input = {};
        resetSound = true;
      } else value = machine[method](...args);
      values.push({ ok: true, value });
    } catch (error) { values.push({ ok: false, error: String(error?.message ?? error) }); }
  }
  if (resetSound) await sound.reset();
  // Also acknowledge chunks generated by long paused debugger runs.
  await capture(false);
  const result = { values, epoch, view: view() };
  if (playWanted) await start(playOptions);
  return result;
}
function snapshot() {
  return {
    frames: Number(machine.frames()), clocks: Number(machine.peripheral_clocks()),
    registers: [...machine.registers()], special: [...machine.special_registers()],
    instructions: machine.instruction_count(), exceptions: machine.exception_count(),
    status: machine.status(), addressErrorPending: machine.address_error_pending(),
    slaveRegisters: [...machine.slave_registers()], slaveInstructions: machine.slave_instruction_count(),
    slavePc: machine.slave_pc(), slaveExceptions: machine.slave_exception_count(),
    soundPc: machine.sound_pc(), soundInstructions: machine.sound_instruction_count(),
    soundRegisters: [...machine.sound_registers()], soundHalted: machine.sound_halted(),
    audioGenerated: machine.audio_generated(), audioDropped: machine.audio_dropped(),
    width: lastSize[0], height: lastSize[1], pendingFrames: pending.size, pendingRenderFrames: video.pending.size, peakPendingFrames,
    coreHashes: machine.loop_state_hashes ? [...machine.loop_state_hashes()] : null,
    soundTransport: { ...sound.stats, disabled: sound.disabled },
  };
}
async function stop() {
  playing = false;
  await run;
  await drain();
}
async function reset(data) {
  await stop();
  await saves?.flush();
  epoch = data.epoch;
  input = {};
  video.activate();
  if (data.mode === "bios") machine.reset_bios(data.rom);
  else if (data.mode === "fixture") machine.reset_hle();
  else if (data.mode === "disc" && data.cue) machine.reset_cue(data.cue, data.names, data.lengths, data.bytes);
  else if (data.mode === "disc" && data.bytes) machine.reset_disc(data.bytes);
  else if (data.mode === "disc") machine.reset_loaded_disc();
  else throw new Error("Unknown boot mode");
  if (data.mode === "bios" && data.cue) machine.mount_cue(data.cue, data.names, data.lengths, data.bytes);
  else if (data.mode === "bios" && data.bytes) machine.mount_disc(data.bytes);
  await sound.reset();
  lastSize = [320, 224];
  return snapshot();
}
async function start({ paced = true } = {}) {
  await stop();
  playing = playWanted;
  run = (async () => {
    let due = performance.now();
    let previousCompletion = due;
    while (playing) {
      if (paced && due > performance.now()) await delay(due - performance.now());
      const period = await produce(true);
      if (period == null) break;
      due += period;
      const now = performance.now();
      const budget = period * MAX_PACING_LAG_FIELDS;
      // Keep bounded catch-up time after a sequence of expensive fields.
      // A single long gap starts a new pacing phase on return.
      if (now > due && now - previousCompletion >= period * RESET_PACING_GAP_FIELDS) {
        due = now;
      } else if (now - due >= budget) {
        due = now - budget;
      }
      previousCompletion = now;
    }
  })().catch(error => {
    playing = false;
    playWanted = false;
    self.postMessage({ type: "playback-error", epoch, message: String(error?.message ?? error) });
  });
  return { playing: true, epoch };
}
async function dispatch(data) {
  switch (data.command) {
    case "reset": return reset(data);
    case "play": return start(data);
    case "pause": await stop(); await saves?.flush(); return snapshot();
    case "view": await stop(); await saves?.flush(); applyInput(); return { epoch, view: view() };
    case "retry-saves": await stop(); await saves?.flush({ retry: true }); return { epoch, view: view() };
    case "advance": {
      await stop();
      if (!Number.isInteger(data.fields) || data.fields < 1 || data.fields > 8) throw new Error("Advance one to eight fields");
      for (let i = 0; i < data.fields; i++) await produce();
      await drain();
      return data.inspect === false ? { frames: Number(machine.frames()), clocks: Number(machine.peripheral_clocks()),
        pendingFrames: pending.size, peakPendingFrames } : snapshot();
    }
    case "inspect": await stop(); return snapshot();
    case "invoke": return invoke(data);
    default: throw new Error("Unknown playback command");
  }
}
self.onmessage = ({ data }) => {
  if (data.type === "presentation") {
    resolvePresentation({ port: data.port, persistSaves: data.persistSaves });
    return;
  }
  if (data.type === "presented") {
    const item = pending.get(data.sequence);
    if (!item || item.epoch !== data.epoch || item.presented) { fatal(new Error("Unexpected presentation receipt")); return; }
    item.presented = true;
    item.acknowledge();
    return;
  }
  if (data.type === "input") {
    if (data.epoch === epoch) {
      input = data.input;
      // Physical trigger hysteresis observes every host report, even when
      // several reports arrive before the next guest execution command.
      if (machine) applyInput();
    }
    return;
  }
  if (data.type !== "command") { fatal(new Error("Unknown playback message")); return; }
  if (data.command === "play") {
    playWanted = true;
    playOptions = { paced: data.paced !== false };
  } else {
    playing = false;
    if (data.command !== "invoke" || data.operations?.some(op => mediaChange(op.method) || saveReplacement(op.method))) playWanted = false;
  }
  control = control.then(async () => {
    try { self.postMessage({ type: "result", id: data.id, result: await dispatch(data) }); }
    catch (error) { self.postMessage({ type: "error", id: data.id, message: String(error?.message ?? error) }); }
  }).catch(fatal);
};
