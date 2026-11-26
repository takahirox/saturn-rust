// One local console per origin. Head and changed devices commit atomically.
const DATABASE = "saturn-rust-console";
const STORE = "battery";
const VERSION = 1;
const KEYS = ["internal", "cartridge", "smpc"];
const SIZES = [32768, 524288, 16];
const IO_TIMEOUT = 5000;
const RTC_CHECKPOINT_MS = 30000;
const BATTERY_SIGNATURE = new TextEncoder().encode("SRSMPC01");
export const BATTERY_RECOVERY_SIZE = 36;
const conflict = () => new Error("Local saves changed in another tab or were cleared. Export this session’s saves and clock settings before reloading.");

function checksum(bytes) {
  let hash = 2166136261;
  for (const byte of bytes) hash = Math.imul(hash ^ byte, 16777619);
  return hash >>> 0;
}

function nowMillis(now) {
  const value = now();
  if (!Number.isSafeInteger(value) || value < 0) throw new Error("Invalid host clock");
  return value;
}

function openDatabase(factory) {
  return new Promise((resolve, reject) => {
    let expired = false;
    const request = factory.open(DATABASE, VERSION);
    const timer = setTimeout(() => {
      expired = true;
      reject(new Error("Timed out opening local save storage"));
    }, IO_TIMEOUT);
    request.onupgradeneeded = () => {
      if (expired) request.transaction.abort();
      else request.result.createObjectStore(STORE);
    };
    request.onerror = () => {
      clearTimeout(timer);
      reject(request.error);
    };
    request.onsuccess = () => {
      clearTimeout(timer);
      if (expired) request.result.close();
      else resolve(request.result);
    };
  });
}

function transaction(db, mode, run) {
  const tx = mode === "readwrite"
    ? db.transaction(STORE, mode, { durability: "strict" })
    : db.transaction(STORE, mode);
  return new Promise((resolve, reject) => {
    let value, cause;
    const timer = setTimeout(() => {
      try {
        tx.abort();
        cause = new Error("Local save transaction timed out");
      } catch { /* Completion is already queued. */ }
    }, IO_TIMEOUT);
    const cancel = error => { cause = error; tx.abort(); };
    tx.oncomplete = () => { clearTimeout(timer); resolve(value); };
    tx.onabort = () => {
      clearTimeout(timer);
      reject(cause ?? tx.error ?? new Error("Local save transaction aborted"));
    };
    try {
      run(tx.objectStore(STORE), result => { value = result; }, cancel);
    } catch (error) { cancel(error); }
  });
}

function readConsole(db) {
  return transaction(db, "readonly", (store, finish) => {
    const head = store.get("head");
    const images = KEYS.map(key => store.get(key));
    const count = store.count();
    count.onsuccess = () => finish([head.result, images.map(request => request.result), count.result]);
  });
}

function validateConsole(head, images, count) {
  if (head === undefined && count === 0) return;
  if (!head || head.version !== VERSION) throw new Error("Unsupported console save record");
  if (typeof head.token !== "string" || !/^[0-9a-f]{32}$/.test(head.token)
      || !Array.isArray(head.hashes) || head.hashes.length !== KEYS.length
      || !Number.isSafeInteger(head.anchor) || head.anchor < 0 || !images[2]) {
    throw new Error("Damaged console save record");
  }
  for (const [index, bytes] of images.entries()) {
    if (bytes === undefined && head.hashes[index] === null) continue;
    if (!(bytes instanceof Uint8Array) || bytes.length !== SIZES[index] || checksum(bytes) !== head.hashes[index]) {
      throw new Error("Damaged console save image");
    }
  }
}

export class ConsoleSaves {
  constructor(machine, report, { now = Date.now, database = () => globalThis.indexedDB } = {}) {
    this.machine = machine;
    this.report = report;
    this.now = now;
    this.database = database;
    this.revisions = this.currentRevisions();
    this.state = "loading";
    this.report({ state: this.state });
  }

  currentRevisions() {
    return [this.machine.backup_revision(), this.machine.cartridge_backup_revision(), this.machine.smpc_battery_revision()];
  }

  async connect() {
    if (this.db) return this.db;
    const factory = this.database();
    if (!factory) throw new Error("IndexedDB is unavailable");
    const db = await openDatabase(factory);
    db.onclose = () => { if (this.db === db) this.db = null; };
    db.onversionchange = () => { db.close(); db.onclose(); };
    this.db = db;
    return db;
  }

  status(state, error) {
    this.state = state;
    this.error = error;
    this.report({ state, message: error?.message, devices: this.head?.hashes.map(hash => hash !== null) });
  }

  async restore() {
    try {
      if (this.currentRevisions().some((revision, i) => revision !== this.revisions[i])) {
        throw new Error("This session has changed saves or clock settings. Export both before reloading to recover local storage.");
      }
      const [head, images, count] = await readConsole(await this.connect());
      validateConsole(head, images, count);
      const now = nowMillis(this.now);
      if (head) {
        // The core validates BCD, calendar ranges, flags and phase before
        // installing the snapshot. Both image lengths are already validated.
        this.machine.restore_smpc_battery(images[2], BigInt(Math.max(0, now - head.anchor)));
        if (images[0]) this.machine.import_backup_ram(images[0]);
        if (images[1]) this.machine.import_cartridge_backup_ram(images[1]);
      }
      this.head = head;
      this.restored = true;
      this.revisions = this.currentRevisions();
      // Use the clock after offline advancement as the baseline for local changes.
      this.battery = this.machine.export_smpc_battery();
      this.hasCommitted = false;
      this.lastCheckpoint = now;
      this.status(head ? "ready" : "empty");
      return Boolean(head);
    } catch (error) { this.status("restore-error", error); }
  }

  // A low-frequency timer checks three small revision numbers. The clock
  // snapshot is sampled separately. Backup images cross Wasm only when dirty.
  poll() {
    if (!this.restored || this.saving || this.error || this.state === "session-only") return;
    this.saveIfChanged();
  }

  async flush({ checkpoint = false, retry = false, replacement } = {}) {
    while (this.saving) await this.saving;
    if (this.state === "session-only") {
      if (replacement) throw new Error("Reload the player to resume local storage.");
      return;
    }
    const restoredRecord = !this.restored && retry && await this.restore();
    if (!this.restored || (this.error && !retry)) {
      if (replacement) throw this.error ?? new Error("Local saves are still loading");
      return;
    }
    if (this.error && retry) {
      // A rejected replacement can leave no dirty bytes. Confirm that this
      // session still owns its observed revision before clearing the error.
      try {
        const matches = await transaction(await this.connect(), "readonly", (store, finish) => {
          this.checkRevision(store, finish);
        });
        if (!matches) throw conflict();
      } catch (error) { this.status("write-error", error); return; }
    }
    await this.saveIfChanged({ immediate: true, checkpoint, replacement });
    if (replacement && this.error) throw this.error;
    return restoredRecord;
  }

  async replace(index, bytes, { sessionOnly = false } = {}) {
    if ((index !== 0 && index !== 1) || !(bytes instanceof Uint8Array) || bytes.length !== SIZES[index]) {
      throw new Error(`Expected a packed ${SIZES[index]}-byte backup image`);
    }
    return this.replaceImage({ index, bytes }, sessionOnly);
  }

  exportBatteryRecovery() {
    const recovery = new Uint8Array(BATTERY_RECOVERY_SIZE);
    const view = new DataView(recovery.buffer);
    recovery.set(BATTERY_SIGNATURE);
    view.setBigUint64(8, BigInt(Math.max(nowMillis(this.now), this.head?.anchor ?? 0)));
    recovery.set(this.machine.export_smpc_battery(), 16);
    view.setUint32(32, checksum(recovery.subarray(0, 32)));
    return recovery;
  }

  async importBatteryRecovery(recovery, { sessionOnly = false } = {}) {
    if (!(recovery instanceof Uint8Array) || recovery.length !== BATTERY_RECOVERY_SIZE
        || !BATTERY_SIGNATURE.every((byte, i) => byte === recovery[i])) {
      throw new Error("Unsupported clock and settings file");
    }
    const view = new DataView(recovery.buffer, recovery.byteOffset, recovery.byteLength);
    const anchor = Number(view.getBigUint64(8));
    if (!Number.isSafeInteger(anchor) || checksum(recovery.subarray(0, 32)) !== view.getUint32(32)) {
      throw new Error("Damaged clock and settings file");
    }
    const now = nowMillis(this.now);
    const bytes = this.machine.constructor.advance_smpc_battery_snapshot(
      recovery.subarray(16, 32), BigInt(Math.max(0, now - anchor)));
    return this.replaceImage({ index: 2, bytes, anchor: Math.max(now, anchor) }, sessionOnly);
  }

  async replaceImage(replacement, sessionOnly) {
    if (!sessionOnly) return this.flush({ replacement });
    while (this.saving) await this.saving;
    this.installImage(replacement);
    // Until reload, automatic writes and retries must leave the stored copy alone.
    this.status("session-only");
  }

  installImage({ index, bytes }) {
    if (index === 0) this.machine.import_backup_ram(bytes);
    else if (index === 1) this.machine.import_cartridge_backup_ram(bytes);
    else this.machine.restore_smpc_battery(bytes, 0n);
  }

  saveIfChanged({ immediate = false, checkpoint = false, replacement } = {}) {
    try {
      const now = nowMillis(this.now);
      const revisions = this.currentRevisions();
      const changed = revisions.map((revision, i) => revision !== this.revisions[i]);
      const checkClock = immediate || now - this.lastCheckpoint >= RTC_CHECKPOINT_MS;
      if (!replacement && !changed.some(Boolean) && !checkClock) return;
      const battery = replacement?.index === 2 ? replacement.bytes : this.machine.export_smpc_battery();
      // Only session boundaries can reanchor an unchanged clock, and only
      // after this session has committed changes of its own.
      const reanchor = checkpoint && this.hasCommitted && now > this.head.anchor;
      if (!replacement && !reanchor && !changed.some(Boolean) && battery.every((byte, i) => byte === this.battery[i])) {
        this.lastCheckpoint = now;
        if (this.error) this.status(this.head ? "ready" : "empty");
        return;
      }
      if (replacement) changed[replacement.index] = true;
      const images = [
        changed[0] && (replacement?.index === 0 ? replacement.bytes : this.machine.export_backup_ram()),
        changed[1] && (replacement?.index === 1 ? replacement.bytes : this.machine.export_cartridge_backup_ram()),
        battery,
      ];
      this.status("saving");
      this.saving = this.commit(images, revisions, now, replacement).finally(() => { this.saving = null; });
      return this.saving;
    } catch (error) { this.status("write-error", error); }
  }

  checkRevision(store, finish) {
    if (this.head === undefined) {
      // A missing head alone can also mean that the store contains orphaned images.
      const count = store.count();
      count.onsuccess = () => finish(count.result === 0);
    } else {
      const read = store.get("head");
      read.onsuccess = () => finish(read.result?.version === VERSION && read.result.token === this.head.token);
    }
  }

  async commit(images, revisions, now, replacement) {
    try {
      const token = Array.from(crypto.getRandomValues(new Uint8Array(16)),
        byte => byte.toString(16).padStart(2, "0")).join("");
      const head = {
        version: VERSION, token, anchor: Math.max(now, replacement?.anchor ?? 0, this.head?.anchor ?? 0),
        hashes: images.map((bytes, i) => bytes ? checksum(bytes) : this.head?.hashes[i] ?? null),
      };
      await transaction(await this.connect(), "readwrite", (store, _finish, cancel) => {
        this.checkRevision(store, matches => {
          if (!matches) {
            cancel(conflict());
            return;
          }
          try {
            for (const [index, bytes] of images.entries()) {
              if (bytes) store.put(bytes, KEYS[index]);
            }
            store.put(head, "head");
          } catch (error) { cancel(error); }
        });
      });
      if (replacement) {
        const { index } = replacement;
        this.installImage(replacement);
        revisions[index] = this.currentRevisions()[index];
      }
      this.head = head;
      this.hasCommitted = true;
      this.revisions = revisions;
      this.battery = images[2];
      this.lastCheckpoint = now;
      this.status(this.currentRevisions().some((revision, i) => revision !== revisions[i]) ? "pending" : "ready");
    } catch (error) { this.status("write-error", error); }
  }
}
