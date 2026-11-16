// Main consumes a verified Sound prefix while its exclusive Worker computes
// ahead. Every shared input and completed field returns canonical Sound state.
const clock = (lo, hi) => BigInt(lo) | BigInt(hi) << 32n;
class TransportError extends Error {}
export const SOUND_TRANSPORT_METHODS = new Set([
  "sound_worker_field_eligible", "export_sound_delta", "restore_remote_sound_delta",
  "remote_sound_projection_range", "accept_remote_sound_projection", "extend_remote_sound_projection",
  "remote_sound_projection_time", "advance_sound_projection_field", "advance_sound_rendezvous_field",
  "sound_rendezvous_boundary", "acknowledge_sound_rendezvous", "reset_sound_transport", "abort_sound_transport",
]);
export class SoundClient {
  constructor(machine) {
    this.machine = machine;
    this.serial = 0;
    this.waiters = [];
    this.stats = { jobs: 0, extensions: 0, toWorkerBytes: 0, toMainBytes: 0 };
    this.spawn();
  }
  spawn() {
    this.disabled = false; this.failure = null; this.active = false;
    if (!this.machine.export_sound_delta) { this.disabled = true; this.ready = Promise.resolve(); return; }
    let worker;
    try { worker = this.worker = new Worker(new URL("./sound-worker.js", import.meta.url), { type: "module" }); }
    catch (error) { this.disabled = true; this.failure = error; this.ready = Promise.resolve(); return; }
    const ready = new Promise((resolve, reject) => { this.readyResolve = resolve; this.readyReject = reject; });
    this.ready = ready.catch(() => { this.disabled = true; });
    worker.onmessage = ({ data }) => {
      if (this.worker !== worker) return;
      if (data.type === "ready") { this.readyResolve(); return; }
      if (data.type === "reset" && this.resetResolve && data.job === this.serial && !this.active) {
        const resolve = this.resetResolve;
        this.resetResolve = null; this.resetReject = null; resolve(); return;
      }
      if (data.type === "error") { this.fail(new TransportError(data.message)); return; }
      if (!this.active || data.job !== this.serial) { this.fail(new TransportError("Stale Sound reply")); return; }
      if (data.type === "prefix") {
        if (!(data.prefix instanceof Uint32Array) || data.prefix.length !== 7) {
          this.fail(new TransportError("Invalid Sound prefix")); return;
        }
        this.latest = { stable: clock(data.prefix[4], data.prefix[5]), irq: Boolean(data.prefix[6]), done: data.done };
        for (const waiter of this.waiters.splice(0)) waiter.resolve();
      } else if (data.type === "state" && this.finishResolve && data.state instanceof Uint8Array) {
        this.active = false; this.stats.toMainBytes += data.state.length;
        const resolve = this.finishResolve;
        this.finishResolve = null; this.finishReject = null; resolve(data.state);
      } else this.fail(new TransportError("Unexpected Sound reply"));
    };
    worker.onerror = event => { if (this.worker === worker) this.fail(new TransportError(event.message)); };
  }
  fail(error) {
    this.disabled = true; this.failure = error;
    this.worker?.terminate(); this.worker = null;
    this.readyReject?.(error); this.finishReject?.(error); this.resetReject?.(error);
    this.resetResolve = null; this.resetReject = null;
    this.finishResolve = null; this.finishReject = null;
    for (const waiter of this.waiters.splice(0)) waiter.reject(error);
  }
  close() { this.fail(new TransportError("Sound owner closed")); }
  async reset() {
    await this.ready;
    if (this.active || this.finishResolve || this.waiters.length) throw new TransportError("Reset requires canonical Sound ownership");
    this.machine.reset_sound_transport?.();
    if (this.disabled || !this.worker) {
      this.close(); this.spawn(); await this.ready; return;
    }
    // Keep a healthy owner and its loaded module. Local media replacement and
    // restart must not fetch scripts or Wasm again; only reset its Rust state.
    this.serial++; this.latest = null;
    await new Promise((resolve, reject) => {
      this.resetResolve = resolve; this.resetReject = reject;
      this.worker.postMessage({ type: "reset", job: this.serial });
    });
  }
  async next(after = null) {
    if (this.failure) throw this.failure;
    if (this.latest && (after === null || this.latest.stable > after || this.latest.done)) return this.latest;
    await new Promise((resolve, reject) => this.waiters.push({ resolve, reject }));
    return this.next(after);
  }
  async begin(state, end) {
    if (this.active) throw new TransportError("Duplicate Sound ownership");
    this.active = true; this.latest = null; this.serial++; this.stats.jobs++; this.stats.toWorkerBytes += state.length;
    this.worker.postMessage({ type: "load", job: this.serial, state, end }, [state.buffer]);
    return this.next();
  }
  finish(at) {
    if (this.failure) return Promise.reject(this.failure);
    if (!this.active || this.finishResolve) return Promise.reject(new TransportError("Invalid Sound return"));
    return new Promise((resolve, reject) => {
      this.finishResolve = resolve; this.finishReject = reject;
      this.worker.postMessage({ type: "finish", job: this.serial, at });
    });
  }
  finishSynchronousField() {
    const m = this.machine;
    m.acknowledge_sound_rendezvous();
    while (!m.advance_sound_rendezvous_field()) m.acknowledge_sound_rendezvous();
  }
  async advanceField() {
    await this.ready;
    const m = this.machine;
    if (this.disabled || !m.sound_worker_field_eligible()) { m.advance_video_field(); return; }
    let remote = false, frontier = 0n, end = 0n, done = false, nativeFailure = false;
    const canonicalize = async () => {
      const state = await this.finish(m.remote_sound_projection_time());
      m.restore_remote_sound_delta(state); remote = false;
    };
    try {
      for (;;) {
        if (!remote) {
          if (!m.sound_worker_field_eligible()) { this.finishSynchronousField(); return; }
          const range = m.remote_sound_projection_range(524288);
          if (range.length) {
            const origin = clock(range[0], range[1]); end = clock(range[2], range[3]);
            const prefix = await this.begin(m.export_sound_delta(), end);
            // Physical controller reports can update SMPC deadlines while Main
            // is stationary. Recheck admission before publishing any prefix.
            const current = m.remote_sound_projection_range(524288);
            if (current.length && clock(current[0], current[1]) === origin) {
              const freshEnd = clock(current[2], current[3]);
              end = freshEnd < end ? freshEnd : end;
              frontier = prefix.stable < end ? prefix.stable : end;
            } else frontier = origin;
            if (frontier > origin) {
              m.accept_remote_sound_projection(origin, end, frontier, prefix.irq); remote = true;
            } else m.restore_remote_sound_delta(await this.finish(origin));
          }
        }
        try { done = m.advance_sound_projection_field(); }
        catch (error) { nativeFailure = true; throw error; }
        if (remote) {
          if (done || m.sound_rendezvous_boundary().length) await canonicalize();
          else {
            const at = m.remote_sound_projection_time();
            if (at !== frontier) throw new TransportError("Main stopped outside its Sound frontier");
            if (at < end) {
              const prefix = await this.next(frontier);
              const through = prefix.stable < end ? prefix.stable : end;
              if (through > at) { m.extend_remote_sound_projection(through); frontier = through; this.stats.extensions++; continue; }
            }
            await canonicalize();
          }
        }
        if (done) return;
        m.acknowledge_sound_rendezvous();
      }
    } catch (error) {
      this.close(); this.active = false;
      // Main retained real hardware at the forecast origin. Replay only that
      // input-free prefix on transport failure, then continue synchronously.
      m.abort_sound_transport();
      if (nativeFailure) throw error;
      if (!done) this.finishSynchronousField();
    }
  }
}
