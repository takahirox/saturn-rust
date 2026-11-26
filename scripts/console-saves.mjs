// Production Worker, original guest saves and local IndexedDB transactions.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { waitForGuest, waitForLoadedDisc } from "./browser-transport.mjs";

const TRACE = 0x06024000;
const BIOS_TRACE = 0x06028000;
const filename = (name, buffer) => ({ name, mimeType: "application/octet-stream", buffer });
const checksum = bytes => createHash("sha256").update(bytes).digest("hex");
const text = (page, id) => page.locator("#" + id).textContent();
const settings = page => page.locator("#settings > summary");
const ready = page => page.waitForFunction(() => !document.getElementById("controls").disabled);
const standby = page => page.waitForFunction(() => document.getElementById("main").getAttribute("aria-busy") === "false");
const status = (page, from) => page.waitForFunction(from => document.getElementById("save-storage-status").textContent.startsWith(from), from);

function guestWords(page, address, count) {
  return page.evaluate(async ({ address, count }) => {
    const { emulator } = await import("./app.js");
    return Promise.all(Array.from({ length: count }, (_, i) => emulator.read_long(address + i * 4)));
  }, { address, count });
}

async function playUntil(page, address) {
  await page.locator("#play").click();
  await waitForGuest(page, async address => {
    const { emulator } = await import("./app.js");
    await new Promise(resolve => setTimeout(resolve, 100));
    return (await emulator.read_long(address)) === 0x600D;
  }, address, { timeout: 120000 });
  await page.locator("#play").click();
}

function digests(page) {
  return page.evaluate(async () => {
    const { emulator } = await import("./app.js");
    const bytes = await Promise.all([emulator.export_backup_ram(), emulator.export_cartridge_backup_ram()]);
    return Promise.all(bytes.map(async image => Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", image)),
      byte => byte.toString(16).padStart(2, "0")).join("")));
  });
}

async function download(page, id) {
  const downloaded = page.waitForEvent("download");
  await page.locator("#" + id).click();
  const file = await downloaded;
  return [file.suggestedFilename(), await readFile(await file.path())];
}

async function importAcrossReset(browser, url) {
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    await page.goto(url);
    await ready(page);
    await page.locator("#view-debug").click();
    await settings(page).click();
    await standby(page);
    const worker = page.workers().find(worker => worker.url() === url + "playback-worker.js");
    assert(worker);
    const committed = Buffer.alloc(32768, 0x55);
    for (const abort of [false, true]) {
      // Let IndexedDB commit or roll back normally. Hold the event handler
      // that informs the Worker of the outcome until Reset is requested.
      await worker.evaluate(abort => {
        const transaction = IDBDatabase.prototype.transaction;
        self.importResultAvailable = new Promise(available => {
          IDBDatabase.prototype.transaction = function (...args) {
            const tx = transaction.apply(this, args);
            if (args[1] !== "readwrite") return tx;
            IDBDatabase.prototype.transaction = transaction;
            const event = abort ? "abort" : "complete";
            Object.defineProperty(tx, "on" + event, {
              set(callback) {
                tx.addEventListener(event, receipt => {
                  self.releaseImportResult = () => callback(receipt);
                  available();
                }, { once: true });
              },
            });
            if (abort) {
              const store = tx.objectStore("battery");
              const put = store.put;
              store.put = function (value, key) {
                if (key === "head") throw new DOMException("Test quota failure", "QuotaExceededError");
                return put.call(this, value, key);
              };
            }
            return tx;
          };
        });
      }, abort);
      const image = abort ? Buffer.alloc(32768, 0xA4) : committed;
      await page.locator("#backup-import").setInputFiles(filename("reset-import.bin", image));
      await worker.evaluate(() => self.importResultAvailable);
      await page.evaluate(() => { document.getElementById("reset").click(); });
      assert.equal(await page.locator("#main").getAttribute("aria-busy"), "true");
      assert.match(await text(page, "backup-status"), /^Importing /);
      assert.equal(await page.locator("#backup-import").isDisabled(), true);
      await worker.evaluate(() => { self.releaseImportResult(); });
      await page.waitForFunction(abort => document.getElementById("backup-status").textContent.startsWith(
        abort ? "Backup unchanged: Error: Test quota failure" : "Imported"), abort);
      await standby(page);
      assert.equal((await digests(page))[0], checksum(committed));
    }
    await page.reload();
    await ready(page);
    assert.equal((await digests(page))[0], checksum(committed));
  } finally { await context.close(); }
}

async function sessionRecovery(page, url, storedImages) {
  const workerUrl = url + "playback-worker.js";
  const storedHead = () => page.evaluate(async () => {
    const db = await new Promise((resolve, reject) => {
      const request = indexedDB.open("saturn-rust-console");
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    try {
      return await new Promise((resolve, reject) => {
        const request = db.transaction("battery").objectStore("battery").get("head");
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
    } finally { db.close(); }
  });
  await standby(page);
  const head = await storedHead();
  const source = await readFile("web/playback-worker.js", "utf8");
  await page.route(workerUrl, route => route.fulfill({ contentType: "text/javascript", body: `
    self.testStorageDenied = true;
    const testDatabase = self.indexedDB;
    Object.defineProperty(self, "indexedDB", { get() {
      if (self.testStorageDenied) throw new DOMException("Test storage denied", "SecurityError");
      return testDatabase;
    } });
    ${source}
  ` }));
  await page.reload();
  await ready(page);
  await settings(page).click();
  await status(page, "Could not restore local saves.");
  const sessionOnly = page.getByRole("checkbox", { name: "Import for this session only" });
  assert.equal(await sessionOnly.isChecked(), false);
  await sessionOnly.focus();
  await page.keyboard.press("Space");
  assert.equal(await sessionOnly.isChecked(), true);
  const recovered = [Buffer.alloc(32768, 0x59), Buffer.alloc(524288, 0xA7)];
  for (const [index, id] of ["backup", "cartridge-backup"].entries()) {
    const bytes = recovered[index];
    await page.locator(`#${id}-import`).setInputFiles(filename(`${id}-recovery.bin`, bytes));
    await page.waitForFunction(id => document.getElementById(id + "-status").textContent.includes("for this session only"), id);
    await status(page, "Console changes are kept in this session.");
    const [, exported] = await download(page, `${id}-export`);
    assert.deepEqual(exported, bytes);
  }
  // Restore access in the same Worker. Explicit Retry and reset must still
  // leave the stored revision untouched until the session is discarded.
  const worker = page.workers().find(worker => worker.url() === workerUrl);
  assert(worker);
  await worker.evaluate(() => { self.testStorageDenied = false; });
  await page.evaluate(async () => (await import("./app.js")).emulator.retrySaves());
  await page.locator("#view-debug").click();
  await page.locator("#reset").click();
  await standby(page);
  await status(page, "Console changes are kept in this session.");
  assert.deepEqual(await digests(page), recovered.map(checksum));
  assert.deepEqual(await storedHead(), head);
  await page.unroute(workerUrl);
  await page.reload();
  await ready(page);
  assert.deepEqual(await digests(page), storedImages);
}

async function bios() {
  const bytes = Buffer.alloc(524288);
  for (const line of (await readFile("tests/fixtures/cartridge.hex", "utf8")).trim().split("\n").slice(1)) {
    const [at, ...words] = line.split(/\s+/).map(word => parseInt(word, 16));
    words.forEach((word, i) => bytes.writeUInt16BE(word, at + i * 2));
  }
  return bytes;
}

async function production(browser, url) {
  const context = await browser.newContext({ viewport: { width: 360, height: 780 } });
  const errors = [], unexpectedRequests = [];
  context.on("request", request => {
    if (!request.url().startsWith(url) || request.method() !== "GET" || request.postData()
        || /\.(iso|chd|cue|bin)(?:$|\?)/.test(request.url())) {
      if (!request.url().endsWith("/favicon.ico")) unexpectedRequests.push(request.url());
    }
  });
  const page = await context.newPage();
  page.on("pageerror", error => errors.push(error.message));
  const iso = await readFile("tests/fixtures/hle-cartridge-backup.iso");
  const expected = (await readFile("tests/fixtures/hle-cartridge-backup.tsv", "utf8")).trim().split("\n")
    .map(line => parseInt(line.split("\t")[1], 16));
  try {
    await page.goto(url);
    await ready(page);
    await settings(page).click();
    assert.equal(await page.locator("#cartridge").inputValue(), "0");
    assert.equal(await page.getByRole("group", { name: "Internal memory · 32 KiB" }).count(), 1);
    assert.equal(await page.getByRole("group", { name: "Backup cartridge · 512 KiB" }).count(), 1);
    await status(page, "Local storage is available.");
    const selector = page.getByLabel("Virtual cartridge");
    await page.waitForFunction(() => !document.getElementById("cartridge").disabled);
    await selector.focus();
    await page.keyboard.press("Tab");
    await page.keyboard.press("Shift+Tab");
    assert.equal(await page.evaluate(() => document.activeElement.id), "cartridge");
    await selector.selectOption("1");
    await standby(page);
    assert.equal(await selector.inputValue(), "1");
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 360);
    await settings(page).click();

    await page.locator("#disc").setInputFiles(filename("original-cartridge-backup.iso", iso));
    await waitForLoadedDisc(page);
    await playUntil(page, TRACE + (expected.length - 1) * 4);
    await status(page, "Local storage is up to date.");
    assert.deepEqual(await guestWords(page, TRACE, expected.length), expected);
    const original = await digests(page);

    // The entire storage inventory is a single shared console. The media
    // file, expansion RAM, renderer and audio/trace state are absent.
    assert.deepEqual(await page.evaluate(async () => {
      const dbs = await indexedDB.databases();
      const db = await new Promise((resolve, reject) => {
        const request = indexedDB.open("saturn-rust-console");
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      const values = await new Promise(resolve => {
        const store = db.transaction("battery").objectStore("battery");
        const keys = store.getAllKeys(), records = store.getAll();
        records.onsuccess = () => resolve([keys.result, records.result.map(record => record.length ?? 0)]);
      });
      db.close();
      return [dbs.map(db => db.name), values, [...Object.keys(localStorage)].sort()];
    }), [["saturn-rust-console"], [["cartridge", "head", "internal", "smpc"], [524288, 0, 32768, 16]], ["saturn-rust-cartridge"]]);

    await settings(page).click();
    await selector.selectOption("3");
    await standby(page);
    assert.deepEqual(await digests(page), original);
    assert.match(await text(page, "play"), /Resume/);
    assert.match(await text(page, "disc-selection"), /original-cartridge-backup.iso/);
    assert.deepEqual(await guestWords(page, TRACE, 2), [0, 0]); // Cold-boot, still paused.
    assert.equal((await guestWords(page, 0x24FFFFFC, 1))[0], 0xFFFFFF5C);
    await page.reload();
    await ready(page);
    await settings(page).click();
    assert.equal(await selector.inputValue(), "3");
    assert.deepEqual(await digests(page), original); // Before any guest can execute.
    assert.equal(await text(page, "disc-selection"), "No disc selected.");
    assert.equal(await text(page, "bios-selection"), "No BIOS selected.");
    await selector.selectOption("1");
    await standby(page);
    const [name, exported] = await download(page, "cartridge-backup-export");
    assert.equal(name, "saturn-power-memory.bin");
    assert.equal(exported.length, 524288);
    assert.equal(checksum(exported), original[1]);
    assert.deepEqual(exported.subarray(0, 8), Buffer.from("SRBUP001"));
    await page.locator("#cartridge-backup-clear").click();
    const dialog = page.getByRole("dialog", { name: "Clear backup cartridge?" });
    assert(await dialog.isVisible());
    assert.equal(await page.evaluate(() => document.activeElement.textContent), "Cancel");
    await page.keyboard.press("Escape");
    assert.equal(await page.evaluate(() => document.activeElement.id), "cartridge-backup-clear");
    assert.deepEqual(await digests(page), original);

    // Cancel an in-flight local cartridge read via the normal boot boundary.
    await page.evaluate(() => {
      const read = File.prototype.arrayBuffer;
      File.prototype.arrayBuffer = function () {
        if (this.name !== "cancel-cartridge.bin") return read.call(this);
        return new Promise(resolve => { window.finishCartridgeRead = () => {
          File.prototype.arrayBuffer = read;
          resolve(read.call(this));
        }; });
      };
    });
    await page.locator("#cartridge-backup-import").setInputFiles(filename("cancel-cartridge.bin", Buffer.alloc(524288, 0x9F)));
    await page.waitForFunction(() => Boolean(window.finishCartridgeRead));
    await page.locator("#view-debug").click();
    await page.locator("#reset").click();
    await page.evaluate(() => { window.finishCartridgeRead(); });
    assert.match(await text(page, "cartridge-backup-status"), /cancelled by boot change/);
    assert.deepEqual(await digests(page), original);
    await page.locator("#view-play").click();
    await settings(page).click();

    const probe = Buffer.from(iso);
    probe.writeUInt32BE(0x0600E000, 0xE28);
    await page.locator("#disc").setInputFiles(filename("another-disc.iso", probe));
    await waitForLoadedDisc(page);
    await playUntil(page, TRACE + 24);
    assert.deepEqual(await guestWords(page, TRACE, 7), [0, 0, 0xDECAFBAD, 0, 0, 0x12345678, 0x600D]);
    const firmware = await bios();
    firmware.writeUInt32BE(0x1400); // Read-only reset vector, no HLE or writes.
    await page.locator("#bios").setInputFiles(filename("original-firmware.bin", firmware));
    await page.waitForFunction(() => !document.getElementById("play").disabled && document.getElementById("bios-state").textContent === "Loaded locally");
    await playUntil(page, BIOS_TRACE + 21 * 4);
    assert.deepEqual(await guestWords(page, BIOS_TRACE, 5), [0x21, 0xFF21, 0xFF00FF21, 0xFF53FF52, 0xFF53FF52]);
    assert.deepEqual(await digests(page), original);
    await status(page, "Local storage is up to date.");

    const other = await context.newPage();
    await other.goto(url);
    await ready(other);
    await settings(other).click();
    assert.deepEqual(await digests(other), original);
    await settings(page).click();
    await page.locator("#cartridge-backup-clear").click();
    await dialog.getByRole("button", { name: "Erase all saves" }).click();
    await status(page, "Local storage is up to date.");
    const cleared = [original[0], checksum(Buffer.alloc(524288))];
    assert.deepEqual(await digests(page), cleared);
    const picker = other.locator("#cartridge-backup-import");
    await picker.setInputFiles(filename("bad-size.bin", Buffer.alloc(524287)));
    await other.waitForFunction(() => document.getElementById("cartridge-backup-status").textContent.includes("received 524287"));
    assert.deepEqual(await digests(other), original);
    await picker.setInputFiles(filename("stale-writer.bin", Buffer.alloc(524288, 0xA5)));
    await status(other, "Changes are unsaved.");
    assert.deepEqual(await digests(other), original);
    await other.locator("#retry-saves").click();
    await status(other, "Changes are unsaved.");
    const [, recovered] = await download(other, "cartridge-backup-export");
    assert.deepEqual(recovered, exported);
    await other.close();

    await page.reload();
    await ready(page);
    assert.deepEqual(await digests(page), cleared);
    await settings(page).click();
    await page.locator("#cartridge-backup-import").setInputFiles(filename(name, exported));
    await page.waitForFunction(() => document.getElementById("cartridge-backup-status").textContent.startsWith("Imported"));
    await status(page, "Local storage is up to date.");
    await page.reload();
    await ready(page);
    assert.deepEqual(await digests(page), original);
    const devtools = await context.newCDPSession(page);
    await devtools.send("Storage.clearDataForOrigin", { origin: new URL(url).origin, storageTypes: "indexeddb,local_storage" });
    await page.reload();
    await ready(page);
    await status(page, "Local storage is available.");
    assert.deepEqual(await digests(page), [checksum(Buffer.alloc(32768)), checksum(Buffer.alloc(524288))]);
    assert.equal(await selector.inputValue(), "0");

    // A firmware reset-vector guest also writes saves and SMPC battery fields.
    await settings(page).click();
    await selector.selectOption("1");
    await standby(page);
    await settings(page).click();
    const writer = await bios();
    await page.locator("#bios").setInputFiles(filename("original-battery-writer.bin", writer));
    await page.waitForFunction(() => !document.getElementById("play").disabled);
    await playUntil(page, BIOS_TRACE + 42 * 4);
    await status(page, "Local storage is up to date.");
    const newInternal = Buffer.alloc(32768), newCartridge = Buffer.alloc(524288);
    newInternal.set([0x22, 0x44]);
    newCartridge.set([0xB2, 0xD4, 0x67]);
    newCartridge[524287] = 0x34;
    const firmwareSaves = [checksum(newInternal), checksum(newCartridge)];
    assert.deepEqual(await digests(page), firmwareSaves);
    await page.reload();
    await ready(page);
    assert.deepEqual(await digests(page), firmwareSaves);
    await page.locator("#bios").setInputFiles(filename("original-battery-probe.bin", firmware));
    await page.waitForFunction(() => !document.getElementById("play").disabled);
    await playUntil(page, BIOS_TRACE + 21 * 4);
    const report = await guestWords(page, BIOS_TRACE, 21);
    assert.deepEqual(report.slice(0, 8), [0x21, 0xFF21, 0xFF00FF21, 0xFF22FF44, 0xFFB2FFD4, 0xC0, 0x20, 0x24]);
    assert([0x32, 0x43].includes(report[8]));
    assert.deepEqual(report.slice(17), [0x12, 0x34, 0x56, 0x78]);
    assert.deepEqual(await digests(page), firmwareSaves);
    await sessionRecovery(page, url, firmwareSaves);
    assert.deepEqual(errors, []);
    assert.deepEqual(unexpectedRequests, []);
  } finally { await context.close(); }
}

async function storageFailures(browser, url) {
  const context = await browser.newContext();
  try {
    const page = await context.newPage();
    const testPage = url + "console-storage-test.html";
    await page.route(testPage, route => route.fulfill({ contentType: "text/html", body: "<title>Console storage test</title>" }));
    await page.goto(testPage);
    const result = await page.evaluate(async () => {
      const { ConsoleSaves } = await import("./console-saves.js");
      const { Emulator, default: init } = await import("./pkg/saturn_rust.js");
      await init();
      const require = (value, description) => { if (!value) throw new Error(description); };
      const failed = async (promise, pattern) => {
        try { await promise; } catch (error) { require(pattern.test(error.message), error.message); return; }
        throw new Error("Expected a storage failure");
      };
      const snapshot = Uint8Array.of(0x20, 0x24, 0x32, 0x29, 0x23, 0x59, 0x59, 1, 0x12, 0x34, 0x56, 0x78, 0, 0, 0, 0);
      const plain = new Uint8Array(32768).fill(0x61);
      const newer = new Uint8Array(32768).fill(0x62);
      const received = [];
      let now = 20000;
      const make = machine => new ConsoleSaves(machine, status => received.push(status), { now: () => now });
      const editStorage = (db, edit) => new Promise((resolve, reject) => {
        const tx = db.transaction("battery", "readwrite");
        tx.oncomplete = resolve;
        tx.onabort = reject;
        edit(tx.objectStore("battery"));
      });
      for (const heads of [[{ version: 1 }], [undefined], []]) {
        const stale = make(new Emulator());
        await stale.restore();
        require(stale.state === "empty", "empty storage before concurrent records");
        await editStorage(stale.db, store => {
          store.put(newer, "internal");
          for (const head of heads) store.put(head, "head");
        });
        await failed(stale.replace(0, plain), /Local saves changed/);
        await stale.flush({ retry: true });
        require(stale.state === "write-error" && stale.machine.export_backup_ram()[0] === 0,
          "retry preserved local RAM and refused to adopt the unreadable revision");
        const fresh = make(new Emulator());
        await fresh.restore();
        require(fresh.state === "restore-error", "nonempty storage with a missing or malformed head");
        await editStorage(stale.db, store => {
          const keys = store.getAllKeys(), image = store.get("internal");
          image.onsuccess = () => {
            require(keys.result.join() === (heads.length ? "head,internal" : "internal"),
              "no records created or removed by the rejected first commit");
            require(image.result.every(byte => byte === 0x62), "unreadable stored image was preserved");
          };
        });
        await editStorage(stale.db, store => store.clear());
      }
      const machine = new Emulator(), writer = make(machine);
      await writer.restore();
      require(writer.state === "empty", "fresh storage");
      machine.restore_smpc_battery(snapshot, 0n);
      machine.import_backup_ram(plain);
      const first = writer.flush();
      machine.import_backup_ram(newer); // An asynchronous acknowledgement must leave the newest revision dirty.
      await first;
      require(writer.state === "pending", "stale acknowledgement discarded");
      await writer.flush();
      require(writer.state === "ready", "newest revision committed");

      const target = new Emulator();
      let accessible = false;
      const retry = new ConsoleSaves(target, status => received.push(status), { now: () => now,
        database: () => accessible ? indexedDB : ({ open() { throw new DOMException("denied", "SecurityError"); } }) });
      await retry.restore();
      require(retry.state === "restore-error" && target.export_backup_ram()[0] === 0, "read failure protected");
      await failed(retry.replace(0, plain), /denied/);
      accessible = true;
      now = 19000; // A backward host-clock jump advances by zero.
      await retry.flush({ retry: true });
      require(target.export_backup_ram()[0] === 0x62, "retry restored accepted memory");
      require(target.export_smpc_battery().every((byte, i) => byte === snapshot[i]), "clock rollback changed calendar");

      const originalTx = IDBDatabase.prototype.transaction;
      IDBDatabase.prototype.transaction = function(...args) {
        if (args[1] === "readonly") throw new DOMException("test read failure", "UnknownError");
        return originalTx.apply(this, args);
      };
      const unreadable = make(new Emulator());
      await unreadable.restore();
      IDBDatabase.prototype.transaction = originalTx;
      require(unreadable.state === "restore-error" && unreadable.machine.export_backup_ram()[0] === 0, "read transaction failed");

      now = 22000;
      const later = make(new Emulator());
      await later.restore();
      require([...later.machine.export_smpc_battery().slice(0, 12)].join() ===
        [0x20, 0x24, 0x43, 1, 0, 0, 1, 1, 0x12, 0x34, 0x56, 0x78].join(), "offline leap-day rollover");

      const originalPut = IDBObjectStore.prototype.put;
      IDBObjectStore.prototype.put = function (...args) {
        if (args[1] === "head") throw new DOMException("test quota failure", "QuotaExceededError");
        return originalPut.apply(this, args);
      };
      target.import_backup_ram(plain);
      await retry.flush();
      require(retry.state === "write-error" && target.export_backup_ram()[0] === 0x61, "RAM export after failed commit");
      await failed(retry.replace(0, newer), /quota/);
      const previous = make(new Emulator());
      await previous.restore();
      require(previous.machine.export_backup_ram()[0] === 0x62, "aborted transaction retained previous image");
      IDBObjectStore.prototype.put = originalPut;
      await retry.flush({ retry: true });
      require(retry.state === "ready", "retry acknowledged");
      const changed = make(new Emulator());
      await changed.restore();
      require(changed.machine.export_backup_ram()[0] === 0x61, "retry wrote dirty image");
      // Recovery also works after a successful restore. The dirty session
      // image must stay off disk even when the connection is writable.
      await changed.replace(0, newer, { sessionOnly: true });
      changed.poll();
      await changed.flush({ retry: true });
      require(changed.state === "session-only" && changed.machine.export_backup_ram()[0] === 0x62, "session image remains active");
      const durable = make(new Emulator());
      await durable.restore();
      require(durable.machine.export_backup_ram()[0] === 0x61, "poll and retry preserved the stored image");

      // Modify real IndexedDB records between separate restores.
      const transaction = (key, value) => editStorage(retry.db, store => store.put(value, key));
      await transaction("cartridge", new Uint8Array(524288)); // No matching committed checksum.
      const damaged = make(new Emulator());
      await damaged.restore();
      require(damaged.state === "restore-error" && /Damaged/.test(damaged.error.message), "corruption detected");
      damaged.machine.import_backup_ram(plain);
      await damaged.flush({ retry: true });
      require(damaged.machine.export_backup_ram()[0] === 0x61 && /Export/.test(damaged.error.message), "dirty retry preserved RAM");
      await transaction("head", { ...retry.head, version: 999 });
      const unsupported = make(new Emulator());
      await unsupported.restore();
      require(unsupported.state === "restore-error" && /Unsupported/.test(unsupported.error.message), "unsupported version protected");
      return received.some(status => status.state === "saving") && received.some(status => status.state === "write-error");
    });
    assert.equal(result, true);
  } finally { await context.close(); }
}

export async function consoleSaves(browser, url) {
  await production(browser, url);
  await importAcrossReset(browser, url);
  await storageFailures(browser, url);
}
