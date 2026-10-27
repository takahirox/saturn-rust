import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

export const groups = JSON.parse(readFileSync(new URL("./browser-groups.json", import.meta.url)));
export const shards = JSON.parse(readFileSync(new URL("./browser-shards.json", import.meta.url)));
assert.deepEqual(Object.keys(shards), ["shared", "scenarios"]);
assert.equal(new Set(Object.values(shards).flat()).size, groups.length, "Duplicate/missing shard group");
assert.deepEqual(Object.values(shards).flat().sort(), [...groups].sort(), "Shard coverage must equal the original inventory");

export function selection(args) {
  let mode = "serial", selected = groups, report;
  let subset = false;
  for (const arg of args) {
    if (arg.startsWith("--report=")) {
      assert(!report, "Duplicate report path");
      report = arg.slice(9);
      assert(report, "Empty report path");
    } else if (arg.startsWith("--shard=") || arg.startsWith("--group=")) {
      assert(!subset, "Choose one shard or group");
      subset = true;
      const flag = arg.slice(0, arg.indexOf("=")), name = arg.slice(arg.indexOf("=") + 1);
      if (flag === "--shard") {
        assert(Object.hasOwn(shards, name), `Unknown shard: ${name}`);
        mode = name; selected = shards[name];
      } else {
        assert(shards.scenarios.includes(name), `Group ${name} belongs to a shared trace or is unknown; use --shard=shared for shared UI groups`);
        mode = "scenarios"; selected = [name];
      }
    } else throw new Error(`Unknown browser argument: ${arg}`);
  }
  return { mode, groups: selected, report };
}
