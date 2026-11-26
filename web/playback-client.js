// Asynchronous owner of an original-Rust playback Worker. Presentation is
// synchronous: the receipt is returned only after the caller draws the field.
export class PlaybackClient {
  constructor({ onFrame = () => {}, onAudio = () => {}, onError = () => {}, directPresentation = false, persistSaves = false, onSaveStatus = () => {} } = {}) {
    this.onFrame = onFrame; this.onAudio = onAudio; this.onError = onError;
    this.persistSaves = persistSaves;
    this.epoch = 0; this.command = 0; this.nextFrame = 0; this.nextAudio = 0;
    this.pending = new Map(); this.error = null; this.closed = false;
    this.ready = new Promise((resolve, reject) => { this.resolveReady = resolve; this.rejectReady = reject; });
    this.ready.catch(() => {});
    this.worker = new Worker(new URL("./playback-worker.js", import.meta.url), { type: "module" });
    this.worker.onerror = event => this.fail(new Error(event.message || "Playback Worker failed"));
    const frames = [], audio = [];
    let audioAccepted = 0, awaitingAudio = false;
    const present = () => {
      while (frames.length && frames[0].sequence < audioAccepted) {
        const data = frames.shift();
        if (data.epoch === this.epoch) {
          const result = this.onFrame(data);
          if (result?.then) throw new Error("Presentation callback must finish synchronously");
        }
        this.worker.postMessage({ type: "presented", epoch: data.epoch, sequence: data.sequence });
      }
    };
    const acceptAudio = () => {
      while (!awaitingAudio && !this.closed && !this.error && audio.length) {
        const packet = audio.shift();
        const result = packet.epoch === this.epoch ? this.onAudio(packet) : null;
        if (result?.then) {
          // Withhold presentation credit while the Worklet queue is full.
          // The producer can retain at most its normal two pending fields.
          awaitingAudio = true;
          Promise.resolve(result).then(() => {
            awaitingAudio = false;
            if (this.closed || this.error) return;
            audioAccepted++;
            present();
            acceptAudio();
          }).catch(error => this.fail(error));
          return;
        }
        audioAccepted++;
        present();
      }
    };
    const receive = ({ data }) => {
      if (this.closed || this.error) return;
      try {
        if (data.type === "ready") { this.methods = data.methods ?? []; this.view = data.view; this.resolveReady(); return; }
        if (data.type === "save-status") { onSaveStatus(data.status); return; }
        if (data.type === "fatal") throw new Error(data.message);
        if (data.type === "playback-error") { if (data.epoch === this.epoch) this.onError(new Error(data.message)); return; }
        if (data.type === "audio") {
          if (data.sequence !== this.nextAudio++ || !(data.pcm instanceof Int16Array)) throw new Error("PCM order changed");
          audio.push(data);
          acceptAudio();
          return;
        }
        if (data.type === "frame") {
          if (data.sequence !== this.nextFrame++ || !(data.pixels instanceof Uint8Array)
              || data.pixels.length !== data.width * data.height * 4) throw new Error("Presentation order changed");
          // Separate ports can deliver a completed image before its PCM. Join
          // the two ordered streams before presentation and release its credit.
          frames.push(data);
          present();
          return;
        }
        const item = this.pending.get(data.id);
        if (!item || !["result", "error"].includes(data.type)) throw new Error("Unexpected command receipt");
        this.pending.delete(data.id);
        if (data.type === "error") item.reject(new Error(data.message));
        else item.resolve(data.result);
      } catch (error) { this.fail(error); }
    };
    this.worker.onmessage = receive;
    if (directPresentation) {
      const channel = new MessageChannel();
      this.presentationPort = channel.port1;
      channel.port1.onmessage = receive;
      this.worker.postMessage({ type: "presentation", port: channel.port2, persistSaves }, [channel.port2]);
    } else this.worker.postMessage({ type: "presentation", persistSaves });
  }
  fail(error, notify = true) {
    if (this.error) return;
    this.error = error; this.rejectReady(error);
    for (const item of this.pending.values()) item.reject(error);
    this.pending.clear(); this.presentationPort?.close(); this.worker.terminate(); if (notify) this.onError(error);
  }
  async call(command, args = {}) {
    await this.ready;
    if (this.error || this.closed) throw this.error ?? new Error("Playback Worker closed");
    const id = this.command++;
    const result = new Promise((resolve, reject) => this.pending.set(id, { resolve, reject }));
    try { this.worker.postMessage({ ...args, type: "command", id, command }); }
    catch (error) { this.pending.get(id).reject(error); this.pending.delete(id); }
    return result;
  }
  reset(args) { this.epoch++; return this.call("reset", { ...args, epoch: this.epoch }); }
  play({ paced = true } = {}) { return this.call("play", { paced }); }
  pause() { return this.call("pause"); }
  retrySaves() { return this.call("retry-saves", { epoch: ++this.epoch }); }
  advance(fields = 1, { inspect = true } = {}) { return this.call("advance", { fields, inspect }); }
  inspect() { return this.call("inspect"); }
  invoke(operations) {
    if (operations.some(({ method }) => method.startsWith("reset_") || method === "set_cartridge"
      || (this.persistSaves && (method === "import_backup_ram" || method === "import_cartridge_backup_ram"
        || method === "import_battery_recovery")))) this.epoch++;
    return this.call("invoke", { operations, epoch: this.epoch });
  }
  setInput(input) {
    if (this.error || this.closed) throw this.error ?? new Error("Playback Worker closed");
    this.worker.postMessage({ type: "input", epoch: this.epoch, input });
  }
  close() { if (!this.closed) { this.closed = true; this.fail(new Error("Playback Worker closed"), false); } }
}
