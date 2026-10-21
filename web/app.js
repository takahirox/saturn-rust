import { AudioOutput } from "./audio.js";
const $ = (id) => document.getElementById(id);
const hex = (value) => `0x${value.toString(16).toUpperCase().padStart(8, "0")}`;
export let emulator;
let discBytes = null;
let playing = false;
let playAfterLoad = false;
let buttons = 0;
let port2Buttons = 0;
const gamepadMasks = [0, 0];
const analogConnected = [false, false];
export function pollGamepads() {
  if (!emulator || document.hidden || !document.hasFocus() || document.activeElement !== $("screen")) return;
  const pads = Array.from(navigator.getGamepads?.() ?? []).filter((pad) => pad?.connected).slice(0, 2);
  // Standard Gamepad buttons -> Saturn A/B/C/X/Y/Z/L/R/Start and four directions.
  const mapping = [[0,64],[1,16],[2,32],[3,1024],[4,512],[5,256],[6,4096],[7,2048],[9,128],[12,1],[13,2],[14,4],[15,8]];
  const axis = (value) => Math.round((Math.max(-1, Math.min(1, Number.isFinite(value) ? value : 0)) + 1) * 127.5);
  for (let port = 0; port < 2; port++) {
    const pad = pads[port];
    let mask = 0;
    if (pad) for (const [index, bit] of mapping) if (pad.buttons[index]?.pressed || pad.buttons[index]?.value > 0.5) mask |= bit;
    gamepadMasks[port] = mask;
    analogConnected[port] = Boolean(pad && pad.axes.length >= 2);
    emulator.set_analog(port, analogConnected[port], pad ? axis(pad.axes[0]) : 128, pad ? axis(pad.axes[1]) : 128, pad ? axis(pad.axes[2] ?? -1) : 0);
  }
  emulator.set_buttons(buttons | gamepadMasks[0]);
  emulator.set_port2_buttons(port2Buttons | gamepadMasks[1]);
}
let lastFrame = 0;
const context = $("screen").getContext("2d");
let biosBytes = null;
let fileGeneration = 0;
let loadingFile = false;
let backupGeneration = 0;
let loadingBackup = false;
let view = "play";

export const audio = new AudioOutput((stats) => {
  $("audio-status").textContent = `Audio: ${stats.state} · ${stats.rendered} frames · ${stats.underflow} underflow · ${stats.dropped} dropped`;
});
$("audio-enable").addEventListener("click", async () => {
  try {
    if (!audio.node || audio.context.state !== "running") {
      await audio.enable();
      audio.setPaused(!playing);
    } else audio.setMuted(!audio.muted);
    $("audio-enable").textContent = audio.muted ? "Unmute audio" : "Mute audio";
  } catch (error) { $("audio-status").textContent = `Audio: blocked · ${error}`; }
});
export function pumpAudio() { audio.deliver(emulator.audio_samples(8192)); }

function rows(target, names, values) {
  target.replaceChildren(...names.map((name, i) => {
    const row = document.createElement("tr");
    const label = document.createElement("th");
    label.scope = "row";
    label.textContent = name;
    const value = document.createElement("td");
    value.id = name.toLowerCase();
    value.textContent = hex(values[i]);
    row.append(label, value);
    return row;
  }));
}

function releaseButtons() {
  buttons = 0;
  emulator?.set_buttons(0);
  port2Buttons = 0;
  emulator?.set_port2_buttons(0);
  gamepadMasks.fill(0);
  for (let port = 0; port < 2; port++) emulator?.set_analog(port, analogConnected[port], 128, 128, 0);
}

function pause() {
  playing = false;
  audio.setPaused(true);
  playAfterLoad = false;
  releaseButtons();
}

export function advanceFrame() {
  pollGamepads();
  emulator.advance_frame();
  audio.setPaused(!playing);
  pumpAudio();
  const pixels = new Uint8ClampedArray(emulator.framebuffer());
  context.putImageData(new ImageData(pixels, 320, 224), 0, 0);
}

function animate(now) {
  if (playing && now - lastFrame >= 1000 / 60) {
    lastFrame = now;
    act(advanceFrame);
  }
  requestAnimationFrame(animate);
}

function setText(id, text) {
  // Live regions should announce transitions, not every animation frame.
  if ($(id).textContent !== text) $(id).textContent = text;
}

function canExecute(status = emulator.status()) {
  return status === "Ready" || (status.startsWith("Resetting") && emulator.hardware_wake_possible()) || (status.startsWith("Sleeping") && (emulator.address_error_pending() || emulator.hardware_wake_possible()));
}

function render() {
  if (!emulator) return;
  const machineStatus = emulator.status();
  const mode = emulator.mode();
  $("active-mode").textContent = mode;
  setText("status", machineStatus);
  // Keep register allocation and Wasm register reads out of the playback loop.
  if (view === "debug") {
    $("instructions").textContent = emulator.instruction_count();
    $("exceptions").textContent = emulator.exception_count();
    $("last-exception").textContent = emulator.last_exception();
    const target = emulator.delay_target();
    $("delay").textContent = target === undefined ? "none" : hex(target);
    rows($("registers"), Array.from({ length: 16 }, (_, i) => `R${i}`), emulator.registers());
    rows($("special"), ["PC", "SR", "PR", "GBR", "VBR", "MACH", "MACL"], emulator.special_registers());
    $("frames").textContent = emulator.frames();
  }
  const busy = loadingFile || loadingBackup;
  const ready = !busy && canExecute(machineStatus);
  $("step").disabled = !ready;
  $("run").disabled = !ready;
  $("reset").disabled = loadingFile;
  const discReady = ready && $("mode").value === "disc";
  $("pause").disabled = !discReady;
  $("pause").textContent = playing ? "Pause" : "Resume";
  $("restart").disabled = busy || $("mode").value !== "disc" || !discBytes;
  $("frame").disabled = !discReady || playing;
  $("play-homebrew").disabled = busy;
  $("backup-export").disabled = busy;
  const state = loadingFile ? "Loading image…" : loadingBackup ? "Reading backup RAM…" : $("error").textContent ? "Stopped — see error below" :
    playing ? "Running" : discReady ? "Paused — Resume to play" : "Ready — Play Star Collector or open Debug";
  setText("player-status", `${state} · ${mode}`);
  $("recovery").hidden = !$("error").textContent || loadingFile;
}

// Navigation is presentational: preserve the machine, file generations and errors.
for (const nextView of ["play", "debug"]) {
  $(`view-${nextView}`).addEventListener("click", () => {
    if (view === nextView) return;
    pause();
    view = nextView;
    $("main").className = `view-${view}`;
    for (const name of ["play", "debug"]) $(`view-${name}`).setAttribute("aria-pressed", String(view === name));
    $("play-intro").hidden = view !== "play";
    for (const id of ["debug-intro", "debug-view", "register-panel"]) $(id).hidden = view !== "debug";
    render();
  });
}
$("settings").addEventListener("toggle", () => {
  if ($("settings").open) { pause(); render(); }
});

let fullscreenReturnFocus;
$("fullscreen").addEventListener("click", async () => {
  $("fullscreen-status").textContent = "";
  if (!$("player-shell").requestFullscreen || !document.fullscreenEnabled) {
    $("fullscreen-status").textContent = "Fullscreen is unavailable in this browser. The player still resizes with the window.";
    return;
  }
  const exiting = document.fullscreenElement === $("player-shell");
  try {
    if (exiting) await document.exitFullscreen();
    else {
      fullscreenReturnFocus = document.activeElement;
      await $("player-shell").requestFullscreen();
    }
  } catch (error) {
    $("fullscreen-status").textContent = `Fullscreen could not ${exiting ? "exit" : "open"}: ${error}. You can keep playing here.`;
  }
});
document.addEventListener("fullscreenchange", () => {
  const fullscreen = document.fullscreenElement === $("player-shell");
  $("fullscreen").textContent = fullscreen ? "Exit fullscreen" : "Fullscreen";
  releaseButtons();
  if (fullscreen) $("screen").focus();
  else fullscreenReturnFocus?.focus();
});

function act(operation) {
  $("error").textContent = "";
  try { operation(); } catch (error) { pause(); $("error").textContent = String(error); }
  render();
}

function reset() {
  if (loadingBackup) $("backup-status").textContent = "Backup import cancelled by boot change; backup RAM was preserved.";
  backupGeneration++;
  loadingBackup = false;
  pause();
  context.clearRect(0, 0, 320, 224);
  act(() => {
    if ($("mode").value === "disc") emulator.reset_disc(discBytes ?? new Uint8Array());
    else if ($("mode").value === "hle") emulator.reset_hle();
    else emulator.reset_bios(biosBytes ?? new Uint8Array());
  });
}

try {
  const { default: init, Emulator } = await import("./pkg/saturn_rust.js");
  await init();
  emulator = new Emulator();
  $("backup-import").addEventListener("change", async () => {
    const generation = ++backupGeneration;
    const file = $("backup-import").files[0];
    $("backup-import").value = ""; // Selecting the same file again must work.
    loadingBackup = false;
    pause();
    if (!file) { render(); return; }
    if (file.size !== 32768) {
      $("backup-status").textContent = `Backup unchanged: select a packed 32 KiB file (32768 bytes); received ${file.size} bytes.`;
      render();
      return;
    }
    loadingBackup = true;
    $("backup-status").textContent = "Reading backup RAM locally…";
    render();
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      if (generation !== backupGeneration) return;
      emulator.import_backup_ram(bytes);
      $("backup-status").textContent = "Imported 32768 backup bytes. Playback is paused; Reset to let the guest read the new data.";
    } catch (error) {
      if (generation !== backupGeneration) return;
      $("backup-status").textContent = `Backup unchanged: could not import locally: ${error}`;
    } finally {
      if (generation === backupGeneration) { loadingBackup = false; render(); }
    }
  });
  $("backup-export").addEventListener("click", () => {
    pause();
    let url;
    try {
      const bytes = emulator.export_backup_ram();
      url = URL.createObjectURL(new Blob([bytes], { type: "application/octet-stream" }));
      const link = document.createElement("a");
      link.href = url;
      link.download = "saturn-backup.bin";
      link.click();
      $("backup-status").textContent = "Exported 32768 backup bytes. Keep this file to restore the backup in a later session.";
    } catch (error) {
      $("backup-status").textContent = `Could not export backup RAM: ${error}`;
    } finally {
      if (url) setTimeout(() => URL.revokeObjectURL(url), 1000);
      render();
    }
  });
  $("reset").addEventListener("click", reset);
  $("restart").addEventListener("click", () => {
    reset();
    if ($("mode").value === "disc" && canExecute() && !$("settings").open && view === "play") {
      playing = true;
      lastFrame = 0;
      $("screen").focus();
      render();
    }
  });
  $("step").addEventListener("click", () => act(() => emulator.step()));
  $("run").addEventListener("click", () => act(() => {
    const count = Number($("count").value);
    if (!Number.isInteger(count) || count < 1 || count > 10000) {
      throw new Error("Enter a whole instruction count from 1 to 10000.");
    }
    emulator.run_steps(count);
  }));
  $("mode").addEventListener("change", () => {
    // Invalidate pending file reads and discard bytes on every mode change.
    fileGeneration++;
    biosBytes = null;
    discBytes = null;
    $("disc").value = "";
    loadingFile = false;
    $("bios").value = "";
    $("disc-controls").hidden = $("mode").value !== "disc";
    $("bios-controls").hidden = $("mode").value !== "bios";
    reset();
  });
  $("bios").addEventListener("change", async () => {
    const generation = ++fileGeneration;
    const file = $("bios").files[0];
    biosBytes = null;
    loadingFile = false;
    reset(); // Immediately stop the old machine, even if reading the new file fails.
    if (!file) return;
    if (file.size !== 524288) {
      $("error").textContent = `Select a raw 512 KiB BIOS (524288 bytes); received ${file.size} bytes.`;
      render();
      return;
    }
    loadingFile = true;
    $("error").textContent = "";
    render();
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      if (generation !== fileGeneration) return;
      biosBytes = bytes;
      loadingFile = false;
      reset();
    } catch (error) {
      if (generation !== fileGeneration) return;
      loadingFile = false;
      act(() => { throw new Error(`Could not read BIOS locally: ${error}. Select the file again.`); });
    }
  });
  const beginDisc = () => {
    const startPlaying = playAfterLoad;
    reset();
    if (canExecute()) {
      playing = startPlaying && view === "play" && !$("settings").open;
      lastFrame = 0;
      if (playing) $("screen").focus();
      render();
    }
  };
  $("play-homebrew").addEventListener("click", async () => {
    $("mode").value = "disc";
    $("mode").dispatchEvent(new Event("change"));
    const generation = ++fileGeneration;
    loadingFile = true;
    playAfterLoad = true;
    $("error").textContent = "";
    render();
    try {
      const response = await fetch("./star-collector.iso");
      if (!response.ok) throw new Error(`Sample ISO HTTP ${response.status}; rebuild or select the ISO locally.`);
      const bytes = new Uint8Array(await response.arrayBuffer());
      if (generation !== fileGeneration) return;
      discBytes = bytes;
      loadingFile = false;
      beginDisc();
    } catch (error) {
      if (generation !== fileGeneration) return;
      loadingFile = false;
      act(() => { throw error; });
    }
  });
  $("disc").addEventListener("change", async () => {
    const generation = ++fileGeneration;
    const file = $("disc").files[0];
    discBytes = null;
    loadingFile = false;
    reset();
    if (!file) return;
    if (file.size < 45056 || file.size > 16777216 || file.size % 2048 !== 0) {
      $("error").textContent = "Select a complete 2048-byte-sector ISO (22 sectors to 16 MiB). BIN/CUE and compressed images are unsupported.";
      render();
      return;
    }
    loadingFile = true;
    $("error").textContent = "";
    render();
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      if (generation !== fileGeneration) return;
      discBytes = bytes;
      loadingFile = false;
      beginDisc();
    } catch (error) {
      if (generation !== fileGeneration) return;
      loadingFile = false;
      act(() => { throw new Error(`Could not read disc locally: ${error}. Select the file again.`); });
    }
  });
  $("pause").addEventListener("click", () => {
    if (playing) pause();
    else { playing = true; lastFrame = 0; $("screen").focus(); }
    render();
  });
  $("frame").addEventListener("click", () => act(advanceFrame));
  const keys = { ArrowUp: 1, ArrowDown: 2, ArrowLeft: 4, ArrowRight: 8, z: 64, Enter: 128 };
  const port2Keys = { KeyI: 1, KeyK: 2, KeyJ: 4, KeyL: 8, KeyO: 16, KeyP: 32, KeyU: 64, ShiftRight: 128 };
  for (const eventName of ["keydown", "keyup"]) {
    window.addEventListener(eventName, (event) => {
      const mask = keys[event.key];
      const port2Mask = port2Keys[event.code];
      if ((!mask && !port2Mask) || $("mode").value !== "disc" || loadingFile || loadingBackup || document.activeElement !== $("screen")) return;
      event.preventDefault();
      if (eventName === "keydown") { buttons |= mask ?? 0; port2Buttons |= port2Mask ?? 0; }
      else { buttons &= ~(mask ?? 0); port2Buttons &= ~(port2Mask ?? 0); }
      emulator.set_buttons(buttons | gamepadMasks[0]);
      emulator.set_port2_buttons(port2Buttons | gamepadMasks[1]);
    });
  }
  $("screen").addEventListener("blur", releaseButtons);
  window.addEventListener("blur", releaseButtons);
  document.addEventListener("visibilitychange", () => { if (document.hidden) { pause(); render(); } });
  requestAnimationFrame(animate);
  $("controls").disabled = false;
  $("debug-controls").disabled = false;
  $("backup-controls").disabled = false;
  reset();
} catch (error) {
  $("error").textContent = `Could not load the Wasm module: ${error}. Build web/pkg and serve web/ over HTTP as described in README.`;
  $("status").textContent = "Initialization failed";
  $("player-status").textContent = "Initialization failed — emulator unavailable";
  $("recovery").hidden = false;
}
