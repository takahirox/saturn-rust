import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { runInNewContext } from "node:vm";

// Execute the production loop with a virtual host clock. These assertions do
// not require spare CPU capacity on a shared browser/CI machine.
const source = readFileSync(new URL("../web/playback-worker.js", import.meta.url), "utf8");
const from = source.indexOf("async function start(");
const to = source.indexOf("\nasync function dispatch(", from);
const budget = source.match(/^const MAX_PACING_LAG_FIELDS = .*;$/m)?.[0];
assert(from >= 0 && to > from && budget, "production pacing loop must be exercised");

async function simulate({ period = 449204000 / 26874100, cost, paced = true,
  stallAt = 0, stallMs = 0, duration = 5000 }) {
  let now = 0, fields = 0;
  const delays = [], messages = [];
  const context = {
    playing: false, playWanted: true, run: Promise.resolve(), epoch: 0,
    performance: { now: () => now },
    self: { postMessage: message => messages.push(message) },
    stop: async () => { context.playing = false; },
    delay: async ms => {
      assert(ms > 0);
      delays.push({ after: fields, ms, at: now }); now += ms;
      if (now >= duration) context.playing = false;
    },
    produce: async automatic => {
      if (automatic && !context.playing) return null;
      fields++;
      now += cost(fields, period);
      if (fields === stallAt) now += stallMs;
      if (now >= duration) context.playing = false;
      assert(fields < 100000, "virtual producer must remain bounded");
      return period;
    },
  };
  runInNewContext(`${budget}\n${source.slice(from, to)}\nglobalThis.begin = start;`, context);
  await context.begin({ paced }); await context.run;
  assert.deepEqual(messages, []);
  return { now, fields, delays, guestMs: fields * period };
}

test("short slow fields retain nominal cadence without requiring host headroom", async () => {
  for (const period of [449204000 / 26874100, 20]) {
    const result = await simulate({ period, cost: (field, p) => (field % 22 ? 0.84 : 3.24) * p });
    assert(Math.abs(result.guestMs - 5000) <= period * 2, JSON.stringify(result));
    assert(result.delays.length > 0, "spare time must still be paced");
  }
});

test("a long host stall rebases rather than replaying the wall-time backlog", async () => {
  const period = 449204000 / 26874100;
  const result = await simulate({ period, cost: (_, p) => p / 10, stallAt: 60, stallMs: 1000 });
  assert(Math.abs(result.guestMs - 4000) <= period * 2, JSON.stringify(result));
  const nextDelay = result.delays.find(delay => delay.after >= 60);
  assert(nextDelay && nextDelay.after <= 62, "a long stall must not trigger a long catch-up burst");
});

test("a permanently slow host keeps every produced field and reports its lower rate", async () => {
  const period = 449204000 / 26874100;
  const result = await simulate({ period, cost: (_, p) => p * 2 });
  assert.equal(result.fields, Math.ceil(5000 / (period * 2)));
  assert.equal(result.delays.length, 0, "a slow producer must not add artificial waiting");
});

test("uncapped playback does not acquire pacing waits", async () => {
  const result = await simulate({ cost: () => 1, paced: false });
  assert.equal(result.fields, 5000);
  assert.equal(result.delays.length, 0);
});
