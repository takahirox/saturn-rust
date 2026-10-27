import { performance } from "node:perf_hooks";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
const groups = JSON.parse(readFileSync(new URL("./browser-groups.json", import.meta.url)));

// Timings never swallow failures; the active group is reported by finally.
export class BrowserTimings {
  rows = [];
  success = false;
  started = performance.now();
  operations = {};

  constructor() {
    // Includes early asset/server failures, before the browser's try/finally.
    process.once("exit", () => { if (!this.reported) this.report(); });
  }

  instrumentAssertions(original) {
    const measure = (run, args) => {
      const started = performance.now();
      try { return Reflect.apply(run, original, args); }
      finally { this.operation("assertions-wall", performance.now() - started); }
    };
    // Imported scenarios share Node's strict assert methods. Bare assertions in
    // the integration runner additionally use the proxy's call trap.
    for (const name of ["equal", "deepEqual", "match", "throws", "notEqual", "notDeepEqual"]) {
      const run = original[name];
      original[name] = (...args) => measure(run, args);
    }
    return new Proxy(original, { apply: (run, _receiver, args) => measure(run, args) });
  }

  async instrument(page) {
    const evaluate = page.evaluate.bind(page);
    await evaluate(async () => {
      const { Emulator } = await import("./pkg/saturn_rust.js");
      window.browserWasmTimings = {};
      for (const name of ["run_steps", "advance_frame", "reset_bios", "reset_hle", "framebuffer", "mount_disc", "mount_cue", "audio_samples"]) {
        const original = Emulator.prototype[name];
        if (!original) continue;
        Emulator.prototype[name] = function (...args) {
          const started = performance.now();
          try { return original.apply(this, args); }
          finally {
            const row = window.browserWasmTimings[name] ??= { calls: 0, ms: 0 };
            row.calls++; row.ms += performance.now() - started;
          }
        };
      }
    });
    this.wasmReport = async () => console.log(`BROWSER_WASM ${JSON.stringify(await evaluate(() => window.browserWasmTimings))}`);
  }

  operation(category, ms, browserMs) {
    const key = `${this.active?.name}/${category}`;
    const row = this.operations[key] ??= { calls: 0, ms: 0, browserMs: 0 };
    row.calls++; row.ms += ms; row.browserMs += browserMs ?? 0;
  }

  group(name) {
    this.finish();
    this.active = { name, started: performance.now() };
  }

  finish(status = "passed") {
    if (!this.active) return;
    const row = { name: this.active.name, ms: Math.round(performance.now() - this.active.started), status };
    this.rows.push(row);
    console.log(`BROWSER_TIMING ${JSON.stringify(row)}`);
    this.active = null;
  }

  async measure(name, run) {
    const parent = this.active?.name;
    this.group(name);
    try {
      const result = await run();
      this.finish();
      return result;
    } catch (error) {
      this.finish("failed");
      throw error;
    } finally {
      if (parent) this.group(parent);
    }
  }

  report() {
    this.reported = true;
    this.finish(this.success ? "passed" : "failed");
    const totals = new Map();
    for (const row of this.rows) {
      const total = totals.get(row.name) ?? { name: row.name, ms: 0, status: "passed" };
      total.ms += row.ms;
      if (row.status !== "passed") total.status = row.status;
      totals.set(row.name, total);
    }
    console.log(`BROWSER_TOTAL ${JSON.stringify({ ms: Math.round(performance.now() - this.started), status: this.success ? "passed" : "failed", groups: totals.size, expected: groups.length })}`);
    console.log(`BROWSER_MANIFEST ${JSON.stringify([...totals.values()])}`);
    console.log("Slowest browser groups:", [...totals.values()].sort((a, b) => b.ms - a.ms).slice(0, 10));
    console.log(`BROWSER_OPERATIONS ${JSON.stringify(this.operations)}`);
  }

  assertComplete() {
    const names = new Set([...this.rows.map(row => row.name), this.active?.name].filter(Boolean));
    assert.deepEqual([...names].sort(), [...groups].sort(), "Every registered browser group must execute");
    assert(this.rows.every(row => row.status === "passed"), "A browser group failed");
  }
}
