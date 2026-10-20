const $ = (id) => document.getElementById(id);
const hex = (value) => `0x${value.toString(16).toUpperCase().padStart(8, "0")}`;
let emulator;
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
}

function act(operation) {
  $("error").textContent = "";
  try { operation(); } catch (error) { $("error").textContent = String(error); }
  render();
}

function reset() {
  act(() => {
    if ($("mode").value === "hle") emulator.reset_hle();
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
    loadingFile = false;
    $("bios").value = "";
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
  $("controls").disabled = false;
  reset();
} catch (error) {
  $("error").textContent = `Could not load the Wasm module: ${error}. Build web/pkg and serve web/ over HTTP as described in README.`;
  $("status").textContent = "Initialization failed";
}
