// Optional continuation-kernel experiment, using original guests only.
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
  count: { type: "string", default: "1000000" }, runs: { type: "string", default: "6" } } });
const count = Number(values.count), runs = Number(values.runs);
assert(Number.isInteger(count) && count > 0 && count <= 0xFFFFFFFF);
assert(Number.isInteger(runs) && runs >= 2 && runs <= 100);
const assets = new Map(), sha256 = {};
for (const name of ["saturn_rust.js", "saturn_rust_bg.wasm"]) {
  const bytes = await readFile(resolve(values.pkg, name));
  assets.set(`/pkg/${name}`, bytes);
  sha256[name] = createHash("sha256").update(bytes).digest("hex");
}
async function benchmark({ count, runs }) {
  const { default: init, KernelExperiment, verify_owned_read_port } = await import("/pkg/saturn_rust.js");
  await init();
  if (!KernelExperiment || !verify_owned_read_port) throw new Error("Build with --features kernel-experiment first");
  // Component conformance precedes the timed rounds. These are actual board
  // reads, separate from the CPU experiment's deliberately synthetic bus.
  const read_port = [...verify_owned_read_port()];
  if (read_port[0] !== 904) throw new Error("Owned read-port fixture did not finish");
  const results = [];
  for (const memory of [false, true]) {
    const warm = new KernelExperiment(memory, 1);
    warm.run_kernel(100000); warm.run_reference(100000); warm.verify(); warm.free();
    for (let round = 0; round < runs; round++) {
      const experiment = new KernelExperiment(memory, 1);
      const ms = [];
      for (const variant of round % 2 ? [1, 0] : [0, 1]) {
        const start = performance.now();
        if (variant === 0) experiment.run_kernel(count);
        else experiment.run_reference(count);
        ms[variant] = performance.now() - start;
      }
      experiment.verify();
      results.push({ memory, round, instructions_per_cpu: count,
        kernel_ms: ms[0], immediate_reference_ms: ms[1], digest: [...experiment.digest()] });
      experiment.free();
    }
  }
  return { results, read_port };
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
  const mainResult = await page.evaluate(benchmark, { count, runs });
  const main = mainResult.results;
  const worker = await page.evaluate(async ({ count, runs }) => {
    let ticks = 0;
    const timer = setInterval(() => ticks++, 10);
    const worker = new Worker("/worker.js", { type: "module" });
    try {
      const results = await new Promise((resolve, reject) => {
        worker.onmessage = ({ data }) => data.error ? reject(new Error(data.error)) : resolve(data.results);
        worker.onerror = event => reject(new Error(event.message));
        worker.postMessage({ count, runs });
      });
      return { results: results.results, read_port: results.read_port,
        main_timer_ticks: ticks, cross_origin_isolated: crossOriginIsolated };
    } finally { clearInterval(timer); worker.terminate(); }
  }, { count, runs });
  assert(worker.main_timer_ticks > 0, "main thread should service timers during Worker execution");
  assert.deepEqual(worker.read_port, mainResult.read_port, "owned read-port digest differs between browser contexts");
  for (const memory of [false, true]) {
    const expected = main.find(row => row.memory === memory).digest;
    for (const row of [...main, ...worker.results].filter(row => row.memory === memory)) assert.deepEqual(row.digest, expected);
  }
  console.log(JSON.stringify({ contract: "serial synthetic bus; immediate-RAM ISA reference, not production Machine or game FPS",
    read_port_contract: "WRAM component: production arbiter/BSC read values and completion edges; CPU IF/MA integration remains pending",
    read_port: mainResult.read_port, browser: browser.version(), sha256, count, runs, main, worker }));
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)); }
