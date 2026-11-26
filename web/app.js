import { AudioOutput } from "./audio.js";
import { CrtDisplay } from "./crt-display.js";
import { WorkerEmulator } from "./worker-emulator.js";
const $ = (id) => document.getElementById(id);
const hex = (value) => `0x${value.toString(16).toUpperCase().padStart(8, "0")}`;
export let emulator;
let discBytes = null;
let cueMedia = null;
let loadedDisc = false;
// Resource bounds mirror disc.rs, independent of Saturn's recording standards.
const MAX_ISO_BYTES = 450000 * 2048;
const MAX_MEDIA_BYTES = 450000 * 2352;
let playing = false;
let sessionStarted = false;
let attemptedBoot = false;
let bootValid = false;
let buttons = 0;
let port2Buttons = 0;
const gamepadMasks = [0, 0];
const analogConnected = [false, false];
const pad3d = [false, false];
const gamepadSlots = [null, null];
export function pollGamepads() {
  if (!emulator || !acceptsInput() || document.hidden || !document.hasFocus() || document.activeElement !== $("screen")) return;
  // Assign the first two connected pads to free ports, retaining a surviving
  // pad's port when its neighbour disconnects (the API can contain sparse slots).
  const browserPads = Array.from(navigator.getGamepads?.() ?? []);
  const connected = browserPads.flatMap((pad, index) => pad?.connected ? [index] : []);
  for (let port = 0; port < 2; port++) if (!connected.includes(gamepadSlots[port])) gamepadSlots[port] = null;
  for (const index of connected) {
    const free = gamepadSlots.indexOf(null);
    if (free !== -1 && !gamepadSlots.includes(index)) gamepadSlots[free] = index;
  }
  const pads = gamepadSlots.map(index => browserPads[index]);
  // Standard Gamepad buttons -> Saturn A/B/C/X/Y/Z/L/R/Start and four directions.
  const mapping = [[0,64],[1,16],[2,32],[3,1024],[4,512],[5,256],[6,4096],[7,2048],[9,128],[12,1],[13,2],[14,4],[15,8]];
  const axis = (value) => Math.round((Math.max(-1, Math.min(1, Number.isFinite(value) ? value : 0)) + 1) * 127.5);
  for (let port = 0; port < 2; port++) {
    const pad = pads[port]?.connected ? pads[port] : null;
    let mask = 0;
    if (pad) for (const [index, bit] of mapping) if (pad.buttons[index]?.pressed || pad.buttons[index]?.value > 0.5) mask |= bit;
    gamepadMasks[port] = mask;
    analogConnected[port] = Boolean(pad && pad.axes.length >= 2);
    pad3d[port] = analogConnected[port] && pad.mapping === "standard";
    if (pad3d[port]) {
      const rawX = Number.isFinite(pad.axes[0]) ? pad.axes[0] : 0;
      const rawY = Number.isFinite(pad.axes[1]) ? pad.axes[1] : 0;
      const radius = Math.max(1, Math.hypot(rawX, rawY));
      const trigger = (i) => Math.round(Math.max(0, Math.min(1, Number.isFinite(pad.buttons[i]?.value) ? pad.buttons[i].value : 0)) * 255);
      // Keep physical trigger hysteresis independent of digital keyboard input.
      // The report overlay supplies full keyboard travel without changing its latch.
      emulator.set_3d_pad(port, true, axis(rawX / radius), axis(rawY / radius),
        trigger(7), trigger(6));
      emulator.set_3d_pad_digital(port, port === 0 ? buttons : port2Buttons);
    } else emulator.set_analog(port, analogConnected[port], pad ? axis(pad.axes[0]) : 128, pad ? axis(pad.axes[1]) : 128, pad ? axis(pad.axes[2] ?? -1) : 0);
  }
  emulator.set_buttons(buttons | gamepadMasks[0]);
  emulator.set_port2_buttons(port2Buttons | gamepadMasks[1]);
}
let pendingActions = 0;
let bootGeneration = 0;
let resetting = false;
let queuedAudio = [];
const screen = $("screen");
const context = screen.getContext("2d");
const crtDisplay = new CrtDisplay(screen, $("crt-screen"), $("display-filter"), $("display-filter-status"));
let biosBytes = null;
let biosSelected = false;
let debugFixture = false;
// BIOS and disc reads may overlap; only a replacement of the same image cancels it.
let biosGeneration = 0;
let discGeneration = 0;
let loadingBios = false;
let loadingDisc = false;
const loadingFile = () => loadingBios || loadingDisc;
const selectionErrors = { bios: "", disc: "" };
const selectionError = () => selectionErrors.bios || selectionErrors.disc;
// A selected but unaccepted BIOS always blocks Play, including cancelled reads.
function effectiveBootMode() {
  if (debugFixture) return "fixture";
  if (biosSelected) return biosBytes && !loadingBios && !selectionErrors.bios ? "bios" : "blocked";
  return "disc";
}
function playerInputReady(mode = effectiveBootMode()) {
  return mode === "bios" || (mode === "disc" && loadedDisc);
}
function acceptsInput() {
  return bootValid && playerInputReady() && !loadingFile() && !loadingBackup && pendingActions === 0 && !selectionError();
}
let backupGeneration = 0;
let loadingBackup = false;
let readingBackup = false;
let view = "play";

export const audio = new AudioOutput((stats) => {
  setText("audio-status", `Audio: ${stats.state}${stats.state === "underflow" ? " · buffer empty" : ""}`);
  $("audio-diagnostics").textContent = `${stats.rendered} audio frames · ${stats.underflow} underflow · ${stats.dropped} dropped`;
});
$("audio-enable").addEventListener("click", async () => {
  try {
    if (!audio.node || audio.context.state !== "running") {
      await audio.enable();
      queuedAudio = [];
      audio.setPaused(true);
    } else audio.setMuted(!audio.muted);
    $("audio-enable").textContent = audio.muted ? "Unmute audio" : "Mute audio";
  } catch (error) { $("audio-status").textContent = `Audio: blocked · ${error}`; }
});
export async function pumpAudio() { audio.deliver(await emulator.audio_samples(8192)); }

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
  for (let port = 0; port < 2; port++) {
    if (pad3d[port]) {
      emulator?.set_3d_pad(port, analogConnected[port], 128, 128, 0, 0);
      emulator?.set_3d_pad_digital(port, 0);
    }
    else emulator?.set_analog(port, analogConnected[port], 128, 128, 0);
  }
}

async function pause() {
  playing = false;
  queuedAudio = [];
  audio.setPaused(true);
  releaseButtons();
  render();
  if (emulator) await emulator.pause();
}
function deliverAudio({ pcm }) {
  if (!playing || !audio.node || audio.context.state !== "running") return;
  if (!audio.paused) return audio.deliverPaced(pcm);
  const stretch = effectiveBootMode() === "bios";
  queuedAudio.push(pcm);
  const samplesLength = queuedAudio.reduce((sum, part) => sum + part.length, 0);
  // The BIOS starts with silence. A few more samples give the audio
  // clock time to follow slow startup before the first sound.
  if (samplesLength / 2 < (stretch ? 2400 : 1024)) return;
  const samples = new Int16Array(samplesLength);
  let at = 0;
  for (const part of queuedAudio) { samples.set(part, at); at += part.length; }
  if (audio.start(samples, stretch)) queuedAudio = [];
}

function setDisplayRatio(width, height) {
  const ratio = width === 320 && height === 224 ? "4 / 3" : `${width} / ${height}`;
  screen.style.aspectRatio = ratio;
  $("player-shell").style.setProperty("--display-ratio", ratio);
}

function presentFrame({ pixels, width, height }) {
  if (resetting) return;
  $("empty-state").hidden = true;
  if (screen.width !== width || screen.height !== height) {
    screen.width = width;
    screen.height = height;
    setDisplayRatio(width, height);
  }
  // The transferred frame remains owned by this page. ImageData can use the
  // same storage for the raw display and the optional shader upload.
  const rgba = new Uint8ClampedArray(pixels.buffer, pixels.byteOffset, pixels.byteLength);
  context.putImageData(new ImageData(rgba, width, height), 0, 0);
  crtDisplay.present(pixels, width, height);
  render();
}
export async function advanceFrame() {
  pollGamepads();
  await emulator.advanceField();
}
function animate() {
  if (playing) pollGamepads();
  requestAnimationFrame(animate);
}
async function startPlaying() {
  playing = true;
  queuedAudio = [];
  audio.setPaused(true);
  $("screen").focus();
  await emulator.play();
}

function setText(id, text) {
  // Live regions should announce transitions, not every animation frame.
  if ($(id).textContent !== text) $(id).textContent = text;
}

function canExecute(status = emulator.state.status) {
  return status === "Ready" || (status.startsWith("Resetting") && emulator.state.hardwareWakePossible) || (status.startsWith("Sleeping") && (emulator.state.addressErrorPending || emulator.state.hardwareWakePossible));
}

function render() {
  if (!emulator?.state) return;
  $("main").setAttribute("aria-busy", String(pendingActions > 0));
  const machineStatus = attemptedBoot ? emulator.state.status : "Ready";
  const mode = bootValid ? emulator.state.mode : "Not started";
  const bootMode = effectiveBootMode();
  const bootMethod = bootMode === "fixture" ? "Debug · HLE bootstrap fixture" :
    bootMode === "blocked" ? (loadingBios ? "Auto · BIOS loading" : "Auto · BIOS selection blocked") :
    bootMode === "bios" ? "Auto · User-provided BIOS" : "Auto · BIOS-less HLE";
  setText("boot-method", bootMethod);
  $("active-mode").textContent = mode;
  setText("status", machineStatus);
  // Keep register allocation and Wasm register reads out of the playback loop.
  if (view === "debug") {
    $("instructions").textContent = emulator.state.instructions;
    $("exceptions").textContent = emulator.state.exceptions;
    $("last-exception").textContent = emulator.state.lastException;
    const target = emulator.state.delayTarget;
    $("delay").textContent = target === undefined ? "none" : hex(target);
    rows($("registers"), Array.from({ length: 16 }, (_, i) => `R${i}`), emulator.state.registers);
    rows($("special"), ["PC", "SR", "PR", "GBR", "VBR", "MACH", "MACL"], emulator.state.special);
    $("frames").textContent = emulator.state.frames;
  }
  $("bios-clear").disabled = !biosSelected;
  $("disc-clear").disabled = !loadedDisc && !loadingDisc && !selectionErrors.disc;
  const busy = loadingFile() || loadingBackup || pendingActions > 0;
  const ready = bootValid && !busy && !selectionError() && canExecute(machineStatus);
  $("step").disabled = !ready;
  $("run").disabled = !ready;
  $("reset").disabled = false;
  const playerReady = ready && playerInputReady(bootMode);
  const inputReady = playerInputReady(bootMode);
  $("pause").disabled = busy || !sessionStarted || (!playing && !playerReady);
  $("pause").hidden = !sessionStarted;
  $("pause").textContent = playing ? "Pause" : "Resume";
  $("restart").disabled = busy || Boolean(selectionError()) || !inputReady;
  $("frame").disabled = !playerReady || playing;
  const launchReady = playerReady && !$("error").textContent && inputReady;
  $("start").disabled = !launchReady || playing;
  $("start").hidden = sessionStarted;
  $("backup-export").disabled = busy;
  const needsInput = "Select a disc or add a 512 KiB BIOS to begin";
  const state = pendingActions > 0 && !loadingFile() && !loadingBackup ? "Working…" : loadingFile() ? "Loading image…" : loadingBackup ? "Reading backup RAM…" : $("error").textContent ? "Stopped — check the error" :
    playing ? `Running${machineStatus === "Ready" ? "" : ` · ${machineStatus}`}` : launchReady ? (sessionStarted ? "Paused — Resume to play" : `${bootMode === "bios" ? "BIOS" : "Disc"} ready — press Start`) :
    bootValid ? machineStatus : needsInput;
  setText("player-status", `${state} · ${bootMethod}`);
  setText("launch-status", busy ? (loadingBios ? "Reading BIOS locally… Start is blocked until validation completes." : "Reading local data…") : $("error").textContent ? "Check the error beside the player. Replace or clear the rejected selection." :
    bootMode === "fixture" ? "Debugger fixture selected. Use the advanced CPU controls." :
    launchReady ? (sessionStarted ? "Playback controls are beneath the display." : `Ready to start.${bootMode === "bios" ? " Disc optional in BIOS mode." : ""} Audio is enabled separately.`) :
    bootValid ? `Machine: ${machineStatus}. See Debug for details.` : needsInput);
  setText("empty-title", bootMode === "bios" ? "Start with your BIOS" : bootMode === "blocked" ? "BIOS selection needs attention" : "Load a disc to begin");
  setText("empty-hint", bootMode === "bios" ? "Disc optional. Firmware support is incomplete." : bootMode === "blocked" ? "Wait for loading, or replace or clear the BIOS selection." : "Or add a local BIOS to start without a disc.");
  $("recovery").hidden = !$("error").textContent || loadingFile();
}

// Navigation is presentational: preserve the machine, file generations and errors.
for (const nextView of ["play", "debug"]) {
  $(`view-${nextView}`).addEventListener("click", () => {
    if (view === nextView) return;
    void act(pause, { clearError: false });
    view = nextView;
    $("main").className = `view-${view}`;
    for (const name of ["play", "debug"]) $(`view-${name}`).setAttribute("aria-pressed", String(view === name));
    for (const id of ["debug-view", "register-panel"]) $(id).hidden = view !== "debug";
    render();
  });
}
$("settings").addEventListener("toggle", () => {
  if ($("settings").open) void act(pause, { clearError: false });
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

async function act(operation, { current = () => true, clearError = true } = {}) {
  pendingActions++;
  if (current() && clearError) $("error").textContent = selectionError();
  render();
  try { await operation(); }
  catch (error) {
    if (current()) {
      await pause().catch(() => {});
      $("error").textContent = selectionError() || String(error);
    }
  } finally { pendingActions--; render(); }
}

async function reset({ clearMedia = false } = {}) {
  const generation = ++bootGeneration;
  resetting = true;
  const current = () => generation === bootGeneration;
  const media = { cue: cueMedia, bytes: discBytes }, discToken = discGeneration;
  discBytes = cueMedia = null;
  if (loadingBackup) $("backup-status").textContent = readingBackup
    ? "Backup import cancelled by boot change; backup RAM was preserved."
    : "Accepted backup import retained through the boot change.";
  backupGeneration++;
  loadingBackup = readingBackup = false;
  playing = false;
  queuedAudio = [];
  audio.setPaused(true);
  releaseButtons();
  crtDisplay.clear();
  context.clearRect(0, 0, screen.width, screen.height);
  attemptedBoot = true;
  bootValid = false;
  $("empty-state").hidden = false;
  const mode = effectiveBootMode(), rom = biosBytes;
  await act(async () => {
    if (clearMedia) await emulator.clear_media();
    if (!current()) return;
    // The rejected empty ROM clears execution while retaining mounted media.
    try { await emulator.reset_bios(new Uint8Array()); } catch { /* Expected size rejection. */ }
    if (!current()) return;
    try {
      if (media.cue) await emulator.mount_cue(media.cue.cue, media.cue.names, media.cue.lengths, media.cue.bytes);
      else if (media.bytes) await emulator.mount_disc(media.bytes);
      if (discToken === discGeneration) loadedDisc = loadedDisc || Boolean(media.cue || media.bytes);
      if (!current()) return;
    } catch (error) {
      if (discToken === discGeneration) selectionErrors.disc = String(error);
      throw error;
    }
    if (!current() || loadingFile() || selectionError() || mode === "blocked") return;
    if (mode === "fixture") await emulator.reset_hle();
    else if (mode === "bios") await emulator.reset_bios(rom);
    else if (loadedDisc) await emulator.reset_loaded_disc();
    else return;
    if (current()) bootValid = true;
  }, { current });
  if (current()) {
    resetting = false;
    screen.width = emulator.state.width;
    screen.height = emulator.state.height;
    setDisplayRatio(screen.width, screen.height);
  }
}

try {
  emulator = new WorkerEmulator({ onFrame: presentFrame, onAudio: deliverAudio,
    onError: error => {
      // The ready rejection below owns actionable startup diagnostics.
      if (emulator?.state) void act(async () => { throw error; });
    } });
  await emulator.ready;
  // Playback uses relaxed timing; `?timing=exact` keeps the cycle scheduler.
  if (new URLSearchParams(location.search).get("timing") === "exact") await emulator.set_fast_mode(false);
  $("backup-import").addEventListener("change", async () => {
    const generation = ++backupGeneration;
    const file = $("backup-import").files[0];
    $("backup-import").value = ""; // Selecting the same file again must work.
    loadingBackup = readingBackup = Boolean(file);
    await pause();
    if (generation !== backupGeneration) return;
    if (!file) { render(); return; }
    if (file.size !== 32768) {
      loadingBackup = readingBackup = false;
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
      readingBackup = false;
      $("backup-status").textContent = "Importing 32768 backup bytes…";
      await emulator.import_backup_ram(bytes);
      if (generation !== backupGeneration) return;
      $("backup-status").textContent = "Imported 32768 backup bytes. Playback is paused; Reset to let the guest read the new data.";
    } catch (error) {
      if (generation !== backupGeneration) return;
      $("backup-status").textContent = `Backup unchanged: could not import locally: ${error}`;
    } finally {
      if (generation === backupGeneration) { loadingBackup = readingBackup = false; render(); }
    }
  });
  $("backup-export").addEventListener("click", async () => {
    await pause();
    let url;
    try {
      const bytes = await emulator.export_backup_ram();
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
  $("reset").addEventListener("click", () => {
    biosGeneration++;
    discGeneration++;
    if (loadingBios) selectionErrors.bios = "BIOS read cancelled by Reset. Replace or clear the BIOS selection.";
    if (loadingDisc) selectionErrors.disc = "Disc read cancelled by Reset. Replace or clear the disc selection.";
    loadingBios = false;
    loadingDisc = false;
    reset();
  });
  $("start").addEventListener("click", () => {
    if ($("start").disabled) return;
    sessionStarted = true;
    void act(startPlaying);
  });
  $("fixture").addEventListener("click", () => {
    sessionStarted = false;
    biosGeneration++;
    discGeneration++;
    loadingBios = loadingDisc = false;
    selectionErrors.bios = selectionErrors.disc = "";
    biosBytes = discBytes = cueMedia = null;
    biosSelected = loadedDisc = false;
    $("disc").value = $("bios").value = "";
    $("disc-selection").textContent = "No disc selected.";
    $("bios-selection").textContent = "No BIOS selected.";
    debugFixture = true;
    void reset({ clearMedia: true });
  });
  $("restart").addEventListener("click", async () => {
    if ($("restart").disabled) return;
    await reset();
    if (sessionStarted && bootValid && !selectionError() && playerInputReady() && canExecute() && !$("settings").open && view === "play") {
      await act(startPlaying);
    }
  });
  $("step").addEventListener("click", () => act(() => emulator.step()));
  $("run").addEventListener("click", () => act(() => {
    const count = Number($("count").value);
    if (!Number.isInteger(count) || count < 1 || count > 10000) {
      throw new Error("Enter a whole instruction count from 1 to 10000.");
    }
    return emulator.run_steps(count);
  }));
  $("bios").addEventListener("change", async () => {
    selectionErrors.bios = "";
    const generation = ++biosGeneration;
    const file = $("bios").files[0];
    biosSelected = Boolean(file);
    debugFixture = false;
    sessionStarted = false;
    $("bios-selection").textContent = file ? `${file.name} · ${file.size.toLocaleString()} bytes (not yet validated)` : "No BIOS selected.";
    biosBytes = null;
    loadingBios = Boolean(file);
    reset(); // Immediately stop the old machine, even if reading the new file fails.
    if (!file) return;
    if (file.size !== 524288) {
      loadingBios = false;
      selectionErrors.bios = $("error").textContent = `Select a raw 512 KiB BIOS (524288 bytes); received ${file.size} bytes.`;
      render();
      return;
    }
    loadingBios = true;
    $("error").textContent = selectionError();
    render();
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      if (generation !== biosGeneration) return;
      if (bytes.length !== 524288) throw new Error("BIOS read did not contain 524288 bytes");
      biosBytes = bytes;
      $("bios-selection").textContent = `${file.name} · 512 KiB · loaded locally`;
      loadingBios = false;
      await reset();
    } catch (error) {
      if (generation !== biosGeneration) return;
      loadingBios = false;
      selectionErrors.bios = `Could not read BIOS locally: ${error}. Select the file again.`;
      act(() => {});
    }
  });
  const selectDisc = async (files) => {
    if ($("controls").disabled) return;
    selectionErrors.disc = "";
    const generation = ++discGeneration;
    debugFixture = false;
    sessionStarted = false;
    $("disc-selection").textContent = files.length ? files.map(file => `${file.name} · ${file.size.toLocaleString()} bytes`).join("\n") + "\nNot yet validated." : "No disc selected.";
    discBytes = null;
    cueMedia = null;
    loadedDisc = false;
    loadingDisc = Boolean(files.length);
    void reset({ clearMedia: true });
    if (!files.length) return;
    const cueFiles = files.filter((file) => /\.cue$/i.test(file.name));
    const cue = cueFiles.length === 1 ? cueFiles[0] : null;
    const file = files[0];
    const chd = !cue && /\.chd$/i.test(file.name);
    if ((!cue && (files.length !== 1 || (chd ? file.size < 124 || file.size > MAX_MEDIA_BYTES : !/\.iso$/i.test(file.name) || file.size < 18 * 2048 || file.size > MAX_ISO_BYTES || file.size % 2048 !== 0))) ||
        cueFiles.length > 1 || files.filter((file) => file !== cue).reduce((total, file) => total + file.size, 0) > MAX_MEDIA_BYTES ||
        (cue && (cue.size > 1048576 || files.length < 2 || files.length > 100))) {
      loadingDisc = false;
      selectionErrors.disc = $("error").textContent = "Select one self-contained CHD v5 CD-ROM, one complete 2048-byte-sector ISO (18 to 450000 sectors), or one CUE and all its referenced BINARY track files together (up to 1058400000 bytes). A lone raw BIN needs its CUE.";
      render();
      return;
    }
    loadingDisc = true;
    $("error").textContent = selectionError();
    render();
    try {
      if (cue) {
        const tracks = files.filter((file) => file !== cue);
        const cueText = new TextDecoder("utf-8", { fatal: true }).decode(await cue.arrayBuffer());
        if (generation !== discGeneration) return;
        // Blob concatenates the selected local file handles, avoiding retained
        // per-file ArrayBuffers plus a second complete JS aggregate copy.
        const bytes = new Uint8Array(await new Blob(tracks).arrayBuffer());
        if (generation !== discGeneration) return;
        cueMedia = { cue: cueText, names: tracks.map((file) => file.name).join("\0"), lengths: new Uint32Array(tracks.map((file) => file.size)), bytes };
      } else {
        const bytes = new Uint8Array(await file.arrayBuffer());
        if (generation !== discGeneration) return;
        if (chd && new TextDecoder().decode(bytes.subarray(0, 8)) !== "MComprHD") throw new Error("Invalid CHD header; expected CHD v5 CD-ROM.");
        discBytes = bytes;
      }
      loadingDisc = false;
      await reset();
      if (loadedDisc) $("disc-selection").textContent = files.map(file => `${file.name} · ${file.size.toLocaleString()} bytes`).join("\n") + "\nLoaded locally. Replacement stops playback.";
    } catch (error) {
      if (generation !== discGeneration) return;
      loadingDisc = false;
      selectionErrors.disc = `Could not read disc locally: ${error}. Select the file again.`;
      act(() => {});
    }
  };
  $("disc").addEventListener("change", () => selectDisc([...$("disc").files]));
  for (const id of ["bios", "disc"]) $(id + "-clear").addEventListener("click", () => {
    $(id).value = "";
    $(id).dispatchEvent(new Event("change"));
  });
  const dropZone = $("disc-drop");
  for (const type of ["dragenter", "dragover"]) dropZone.addEventListener(type, event => {
    event.preventDefault();
    if (!$("controls").disabled) dropZone.classList.add("drag-over");
  });
  dropZone.addEventListener("dragleave", () => dropZone.classList.remove("drag-over"));
  dropZone.addEventListener("drop", event => {
    event.preventDefault();
    dropZone.classList.remove("drag-over");
    if ($("controls").disabled) return;
    $("disc").value = "";
    selectDisc([...event.dataTransfer.files]);
  });
  // Prevent a dropped image outside the target from navigating away from the tab.
  for (const type of ["dragover", "drop"]) window.addEventListener(type, event => event.preventDefault());
  $("pause").addEventListener("click", () => {
    if ($("pause").disabled) return;
    void act(playing ? pause : startPlaying);
  });
  $("frame").addEventListener("click", () => act(advanceFrame));
  const keys = { ArrowUp: 1, ArrowDown: 2, ArrowLeft: 4, ArrowRight: 8,
    KeyZ: 64, KeyX: 16, KeyC: 32, KeyA: 1024, KeyS: 512, KeyD: 256,
    KeyQ: 2048, KeyW: 4096, Enter: 128 };
  const port2Keys = { KeyI: 1, KeyK: 2, KeyJ: 4, KeyL: 8, KeyO: 16, KeyP: 32, KeyU: 64, ShiftRight: 128 };
  for (const eventName of ["keydown", "keyup"]) {
    window.addEventListener(eventName, (event) => {
      const mask = keys[event.code];
      const port2Mask = port2Keys[event.code];
      if ((!mask && !port2Mask) || !acceptsInput() || document.hidden || !document.hasFocus() || document.activeElement !== $("screen")) return;
      // Leave host shortcuts alone. Always accept keyup, even if a modifier
      // changed while held; repeats cannot reacquire a key released by blur/pause.
      if (eventName === "keydown" && (event.ctrlKey || event.altKey || event.metaKey)) return;
      event.preventDefault();
      if (eventName === "keydown" && event.repeat) return;
      if (eventName === "keydown") { buttons |= mask ?? 0; port2Buttons |= port2Mask ?? 0; }
      else { buttons &= ~(mask ?? 0); port2Buttons &= ~(port2Mask ?? 0); }
      pollGamepads();
    });
  }
  $("screen").addEventListener("blur", releaseButtons);
  window.addEventListener("blur", releaseButtons);
  document.addEventListener("visibilitychange", () => { if (document.hidden) void act(pause, { clearError: false }); });
  requestAnimationFrame(animate);
  $("controls").disabled = false;
  $("debug-controls").disabled = false;
  $("backup-controls").disabled = false;
  render();
} catch (error) {
  $("error").textContent = `Could not load the Wasm module: ${error}. Build web/pkg and serve web/ over HTTP as described in README.`;
  $("status").textContent = "Initialization failed";
  $("player-status").textContent = "Initialization failed — emulator unavailable";
  $("recovery").hidden = false;
}
