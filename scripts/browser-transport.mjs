// Playwright encodes each array element as a separate protocol value. Keep bulk
// numeric data in one lossless JSON string, then restore the exact plain array
// before running the original callback/assertions. This is test transport only.
export function pack(value) {
  if (Array.isArray(value)) {
    // findIndex visits holes as undefined, so sparse arrays stay on the normal
    // protocol path instead of JSON silently replacing their holes with null.
    if (value.length >= 4096 && value.findIndex(v => !Number.isFinite(v) || Object.is(v, -0)) === -1) {
      return { __saturnNumericArray: JSON.stringify(value) };
    }
    return value.map(pack);
  }
  if (value && Object.getPrototypeOf(value) === Object.prototype) {
    if (Object.hasOwn(value, "__saturnNumericArray")) throw new Error("Reserved transport key");
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, pack(v)]));
  }
  return value;
}

export function unpack(value) {
  if (Array.isArray(value)) return value.map(unpack);
  if (value && Object.getPrototypeOf(value) === Object.prototype) {
    if (Object.hasOwn(value, "__saturnNumericArray")) {
      if (Object.keys(value).length !== 1) throw new Error("Malformed numeric array envelope");
      return JSON.parse(value.__saturnNumericArray);
    }
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, unpack(v)]));
  }
  return value;
}

export function instrumentPage(page, { compact, record }) {
  const evaluate = page.evaluate.bind(page);
  page.evaluate = async (callback, argument) => {
    if (typeof callback !== "function") throw new Error("Expected a serializable browser callback");
    // Like Playwright itself, compile the trusted test callback's source in the
    // page. No callback executes locally and no closure is transferred.
    const wrapped = new Function(`return async (wire) => {
      const pack = ${pack};
      const unpack = ${unpack};
      const started = performance.now();
      const result = await (${callback})( ${compact ? "unpack(wire)" : "wire"} );
      return { result: ${compact ? "pack(result)" : "result"}, browserMs: performance.now() - started };
    }`)();
    const started = performance.now();
    let browserMs;
    try {
      const response = await evaluate(wrapped, compact ? pack(argument) : argument);
      browserMs = response.browserMs;
      return compact ? unpack(response.result) : response.result;
    } finally {
      record("evaluate-wall", performance.now() - started, browserMs);
    }
  };
  for (const [method, category] of [["goto", "network-and-init"], ["waitForFunction", "polling-wall"]]) {
    const original = page[method].bind(page);
    page[method] = async (...args) => {
      const started = performance.now();
      try { return await original(...args); }
      finally { record(category, performance.now() - started); }
    };
  }
}

export function instrumentBrowser(browser, timings, mode = process.env.BROWSER_TRANSPORT ?? "compact") {
  if (!["compact", "raw"].includes(mode)) throw new Error(`Unknown BROWSER_TRANSPORT: ${mode}`);
  const newContext = browser.newContext.bind(browser);
  browser.newContext = async (...args) => {
    const context = await newContext(...args);
    context.on("page", page => instrumentPage(page, {
      compact: mode === "compact",
      record: (category, ms, browserMs) => timings.operation(category, ms, browserMs),
    }));
    return context;
  };
  console.log(`BROWSER_TRANSPORT ${mode}`);
}
