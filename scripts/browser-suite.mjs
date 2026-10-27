// Two independent Node/Chromium process trees, capped regardless of host CPU
// count. The original single-process order remains available with --serial.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { selection, shards } from "./browser-selection.mjs";
import { verifyDirectory } from "./browser-results.mjs";

export function runWorker(args, { timeout = (process.env.BROWSER_TRANSPORT === "raw" ? 60 : 20) * 60 * 1000, nodeArgs = [] } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [...nodeArgs, "scripts/browser-smoke.mjs", ...args], {
      stdio: "inherit", detached: process.platform !== "win32",
    });
    const kill = signal => {
      if (process.platform === "win32") child.kill(signal);
      else {
        try { process.kill(-child.pid, signal); }
        catch (error) { if (error.code !== "ESRCH") throw error; }
      }
    };
    let expired = false, cancelled = false, stopping = false, force;
    const stop = () => {
      if (stopping) return;
      stopping = true;
      kill("SIGTERM");
      force = setTimeout(() => kill("SIGKILL"), 5000);
    };
    const cancel = () => { cancelled = true; stop(); };
    process.once("SIGINT", cancel);
    process.once("SIGTERM", cancel);
    const cleanup = () => {
      clearTimeout(timer); clearTimeout(force);
      process.removeListener("SIGINT", cancel);
      process.removeListener("SIGTERM", cancel);
    };
    const timer = setTimeout(() => {
      expired = true;
      stop();
    }, timeout);
    child.once("error", error => { cleanup(); reject(error); });
    child.once("close", (code, signal) => {
      cleanup();
      if (expired || cancelled || signal || code !== 0) reject(new Error(`Browser worker ${args.join(" ")} failed: code=${code}, signal=${signal}, timeout=${expired}, cancelled=${cancelled}`));
      else resolve();
    });
  });
}

export async function runSuite(args = process.argv.slice(2)) {
  if (args[0] === "--verify") {
    assert.equal(args.length, 2, "Usage: --verify report-directory");
    await verifyDirectory(args[1]);
  } else if (args[0] === "--serial") {
    assert.equal(args.length, 1);
    await runWorker([]);
  } else if (args.length) {
    selection(args); // Reject misspellings before launching any worker.
    await runWorker(args);
  } else {
    const directory = await mkdtemp(join(tmpdir(), "saturn-browser-results-"));
    const started = performance.now();
    try {
      const results = await Promise.allSettled(Object.keys(shards).map(shard =>
        runWorker([`--shard=${shard}`, `--report=${join(directory, `${shard}.json`)}`])));
      for (const result of results) if (result.status === "rejected") throw result.reason;
      await verifyDirectory(directory);
      console.log(`BROWSER_SUITE ${JSON.stringify({ ms: Math.round(performance.now() - started), workers: 2, status: "passed" })}`);
    } finally { await rm(directory, { recursive: true, force: true }); }
  }
}

// Importable for real worker-failure regressions without running a second suite.
if (process.argv[1]?.endsWith("/browser-suite.mjs") || process.argv[1] === "scripts/browser-suite.mjs") {
  await runSuite();
}
