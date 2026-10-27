import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { lstat, readFile, readdir } from "node:fs/promises";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { shards } from "./browser-selection.mjs";

export function revision() {
  return process.env.GITHUB_SHA ?? execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim();
}

export async function siteDigest(root = "web") {
  const hash = createHash("sha256");
  for (const name of (await readdir(root, { recursive: true })).sort()) {
    const path = join(root, name), stat = await lstat(path);
    assert(stat.isDirectory() || stat.isFile(), `Unexpected site entry: ${name}`);
    if (stat.isFile()) hash.update(`${name}\0${stat.size}\0`).update(await readFile(path));
  }
  return hash.digest("hex");
}

export function verifyResults(reports, { sha, digest }) {
  assert.equal(reports.length, Object.keys(shards).length, "Missing/extra shard report");
  assert.deepEqual(reports.map(r => r.shard).sort(), Object.keys(shards).sort(), "Missing/duplicate/unknown shard");
  for (const report of reports) {
    assert.equal(report.revision, sha, `${report.shard}: stale revision`);
    assert.equal(report.siteDigest, digest, `${report.shard}: stale/corrupted tested artifact`);
    assert.equal(report.status, "passed", `${report.shard}: failed shard`);
    assert.deepEqual(report.groups.map(r => r.name).sort(), [...shards[report.shard]].sort(), `${report.shard}: missing/duplicate/unexpected groups`);
    assert(report.groups.every(r => r.status === "passed"), `${report.shard}: failed group`);
  }
}

export async function verifyDirectory(directory) {
  const names = (await readdir(directory)).sort();
  assert.deepEqual(names, Object.keys(shards).map(s => `${s}.json`).sort(), "Missing/extra shard file");
  const reports = await Promise.all(names.map(name => readFile(join(directory, name), "utf8").then(JSON.parse)));
  verifyResults(reports, { sha: revision(), digest: await siteDigest() });
  console.log(`Browser aggregate passed: ${reports.map(r => `${r.shard}=${r.ms}ms/${r.groups.length} groups`).join(", ")}`);
}
