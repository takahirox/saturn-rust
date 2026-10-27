import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
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

test("complete-suite inventory covers all original groups and missing groups fail", async () => {
  const source = await readFile(new URL("./browser-smoke.mjs", import.meta.url), "utf8");
  const registered = [...source.matchAll(/timings\.(?:group|measure)\("([^"]+)"/g)].map(m => m[1]);
  assert.equal(groups.length, 64);
  assert.deepEqual([...new Set(registered)], groups);
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

test("CI gates publication on both suites and retains the actually tested assets", async () => {
  const workflow = await readFile(".github/workflows/pages.yml", "utf8");
  const job = name => workflow.split(`\n  ${name}:\n`)[1]?.split(/\n  \w+:\n/)[0];
  const native = job("native"), build = job("build"), site = job("site"), deploy = job("deploy");
  for (const command of ["cargo fmt --check", "cargo build --locked", "cargo test --locked",
    "cargo clippy --locked --all-targets -- -D warnings", "sh scripts/check-fixtures.sh"]) assert(native.includes(command));
  for (const command of ["sh scripts/build-web.sh", "cargo clippy --locked --target wasm32-unknown-unknown -- -D warnings",
    "node --test scripts/ci-runner.test.mjs", "npm run test:browser"]) assert(build.includes(command));
  assert(!build.includes("needs:"));
  assert.match(site, /needs: \[native, build\]/);
  for (const gated of [site, deploy]) assert(gated.includes("if: github.event_name == 'push' && github.ref == 'refs/heads/main'"));
  assert.match(build, /name: validated-web\s+path: web\s+if-no-files-found: error/);
  assert.match(site, /name: validated-web\s+path: web/);
  assert.match(site, /actions\/upload-pages-artifact@v4/);
  assert.match(deploy, /needs: site/);
  assert.match(deploy, /if \[ "\$latest" = "\$GITHUB_SHA" \]/);
  assert.match(workflow, /cancel-in-progress: \$\{\{ github.event_name == 'pull_request' \}\}/);
  assert(!/continue-on-error|always\(\)|\|\| true/.test(workflow));
  assert.equal(JSON.parse(await readFile("package.json", "utf8")).scripts["test:browser"], "node scripts/browser-smoke.mjs");
  // Exact original registry: moving commands must not lose fixtures/options.
  const fixtures = (await readFile("scripts/check-fixtures.sh", "utf8")).split("\n").filter(s => s.startsWith("python3 "));
  assert.equal(fixtures.length, 90);
  assert(fixtures.includes("python3 tests/fixtures/build_large_media.py --check --output target/large-media"));
  assert(fixtures.every(s => s.includes("--check")));
});
