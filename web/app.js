const $ = (id) => document.getElementById(id);
const hex = (value) => `0x${value.toString(16).toUpperCase().padStart(8, "0")}`;
export let emulator;
let discBytes = null;
let playing = false;
let buttons = 0;
let lastFrame = 0;
const context = $("screen").getContext("2d");
let biosBytes = null;
let fileGeneration = 0;
let loadingFile = false;

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
}

function pause() {
  playing = false;
  releaseButtons();
}

export function advanceFrame() {
  emulator.advance_frame();
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

function render() {
  $("active-mode").textContent = emulator.mode();
  $("status").textContent = emulator.status();
  $("instructions").textContent = emulator.instruction_count();
  const target = emulator.delay_target();
  $("delay").textContent = target === undefined ? "none" : hex(target);
  rows($("registers"), Array.from({ length: 16 }, (_, i) => `R${i}`), emulator.registers());
  rows($("special"), ["PC", "SR", "PR", "GBR", "VBR", "MACH", "MACL"], emulator.special_registers());
  const ready = !loadingFile && emulator.status() === "Ready";
  $("step").disabled = !ready;
  $("run").disabled = !ready;
  $("reset").disabled = loadingFile;
  const discReady = ready && $("mode").value === "disc";
  $("pause").disabled = !discReady;
  $("pause").textContent = playing ? "Pause" : "Resume";
  $("frame").disabled = !discReady || playing;
  $("frames").textContent = emulator.frames();
  $("play-homebrew").disabled = loadingFile;
}

function act(operation) {
  $("error").textContent = "";
  try { operation(); } catch (error) { pause(); $("error").textContent = String(error); }
  render();
}

function reset() {
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
  $("reset").addEventListener("click", reset);
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
      return;
    }
    loadingFile = true;
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
    reset();
    if (emulator.status() === "Ready") {
      playing = true;
      lastFrame = 0;
      $("screen").focus();
      render();
    }
  };
  $("play-homebrew").addEventListener("click", async () => {
    $("mode").value = "disc";
    $("mode").dispatchEvent(new Event("change"));
    const generation = ++fileGeneration;
    loadingFile = true;
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
      return;
    }
    loadingFile = true;
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
  for (const eventName of ["keydown", "keyup"]) {
    window.addEventListener(eventName, (event) => {
      const mask = keys[event.key];
      if (!mask || $("mode").value !== "disc" || loadingFile || document.activeElement !== $("screen")) return;
      event.preventDefault();
      if (eventName === "keydown") buttons |= mask;
      else buttons &= ~mask;
      emulator.set_buttons(buttons);
    });
  }
  $("screen").addEventListener("blur", releaseButtons);
  window.addEventListener("blur", releaseButtons);
  document.addEventListener("visibilitychange", () => { if (document.hidden) { pause(); render(); } });
  requestAnimationFrame(animate);
  $("controls").disabled = false;
  reset();
} catch (error) {
  $("error").textContent = `Could not load the Wasm module: ${error}. Build web/pkg and serve web/ over HTTP as described in README.`;
  $("status").textContent = "Initialization failed";
}
