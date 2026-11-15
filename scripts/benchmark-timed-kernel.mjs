// Actual-board continuation CPU comparison, using original guests only.
// No private firmware/media or synthetic-to-game FPS conversion.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { once } from "node:events";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { resolve } from "node:path";
import { parseArgs } from "node:util";
import { chromium } from "playwright";

const { values } = parseArgs({ options: { pkg: { type: "string", default: "web/pkg" },
  "worker-first": { type: "boolean", default: false },
  "verify-only": { type: "boolean", default: false },
  fields: { type: "string", default: "2" }, runs: { type: "string", default: "6" } } });
const fields = Number(values.fields), runs = Number(values.runs);
assert(Number.isInteger(fields) && fields >= 1 && fields <= 10);
assert(Number.isInteger(runs) && runs >= 2 && runs <= 100);
const assets = new Map(), sha256 = {};
for (const name of ["saturn_rust.js", "saturn_rust_bg.wasm"]) {
  const bytes = await readFile(resolve(values.pkg, name));
  assets.set(`/pkg/${name}`, bytes);
  sha256[name] = createHash("sha256").update(bytes).digest("hex");
}
async function benchmark({ fields, runs, verifyOnly }) {
  const { default: init, TimedKernelExperiment, verify_timed_board, verify_owned_store_board, verify_active_wram_board, verify_loop_probe } = await import("/pkg/saturn_rust.js");
  await init();
  if (!TimedKernelExperiment || !verify_timed_board || !verify_owned_store_board || !verify_active_wram_board || !verify_loop_probe) throw new Error("Use the matching --features kernel-experiment build");
  const conformance = [...verify_timed_board()];
  if (conformance[0] !== 4360) throw new Error("Actual-board CPU fixture did not finish");
  const store_conformance = [...verify_owned_store_board()];
  if (store_conformance[0] !== 5120) throw new Error("Owned-store fixture did not finish");
  const active_conformance = [...verify_active_wram_board()];
  if (active_conformance[0] !== 8) throw new Error("Active-device fixture did not finish");
  const loop_conformance = [...verify_loop_probe()];
  if (loop_conformance[0] !== 4) throw new Error("Master-loop controls did not finish");
  const results = [];
  if (verifyOnly) return { results, conformance, store_conformance, active_conformance, loop_conformance };
  for (const memory of [false, true]) for (const dual of [false, true]) for (const bypass of [false, true]) {
    // Separate, unmeasured normal-Machine control fixes the common end clock
    // and warms both variants before either timed round. No heaps are copied.
    const warm = new TimedKernelExperiment(memory, dual, bypass);
    warm.run_reference(fields);
    const end = warm.reference_clock();
    warm.run_candidate_until(end); warm.verify(); warm.free();
    for (let round = 0; round < runs; round++) {
      const experiment = new TimedKernelExperiment(memory, dual, bypass);
      const ms = [];
      for (const variant of round % 2 ? [1, 0] : [0, 1]) {
        const start = performance.now();
        if (variant === 0) experiment.run_candidate_until(end);
        else experiment.run_reference(fields);
        ms[variant] = performance.now() - start;
      }
      if (experiment.reference_clock() !== end) throw new Error("Reference endpoint changed");
      experiment.verify();
      results.push({ workload: memory ? "load-store" : "register", memory, dual, bypass, round, fields, end_main_clock: end,
        candidate_ms: ms[0], production_machine_ms: ms[1], digest: [...experiment.digest()] });
      experiment.free();
    }
  }
  return { results, conformance, store_conformance, active_conformance, loop_conformance };
}
assets.set("/worker.js", Buffer.from(`onmessage = async ({data}) => {
  try { postMessage({results: await (${benchmark.toString()})(data)}); }
  catch (error) { postMessage({error: String(error?.stack ?? error)}); }
};`));
const server = createServer((req, res) => {
  const path = new URL(req.url, "http://localhost").pathname;
  if (req.method !== "GET") { res.writeHead(405).end(); return; }
  if (path === "/") { res.setHeader("Content-Type", "text/html"); res.end("<!doctype html><title>Kernel experiment</title>"); return; }
  const bytes = assets.get(path);
  if (!bytes) { res.writeHead(404).end(); return; }
  res.setHeader("Content-Type", path.endsWith(".wasm") ? "application/wasm" : "text/javascript");
  res.end(bytes);
});
let browser;
try {
  server.listen(0, "127.0.0.1"); await once(server, "listening");
  browser = await chromium.launch({ headless: true, channel: process.env.BROWSER_CHANNEL });
  const page = await browser.newPage();
  const origin = `http://127.0.0.1:${server.address().port}`;
  await page.route("**/*", route => route.request().url().startsWith(origin + "/") && route.request().method() === "GET" ? route.continue() : route.abort());
  await page.goto(origin + "/");
  const runMain = () => page.evaluate(benchmark, { fields, runs, verifyOnly: values["verify-only"] });
  const runWorker = () => page.evaluate(async ({ fields, runs, verifyOnly }) => {
    let ticks = 0;
    const timer = setInterval(() => ticks++, 10);
    const worker = new Worker("/worker.js", { type: "module" });
    try {
      const results = await new Promise((resolve, reject) => {
        worker.onmessage = ({ data }) => data.error ? reject(new Error(data.error)) : resolve(data.results);
        worker.onerror = event => reject(new Error(event.message));
        worker.postMessage({ fields, runs, verifyOnly });
      });
      return { results: results.results, conformance: results.conformance, store_conformance: results.store_conformance, active_conformance: results.active_conformance, loop_conformance: results.loop_conformance,
        main_timer_ticks: ticks, cross_origin_isolated: crossOriginIsolated };
    } finally { clearInterval(timer); worker.terminate(); }
  }, { fields, runs, verifyOnly: values["verify-only"] });
  let mainResult, worker;
  if (values["worker-first"]) { worker = await runWorker(); mainResult = await runMain(); }
  else { mainResult = await runMain(); worker = await runWorker(); }
  const main = mainResult.results;
  assert(worker.main_timer_ticks > 0, "main thread should service timers during Worker execution");
  assert.deepEqual(worker.conformance, mainResult.conformance, "actual-board CPU digest differs between browser contexts");
  assert.deepEqual(worker.store_conformance, mainResult.store_conformance, "owned-store digest differs between browser contexts");
  assert.deepEqual(worker.active_conformance, mainResult.active_conformance, "active-device digest differs between browser contexts");
  assert.deepEqual(worker.loop_conformance, mainResult.loop_conformance, "master-loop digest differs between browser contexts");
  for (const memory of [false, true]) for (const dual of [false, true]) for (const bypass of [false, true]) {
    const rows = [...main, ...worker.results].filter(row => row.memory === memory && row.dual === dual && row.bypass === bypass);
    for (const row of rows) assert.deepEqual(row.digest, rows[0].digest);
  }
  console.log(JSON.stringify({ contract: "original register/load-store loops; same real board clocks and production Machine cache batches/fallback; not game FPS",
    endpoint: "independent control fixes main clock; both variants settle zero-time CPU issue at that edge; PCM checked outside timing",
    context_order: values["worker-first"] ? ["worker", "main"] : ["main", "worker"],
    conformance: mainResult.conformance, store_conformance: mainResult.store_conformance, active_conformance: mainResult.active_conformance, loop_conformance: mainResult.loop_conformance,
    verify_only: values["verify-only"], browser: browser.version(), sha256, fields, runs, main, worker }));
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }
