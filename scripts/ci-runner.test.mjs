import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile, mkdtemp, mkdir, writeFile, rm, access, cp, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "node:http";
import { once } from "node:events";
import { shards, selection } from "./browser-selection.mjs";
import { siteDigest, revision, verifyResults } from "./browser-results.mjs";
import { runWorker } from "./browser-suite.mjs";
import { videoWrites } from "./video-writes.mjs";
import { vdp2Status } from "./vdp2-status.mjs";
import { spawnSync } from "node:child_process";
import { chromium } from "playwright";
import { pack, unpack, instrumentPage } from "./browser-transport.mjs";
import { BrowserTimings } from "./browser-timings.mjs";
import { readFileSync } from "node:fs";
const groups = JSON.parse(readFileSync(new URL("./browser-groups.json", import.meta.url)));

test("bulk transport round trips every number and preserves special values", () => {
  const value = { rom: Array.from({ length: 524288 }, (_, i) => i % 256),
    pcm: Array.from({ length: 8192 }, (_, i) => i - 4096),
    special: [NaN, Infinity, -Infinity, -0, undefined, 1n], empty: [], absent: undefined };
  const packed = pack(value);
  assert.equal(typeof packed.rom.__saturnNumericArray, "string");
  assert.equal(typeof packed.pcm.__saturnNumericArray, "string");
  assert.deepEqual(unpack(packed), value);
  assert.deepEqual(value.rom.slice(0, 4), [0, 1, 2, 3]);
  const sparse = Array(4096); sparse[0] = 1;
  assert.deepEqual(unpack(pack(sparse)), sparse);
  for (const special of [NaN, Infinity, -Infinity, -0, undefined]) {
    const array = Array(4096).fill(1); array[17] = special;
    assert.deepEqual(unpack(pack(array)), array);
  }
  assert.throws(() => pack({ __saturnNumericArray: "collision" }), /Reserved/);
  assert.throws(() => unpack({ __saturnNumericArray: "[]", extra: 1 }), /Malformed/);
});

test("real Chromium callbacks retain all results, state, imports and failures in both modes", async () => {
  const browser = await chromium.launch();
  try {
    const results = [];
    for (const compact of [false, true, true, false]) {
      const context = await browser.newContext();
      try {
        const page = await context.newPage();
        const measurements = [];
        instrumentPage(page, { compact, record: (...row) => measurements.push(row) });
        await page.goto("data:text/html,<title>transport</title>");
        const input = { bytes: Array.from({ length: 524288 }, (_, i) => i % 256) };
        const result = await page.evaluate(async ({ bytes }) => {
          const { number } = await import("data:text/javascript,export const number=17");
          window.transportCount = (window.transportCount ?? 0) + 1;
          return { bytes: bytes.map(v => 255 - v), pcm: Array.from({ length: 8192 }, (_, i) => i - 4096),
            special: [NaN, Infinity, -0, undefined, 1n], count: window.transportCount, number };
        }, input);
        assert.equal(result.count, 1);
        assert.deepEqual(result.bytes, input.bytes.map(v => 255 - v));
        assert.equal(await page.evaluate(() => ++window.transportCount), 2);
        await assert.rejects(page.evaluate(() => { throw new Error("representative assertion failure"); }), /representative assertion failure/);
        await assert.rejects(page.evaluate(async () => { throw new Error("rejected guest promise"); }), /rejected guest promise/);
        await page.close();
        await assert.rejects(page.evaluate(() => 1), /closed/);
        assert(measurements.some(([category, wall, browserMs]) => category === "evaluate-wall" && wall >= browserMs));
        results.push(result);
      } finally { await context.close(); }
    }
    for (const result of results) assert.deepEqual(result, results[0]);
  } finally { await browser.close(); }
});

test("shared versus fresh contexts retain complete guest clock, DMAC, Canvas and PCM results", async () => {
  const assets = new Set(await readdir("web", { recursive: true }));
  const server = createServer(async (request, response) => {
    const path = new URL(request.url, "http://localhost").pathname;
    const name = path === "/saturn-rust/" ? "index.html" : path.slice("/saturn-rust/".length);
    try {
      assert(path.startsWith("/saturn-rust/") && assets.has(name));
      response.setHeader("Content-Type", name.endsWith(".wasm") ? "application/wasm" :
        name.endsWith(".js") ? "text/javascript" : name.endsWith(".css") ? "text/css" : "text/html");
      response.end(await readFile(join("web", name)));
    } catch { response.writeHead(404); response.end(); }
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  let browser;
  try {
    browser = await chromium.launch();
    const errors = [], contexts = [];
    const fresh = async () => {
      const context = await browser.newContext(); contexts.push(context);
      const page = await context.newPage();
      page.on("pageerror", error => errors.push(error.message));
      instrumentPage(page, { compact: true, record: () => {} });
      await page.goto(`http://127.0.0.1:${server.address().port}/saturn-rust/`);
      await page.waitForFunction(() => document.getElementById("status").textContent === "Ready");
      return page;
    };
    const captureVideo = async page => {
      const results = [], evaluate = page.evaluate.bind(page);
      page.evaluate = async (...args) => {
        const value = await evaluate(...args); results.push(value); return value;
      };
      try { await videoWrites(page); } finally { page.evaluate = evaluate; }
      return results;
    };
    const rom = Buffer.alloc(524288);
    for (const line of (await readFile("tests/fixtures/scsp-clock.hex", "utf8")).split(/\r?\n/)) {
      if (!line || line.startsWith("#")) continue;
      const [address, ...words] = line.split(/\s+/); let at = parseInt(address, 16);
      for (const word of words) { rom.writeUInt16BE(parseInt(word, 16), at); at += 2; }
    }
    const captureSound = page => page.evaluate(async bytes => {
      const { emulator } = await import("./app.js");
      emulator.reset_bios(Uint8Array.from(bytes));
      return Array.from({ length: 10 }, () => {
        emulator.run_steps(10000);
        return { clocks: emulator.peripheral_clocks(), retired: emulator.instruction_count(),
          registers: [...emulator.registers()], special: [...emulator.special_registers()],
          exceptions: [emulator.exception_count(), emulator.slave_exception_count()],
          records: [emulator.read_long(0x0600EA00), emulator.read_long(0x0600EA04)],
          generated: emulator.audio_generated(), pcm: [...emulator.audio_samples(8192)] };
      });
    }, [...rom]);
    const shared = await fresh();
    await vdp2Status(shared); // Prior independent guest on the original shared page.
    const sharedVideo = await captureVideo(shared), sharedSound = await captureSound(shared);
    const isolatedVideo = await captureVideo(await fresh());
    const isolatedSound = await captureSound(await fresh());
    assert.equal(sharedVideo.length, 6);
    assert(sharedVideo.some(result => result.canvas.some((value, i) => i % 4 !== 3 && value !== 0)));
    assert(sharedSound.some(result => result.pcm.some(value => value !== 0)));
    assert.deepEqual(isolatedVideo, sharedVideo, "Every returned pixel, guest record and dual-SH-2 DMAC/FRT result");
    assert.deepEqual(isolatedSound, sharedSound, "Every PCM sample, clock, register and exception state");
    assert.deepEqual(errors, []);
    for (const context of contexts) await context.close();
  } finally {
    if (browser) await browser.close();
    await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  }
});

test("complete-suite inventory covers all original groups and missing groups fail", async () => {
  const source = await readFile(new URL("./browser-smoke.mjs", import.meta.url), "utf8");
  const registered = [...source.matchAll(/(?:timings\.(?:group|measure)|setup)\("([^"]+)"/g)].map(m => m[1]);
  assert.equal(groups.length, 65);
  assert(groups.includes("sh2Store"));
  assert.deepEqual([...new Set(registered)], groups);
  assert.deepEqual(shards.scenarios, [...source.matchAll(/timings.measure\("([^"]+)"/g)].map(m => m[1]));
  assert.deepEqual(shards.shared, groups.filter(g => !shards.scenarios.includes(g)));
  const scenarioRegistry = source.match(/const scenarios = \{ ([^}]+) \}/)[1].split(/,\s+/);
  assert.deepEqual([...scenarioRegistry, "largeMedia"].sort(), [...shards.scenarios].sort());
  assert.equal((source.match(/if \(includeScenarios\) await timings.measure/g) ?? []).length, 35);
  assert.deepEqual(selection([]).groups, groups);
  for (const args of [["--shard=missing"], ["--group=missing"], ["--shard=shared", "--group=scspMvol"], ["--typo"]]) {
    assert.throws(() => selection(args));
  }
  const timedCalls = [...source.matchAll(/timings\.measure\("([^"]+)", async \(\) => \{\s+await (\w+)\(/g)];
  const measuredNames = [...source.matchAll(/timings\.measure\("([^"]+)"/g)].map(m => m[1]);
  assert.deepEqual(timedCalls.map(m => m[1]), measuredNames, "Every timed module must invoke its test");
  for (const match of timedCalls) assert.equal(match[1], match[2]);
  // Baseline has 598 direct assertion call sites in the integrated runner.
  // Scenario modules themselves are unchanged by this optimization.
  assert(source.match(/\bassert(?:\.\w+)?\(/g).length >= 598);
  assert(source.indexOf("timings.assertComplete();") < source.indexOf("timings.success = true;"));
  const timings = new BrowserTimings();
  timings.group(groups[0]);
  assert.throws(() => timings.assertComplete(), /Every registered browser group/);
  timings.report();
});

test("timed commands preserve representative failure and print failure timings", () => {
  const child = spawnSync("python3", ["scripts/ci-time.py", "negative-control", process.execPath,
    "-e", "require('node:assert/strict').equal(1,2)"], { encoding: "utf8" });
  assert.equal(child.status, 1);
  assert.match(child.stdout, /AssertionError/);
  assert.match(child.stdout, /CI_TIMING .*"exit_code": 1/);
});

test("a deliberately failing real browser assertion fails the complete command", () => {
  // Inject only into this disposable child; no production failure/skip flag.
  const source = `
    import assert from 'node:assert/strict';
    await import('./scripts/browser-selection.mjs');
    await import('./scripts/browser-timings.mjs');
    const equal = assert.equal;
    assert.equal = () => equal(1, 2, 'deliberate representative browser assertion');
    await import('./scripts/browser-smoke.mjs');
  `;
  const child = spawnSync(process.execPath, ["--input-type=module", "-e", source], { encoding: "utf8" });
  assert.equal(child.status, 1);
  assert.match(child.stderr, /deliberate representative browser assertion/);
  assert.match(child.stdout, /BROWSER_TOTAL .*"status":"failed"/);
  assert.match(child.stdout, /assertions-wall/);
  assert(!child.stdout.includes("Browser smoke passed"));
});

test("a real isolated guest assertion fails through the supervisor and removes stale success", async () => {
  const directory = await mkdtemp(join(tmpdir(), "saturn-negative-shard-"));
  const report = join(directory, "scenarios.json");
  try {
    await writeFile(report, '{"status":"passed"}');
    const preload = `import assert from 'node:assert/strict';
      const original = assert.deepEqual;
      assert.deepEqual = (actual, expected, ...rest) => {
        if (actual?.words?.length === 12) actual.words[0] ^= 1;
        return original(actual, expected, ...rest);
      };`;
    const source = `import { runWorker } from './scripts/browser-suite.mjs';
      await runWorker(${JSON.stringify(["--group=vdp2Status", `--report=${report}`])},
        { nodeArgs: ['--import', ${JSON.stringify(`data:text/javascript,${encodeURIComponent(preload)}`)}] });`;
    const child = spawnSync(process.execPath, ["--input-type=module", "-e", source], { encoding: "utf8" });
    assert.equal(child.status, 1);
    assert.match(child.stderr, /AssertionError/);
    assert.match(child.stderr, /Browser worker .*failed: code=1/);
    assert.match(child.stdout, /BROWSER_TIMING .*"name":"vdp2Status".*"status":"failed"/);
    assert.match(child.stdout, /BROWSER_TOTAL .*"status":"failed"/);
    await assert.rejects(access(report), /ENOENT/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("crashed, rejected and timed-out workers fail the supervisor", async () => {
  for (const source of ["process.kill(process.pid, 'SIGKILL')", "Promise.reject(new Error('unhandled guest rejection'))"]) {
    await assert.rejects(runWorker([], { nodeArgs: ["-e", source] }), /Browser worker .*failed/);
  }
  await assert.rejects(runWorker([], { nodeArgs: ["-e", "setInterval(() => {}, 1000)"], timeout: 100 }), /timeout=true/);
  const source = `import { runWorker } from './scripts/browser-suite.mjs';
    setTimeout(() => process.kill(process.pid, 'SIGTERM'), 100);
    await runWorker([], {nodeArgs: ['-e', 'setInterval(() => {}, 1000)']});`;
  const cancelled = spawnSync(process.execPath, ["--input-type=module", "-e", source], { encoding: "utf8" });
  assert.equal(cancelled.status, 1);
  assert.match(cancelled.stderr, /cancelled=true/);
});

test("aggregate rejects missing/duplicate groups or shards, failed assertions, stale revisions and changed assets", async () => {
  const digest = await siteDigest(), sha = revision();
  const good = Object.entries(shards).map(([shard, names]) => ({ shard, revision: sha, siteDigest: digest,
    status: "passed", groups: names.map(name => ({ name, status: "passed" })) }));
  verifyResults(good, { sha, digest });
  const bad = mutate => {
    const reports = structuredClone(good); mutate(reports);
    assert.throws(() => verifyResults(reports, { sha, digest }));
  };
  bad(r => r.pop());
  bad(r => { r[1] = r[0]; });
  bad(r => r[0].groups.pop());
  bad(r => r[0].groups.push(r[0].groups[0]));
  bad(r => { r[0].status = "failed"; });
  bad(r => { r[0].groups[0].status = "failed"; });
  bad(r => { r[0].revision = "stale"; });
  bad(r => { r[1].siteDigest = "stale"; });
  const directory = await mkdtemp(join(tmpdir(), "saturn-aggregate-"));
  const verify = () => spawnSync(process.execPath, ["scripts/browser-suite.mjs", "--verify", directory], { encoding: "utf8" });
  try {
    await writeFile(join(directory, "shared.json"), JSON.stringify(good[0]));
    assert.equal(verify().status, 1, "A missing shard must fail the real CI aggregate command");
    await writeFile(join(directory, "scenarios.json"), JSON.stringify(good[1]));
    assert.equal(verify().status, 0);
    await writeFile(join(directory, "scenarios.json"), JSON.stringify({ ...good[1], siteDigest: "stale" }));
    const stale = verify();
    assert.equal(stale.status, 1); assert.match(stale.stderr, /stale\/corrupted tested artifact/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("a corrupted real Wasm artifact fails a required shard before browser launch", async () => {
  const directory = await mkdtemp(join(tmpdir(), "saturn-corrupt-site-"));
  try {
    await cp("web", join(directory, "web"), { recursive: true });
    await writeFile(join(directory, "web/pkg/saturn_rust_bg.wasm"), "");
    const child = spawnSync(process.execPath, [fileURLToPath(new URL("./browser-smoke.mjs", import.meta.url)), "--shard=shared"], {
      cwd: directory, encoding: "utf8",
    });
    assert.equal(child.status, 1);
    assert.match(child.stderr, /Expected a nonempty regular file: pkg\/saturn_rust_bg.wasm/);
    assert.match(child.stdout, /BROWSER_TOTAL .*"status":"failed"/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("web build binds the newly compiled custom-target artifact and rejects a stale cached CLI", async () => {
  const directory = await mkdtemp(join(tmpdir(), "saturn-build-target-"));
  try {
    await mkdir(join(directory, "scripts"));
    await mkdir(join(directory, "bin"));
    const oldTarget = join(directory, "target/wasm32-unknown-unknown/release");
    await mkdir(oldTarget, { recursive: true });
    await writeFile(join(oldTarget, "saturn_rust.wasm"), "stale-wasm\n");
    await writeFile(join(directory, "scripts/build-web.sh"), await readFile("scripts/build-web.sh"));
    const tool = (name, body) => writeFile(join(directory, "bin", name), `#!/bin/sh\nset -eu\n${body}\n`, { mode: 0o755 });
    // Only tool plumbing is faked in this disposable directory. The full CI
    // paths separately build real fixtures, compile real Wasm and run Chromium.
    await tool("python3", "exit 0");
    await tool("cp", "exit 0");
    await tool("cargo", 'mkdir -p "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release"\nprintf "fresh-wasm\\n" > "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/saturn_rust.wasm"');
    await tool("wasm-bindgen", 'if [ "$1" = "--version" ]; then cat tool-version.txt; exit 0; fi\nfor input do :; done\ntest "$(cat "$input")" = fresh-wasm\nprintf "%s" "$input" > selected.txt');
    await writeFile(join(directory, "tool-version.txt"), "wasm-bindgen 0.2.129\n");
    const customTarget = join(directory, "custom targets");
    const run = () => spawnSync("sh", ["scripts/build-web.sh"], { cwd: directory, encoding: "utf8",
      env: { ...process.env, PATH: `${join(directory, "bin")}:${process.env.PATH}`, CARGO_TARGET_DIR: customTarget } });
    const built = run();
    assert.equal(built.status, 0, built.stderr);
    assert.equal(await readFile(join(directory, "selected.txt"), "utf8"), join(customTarget, "wasm32-unknown-unknown/release/saturn_rust.wasm"));
    await writeFile(join(directory, "tool-version.txt"), "wasm-bindgen 0.2.128\n");
    const stale = run();
    assert.equal(stale.status, 1);
    assert.match(stale.stderr, /Install the matching CLI/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("CI gates publication on both suites and retains the actually tested assets", async () => {
  const workflow = await readFile(".github/workflows/pages.yml", "utf8");
  const job = name => workflow.split(`\n  ${name}:\n`)[1]?.split(/\n  \w+:\n/)[0];
  const native = job("native"), wasm = job("wasm"), browser = job("browser"), build = job("build"), site = job("site"), deploy = job("deploy");
  for (const command of ["cargo fmt --check", "cargo build --locked", "cargo test --locked",
    "cargo clippy --locked --all-targets -- -D warnings", "sh scripts/check-fixtures.sh"]) assert(native.includes(command));
  for (const command of ["sh scripts/build-web.sh", "cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings"]) assert(wasm.includes(command));
  assert.match(wasm, /path: ~\/\.cargo\/bin\/wasm-bindgen/);
  assert.match(wasm, /key: wasm-bindgen-ubuntu24-\$\{\{ runner.os \}\}-\$\{\{ runner.arch \}\}-0.2.129/);
  assert.match(wasm, /if: steps.bindgen-cache.outputs.cache-hit != 'true'/);
  assert(wasm.includes("cargo install wasm-bindgen-cli --version 0.2.129 --locked"));
  assert(!wasm.includes("restore-keys"), "Do not restore a differently versioned executable");
  const buildWeb = await readFile("scripts/build-web.sh", "utf8");
  assert(buildWeb.includes('expected="wasm-bindgen 0.2.129"'));
  assert(buildWeb.includes('[ "$(wasm-bindgen --version)" != "$expected" ]'));
  const testProfile = (await readFile("Cargo.toml", "utf8")).split("[profile.test]")[1];
  assert.match(testProfile, /opt-level = 3/);
  assert.match(testProfile, /debug-assertions = true/);
  assert.match(testProfile, /overflow-checks = true/);
  for (const command of ["node --test scripts/ci-runner.test.mjs", "npm run test:browser -- --shard=${{ matrix.shard }} --report=target/browser-results/${{ matrix.shard }}.json"]) assert(browser.includes(command));
  assert.match(browser, /needs: wasm/);
  assert.match(browser, /fail-fast: false/);
  assert.match(browser, /max-parallel: 2/);
  assert.match(browser, /shard: \[shared, scenarios\]/);
  assert.match(browser, /name: web-build\s+path: web/);
  assert.match(browser, /name: browser-result-\$\{\{ matrix.shard \}\}/);
  assert(!wasm.includes("needs:"));
  assert.match(build, /needs: \[native, wasm, browser\]/);
  assert.match(build, /node scripts\/browser-suite.mjs --verify target\/browser-results/);
  assert.match(build, /pattern: browser-result-\*/);
  assert.match(build, /name: web-build\s+path: web/);
  assert.match(site, /needs: \[native, wasm, browser, build\]/);
  for (const gated of [site, deploy]) assert(gated.includes("if: github.event_name == 'push' && github.ref == 'refs/heads/main'"));
  assert.match(wasm, /name: web-build\s+path: web\s+if-no-files-found: error/);
  assert.match(site, /name: web-build\s+path: web/);
  assert.match(site, /actions\/upload-pages-artifact@v4/);
  assert(!site.includes("build-web"));
  assert.match(deploy, /needs: site/);
  assert.match(deploy, /if \[ "\$latest" = "\$GITHUB_SHA" \]/);
  assert.match(workflow, /cancel-in-progress: \$\{\{ github.event_name == 'pull_request' \}\}/);
  assert(!/continue-on-error|\|\| true/.test(workflow));
  assert.match(build, /if: always\(\)/);
  for (const other of [native, wasm, browser, site, deploy]) assert(!other.includes("always()"));
  const guard = build.match(/run: \|\n((?:          .*\n)+)/)[1].replace(/^          /gm, "");
  for (const key of ["NATIVE_RESULT", "WASM_RESULT", "BROWSER_RESULT"]) {
    assert(guard.includes(`test "$${key}" = success`));
    for (const status of ["failure", "cancelled", "skipped"]) {
      const result = spawnSync("bash", ["-e", "-c", guard], { encoding: "utf8", env: {
        ...process.env, NATIVE_RESULT: "success", WASM_RESULT: "success", BROWSER_RESULT: "success", [key]: status,
      } });
      assert.equal(result.status, 1, `${key}=${status} must fail the retained build check`);
    }
  }
  assert.equal(spawnSync("bash", ["-e", "-c", guard], { env: { ...process.env,
    NATIVE_RESULT: "success", WASM_RESULT: "success", BROWSER_RESULT: "success" } }).status, 0);
  assert.equal(JSON.parse(await readFile("package.json", "utf8")).scripts["test:browser"], "node scripts/browser-suite.mjs");
  // Exact original registry: moving commands must not lose fixtures/options.
  const fixtures = (await readFile("scripts/check-fixtures.sh", "utf8")).split("\n").filter(s => s.startsWith("python3 "));
  assert.equal(fixtures.length, 91);
  assert(fixtures.includes("python3 tests/fixtures/build_sh2_store.py --check"));
  assert(fixtures.includes("python3 tests/fixtures/build_large_media.py --check --output target/large-media"));
  assert(fixtures.every(s => s.includes("--check")));
});
